use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Map of server name → server configuration
    pub servers: HashMap<String, ServerConfig>,
}

/// Configuration for a single MCP server
///
/// Supports two transport types:
/// - stdio: Run a subprocess (requires `command`)
/// - HTTP: Connect to an HTTP endpoint (requires `url`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// The transport-specific configuration (stdio or HTTP)
    #[serde(flatten)]
    pub transport: TransportConfig,
}

/// Transport-specific configuration
///
/// This enum is flattened when serializing/deserializing JSON,
/// so the fields appear directly in the ServerConfig object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransportConfig {
    /// Stdio transport: runs a subprocessc
    Stdio {
        /// The executable to run (e.g., "uvx", "npx")
        command: String,
        /// Arguments to pass to the command
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Vec<String>>,
        /// Environment variables to set
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    /// HTTP transport: connects to an HTTP endpoint
    Http {
        /// The HTTP endpoint URL
        url: String,
        /// HTTP headers to send
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
}

impl ServerConfig {
    pub fn validate(&self) -> Result<ServerConfig> {
        match &self.transport {
            TransportConfig::Stdio { command, .. } => {
                if command.trim().is_empty() {
                    bail!("Command for stdio transport cannot be empty");
                }
            }
            TransportConfig::Http { url, .. } => {
                if url.trim().is_empty() {
                    bail!("URL for HTTP transport cannot be empty");
                }
            }
        }
        Ok(self.clone())
    }
}

impl Config {
    pub fn get_server(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.get(name)
    }

    pub fn servers(&self) -> impl Iterator<Item = (&String, &ServerConfig)> {
        self.servers.iter()
    }
}
