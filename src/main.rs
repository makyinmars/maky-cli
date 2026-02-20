mod app;

use anyhow::Result;
use clap::Parser;
use tracing::debug;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Debug, Parser)]
#[command(
    name = "maky",
    bin_name = "maky",
    version,
    about = "A lightweight agent CLI built while learning Rust",
    long_about = None
)]
struct Cli;

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt().with_env_filter(env_filter).with_target(false).init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    debug!(?cli, "parsed cli arguments");

    app::run()
}
