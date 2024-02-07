use abbs_meta_apml::parse;
use clap::Parser;
use log::{debug, error, warn};
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
                    error!("{}: VER `{}` contains dash(es) `-`", path.display(), ver);
                }
                if ver.contains("_") {
                    error!(
                        "{}: VER `{}` contains underscore(s) `_`",
                        path.display(),
                        ver
                    );
                }
                if ver != &ver.to_lowercase() {
                    error!(
                        "{}: VER `{}` contains upper-cased letters",
                        path.display(),
                        ver
                    );
                }
            } else {
                error!("{}: Missing VER", path.display());
            }

            if let Some(rel) = context.get("REL") {
                match str::parse::<usize>(&rel) {
                    Ok(rel) => {
                        if rel == 0 {
                            error!("{}: REL `{}` should not be zero", path.display(), rel);
                        }
                    }
                    Err(err) => {
                        error!("{}: REL `{}` is invalid: {}", path.display(), rel, err);
                    }
                }
            }

            if let Some(chkupdate) = context.get("CHKUPDATE") {
            } else {
                warn!("{}: Missing CHKUPDATE", path.display());
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
