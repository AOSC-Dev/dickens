use clap::Parser;
use log::info;
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    process::Command,
};
use xz::read::XzDecoder;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Topic name
    topic: String,
}

#[derive(Debug, Default, Clone)]
struct Package {
    package: String,
    version: String,
    architecture: String,
    filename: String,
    sha256: String,
}

async fn fetch_pkgs(arch: &str, topic: &str) -> anyhow::Result<Vec<Package>> {
    let mut res = vec![];
    let repo = "https://repo.aosc.io";
    let client = reqwest::Client::builder().build()?;
    let url = format!(
        "{}/debs/dists/{}/main/binary-{}/Packages.xz",
        repo, topic, arch
    );
    info!("Fetching {}", url);
    let resp = client.get(url).send().await?;

    if resp.status() == StatusCode::NOT_FOUND {
        return Ok(res);
    }

    // xz decompress
    let bytes = resp.bytes().await?.to_vec();
    let mut cursor = Cursor::new(&bytes);
    let mut decoder = XzDecoder::new(&mut cursor);
    let mut text = String::new();
    decoder.read_to_string(&mut text).unwrap();

    for part in text.split("\n\n") {
        if part.trim().is_empty() {
            continue;
        }

        let mut pkg = Package::default();
        for line in part.split("\n") {
            if let Some(colon) = line.find(":") {
                let key = &line[..colon];
                let value = &line[colon + 1..].trim();
                match key {
                    "Package" => pkg.package = value.to_string(),
                    "Version" => pkg.version = value.to_string(),
                    "Architecture" => pkg.architecture = value.to_string(),
                    "Filename" => pkg.filename = value.to_string(),
                    "SHA256" => pkg.sha256 = value.to_string(),
                    _ => {}
                }
            }
        }
        res.push(pkg);
    }
    Ok(res)
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
                "Skipping already downloaded https://aosc.io/debs/{} at {:?}",
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
            info!(
                "New package {} versioned {}",
                topic_pkg.package, topic_pkg.package
            );
        }
    }
    Ok(res)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opt = Cli::parse();
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
        .map(|arch| tokio::task::spawn(handle_arch(arch, opt.topic.clone())))
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

    for cur in &res {
        println!(
            "{} upgraded from {} to {} on {}:",
            cur.package,
            cur.old_version,
            cur.new_version,
            cur.archs.join(", ")
        );
        println!("{}", cur.diff);
    }
    Ok(())
}
