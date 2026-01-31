use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

use super::http::HttpClient;
use super::protocol::{
    CallToolResult, GetPromptResult, InitializeResult, Prompt, ReadResourceResult, Resource, Tool,
};
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

    /// List all available resources from the server
    async fn list_resources(&mut self) -> Result<Vec<Resource>>;

    /// Read a resource by URI
    async fn read_resource(&mut self, uri: &str) -> Result<ReadResourceResult>;

    /// List all available prompts from the server
    async fn list_prompts(&mut self) -> Result<Vec<Prompt>>;

    /// Get a prompt by name with optional arguments
    async fn get_prompt(
        &mut self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<GetPromptResult>;

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
pub fn create_client(config: &ServerConfig) -> Result<Box<dyn McpClient>> {
    let timeout = Duration::from_secs(config.timeout_secs);

    match &config.transport {
        TransportConfig::Stdio { command, args, env } => {
            let client = StdioClient::new(command.clone(), args.clone(), env.clone(), timeout);
            Ok(Box::new(client))
        }
        TransportConfig::Http { url, headers } => {
            let client = HttpClient::new(url.clone(), headers.clone(), timeout)?;
            Ok(Box::new(client))
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
            timeout_secs: 30,
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
            timeout_secs: 30,
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
            timeout_secs: 30,
        };

        let result = create_client(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_client_http_implemented() {
        let config = ServerConfig {
            transport: TransportConfig::Http {
                url: "http://localhost:8080/mcp".to_string(),
                headers: None,
            },
            timeout_secs: 30,
        };

        let result = create_client(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_client_http_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());

        let config = ServerConfig {
            transport: TransportConfig::Http {
                url: "https://api.example.com/mcp".to_string(),
                headers: Some(headers),
            },
            timeout_secs: 60,
        };

        let result = create_client(&config);
        assert!(result.is_ok());
    }
}
