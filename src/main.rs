mod config;
mod mcp;
mod server;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let loaded = config::LoadedConfig::load(None)?;
    eprintln!("Loaded config from: {}", loaded.source);

    // Run the gateway server
    server::run_server(loaded.config).await
}
