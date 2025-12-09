mod batch;
mod blockchain;
mod cache;
mod cli;
mod commands;
mod contracts;
mod display;
mod error;
mod events;
mod export;
mod hooks;
mod price;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments first to get verbose flag
    let cli = cli::Cli::parse();

    // Initialize tracing with appropriate log level
    let default_level = if cli.verbose {
        "beeport_stamp_stats=debug"
    } else {
        "beeport_stamp_stats=info"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_level.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Execute the command
    cli.execute().await
}
