use clap::Parser;
use dickens::{
    escape_name_for_graphviz,
    sodep::{get_libraries, get_library_deps},
};
use log::{error, warn};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Write,
    path::PathBuf,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Package name
    package: String,

    /// Dependent package names
    depends: Vec<String>,

    /// Dump dependency graph in graphviz format
    #[clap(short, long)]
    graph: Option<PathBuf>,
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
            if let Some(p) = sonames.insert(lib.clone(), pkg) {
                if p != pkg {
                    warn!("{lib} appears in both {p} and {pkg}");
                }
            }
        }
    }

    let target = get_libraries(&opt.package)?;
    for lib in &target {
        if let Some(p) = sonames.insert(lib.clone(), &opt.package) {
            if p != opt.package {
                warn!("{lib} appears in both {p} and {}", opt.package);
            }
        }
    }

    let mut file = if let Some(path) = opt.graph {
        let mut file = File::create(&path)?;
        writeln!(file, "digraph G {{")?;
        for depend in &opt.depends {
            if !builtins.contains(&depend.as_str()) {
                writeln!(
                    file,
                    "  {} [label = \"{}\"];",
                    escape_name_for_graphviz(depend),
                    depend
                )?;
            }
        }
        writeln!(file, "  subgraph cluster_0 {{",)?;
        writeln!(file, "    label = \"{}\";", opt.package)?;
        Some(file)
    } else {
        None
    };

    // find missing
    let mut depended: BTreeSet<&str> = BTreeSet::new();
    let mut per_pkg_depended: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for lib in get_library_deps(&opt.package)? {
        let mut cur_depended: BTreeSet<&str> = BTreeSet::new();
        for needed in lib.needed {
            match sonames.get(&needed) {
                Some(pkg) => {
                    depended.insert(pkg);

                    // skip the package itself for graphviz display
                    if pkg != &opt.package {
                        cur_depended.insert(pkg);
                    }
                }
                None => {
                    error!(
                        "Library/executable {} missing depenedency {}",
                        lib.name, needed
                    );
                }
            }
        }
        per_pkg_depended.insert(lib.name.clone(), cur_depended);
    }

    if let Some(file) = &mut file {
        let mut i = 0;
        for (name, depends) in &per_pkg_depended {
            for depend in depends {
                if !builtins.contains(depend) {
                    writeln!(file, "    file_{} [label=\"{}\"];", i, name)?;
                    break;
                }
            }
            i += 1;
        }
        writeln!(file, "  }}")?;

        i = 0;
        for depends in per_pkg_depended.values() {
            for depend in depends {
                if !builtins.contains(depend) {
                    writeln!(
                        file,
                        "  file_{} -> {};",
                        i,
                        escape_name_for_graphviz(depend)
                    )?;
                }
            }
            i += 1;
        }

        writeln!(file, "}}")?;
    }

    for pkg in &opt.depends {
        if !depended.contains(pkg.as_str()) && !builtins.contains(&pkg.as_str()) {
            warn!("Package {} is not depended by {}", pkg, opt.package);
        }
    }
    Ok(())
}
