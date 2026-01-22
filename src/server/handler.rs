//! Request handlers for gateway tools

use crate::mcp::client::McpClient;
use crate::mcp::protocol::{CallToolResult, Content};
use crate::server::GatewayServer;
use anyhow::Result;
use serde_json::Value;
use tracing::{debug, info};

impl GatewayServer {
    /// Handle the 'search' tool call
    pub async fn handle_search(&mut self, args: Value) -> Result<CallToolResult> {
        // Parse arguments
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: query"))?;

        debug!(query = %query, "Searching for tools");

        // Ensure cache is populated
        if let Err(e) = self.ensure_cache_populated().await {
            return Ok(CallToolResult {
                content: vec![Content::Text {
                    text: format!("Failed to populate tool cache: {}", e),
                }],
                is_error: Some(true),
            });
        }

        // Search the cache
        let results = self.cache.search(query);

        debug!(count = results.len(), "Search completed");

        if results.is_empty() {
            return Ok(CallToolResult {
                content: vec![Content::Text {
                    text: format!("No tools found matching query: {}", query),
                }],
                is_error: Some(false),
            });
        }

        // Format results as "[server] name - description"
        let formatted = results
            .iter()
            .map(|cached| {
                let desc = cached
                    .tool
                    .description
                    .as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default();
                format!("[{}] {}{}", cached.server_name, cached.tool.name, desc)
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(CallToolResult {
            content: vec![Content::Text { text: formatted }],
            is_error: None,
        })
    }

    /// Handle the 'inspect' tool call
    pub async fn handle_inspect(&mut self, args: Value) -> Result<CallToolResult> {
        // Parse arguments
        let server = args
            .get("server")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: server"))?;
        let tool = args
            .get("tool")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: tool"))?;

        debug!(server = %server, tool = %tool, "Inspecting tool");

        // Ensure cache is populated
        if let Err(e) = self.ensure_cache_populated().await {
            return Ok(CallToolResult {
                content: vec![Content::Text {
                    text: format!("Failed to populate tool cache: {}", e),
                }],
                is_error: Some(true),
            });
        }

        // Find the tool
        let cached_tool = match self.cache.find_tool(server, tool) {
            Some(t) => t,
            None => {
                return Ok(CallToolResult {
                    content: vec![Content::Text {
                        text: format!("Tool '{}' not found on server '{}'", tool, server),
                    }],
                    is_error: Some(true),
                });
            }
        };

        // Format tool schema
        let mut output = String::new();
        output.push_str(&format!("Tool: {}\n", cached_tool.tool.name));
        output.push_str(&format!("Server: {}\n\n", cached_tool.server_name));

        if let Some(desc) = &cached_tool.tool.description {
            output.push_str(&format!("Description:\n{}\n\n", desc));
        }

        let required = cached_tool.tool.required_params();
        if !required.is_empty() {
            output.push_str("Required parameters:\n");
            for param in required {
                output.push_str(&format!("  - {}\n", param));
            }
            output.push('\n');
        }

        let optional = cached_tool.tool.optional_params();
        if !optional.is_empty() {
            output.push_str("Optional parameters:\n");
            for param in optional {
                output.push_str(&format!("  - {}\n", param));
            }
            output.push('\n');
        }

        output.push_str(&format!(
            "Example call:\n  call(server=\"{}\", tool=\"{}\", args={{...}})",
            server, tool
        ));

        Ok(CallToolResult {
            content: vec![Content::Text { text: output }],
            is_error: None,
        })
    }

    /// Handle the 'call' tool call
    pub async fn handle_call(&mut self, args: Value) -> Result<CallToolResult> {
        // Parse arguments
        let server = args
            .get("server")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: server"))?;
        let tool = args
            .get("tool")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: tool"))?;
        let tool_args = args
            .get("args")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));

        // Get or create client for the server
        let client = match self.get_or_create_client(server).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(CallToolResult {
                    content: vec![Content::Text {
                        text: format!("Failed to connect to server '{}': {}", server, e),
                    }],
                    is_error: Some(true),
                });
            }
        };

        info!(server = %server, tool = %tool, "Forwarding tool call");

        // Forward the call to upstream server
        match client.call_tool(tool, tool_args).await {
            Ok(result) => {
                debug!(server = %server, tool = %tool, "Tool call succeeded");
                Ok(result)
            }
            Err(e) => {
                debug!(server = %server, tool = %tool, error = %e, "Tool call failed");
                Ok(CallToolResult {
                    content: vec![Content::Text {
                        text: format!(
                            "Error calling tool '{}' on server '{}': {}",
                            tool, server, e
                        ),
                    }],
                    is_error: Some(true),
                })
            }
        }
    }

    /// Handle the 'list' tool call
    pub async fn handle_list(&mut self, args: Value) -> Result<CallToolResult> {
        debug!(server = ?args.get("server"), "Listing tools/servers");

        // Check if server is specified
        if let Some(server) = args.get("server").and_then(|v| v.as_str()) {
            // List tools for specific server
            // Ensure cache is populated
            if let Err(e) = self.ensure_cache_populated().await {
                return Ok(CallToolResult {
                    content: vec![Content::Text {
                        text: format!("Failed to populate tool cache: {}", e),
                    }],
                    is_error: Some(true),
                });
            }

            // Get tools for server
            let tools = self.cache.tools_for_server(server);

            if tools.is_empty() {
                return Ok(CallToolResult {
                    content: vec![Content::Text {
                        text: format!("No tools found for server '{}'", server),
                    }],
                    is_error: Some(false),
                });
            }

            // Format as "[server] name - description"
            let formatted = tools
                .iter()
                .map(|cached| {
                    let desc = cached
                        .tool
                        .description
                        .as_ref()
                        .map(|d| format!(" - {}", d))
                        .unwrap_or_default();
                    format!("[{}] {}{}", cached.server_name, cached.tool.name, desc)
                })
                .collect::<Vec<_>>()
                .join("\n");

            Ok(CallToolResult {
                content: vec![Content::Text { text: formatted }],
                is_error: None,
            })
        } else {
            // List all servers
            let server_names: Vec<String> = self
                .config()
                .servers()
                .map(|(name, _)| name.clone())
                .collect();

            if server_names.is_empty() {
                return Ok(CallToolResult {
                    content: vec![Content::Text {
                        text: "No servers configured".to_string(),
                    }],
                    is_error: Some(false),
                });
            }

            let formatted = format!("Configured servers:\n{}", server_names.join("\n"));

            Ok(CallToolResult {
                content: vec![Content::Text { text: formatted }],
                is_error: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ServerConfig, TransportConfig};
    use std::collections::HashMap;

    fn create_test_server() -> GatewayServer {
        let mut servers = HashMap::new();
        servers.insert(
            "test-server".to_string(),
            ServerConfig {
                transport: TransportConfig::Stdio {
                    command: "echo".to_string(),
                    args: None,
                    env: None,
                },
            },
        );

        let config = Config { servers };
        GatewayServer::new(config)
    }

    #[tokio::test]
    async fn test_handle_search_missing_query() {
        let mut server = create_test_server();
        let args = serde_json::json!({});

        let result = server.handle_search(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_inspect_missing_args() {
        let mut server = create_test_server();

        // Missing server
        let args = serde_json::json!({"tool": "test"});
        let result = server.handle_inspect(args).await;
        assert!(result.is_err());

        // Missing tool
        let args = serde_json::json!({"server": "test"});
        let result = server.handle_inspect(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_call_missing_args() {
        let mut server = create_test_server();

        // Missing server
        let args = serde_json::json!({"tool": "test"});
        let result = server.handle_call(args).await;
        assert!(result.is_err());

        // Missing tool
        let args = serde_json::json!({"server": "test"});
        let result = server.handle_call(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_list_no_servers() {
        let config = Config {
            servers: HashMap::new(),
        };
        let mut server = GatewayServer::new(config);

        let args = serde_json::json!({});
        let result = server.handle_list(args).await.unwrap();

        assert_eq!(result.content.len(), 1);
        if let Content::Text { text } = &result.content[0] {
            assert!(text.contains("No servers configured"));
        } else {
            panic!("Expected text content");
        }
    }

    #[tokio::test]
    async fn test_handle_list_with_servers() {
        let server = create_test_server();
        let mut gateway = server;

        let args = serde_json::json!({});
        let result = gateway.handle_list(args).await.unwrap();

        assert_eq!(result.content.len(), 1);
        if let Content::Text { text } = &result.content[0] {
            assert!(text.contains("Configured servers:"));
            assert!(text.contains("test-server"));
        } else {
            panic!("Expected text content");
        }
    }
}
