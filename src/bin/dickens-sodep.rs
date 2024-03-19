use std::collections::{BTreeMap, BTreeSet};

use clap::Parser;
use dickens::sodep::get_library_and_deps;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Package name
    package: String,

    /// Dependent package names
    depends: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut opt = Cli::parse();

    // always assume gcc
    opt.depends.push("glibc".to_string());

    // map soname => package
    let mut sonames: BTreeMap<String, &str> = BTreeMap::new();
    for pkg in &opt.depends {
        for lib in get_library_and_deps(pkg)? {
            assert_eq!(sonames.insert(lib.soname, pkg), None);
        }
    }

    let target = get_library_and_deps(&opt.package)?;
    for lib in &target {
        assert_eq!(sonames.insert(lib.soname.clone(), &opt.package), None);
    }

    // find missing
    let mut depended: BTreeSet<&str> = BTreeSet::new();
    for lib in target {
        for needed in lib.needed {
            match sonames.get(&needed) {
                Some(pkg) => {
                    depended.insert(pkg);
                }
                None => {
                    error!("Library {} missing depenedency {}", lib.soname, needed);
                }
            }
        }
    }

    for pkg in &opt.depends {
        if !depended.contains(pkg.as_str()) {
            warn!("Package {} is not depended by {}", pkg, opt.package);
        }
    }
    Ok(())
}
