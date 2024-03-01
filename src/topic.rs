use debversion::Version;
use libaosc::packages::{FetchPackagesAsync, FetchPackagesError, Package};
use log::info;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

async fn fetch_pkgs(arch: &str, topic: &str) -> anyhow::Result<Vec<Package>> {
    let fetcher = FetchPackagesAsync::new(
        true,
        format!("dists/{topic}/main/binary-{arch}"),
        None,
    );
    let res = match fetcher.fetch_packages(arch, topic).await {
        Ok(res) => res,
        Err(FetchPackagesError::ReqwestError(err)) => {
            info!("Got reqwest error: {err}");
            return Ok(vec![]);
        }
        Err(err) => {
            return Err(err.into());
        }
    };

    // only keep latest version
    // packages are already sorted by name
    let mut real_res: Vec<Package> = vec![];
    for pkg in res.get_packages().clone() {
        if let Some(last) = real_res.last_mut() {
            if last.package == pkg.package
                && last.architecture == pkg.architecture
                && last.version.parse::<Version>().unwrap()
                    < pkg.version.parse::<Version>().unwrap()
            {
                *last = pkg;
            } else {
                real_res.push(pkg);
            }
        } else {
            real_res.push(pkg);
        }
    }
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
    let response = reqwest::get(format!("https://repo.aosc.io/debs/{}", pkg.filename)).await?;
    let mut file = std::fs::File::create(&out)?;
    let mut content = Cursor::new(response.bytes().await?);
    std::io::copy(&mut content, &mut file)?;
    Ok(out)
}

#[derive(Debug, Clone)]
struct Res {
    package: String,
    archs: Vec<String>,
    old_version: String,
    new_version: String,
    diff: String,
}

async fn handle_arch(arch: &str, topic: String) -> anyhow::Result<Vec<Res>> {
    let mut res = vec![];
    let topic_pkgs = fetch_pkgs(arch, &topic).await?;
    if topic_pkgs.is_empty() {
        // no new packages
        return Ok(res);
    }

    let stable_pkgs = fetch_pkgs(arch, "stable").await?;
    for topic_pkg in topic_pkgs {
        if topic_pkg.package.ends_with("-dbg") {
            continue;
        }

        if let Some(found) = stable_pkgs.iter().find(|p| p.package == topic_pkg.package) {
            if found.version.parse::<Version>().unwrap()
                >= topic_pkg.version.parse::<Version>().unwrap()
            {
                // downgrade or no update
                continue;
            }

            info!(
                "Found upgrade {} from {} to {}",
                topic_pkg.package, found.version, topic_pkg.version
            );

            // download topic pkg
            let left = download_pkg(&found).await?;
            let right = download_pkg(&topic_pkg).await?;
            let diff = Command::new("./diff-deb.sh")
                .arg(left)
                .arg(right)
                .output()?;

            let new_res = Res {
                package: topic_pkg.package.clone(),
                archs: vec![topic_pkg.architecture.clone()],
                old_version: found.version.clone(),
                new_version: topic_pkg.version.clone(),
                diff: String::from_utf8_lossy(&diff.stdout).to_string(),
            };

            res.push(new_res);
        } else {
            let right = download_pkg(&topic_pkg).await?;
            let diff = Command::new("./diff-deb-new.sh").arg(right).output()?;

            let new_res = Res {
                package: topic_pkg.package.clone(),
                archs: vec![topic_pkg.architecture.clone()],
                old_version: "".to_string(),
                new_version: topic_pkg.version.clone(),
                diff: String::from_utf8_lossy(&diff.stdout).to_string(),
            };
            res.push(new_res);
        }
    }
    Ok(res)
}

pub async fn report(topic: &str) -> anyhow::Result<String> {
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
        .map(|arch| tokio::task::spawn(handle_arch(arch, topic.to_string())))
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
            "<summary>{added} added, {removed} removed</summary>"
        )?;
        writeln!(report, "")?;
        writeln!(report, "```diff")?;
        writeln!(report, "{}", cur.diff)?;
        writeln!(report, "```")?;
        writeln!(report, "</details>")?;
    }
    Ok(report)
}
