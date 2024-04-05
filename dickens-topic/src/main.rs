use clap::Parser;
use dickens::topic::report;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Topic name
    topic: String,

    /// Local path to repo if available
    local_repo: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opt = Cli::parse();
    let out = report(&opt.topic, opt.local_repo).await?;
    println!("{}", out);
    Ok(())
}
