//! Stdio transport for the gateway server

use crate::mcp::protocol::{
    CallToolResult, Content, InitializeParams, InitializeResult, JsonRpcError, JsonRpcRequest,
    JsonRpcResponse, ListToolsResult, PROTOCOL_VERSION, RequestId, ServerCapabilities, ServerInfo,
    ToolsCapability,
};
use crate::server::GatewayServer;
use crate::server::tools::gateway_tools;
use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{Instrument, error, info_span};

impl GatewayServer {
    /// Handle a single JSON-RPC request
    pub async fn handle_request(&mut self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id, request.params).await,
            "notifications/initialized" => {
                // This is a notification, no response needed
                // But we return an empty response to satisfy the return type
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: Some(Value::Null),
                    error: None,
                })
            }
            "tools/list" => self.handle_tools_list(request.id).await,
            "tools/call" => self.handle_tools_call(request.id, request.params).await,
            method => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", method),
                    data: None,
                }),
            }),
        }
    }

    async fn handle_initialize(
        &mut self,
        id: RequestId,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse> {
        // Parse initialize params (we don't really need them but validate they exist)
        let _params: InitializeParams = match params {
            Some(p) => serde_json::from_value(p).context("Invalid initialize params")?,
            None => anyhow::bail!("Missing initialize params"),
        };

        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            server_info: ServerInfo {
                name: "mcpg".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
            },
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }

    async fn handle_tools_list(&mut self, id: RequestId) -> Result<JsonRpcResponse> {
        let result = ListToolsResult {
            tools: gateway_tools(),
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }

    async fn handle_tools_call(
        &mut self,
        id: RequestId,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse> {
        // Parse call params
        let params = match params {
            Some(p) => p,
            None => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Missing params for tools/call".to_string(),
                        data: None,
                    }),
                });
            }
        };

        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(name) => name,
            None => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Missing 'name' in tools/call params".to_string(),
                        data: None,
                    }),
                });
            }
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));

        // Route to appropriate handler
        let result: CallToolResult = match tool_name {
            "search" => self.handle_search(arguments).await?,
            "inspect" => self.handle_inspect(arguments).await?,
            "call" => self.handle_call(arguments).await?,
            "list" => self.handle_list(arguments).await?,
            _ => CallToolResult {
                content: vec![Content::Text {
                    text: format!("Unknown gateway tool: {}", tool_name),
                }],
                is_error: Some(true),
            },
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }
}

/// Run the gateway server on stdin/stdout
pub async fn run_server(config: crate::config::Config) -> Result<()> {
    let mut server = GatewayServer::new(config);

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .await
            .context("Failed to read from stdin")?;

        if n == 0 {
            // EOF
            break;
        }

        let line_trimmed = line.trim();
        if line_trimmed.is_empty() {
            continue;
        }

        // Parse JSON-RPC request
        let request: JsonRpcRequest = match serde_json::from_str(line_trimmed) {
            Ok(req) => req,
            Err(e) => {
                error!(error = %e, "Failed to parse request");
                continue;
            }
        };

        // Check if this is a notification (no response needed)
        let is_notification = request.method == "notifications/initialized";

        // Create a span for this request
        let span = info_span!("request", id = ?request.id, method = %request.method);

        // Handle request
        let response = match server.handle_request(request).instrument(span).await {
            Ok(resp) => resp,
            Err(e) => {
                error!(error = %e, "Error handling request");
                continue;
            }
        };

        // Don't send response for notifications
        if is_notification {
            continue;
        }

        // Write response
        let response_json = serde_json::to_string(&response)?;
        stdout
            .write_all(response_json.as_bytes())
            .await
            .context("Failed to write to stdout")?;
        stdout
            .write_all(b"\n")
            .await
            .context("Failed to write newline")?;
        stdout.flush().await.context("Failed to flush stdout")?;
    }

    // Gracefully shutdown all upstream connections
    server.shutdown().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::collections::HashMap;

    fn create_test_server() -> GatewayServer {
        let config = Config {
            servers: HashMap::new(),
        };
        GatewayServer::new(config)
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            1,
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2025-06-18",
                "clientInfo": {"name": "test", "version": "1.0"},
                "capabilities": {}
            })),
        );

        let response = server.handle_request(request).await.unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());

        let result: InitializeResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.server_info.name, "mcpg");
        assert_eq!(result.protocol_version, PROTOCOL_VERSION);
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(2, "tools/list", None);

        let response = server.handle_request(request).await.unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());

        let result: ListToolsResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.tools.len(), 4);
    }

    #[tokio::test]
    async fn test_handle_unknown_method() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(3, "unknown/method", None);

        let response = server.handle_request(request).await.unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, -32601);
        assert!(error.message.contains("Method not found"));
    }

    #[tokio::test]
    async fn test_handle_tools_call_missing_params() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(4, "tools/call", None);

        let response = server.handle_request(request).await.unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, -32602);
    }

    #[tokio::test]
    async fn test_handle_tools_call_missing_name() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(5, "tools/call", Some(serde_json::json!({})));

        let response = server.handle_request(request).await.unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, -32602);
        assert!(error.message.contains("name"));
    }

    #[tokio::test]
    async fn test_handle_tools_call_unknown_tool() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            6,
            "tools/call",
            Some(serde_json::json!({"name": "unknown_tool"})),
        );

        let response = server.handle_request(request).await.unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());

        let result: CallToolResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn test_handle_tools_call_list() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            7,
            "tools/call",
            Some(serde_json::json!({"name": "list", "arguments": {}})),
        );

        let response = server.handle_request(request).await.unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());

        let result: CallToolResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.is_error, Some(false)); // No servers configured, but not an error
    }
}
