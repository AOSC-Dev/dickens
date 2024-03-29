use clap::Parser;
use dickens::escape_name_for_graphviz;
use graph_cycles::Cycles;
use log::{error, info, warn};
use petgraph::Graph;
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

    // loop detection
    let packages: Vec<&String> = known.keys().collect();
    let mut pkg_to_index: BTreeMap<&String, usize> = BTreeMap::new();
    for (index, pkg) in packages.iter().enumerate() {
        pkg_to_index.insert(*pkg, index);
    }
    let mut edges = vec![];
    for (name, pkg) in &known {
        for depend in &pkg.depends {
            edges.push((pkg_to_index[name] as u32, pkg_to_index[&depend] as u32));
        }
    }
    let deps = Graph::<(), ()>::from_edges(&edges);
    deps.visit_all_cycles(|_g, c| {
        let mut edges: Vec<&str> = c.iter().map(|idx| packages[idx.index()].as_str()).collect();
        edges.push(edges[0]);
        println!("Found dependency cycle: {}", edges.join(" -> "));
    });

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
