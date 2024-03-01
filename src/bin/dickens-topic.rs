use clap::Parser;
use dickens::topic::report;
use sha2::{Digest};


#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Topic name
    topic: String,
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opt = Cli::parse();
    let out = report(&opt.topic).await?;
    println!("{}", out);
    Ok(())
}
