use anyhow::Result;
use async_trait::async_trait;

use super::protocol::{CallToolResult, InitializeResult, Tool};

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

// TODO: Add factory function to create clients based on ServerConfig
//
// pub fn create_client(config: &ServerConfig) -> Result<Box<dyn McpClient>> {
//     match &config.transport {
//         TransportConfig::Stdio { command, args, env } => {
//             // Create StdioClient
//             todo!("Implement stdio client creation")
//         }
//         TransportConfig::Http { url, headers } => {
//             // Create HttpClient
//             todo!("Implement HTTP client creation")
//         }
//     }
// }
