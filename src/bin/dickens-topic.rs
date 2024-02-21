use abbs_meta_apml::parse;
use clap::Parser;
use log::{debug, error, warn};
use reqwest::StatusCode;
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Cli::parse();
    let repo = "https://repo.aosc.io";
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
        let resp = reqwest::get(format!(
            "{}/debs/dists/{}/main/binary-{}/Packages",
            repo, opt.topic, arch
        ))
        .await?;
        if resp.status() == StatusCode::NOT_FOUND {
            continue;
        }

        let text = resp.text().await?;
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
            println!("Pkg: {:?}", pkg);
        }
    }
    Ok(())
}
