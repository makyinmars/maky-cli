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
mod util;

use std::{fs::OpenOptions, io};

use anyhow::Result;
use clap::Parser;
use tracing::debug;
use tracing_subscriber::{EnvFilter, fmt};

use crate::cli::Cli;

fn init_tracing() {
    let _ = std::fs::create_dir_all(".maky");

    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(".maky/maky.log")
    {
        Ok(log_file) => {
            fmt()
                .with_env_filter(default_env_filter())
                .with_target(false)
                .with_ansi(false)
                .with_writer(log_file)
                .init();
        }
        Err(_) => {
            // If file logging is unavailable, drop logs instead of corrupting the TUI.
            fmt()
                .with_env_filter(default_env_filter())
                .with_target(false)
                .with_writer(io::sink)
                .init();
        }
    }
}

fn default_env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    debug!(?cli, "parsed cli arguments");

    app::run(app::StartupOptions {
        resume_session_id: cli.resume,
        force_new_session: cli.new_session,
    })
}
