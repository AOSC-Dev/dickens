use abbs_meta_apml::parse;
use clap::Parser;
use log::{debug, error};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to ABBS tree
    #[arg(short, long)]
    abbs_tree: Option<PathBuf>,
}

fn check_abbs_spec(path: &Path) -> anyhow::Result<()> {
    debug!("Linting {}", path.display());

    let mut f = File::open(path)?;
    let mut content = String::new();
    f.read_to_string(&mut content)?;
    let mut context = HashMap::new();
    match parse(&content, &mut context) {
        Ok(_) => {
            if let Some(ver) = context.get("VER") {
                if ver.contains("-") {
                    error!("{}: VER `{}` contains dash `-`", path.display(), ver);
                }
                if ver.contains("_") {
                    error!("{}: VER `{}` contains underscore `_`", path.display(), ver);
                }
            } else {
                error!("{}: Missing VER", path.display());
            }
        }
        Err(vec_err) => {
            for err in vec_err {
                error!("{}: Got error {} when parsing", path.display(), err);
            }
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Cli::parse();

    let walker = walkdir::WalkDir::new(args.abbs_tree.unwrap_or(PathBuf::from("."))).max_depth(4);
    for entry in walker.into_iter() {
        let file = entry?;
        if file.file_name() == "spec" {
            check_abbs_spec(file.path())?;
        }
    }

    Ok(())
}
