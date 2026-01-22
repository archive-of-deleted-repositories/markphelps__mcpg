mod config;
mod mcp;
mod server;
mod telemetry;

use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing/telemetry
    telemetry::init();

    // Load configuration
    let loaded = config::LoadedConfig::load(None)?;
    info!(source = %loaded.source, "Loaded configuration");

    // Run the gateway server
    let result = server::run_server(loaded.config).await;

    // Shutdown telemetry (flush OTEL spans)
    telemetry::shutdown();

    result
}
