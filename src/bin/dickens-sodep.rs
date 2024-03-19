use clap::Parser;
use dickens::sodep::{get_libraries, get_library_deps};
use log::{error, warn};
use std::collections::{BTreeMap, BTreeSet};

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

    // always assume these packages are available
    let builtins = ["glibc", "gcc-runtime", "libxcrypt"];
    for builtin in builtins {
        if !opt.depends.contains(&builtin.to_string()) {
            opt.depends.push(builtin.to_string());
        }
    }

    // map soname => package
    let mut sonames: BTreeMap<String, &str> = BTreeMap::new();
    for pkg in &opt.depends {
        for lib in get_libraries(pkg)? {
            assert_eq!(sonames.insert(lib, pkg), None);
        }
    }

    let target = get_libraries(&opt.package)?;
    for lib in &target {
        assert_eq!(sonames.insert(lib.clone(), &opt.package), None);
    }

    // find missing
    let mut depended: BTreeSet<&str> = BTreeSet::new();
    for lib in get_library_deps(&opt.package)? {
        for needed in lib.needed {
            match sonames.get(&needed) {
                Some(pkg) => {
                    depended.insert(pkg);
                }
                None => {
                    error!(
                        "Library/executable {} missing depenedency {}",
                        lib.name, needed
                    );
                }
            }
        }
    }

    for pkg in &opt.depends {
        if !depended.contains(pkg.as_str()) {
            if !builtins.contains(&pkg.as_str()) {
                warn!("Package {} is not depended by {}", pkg, opt.package);
            }
        }
    }
    Ok(())
}
