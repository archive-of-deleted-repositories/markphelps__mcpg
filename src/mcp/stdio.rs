use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tracing::{Instrument, debug, debug_span, info, info_span};
use uuid::Uuid;

use super::client::McpClient;
use super::protocol::{
    CallToolParams, CallToolResult, ClientCapabilities, ClientInfo, GetPromptResult,
    InitializeParams, InitializeResult, JsonRpcRequest, JsonRpcResponse, ListPromptsResult,
    ListResourcesResult, ListToolsResult, MAX_PAGINATION_PAGES, McpMethod, McpNotification,
    PROTOCOL_VERSION, Prompt, ReadResourceResult, Resource, Tool,
};

/// MCP client that communicates via stdio (stdin/stdout)
///
/// This spawns a subprocess and communicates using newline-delimited JSON
pub struct StdioClient {
    command: String,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,

    // Process handles (None until spawned)
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,

    // Cached initialization result (also indicates if initialized)
    init_result: Option<InitializeResult>,

    // Timeout for read operations
    timeout: Duration,
}

impl StdioClient {
    /// Create a new stdio client (doesn't spawn the process yet)
    pub fn new(
        command: String,
        args: Option<Vec<String>>,
        env: Option<HashMap<String, String>>,
        timeout: Duration,
    ) -> Self {
        Self {
            command,
            args,
            env,
            child: None,
            stdin: None,
            stdout: None,
            init_result: None,
            timeout,
        }
    }

    /// Spawn the subprocess and capture stdin/stdout
    ///
    /// Returns an error if the client has already been spawned.
    fn spawn(&mut self) -> Result<()> {
        if self.child.is_some() {
            anyhow::bail!("Client already spawned - call shutdown() before respawning");
        }

        debug!(command = %self.command, "Spawning upstream process");

        let cmd = tokio::process::Command::new(&self.command)
            .args(self.args.clone().unwrap_or_default())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .envs(self.env.clone().unwrap_or_default())
            .spawn()?;

        self.child = Some(cmd);
        self.stdin = self.child.as_mut().and_then(|c| c.stdin.take());
        self.stdout = self
            .child
            .as_mut()
            .and_then(|c| c.stdout.take())
            .map(BufReader::new);
        Ok(())
    }

    /// Send a JSON-RPC request to the server
    ///
    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<String> {
        let request_id = Uuid::new_v4().to_string();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(request_id.clone().into()),
            method: method.to_string(),
            params,
        };

        let json = serde_json::to_string(&request)? + "\n";
        if let Some(stdin) = &mut self.stdin {
            stdin.write_all(json.as_bytes()).await?;
            stdin.flush().await?;
            return Ok(request_id);
        }

        Err(anyhow::anyhow!("Stdin not available"))
    }

    /// Send a JSON-RPC notification to the server (no response expected)
    ///
    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let json = serde_json::to_string(&notification)? + "\n";
        if let Some(stdin) = &mut self.stdin {
            stdin.write_all(json.as_bytes()).await?;
            stdin.flush().await?;
            return Ok(());
        }

        Err(anyhow::anyhow!("Stdin not available"))
    }

    /// Read a JSON-RPC response from the server with timeout
    async fn read_response(&mut self) -> Result<JsonRpcResponse> {
        let mut line = String::new();
        if let Some(stdout) = &mut self.stdout {
            tokio::time::timeout(self.timeout, stdout.read_line(&mut line))
                .await
                .context("Timeout reading response from upstream server")?
                .context("Failed to read response from upstream server")?;

            let response: JsonRpcResponse =
                serde_json::from_str(&line).context("Failed to parse JSON-RPC response")?;

            if response.error.is_some() {
                return Err(anyhow::anyhow!(
                    "Error response from server: {:?}",
                    response.error
                ));
            }

            return Ok(response);
        }

        Err(anyhow::anyhow!("Stdout not available"))
    }
}

#[async_trait]
impl McpClient for StdioClient {
    async fn initialize(&mut self) -> Result<InitializeResult> {
        async move {
            if let Some(ref result) = self.init_result {
                return Ok(result.clone());
            }

            // Not initialized yet, so initialize...
            if self.child.is_none() {
                self.spawn()?;
            }

            let params = serde_json::to_value(InitializeParams {
                protocol_version: PROTOCOL_VERSION.to_string(),
                client_info: ClientInfo {
                    name: crate::config::APP_NAME.to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                capabilities: ClientCapabilities::default(),
            })?;

            self.send_request(McpMethod::Initialize.as_str(), Some(params))
                .await?;
            let response = self.read_response().await?;
            let init_result: InitializeResult = serde_json::from_value(
                response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("No result in initialize response"))?,
            )?;

            // Protocol version negotiation is not yet implemented.
            self.init_result = Some(init_result.clone());

            // Send notifications/initialized to signal that initialization is complete
            // This is a notification (no ID) not a request
            self.send_notification(McpNotification::Initialized.as_str(), None)
                .await?;

            Ok(init_result)
        }
        .instrument(info_span!("mcp_initialize"))
        .await
    }

    async fn list_tools(&mut self) -> Result<Vec<Tool>> {
        async move {
            if self.init_result.is_none() {
                self.initialize().await?;
            }

            let mut all_tools = Vec::new();
            let mut cursor: Option<String> = None;

            for page in 0..MAX_PAGINATION_PAGES {
                let params = cursor.as_ref().map(|c| serde_json::json!({ "cursor": c }));

                self.send_request(McpMethod::ToolsList.as_str(), params)
                    .await?;
                let response = self.read_response().await?;
                let result: ListToolsResult = serde_json::from_value(
                    response
                        .result
                        .ok_or_else(|| anyhow::anyhow!("No result in tools/list response"))?,
                )?;

                debug!(
                    page = page,
                    count = result.tools.len(),
                    has_next = result.next_cursor.is_some(),
                    "Received tools page from upstream"
                );

                if result.tools.is_empty() && result.next_cursor.is_some() {
                    anyhow::bail!(
                        "Upstream server returned empty page with cursor — possible infinite pagination loop"
                    );
                }

                all_tools.extend(result.tools);

                match result.next_cursor {
                    Some(next) => cursor = Some(next),
                    None => {
                        debug!(count = all_tools.len(), "Listed tools from upstream");
                        return Ok(all_tools);
                    }
                }
            }

            anyhow::bail!(
                "Exceeded maximum pagination limit of {} pages while listing tools",
                MAX_PAGINATION_PAGES
            )
        }
        .instrument(debug_span!("upstream_list_tools"))
        .await
    }

    async fn call_tool(&mut self, name: &str, args: serde_json::Value) -> Result<CallToolResult> {
        let tool_name = name.to_string();
        let span = debug_span!("upstream_call", tool = %tool_name);

        async move {
            // Ensure initialized
            if self.init_result.is_none() {
                self.initialize().await?;
            }

            info!(tool = %tool_name, "Forwarding tool call to upstream");

            // Create CallToolParams with the tool name and arguments
            let tool_params = CallToolParams {
                name: tool_name,
                arguments: Some(args),
            };

            // Serialize to JSON Value for the request
            let params = serde_json::to_value(tool_params)?;

            // Send the tools/call request
            self.send_request(McpMethod::ToolsCall.as_str(), Some(params))
                .await?;

            // Read the response
            let response = self.read_response().await?;

            // Parse CallToolResult from the result field
            let call_result: CallToolResult = serde_json::from_value(
                response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("No result in tools/call response"))?,
            )?;

            Ok(call_result)
        }
        .instrument(span)
        .await
    }

    async fn list_resources(&mut self) -> Result<Vec<Resource>> {
        async move {
            if self.init_result.is_none() {
                self.initialize().await?;
            }

            let mut all_resources = Vec::new();
            let mut cursor: Option<String> = None;

            for page in 0..MAX_PAGINATION_PAGES {
                let params = cursor.as_ref().map(|c| serde_json::json!({ "cursor": c }));

                self.send_request(McpMethod::ResourcesList.as_str(), params)
                    .await?;
                let response = self.read_response().await?;
                let result: ListResourcesResult = serde_json::from_value(
                    response
                        .result
                        .ok_or_else(|| anyhow::anyhow!("No result in resources/list response"))?,
                )?;

                debug!(
                    page = page,
                    count = result.resources.len(),
                    has_next = result.next_cursor.is_some(),
                    "Received resources page from upstream"
                );

                all_resources.extend(result.resources);

                match result.next_cursor {
                    Some(next) => cursor = Some(next),
                    None => {
                        cursor = None;
                        break;
                    }
                }
            }

            if cursor.is_some() {
                anyhow::bail!(
                    "Exceeded maximum pagination limit of {} pages while listing resources",
                    MAX_PAGINATION_PAGES
                );
            }

            debug!(
                count = all_resources.len(),
                "Listed resources from upstream"
            );
            Ok(all_resources)
        }
        .instrument(debug_span!("upstream_list_resources"))
        .await
    }

    async fn read_resource(&mut self, uri: &str) -> Result<ReadResourceResult> {
        let resource_uri = uri.to_string();
        let span = debug_span!("upstream_read_resource", uri = %resource_uri);

        async move {
            if self.init_result.is_none() {
                self.initialize().await?;
            }

            let params = serde_json::json!({ "uri": resource_uri });
            self.send_request(McpMethod::ResourcesRead.as_str(), Some(params))
                .await?;
            let response = self.read_response().await?;
            let result: ReadResourceResult = serde_json::from_value(
                response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("No result in resources/read response"))?,
            )?;

            Ok(result)
        }
        .instrument(span)
        .await
    }

    async fn list_prompts(&mut self) -> Result<Vec<Prompt>> {
        async move {
            if self.init_result.is_none() {
                self.initialize().await?;
            }

            let mut all_prompts = Vec::new();
            let mut cursor: Option<String> = None;

            for page in 0..MAX_PAGINATION_PAGES {
                let params = cursor.as_ref().map(|c| serde_json::json!({ "cursor": c }));

                self.send_request(McpMethod::PromptsList.as_str(), params)
                    .await?;
                let response = self.read_response().await?;
                let result: ListPromptsResult = serde_json::from_value(
                    response
                        .result
                        .ok_or_else(|| anyhow::anyhow!("No result in prompts/list response"))?,
                )?;

                debug!(
                    page = page,
                    count = result.prompts.len(),
                    has_next = result.next_cursor.is_some(),
                    "Received prompts page from upstream"
                );

                all_prompts.extend(result.prompts);

                match result.next_cursor {
                    Some(next) => cursor = Some(next),
                    None => {
                        cursor = None;
                        break;
                    }
                }
            }

            if cursor.is_some() {
                anyhow::bail!(
                    "Exceeded maximum pagination limit of {} pages while listing prompts",
                    MAX_PAGINATION_PAGES
                );
            }

            debug!(count = all_prompts.len(), "Listed prompts from upstream");
            Ok(all_prompts)
        }
        .instrument(debug_span!("upstream_list_prompts"))
        .await
    }

    async fn get_prompt(
        &mut self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<GetPromptResult> {
        let prompt_name = name.to_string();
        let span = debug_span!("upstream_get_prompt", prompt = %prompt_name);

        async move {
            if self.init_result.is_none() {
                self.initialize().await?;
            }

            let mut params = serde_json::json!({ "name": prompt_name });
            if let Some(args) = arguments {
                params
                    .as_object_mut()
                    .unwrap()
                    .insert("arguments".to_string(), args);
            }

            self.send_request(McpMethod::PromptsGet.as_str(), Some(params))
                .await?;
            let response = self.read_response().await?;
            let result: GetPromptResult = serde_json::from_value(
                response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("No result in prompts/get response"))?,
            )?;

            Ok(result)
        }
        .instrument(span)
        .await
    }

    async fn shutdown(&mut self) -> Result<()> {
        debug!("Shutting down upstream client");

        // Gracefully close stdin to signal the process we're done
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.shutdown().await; // Ignore errors on shutdown
        }

        // Kill the child process if it's still running
        if let Some(child) = &mut self.child {
            let _ = child.kill().await; // Ignore errors if already dead
            let _ = child.wait().await; // Wait for cleanup
        }

        // Clear all handles
        self.child = None;
        self.stdin = None;
        self.stdout = None;
        self.init_result = None;

        Ok(())
    }
}

impl Drop for StdioClient {
    /// Ensure child process is killed when client is dropped
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            // Try to kill the process synchronously
            // We can't use async in Drop, so we just try our best
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::{ServerCapabilities, ServerInfo};

    #[test]
    fn test_stdio_client_creation() {
        let client = StdioClient::new(
            "test-command".to_string(),
            Some(vec!["arg1".to_string()]),
            None,
            Duration::from_secs(30),
        );
        assert_eq!(client.command, "test-command");
        assert!(client.init_result.is_none());
    }

    #[test]
    fn test_new_client_has_no_active_process() {
        let client = StdioClient::new("test".to_string(), None, None, Duration::from_secs(30));
        assert!(client.child.is_none());
        assert!(client.stdin.is_none());
        assert!(client.stdout.is_none());
    }

    #[test]
    fn test_client_with_args() {
        let args = vec!["--flag".to_string(), "value".to_string()];
        let client = StdioClient::new(
            "cmd".to_string(),
            Some(args.clone()),
            None,
            Duration::from_secs(30),
        );
        assert_eq!(client.args, Some(args));
    }

    #[test]
    fn test_client_with_env() {
        let mut env = HashMap::new();
        env.insert("KEY".to_string(), "VALUE".to_string());
        let client = StdioClient::new(
            "cmd".to_string(),
            None,
            Some(env.clone()),
            Duration::from_secs(30),
        );
        assert_eq!(client.env, Some(env));
    }

    #[tokio::test]
    async fn test_initialize_caches_result() {
        // This test would need a mock server to be complete
        // For now, we test the caching logic path
        let mut client = StdioClient::new("echo".to_string(), None, None, Duration::from_secs(30));

        // Simulate already initialized state
        let mock_result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities::default(),
            server_info: ServerInfo {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: None,
        };
        client.init_result = Some(mock_result.clone());

        // Initialize should return cached result without spawning
        let result = client.initialize().await.unwrap();
        assert_eq!(result.server_info.name, "test");
        assert!(client.child.is_none()); // Should not have spawned
    }

    #[tokio::test]
    async fn test_shutdown_clears_state() {
        let mut client = StdioClient::new("test".to_string(), None, None, Duration::from_secs(30));

        // Set some state
        let mock_result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities::default(),
            server_info: ServerInfo {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: None,
        };
        client.init_result = Some(mock_result);

        // Shutdown should clear everything
        client.shutdown().await.unwrap();
        assert!(client.child.is_none());
        assert!(client.stdin.is_none());
        assert!(client.stdout.is_none());
        assert!(client.init_result.is_none());
    }

    // === Error path tests ===

    #[tokio::test]
    async fn test_spawn_fails_with_nonexistent_command() {
        let mut client = StdioClient::new(
            "nonexistent-command-that-does-not-exist-12345".to_string(),
            None,
            None,
            Duration::from_secs(30),
        );
        let result = client.spawn();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_double_spawn_returns_error() {
        // Use a real command that exists on all systems
        let mut client = StdioClient::new("cat".to_string(), None, None, Duration::from_secs(30));

        // First spawn should succeed
        let result1 = client.spawn();
        assert!(result1.is_ok());
        assert!(client.child.is_some());

        // Second spawn should fail with error
        let result2 = client.spawn();
        assert!(result2.is_err());
        assert!(result2.unwrap_err().to_string().contains("already spawned"));

        // Clean up
        client.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_send_request_without_stdin_returns_error() {
        let mut client = StdioClient::new("test".to_string(), None, None, Duration::from_secs(30));
        // Don't spawn - stdin is None

        let result = client.send_request("test/method", None).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Stdin not available")
        );
    }

    #[tokio::test]
    async fn test_read_response_without_stdout_returns_error() {
        let mut client = StdioClient::new("test".to_string(), None, None, Duration::from_secs(30));
        // Don't spawn - stdout is None

        let result = client.read_response().await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Stdout not available")
        );
    }

    #[tokio::test]
    async fn test_send_notification_without_stdin_returns_error() {
        let mut client = StdioClient::new("test".to_string(), None, None, Duration::from_secs(30));
        // Don't spawn - stdin is None

        let result = client.send_notification("test/notification", None).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Stdin not available")
        );
    }
}
