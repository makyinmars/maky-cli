#[allow(dead_code)]
mod agent;
mod app;
#[allow(dead_code)]
mod auth;
mod cli;
#[allow(dead_code)]
mod model;
#[allow(dead_code)]
mod providers;
#[allow(dead_code)]
mod storage;
#[allow(dead_code)]
mod tools;

use anyhow::Result;
use clap::Parser;
use tracing::debug;
use tracing_subscriber::{EnvFilter, fmt};

use crate::cli::Cli;

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
