mod config;
mod mcp;
mod server;
mod telemetry;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
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
