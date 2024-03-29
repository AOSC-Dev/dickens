use clap::Parser;
use dickens::escape_name_for_graphviz;
use log::{error, info, warn};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Write,
    path::PathBuf,
    process::Command,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Package names
    packages: Vec<String>,

    /// Dump dependency graph in graphviz format
    #[clap(short, long)]
    graph: PathBuf,
}

#[derive(Debug)]
struct Package {
    name: String,
    depends: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut opt = Cli::parse();

    let mut known: BTreeMap<String, Package> = BTreeMap::new();
    let mut todos = opt.packages.clone();

    while let Some(todo) = todos.pop() {
        if known.contains_key(&todo) {
            continue;
        }
        info!("Handling package {}", todo);

        let output = Command::new("apt").arg("depends").arg(&todo).output()?;
        assert!(output.status.success());

        let mut cur = Package {
            name: todo.clone(),
            depends: vec![],
        };
        for line in String::from_utf8(output.stdout)?.lines() {
            if line.starts_with("  Depends:") {
                let pkg = line.split(" ").skip(3).next().unwrap();
                todos.push(pkg.to_string());
                cur.depends.push(pkg.to_string());
            }
        }

        known.insert(todo, cur);
    }

    let mut file = File::create(opt.graph)?;
    writeln!(file, "digraph G {{")?;
    for (name, pkg) in known {
        writeln!(
            file,
            "  {} [label=\"{}\"];",
            escape_name_for_graphviz(&name),
            name
        )?;
        for depend in pkg.depends {
            writeln!(
                file,
                "  {} -> {};",
                escape_name_for_graphviz(&name),
                escape_name_for_graphviz(&depend)
            )?;
        }
    }
    writeln!(file, "}}")?;

    Ok(())
}
