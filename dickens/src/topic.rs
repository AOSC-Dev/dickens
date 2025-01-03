use libaosc::packages::{FetchPackagesAsync, FetchPackagesError, Package};
use log::info;
use reqwest::ClientBuilder;
use sha2::{Digest, Sha256};
use size::{Base, Size};
use solver::PackageVersion;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};
use tokio::io::AsyncWriteExt;

async fn fetch_pkgs(
    arch: &str,
    topic: &str,
    local_repo: Option<PathBuf>,
) -> anyhow::Result<Vec<Package>> {
    let res = if let Some(local_repo) = local_repo {
        // read package file from local repo directly
        // /debs/dists/{branch}/main/binary-{arch}/Packages
        let mut path = local_repo.clone();
        path.push("debs");
        path.push("dists");
        path.push(&topic);
        path.push("main");
        path.push(format!("binary-{arch}"));
        path.push("Packages");

        if !path.exists() {
            return Ok(vec![]);
        }

        let content = std::fs::read(path)?;
        (content.as_slice()).try_into()?
    } else {
        let fetcher =
            FetchPackagesAsync::new(true, format!("dists/{topic}/main/binary-{arch}"), None);
        match fetcher.fetch_packages(arch, topic).await {
            Ok(res) => res,
            Err(FetchPackagesError::ReqwestError(err)) => {
                info!("Got reqwest error: {err}");
                return Ok(vec![]);
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    };

    // only keep latest version for each (package, architecture) tuple
    let mut pkgs: BTreeMap<(String, String), Package> = BTreeMap::new();
    for pkg in res.0 {
        use std::collections::btree_map::Entry::{Occupied, Vacant};
        match pkgs.entry((pkg.package.clone(), pkg.architecture.clone())) {
            Vacant(entry) => {
                entry.insert(pkg);
            }
            Occupied(mut entry) => {
                if PackageVersion::from(&entry.get().version).unwrap()
                    < PackageVersion::from(&pkg.version).unwrap()
                {
                    entry.insert(pkg);
                }
            }
        }
    }

    let real_res = pkgs.into_values().collect();

    Ok(real_res)
}

async fn download_pkg(pkg: &Package) -> anyhow::Result<PathBuf> {
    // https://georgik.rocks/how-to-download-binary-file-in-rust-by-reqwest/
    let mut out = PathBuf::new();
    out.push("debs");
    out.push(Path::new(&pkg.filename).file_name().unwrap());
    if out.exists() {
        let content = std::fs::read(&out)?;
        let hash = Sha256::digest(&content);
        let hash_str = hash
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .concat();
        if hash_str == pkg.sha256 {
            info!(
                "Skipping already downloaded https://repo.aosc.io/debs/{} at {:?}",
                pkg.filename, out
            );
            return Ok(out);
        }
    }

    info!(
        "Downloading https://aosc.io/debs/{} to {:?}",
        pkg.filename, out
    );
    tokio::fs::create_dir_all("debs").await?;
    let mut file = tokio::fs::File::create(&out).await?;

    let client = ClientBuilder::new().user_agent("dickens").build()?;
    let mut response = client
        .get(format!("https://repo.aosc.io/debs/{}", pkg.filename))
        .send()
        .await?
        .error_for_status()?;

    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
    }

    Ok(out)
}

#[derive(Debug, Clone)]
struct Res {
    package: String,
    archs: Vec<String>,
    old_version: String,
    new_version: String,
    diff: String,
    old_size: u64,
    new_size: u64,
}

async fn handle_arch(
    arch: &str,
    topic: String,
    local_repo: Option<PathBuf>,
) -> anyhow::Result<Vec<Res>> {
    let mut res = vec![];
    let topic_pkgs = fetch_pkgs(arch, &topic, local_repo.clone()).await?;
    if topic_pkgs.is_empty() {
        // no new packages
        return Ok(res);
    }

    let stable_pkgs = fetch_pkgs(arch, "stable", local_repo.clone()).await?;
    for topic_pkg in topic_pkgs {
        if topic_pkg.package.ends_with("-dbg") {
            continue;
        }

        if let Some(found) = stable_pkgs.iter().find(|p| p.package == topic_pkg.package) {
            if PackageVersion::from(&found.version)? >= PackageVersion::from(&topic_pkg.version)? {
                // downgrade or no update
                continue;
            }

            info!(
                "Found upgrade {} from {} to {}",
                topic_pkg.package, found.version, topic_pkg.version
            );

            let (diff, old_size, new_size) = if let Some(local_repo) = &local_repo {
                // diff directly
                let mut left = local_repo.clone();
                left.push("debs");
                left.push(&found.filename);

                let mut right = local_repo.clone();
                right.push("debs");
                right.push(&topic_pkg.filename);

                (
                    Command::new("./diff-deb.sh")
                        .arg(&left)
                        .arg(&right)
                        .output()?,
                    std::fs::metadata(left)?.len(),
                    std::fs::metadata(right)?.len(),
                )
            } else {
                // download topic pkg
                let left = download_pkg(&found).await?;
                let right = download_pkg(&topic_pkg).await?;
                (
                    Command::new("./diff-deb.sh")
                        .arg(&left)
                        .arg(&right)
                        .output()?,
                    std::fs::metadata(left)?.len(),
                    std::fs::metadata(right)?.len(),
                )
            };

            let new_res = Res {
                package: topic_pkg.package.clone(),
                archs: vec![topic_pkg.architecture.clone()],
                old_version: found.version.clone(),
                new_version: topic_pkg.version.clone(),
                diff: String::from_utf8_lossy(&diff.stdout).to_string(),
                old_size,
                new_size,
            };

            res.push(new_res);
        } else {
            let (diff, new_size) = if let Some(local_repo) = &local_repo {
                // diff directly
                let mut path = local_repo.clone();
                path.push("debs");
                path.push(&topic_pkg.filename);
                (
                    Command::new("./diff-deb-new.sh").arg(&path).output()?,
                    std::fs::metadata(path)?.len(),
                )
            } else {
                // download topic pkg
                let right = download_pkg(&topic_pkg).await?;
                (
                    Command::new("./diff-deb-new.sh").arg(&right).output()?,
                    std::fs::metadata(right)?.len(),
                )
            };

            let new_res = Res {
                package: topic_pkg.package.clone(),
                archs: vec![topic_pkg.architecture.clone()],
                old_version: "".to_string(),
                new_version: topic_pkg.version.clone(),
                diff: String::from_utf8_lossy(&diff.stdout).to_string(),
                old_size: 0,
                new_size,
            };
            res.push(new_res);
        }
    }
    Ok(res)
}

pub async fn report(topic: &str, local_repo: Option<PathBuf>) -> anyhow::Result<String> {
    let mut report = String::new();
    let mut res: Vec<Res> = vec![];
    let archs = [
        "all",
        "amd64",
        "arm64",
        "loongarch64",
        "loongson3",
        "mips64r6el",
        "ppc64el",
        "riscv64",
    ];
    let handles: Vec<_> = archs
        .iter()
        .map(|arch| tokio::task::spawn(handle_arch(arch, topic.to_string(), local_repo.clone())))
        .collect();

    for handle in handles {
        for new_res in handle.await?? {
            // merge or insert
            let mut insert = true;
            for cur in &mut res {
                if cur.package == new_res.package
                    && cur.old_version == new_res.old_version
                    && cur.new_version == new_res.new_version
                    && cur.diff == new_res.diff
                {
                    cur.archs.extend(new_res.archs.clone());
                    cur.old_size += new_res.old_size;
                    cur.new_size += new_res.new_size;
                    insert = false;
                    break;
                }
            }

            if insert {
                res.push(new_res);
            }
        }
    }

    res.sort_by(|a, b| a.package.cmp(&b.package));

    writeln!(report, "Dickens-topic report:")?;
    writeln!(report, "")?;
    for cur in &res {
        if cur.old_version.is_empty() {
            writeln!(
                report,
                "{} introduced at {} on {}:",
                cur.package,
                cur.new_version,
                cur.archs.join(", ")
            )?;
        } else {
            writeln!(
                report,
                "{} upgraded from {} to {} on {}:",
                cur.package,
                cur.old_version,
                cur.new_version,
                cur.archs.join(", ")
            )?;
        }

        let size_desc = if cur.new_size >= cur.old_size {
            if cur.old_size != 0 {
                format!(
                    ", size +{} (+{:.1}%)",
                    Size::from_bytes(cur.new_size - cur.old_size)
                        .format()
                        .with_base(Base::Base10),
                    ((cur.new_size as f64 / cur.old_size as f64) - 1.0) * 100.0
                )
            } else {
                format!(
                    ", size +{}",
                    Size::from_bytes(cur.new_size - cur.old_size)
                        .format()
                        .with_base(Base::Base10),
                )
            }
        } else {
            format!(
                ", size -{} (-{:.1}%)",
                Size::from_bytes(cur.old_size - cur.new_size)
                    .format()
                    .with_base(Base::Base10),
                (1.0 - (cur.new_size as f64 / cur.old_size as f64)) * 100.0
            )
        };

        if cur.diff.trim().is_empty() {
            writeln!(report, "")?;
            writeln!(report, "No changes{size_desc}")?;
            writeln!(report, "")?;
            continue;
        }

        writeln!(report, "<details>")?;

        let mut added = 0;
        let mut removed = 0;
        for line in cur.diff.lines() {
            if line.starts_with("---") || line.starts_with("+++") {
                continue;
            } else if line.starts_with("+") {
                added += 1;
            } else if line.starts_with("-") {
                removed += 1;
            }
        }

        writeln!(
            report,
            "<summary>{added} added, {removed} removed{size_desc}</summary>",
        )?;
        writeln!(report, "")?;
        writeln!(report, "```diff")?;
        writeln!(report, "{}", cur.diff)?;
        writeln!(report, "```")?;
        writeln!(report, "</details>")?;
    }
    Ok(report)
}
