use clap::Parser;
use log::info;
use reqwest::StatusCode;
use std::io::{Cursor, Read};
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
                    _ => {}
                }
            }
        }
        res.push(pkg);
    }
    Ok(res)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opt = Cli::parse();
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
    for arch in archs {
        let topic_pkgs = fetch_pkgs(arch, &opt.topic).await?;
        let stable_pkgs = fetch_pkgs(arch, "stable").await?;
        for topic_pkg in topic_pkgs {
            if let Some(found) = stable_pkgs.iter().find(|p| p.package == topic_pkg.package) {
                println!(
                    "Upgrade {} from {} to {}",
                    topic_pkg.package, found.version, topic_pkg.version
                );
            } else {
                println!("New {} {}", topic_pkg.package, topic_pkg.package);
            }
        }
    }
    Ok(())
}
