use anyhow::{Result, bail};
use async_trait::async_trait;

use super::protocol::{CallToolResult, InitializeResult, Tool};
use super::stdio::StdioClient;
use crate::config::{ServerConfig, TransportConfig};

/// Trait defining the MCP client interface
///
/// This trait abstracts over different transport mechanisms (stdio, HTTP)
/// allowing them to be used interchangeably.
#[async_trait]
pub trait McpClient {
    /// Initialize the connection with the MCP server
    ///
    /// This performs the MCP handshake and must be called before other operations.
    /// Returns server information and capabilities.
    async fn initialize(&mut self) -> Result<InitializeResult>;

    /// List all available tools from the server
    ///
    /// Returns a vector of Tool definitions including their schemas.
    async fn list_tools(&mut self) -> Result<Vec<Tool>>;

    /// Call a tool with the given arguments
    ///
    /// # Arguments
    /// * `name` - The name of the tool to call
    /// * `args` - The arguments as a JSON value
    async fn call_tool(&mut self, name: &str, args: serde_json::Value) -> Result<CallToolResult>;

    /// Shutdown the connection gracefully
    async fn shutdown(&mut self) -> Result<()>;
}

/// Create an MCP client based on the server configuration
///
/// # Arguments
/// * `config` - The server configuration containing transport details
///
/// # Returns
/// A boxed trait object implementing McpClient
///
/// # Errors
/// Returns an error if HTTP transport is requested (not yet implemented)
pub fn create_client(config: &ServerConfig) -> Result<Box<dyn McpClient>> {
    match &config.transport {
        TransportConfig::Stdio { command, args, env } => {
            let client = StdioClient::new(command.clone(), args.clone(), env.clone());
            Ok(Box::new(client))
        }
        TransportConfig::Http { .. } => {
            bail!("HTTP transport is not yet implemented")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_create_client_stdio() {
        let config = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "npx".to_string(),
                args: Some(vec!["@modelcontextprotocol/server-everything".to_string()]),
                env: None,
            },
        };

        let result = create_client(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_client_stdio_with_env() {
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "test-key".to_string());

        let config = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "uvx".to_string(),
                args: Some(vec!["mcp-server-fetch".to_string()]),
                env: Some(env),
            },
        };

        let result = create_client(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_client_stdio_minimal() {
        let config = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "node".to_string(),
                args: None,
                env: None,
            },
        };

        let result = create_client(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_client_http_not_implemented() {
        let config = ServerConfig {
            transport: TransportConfig::Http {
                url: "http://localhost:8080".to_string(),
                headers: None,
            },
        };

        let result = create_client(&config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.to_string()
                .contains("HTTP transport is not yet implemented")
        );
    }

    #[test]
    fn test_create_client_http_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());

        let config = ServerConfig {
            transport: TransportConfig::Http {
                url: "https://api.example.com".to_string(),
                headers: Some(headers),
            },
        };

        let result = create_client(&config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.to_string()
                .contains("HTTP transport is not yet implemented")
        );
    }
}
