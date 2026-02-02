//! Stdio transport for the gateway server

use crate::mcp::protocol::{
    CallToolResult, Content, InitializeParams, InitializeResult, JSONRPC_INVALID_PARAMS,
    JSONRPC_METHOD_NOT_FOUND, JSONRPC_NOT_FOUND, JsonRpcError, JsonRpcRequest, JsonRpcResponse,
    ListPromptsResult, ListResourcesResult, ListToolsResult, PROTOCOL_VERSION, PromptsCapability,
    RequestId, ResourcesCapability, ServerCapabilities, ServerInfo, ToolsCapability,
};
use crate::server::GatewayServer;
use crate::server::tools::gateway_tools;
use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{Instrument, error, info_span};

/// Instructions sent to the AI agent explaining how to use the mcpg gateway.
const GATEWAY_INSTRUCTIONS: &str = "\
mcpg is a gateway to upstream MCP servers. Instead of connecting to each server directly, \
use these tools to discover and call tools on upstream servers:

1. search(query) - Find tools by keyword across all servers. Start here when you need a tool.
2. inspect(server, tool) - Get the full schema and parameters for a specific tool before calling it.
3. call(server, tool, args) - Execute a tool on an upstream server.
4. list(server?) - List all available servers, or list tools on a specific server.

Workflow: search -> inspect -> call. Always inspect a tool before calling it to understand its parameters.";

impl GatewayServer {
    /// Handle a single JSON-RPC request
    ///
    /// Returns `None` for notifications (requests without an ID), `Some(response)` for requests.
    pub async fn handle_request(
        &mut self,
        request: JsonRpcRequest,
    ) -> Result<Option<JsonRpcResponse>> {
        // If there's no ID, this is a notification - no response should be sent
        let id = match request.id {
            Some(id) => id,
            None => return Ok(None),
        };

        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(id, request.params).await?,
            "tools/list" => self.handle_tools_list(id, request.params).await?,
            "tools/call" => self.handle_tools_call(id, request.params).await?,
            "resources/list" => self.handle_resources_list(id, request.params).await?,
            "resources/read" => self.handle_resources_read(id, request.params).await?,
            "prompts/list" => self.handle_prompts_list(id, request.params).await?,
            "prompts/get" => self.handle_prompts_get(id, request.params).await?,
            method => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: JSONRPC_METHOD_NOT_FOUND,
                    message: format!("Method not found: {}", method),
                    data: None,
                }),
            },
        };

        Ok(Some(response))
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
                name: crate::config::APP_NAME.to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: Some(ResourcesCapability {
                    subscribe: None,
                    list_changed: Some(false),
                }),
                prompts: Some(PromptsCapability {
                    list_changed: Some(false),
                }),
            },
            instructions: Some(GATEWAY_INSTRUCTIONS.to_string()),
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }

    async fn handle_tools_list(
        &mut self,
        id: RequestId,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse> {
        // MCP pagination: since mcpg never emits cursors for tools/list,
        // any non-null cursor is invalid (error -32602 per spec)
        if let Some(ref p) = params {
            if let Some(cursor) = p.get("cursor") {
                if !cursor.is_null() {
                    return Ok(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: JSONRPC_INVALID_PARAMS,
                            message: "Invalid params: unknown cursor".to_string(),
                            data: None,
                        }),
                    });
                }
            }
        }

        let result = ListToolsResult {
            tools: gateway_tools(),
            next_cursor: None,
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
                        code: JSONRPC_INVALID_PARAMS,
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
                        code: JSONRPC_INVALID_PARAMS,
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

    async fn handle_resources_list(
        &mut self,
        id: RequestId,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse> {
        // Cursor validation — same pattern as tools/list
        if let Some(ref p) = params {
            if let Some(cursor) = p.get("cursor") {
                if !cursor.is_null() {
                    return Ok(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: JSONRPC_INVALID_PARAMS,
                            message: "Invalid params: unknown cursor".to_string(),
                            data: None,
                        }),
                    });
                }
            }
        }

        // Ensure cache is populated (connects to upstream servers)
        self.ensure_cache_populated().await?;

        let result = ListResourcesResult {
            resources: self.cached_resources().to_vec(),
            next_cursor: None,
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }

    async fn handle_resources_read(
        &mut self,
        id: RequestId,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse> {
        let params = match params {
            Some(p) => p,
            None => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: JSONRPC_INVALID_PARAMS,
                        message: "Missing params for resources/read".to_string(),
                        data: None,
                    }),
                });
            }
        };

        let uri = match params.get("uri").and_then(|v| v.as_str()) {
            Some(uri) => uri.to_string(),
            None => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: JSONRPC_INVALID_PARAMS,
                        message: "Missing 'uri' in resources/read params".to_string(),
                        data: None,
                    }),
                });
            }
        };

        // Ensure cache is populated so resource_index is available
        self.ensure_cache_populated().await?;

        let server_name = match self.resource_server(&uri) {
            Some(name) => name.to_string(),
            None => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: JSONRPC_NOT_FOUND,
                        message: format!("Resource not found: {}", uri),
                        data: None,
                    }),
                });
            }
        };

        // Forward to upstream
        let client = self.get_or_create_client(&server_name).await?;
        let result = client.read_resource(&uri).await?;

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }

    async fn handle_prompts_list(
        &mut self,
        id: RequestId,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse> {
        if let Some(ref p) = params {
            if let Some(cursor) = p.get("cursor") {
                if !cursor.is_null() {
                    return Ok(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: JSONRPC_INVALID_PARAMS,
                            message: "Invalid params: unknown cursor".to_string(),
                            data: None,
                        }),
                    });
                }
            }
        }

        self.ensure_cache_populated().await?;

        let result = ListPromptsResult {
            prompts: self.cached_prompts().to_vec(),
            next_cursor: None,
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }

    async fn handle_prompts_get(
        &mut self,
        id: RequestId,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse> {
        let params = match params {
            Some(p) => p,
            None => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: JSONRPC_INVALID_PARAMS,
                        message: "Missing params for prompts/get".to_string(),
                        data: None,
                    }),
                });
            }
        };

        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(name) => name.to_string(),
            None => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: JSONRPC_INVALID_PARAMS,
                        message: "Missing 'name' in prompts/get params".to_string(),
                        data: None,
                    }),
                });
            }
        };

        let arguments = params.get("arguments").cloned();

        self.ensure_cache_populated().await?;

        let server_name = match self.prompt_server(&name) {
            Some(name) => name.to_string(),
            None => {
                return Ok(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: JSONRPC_NOT_FOUND,
                        message: format!("Prompt not found: {}", name),
                        data: None,
                    }),
                });
            }
        };

        let client = self.get_or_create_client(&server_name).await?;
        let result = client.get_prompt(&name, arguments).await?;

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        })
    }
}

/// Run the gateway server on stdin/stdout
pub async fn run(config: crate::config::Config) -> Result<()> {
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

        // Don't send response for notifications (when handle_request returns None)
        let response = match response {
            Some(resp) => resp,
            None => continue,
        };

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

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());

        let result: InitializeResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.server_info.name, crate::config::APP_NAME);
        assert_eq!(result.protocol_version, PROTOCOL_VERSION);
        assert!(result.capabilities.resources.is_some());
        assert!(result.capabilities.prompts.is_some());
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(2, "tools/list", None);

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());

        let result: ListToolsResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.tools.len(), 4);
        assert!(result.next_cursor.is_none());
    }

    #[tokio::test]
    async fn test_handle_tools_list_empty_params() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(2, "tools/list", Some(serde_json::json!({})));

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_none());

        let result: ListToolsResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.tools.len(), 4);
        assert!(result.next_cursor.is_none());
    }

    #[tokio::test]
    async fn test_handle_tools_list_with_cursor_returns_error() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            2,
            "tools/list",
            Some(serde_json::json!({"cursor": "some-cursor"})),
        );

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, JSONRPC_INVALID_PARAMS);
        assert!(error.message.contains("cursor"));
    }

    #[tokio::test]
    async fn test_handle_tools_list_with_null_cursor_succeeds() {
        let mut server = create_test_server();
        let request =
            JsonRpcRequest::new(2, "tools/list", Some(serde_json::json!({"cursor": null})));

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_none());

        let result: ListToolsResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.tools.len(), 4);
    }

    #[tokio::test]
    async fn test_handle_unknown_method() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(3, "unknown/method", None);

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, JSONRPC_METHOD_NOT_FOUND);
        assert!(error.message.contains("Method not found"));
    }

    #[tokio::test]
    async fn test_handle_tools_call_missing_params() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(4, "tools/call", None);

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, JSONRPC_INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_tools_call_missing_name() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(5, "tools/call", Some(serde_json::json!({})));

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, JSONRPC_INVALID_PARAMS);
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

        let response = server.handle_request(request).await.unwrap().unwrap();
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

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());

        let result: CallToolResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(result.is_error, Some(false)); // No servers configured, but not an error
    }

    #[tokio::test]
    async fn test_handle_notification() {
        let mut server = create_test_server();
        // Create a notification (request without an ID)
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "notifications/initialized".to_string(),
            params: None,
        };

        let response = server.handle_request(request).await.unwrap();
        // Notifications should not return a response
        assert!(response.is_none());
    }

    #[tokio::test]
    async fn test_handle_resources_list_empty() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(10, "resources/list", None);

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_none());

        let result: ListResourcesResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert!(result.resources.is_empty());
        assert!(result.next_cursor.is_none());
    }

    #[tokio::test]
    async fn test_handle_resources_list_with_cursor_returns_error() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            10,
            "resources/list",
            Some(serde_json::json!({"cursor": "some-cursor"})),
        );

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, JSONRPC_INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_resources_list_with_null_cursor_succeeds() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            10,
            "resources/list",
            Some(serde_json::json!({"cursor": null})),
        );

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_handle_resources_read_missing_params() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(11, "resources/read", None);

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, JSONRPC_INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_resources_read_missing_uri() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(11, "resources/read", Some(serde_json::json!({})));

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, JSONRPC_INVALID_PARAMS);
        assert!(error.message.contains("uri"));
    }

    #[tokio::test]
    async fn test_handle_resources_read_unknown_uri() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            11,
            "resources/read",
            Some(serde_json::json!({"uri": "file:///nonexistent.txt"})),
        );

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, JSONRPC_NOT_FOUND);
        assert!(error.message.contains("not found"));
    }

    #[tokio::test]
    async fn test_handle_prompts_list_empty() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(20, "prompts/list", None);

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_none());

        let result: ListPromptsResult = serde_json::from_value(response.result.unwrap()).unwrap();
        assert!(result.prompts.is_empty());
        assert!(result.next_cursor.is_none());
    }

    #[tokio::test]
    async fn test_handle_prompts_list_with_cursor_returns_error() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            20,
            "prompts/list",
            Some(serde_json::json!({"cursor": "some-cursor"})),
        );

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, JSONRPC_INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_prompts_list_with_null_cursor_succeeds() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            20,
            "prompts/list",
            Some(serde_json::json!({"cursor": null})),
        );

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_handle_prompts_get_missing_params() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(21, "prompts/get", None);

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, JSONRPC_INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_prompts_get_missing_name() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(21, "prompts/get", Some(serde_json::json!({})));

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, JSONRPC_INVALID_PARAMS);
        assert!(error.message.contains("name"));
    }

    #[tokio::test]
    async fn test_handle_prompts_get_unknown_name() {
        let mut server = create_test_server();
        let request = JsonRpcRequest::new(
            21,
            "prompts/get",
            Some(serde_json::json!({"name": "nonexistent"})),
        );

        let response = server.handle_request(request).await.unwrap().unwrap();
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, JSONRPC_NOT_FOUND);
        assert!(error.message.contains("not found"));
    }
}
