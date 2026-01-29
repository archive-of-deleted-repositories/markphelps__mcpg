mod agents;
mod cli;
mod config;
mod mcp;
mod server;
mod telemetry;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init(args)) => cli::init::run(args, cli.verbose),
        Some(Commands::Serve) | None => {
            // Initialize tracing/telemetry
            telemetry::init();

            // Load configuration (source is logged inside load())
            let config = config::Config::load(None)?;

            // Run the gateway server
            let result = server::run(config).await;

            // Shutdown telemetry (flush OTEL spans)
            telemetry::shutdown();

            result
        }
    }
}
