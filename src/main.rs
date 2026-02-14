use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "maky",
    bin_name = "maky",
    version,
    about = "A lightweight agent CLI built while learning Rust",
    long_about = None
)]
struct Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let _cli = Cli::parse();
    Ok(())
}
