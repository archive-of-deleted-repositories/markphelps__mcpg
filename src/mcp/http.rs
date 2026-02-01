use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tracing::debug;
use uuid::Uuid;

use super::client::McpClient;
use super::error::McpError;
use super::protocol::{
    CallToolParams, CallToolResult, ClientCapabilities, ClientInfo, GetPromptResult,
    InitializeParams, InitializeResult, JsonRpcRequest, JsonRpcResponse, ListPromptsResult,
    ListResourcesResult, ListToolsResult, MAX_PAGINATION_PAGES, McpMethod, McpNotification,
    PROTOCOL_VERSION, Prompt, ReadResourceResult, Resource, Tool,
};

/// MCP client that communicates via HTTP (Streamable HTTP transport)
///
/// Implements the MCP 2025-06-18 Streamable HTTP specification
pub struct HttpClient {
    // Config
    url: String,
    headers: HashMap<String, String>,
    #[allow(dead_code)] // Used during construction, kept for potential future use
    timeout: Duration,

    // HTTP client
    http_client: reqwest::Client,

    // Session management
    session_id: Option<String>,

    // Cached initialization
    init_result: Option<InitializeResult>,
}

impl HttpClient {
    /// Create a new HTTP client (doesn't connect yet)
    pub fn new(
        url: String,
        headers: Option<HashMap<String, String>>,
        timeout: Duration,
    ) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(McpError::Network)?;

        Ok(Self {
            url,
            headers: headers.unwrap_or_default(),
            timeout,
            http_client,
            session_id: None,
            init_result: None,
        })
    }

    /// Send a JSON-RPC request to the server
    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse> {
        let request_id = Uuid::new_v4().to_string();
        let json_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(request_id.into()),
            method: method.to_string(),
            params,
        };

        // Build HTTP POST request
        let mut req = self
            .http_client
            .post(&self.url)
            .header("MCP-Protocol-Version", PROTOCOL_VERSION)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");

        // Add session ID if we have one (after initialization)
        if let Some(ref session_id) = self.session_id {
            req = req.header("Mcp-Session-Id", session_id);
        }

        // Add custom headers
        for (key, value) in &self.headers {
            req = req.header(key, value);
        }

        // Send request
        let response = req
            .json(&json_request)
            .send()
            .await
            .map_err(McpError::Network)?;

        // Check HTTP status code
        let status = response.status();
        if !status.is_success() {
            let message = status.canonical_reason().unwrap_or("Unknown").to_string();
            return Err(McpError::http(status.as_u16(), message).into());
        }

        // Extract session ID from initialize response
        if method == McpMethod::Initialize.as_str()
            && let Some(session_id) = response.headers().get("Mcp-Session-Id")
        {
            let session_id_str = session_id
                .to_str()
                .map_err(|e| McpError::InvalidSessionId(e.to_string()))?
                .to_string();
            self.session_id = Some(session_id_str);
        }

        // Parse response
        let json_response: JsonRpcResponse = response.json().await.map_err(|e| {
            // reqwest JSON parsing errors are wrapped as Other since they're not pure serde_json::Error
            McpError::Other(anyhow::anyhow!("Failed to parse JSON response: {}", e))
        })?;

        // Check for JSON-RPC error
        if let Some(error) = json_response.error {
            return Err(McpError::jsonrpc(error.code, error.message, error.data).into());
        }

        Ok(json_response)
    }

    /// Send a JSON-RPC notification to the server (no response expected)
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

        let mut req = self
            .http_client
            .post(&self.url)
            .header("MCP-Protocol-Version", PROTOCOL_VERSION)
            .header("Content-Type", "application/json");

        if let Some(ref session_id) = self.session_id {
            req = req.header("Mcp-Session-Id", session_id);
        }

        for (key, value) in &self.headers {
            req = req.header(key, value);
        }

        let response = req
            .json(&notification)
            .send()
            .await
            .map_err(McpError::Network)?;

        // MCP spec requires servers to return 202 Accepted for notifications,
        // but we also accept 200 OK for compatibility with non-strict implementations
        let status = response.status();
        if !matches!(status.as_u16(), 200 | 202) {
            let message = format!(
                "Expected 200 OK or 202 Accepted for notification, got {}",
                status.as_u16()
            );
            return Err(McpError::http(status.as_u16(), message).into());
        }

        Ok(())
    }
}

#[async_trait]
impl McpClient for HttpClient {
    async fn initialize(&mut self) -> Result<InitializeResult> {
        // Return cached result if already initialized
        if let Some(ref result) = self.init_result {
            return Ok(result.clone());
        }

        let params = serde_json::to_value(InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            client_info: ClientInfo {
                name: crate::config::APP_NAME.to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: ClientCapabilities::default(),
        })
        .map_err(McpError::Serialization)?;

        let response = self
            .send_request(McpMethod::Initialize.as_str(), Some(params))
            .await?;
        let init_result: InitializeResult =
            serde_json::from_value(response.result.ok_or_else(|| {
                McpError::Other(anyhow::anyhow!("No result in initialize response"))
            })?)
            .map_err(McpError::Serialization)?;

        // Cache the initialization result before sending notification
        self.init_result = Some(init_result.clone());

        // Send notifications/initialized notification
        // If this fails, we must clear the cached state to maintain consistency.
        // The server won't consider the client initialized if it didn't receive
        // the notification, so we shouldn't cache the init_result.
        if let Err(e) = self
            .send_notification(McpNotification::Initialized.as_str(), None)
            .await
        {
            // Clear cached state on notification failure to allow retry
            self.init_result = None;
            self.session_id = None;
            return Err(e);
        }

        Ok(init_result)
    }

    async fn list_tools(&mut self) -> Result<Vec<Tool>> {
        if self.init_result.is_none() {
            self.initialize().await?;
        }

        let mut all_tools = Vec::new();
        let mut cursor: Option<String> = None;

        for page in 0..MAX_PAGINATION_PAGES {
            let params = cursor.as_ref().map(|c| serde_json::json!({ "cursor": c }));

            let response = self
                .send_request(McpMethod::ToolsList.as_str(), params)
                .await?;
            let result: ListToolsResult =
                serde_json::from_value(response.result.ok_or_else(|| {
                    McpError::Other(anyhow::anyhow!("No result in tools/list response"))
                })?)
                .map_err(McpError::Serialization)?;

            debug!(
                page = page,
                count = result.tools.len(),
                has_next = result.next_cursor.is_some(),
                "Received tools page from upstream"
            );

            if result.tools.is_empty() && result.next_cursor.is_some() {
                return Err(McpError::Other(anyhow::anyhow!(
                    "Upstream server returned empty page with cursor — possible infinite pagination loop"
                ))
                .into());
            }

            all_tools.extend(result.tools);

            match result.next_cursor {
                Some(next) => cursor = Some(next),
                None => return Ok(all_tools),
            }
        }

        Err(McpError::Other(anyhow::anyhow!(
            "Exceeded maximum pagination limit of {} pages while listing tools",
            MAX_PAGINATION_PAGES
        ))
        .into())
    }

    async fn call_tool(&mut self, name: &str, args: serde_json::Value) -> Result<CallToolResult> {
        if self.init_result.is_none() {
            self.initialize().await?;
        }

        let params = serde_json::to_value(CallToolParams {
            name: name.to_string(),
            arguments: Some(args),
        })
        .map_err(McpError::Serialization)?;

        let response = self
            .send_request(McpMethod::ToolsCall.as_str(), Some(params))
            .await?;
        let call_result: CallToolResult =
            serde_json::from_value(response.result.ok_or_else(|| {
                McpError::Other(anyhow::anyhow!("No result in tools/call response"))
            })?)
            .map_err(McpError::Serialization)?;

        Ok(call_result)
    }

    async fn list_resources(&mut self) -> Result<Vec<Resource>> {
        if self.init_result.is_none() {
            self.initialize().await?;
        }

        let mut all_resources = Vec::new();
        let mut cursor: Option<String> = None;

        for page in 0..MAX_PAGINATION_PAGES {
            let params = cursor.as_ref().map(|c| serde_json::json!({ "cursor": c }));

            let response = self
                .send_request(McpMethod::ResourcesList.as_str(), params)
                .await?;
            let result: ListResourcesResult =
                serde_json::from_value(response.result.ok_or_else(|| {
                    McpError::Other(anyhow::anyhow!("No result in resources/list response"))
                })?)
                .map_err(McpError::Serialization)?;

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
            return Err(McpError::Other(anyhow::anyhow!(
                "Exceeded maximum pagination limit of {} pages while listing resources",
                MAX_PAGINATION_PAGES
            ))
            .into());
        }

        Ok(all_resources)
    }

    async fn read_resource(&mut self, uri: &str) -> Result<ReadResourceResult> {
        if self.init_result.is_none() {
            self.initialize().await?;
        }

        let params = serde_json::json!({ "uri": uri });
        let response = self
            .send_request(McpMethod::ResourcesRead.as_str(), Some(params))
            .await?;
        let result: ReadResourceResult =
            serde_json::from_value(response.result.ok_or_else(|| {
                McpError::Other(anyhow::anyhow!("No result in resources/read response"))
            })?)
            .map_err(McpError::Serialization)?;

        Ok(result)
    }

    async fn list_prompts(&mut self) -> Result<Vec<Prompt>> {
        if self.init_result.is_none() {
            self.initialize().await?;
        }

        let mut all_prompts = Vec::new();
        let mut cursor: Option<String> = None;

        for page in 0..MAX_PAGINATION_PAGES {
            let params = cursor.as_ref().map(|c| serde_json::json!({ "cursor": c }));

            let response = self
                .send_request(McpMethod::PromptsList.as_str(), params)
                .await?;
            let result: ListPromptsResult =
                serde_json::from_value(response.result.ok_or_else(|| {
                    McpError::Other(anyhow::anyhow!("No result in prompts/list response"))
                })?)
                .map_err(McpError::Serialization)?;

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
            return Err(McpError::Other(anyhow::anyhow!(
                "Exceeded maximum pagination limit of {} pages while listing prompts",
                MAX_PAGINATION_PAGES
            ))
            .into());
        }

        Ok(all_prompts)
    }

    async fn get_prompt(
        &mut self,
        name: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<GetPromptResult> {
        if self.init_result.is_none() {
            self.initialize().await?;
        }

        let mut params = serde_json::json!({ "name": name });
        if let Some(args) = arguments {
            params
                .as_object_mut()
                .unwrap()
                .insert("arguments".to_string(), args);
        }

        let response = self
            .send_request(McpMethod::PromptsGet.as_str(), Some(params))
            .await?;
        let result: GetPromptResult = serde_json::from_value(response.result.ok_or_else(|| {
            McpError::Other(anyhow::anyhow!("No result in prompts/get response"))
        })?)
        .map_err(McpError::Serialization)?;

        Ok(result)
    }

    async fn shutdown(&mut self) -> Result<()> {
        debug!("Shutting down HTTP client");

        // Clear state
        self.session_id = None;
        self.init_result = None;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::protocol::JsonRpcRequest;
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_http_client_creation() {
        let client = HttpClient::new(
            "http://localhost:3232/mcp".to_string(),
            None,
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(client.url, "http://localhost:3232/mcp");
        assert_eq!(client.timeout, Duration::from_secs(30));
        assert!(client.session_id.is_none());
        assert!(client.init_result.is_none());
    }

    #[test]
    fn test_http_client_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());

        let client = HttpClient::new(
            "http://localhost:3232/mcp".to_string(),
            Some(headers.clone()),
            Duration::from_secs(30),
        )
        .unwrap();

        assert_eq!(client.headers, headers);
    }

    #[test]
    fn test_http_client_custom_timeout() {
        let client = HttpClient::new(
            "http://localhost:3232/mcp".to_string(),
            None,
            Duration::from_secs(60),
        )
        .unwrap();
        assert_eq!(client.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_request_id_generation() {
        // Test that we can create request structures
        let request_id = Uuid::new_v4().to_string();
        let json_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(request_id.clone().into()),
            method: "test".to_string(),
            params: None,
        };

        assert_eq!(json_request.method, "test");
        assert!(json_request.id.is_some());
    }

    #[tokio::test]
    async fn test_shutdown_clears_state() {
        let mut client = HttpClient::new(
            "http://localhost:3232/mcp".to_string(),
            None,
            Duration::from_secs(30),
        )
        .unwrap();

        // Simulate having session and init result
        use super::super::protocol::{ServerCapabilities, ServerInfo};
        client.session_id = Some("test-session".to_string());
        client.init_result = Some(InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities::default(),
            server_info: ServerInfo {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: None,
        });

        // Shutdown should clear everything
        client.shutdown().await.unwrap();
        assert!(client.session_id.is_none());
        assert!(client.init_result.is_none());
    }

    #[tokio::test]
    async fn test_notification_accepts_200_ok() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server returns 200 OK for notification
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Send notification - should succeed with 200 OK
        let result = client
            .send_notification("notifications/initialized", None)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_notification_accepts_202_accepted() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server returns 202 Accepted for notification (spec-compliant)
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(202))
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Send notification - should succeed with 202 Accepted
        let result = client
            .send_notification("notifications/initialized", None)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_notification_rejects_invalid_status_codes() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // Test various invalid status codes
        let invalid_codes = vec![
            201, // Created (not acceptable)
            204, // No Content (not acceptable)
            400, // Bad Request
            404, // Not Found
            500, // Internal Server Error
        ];

        for status_code in invalid_codes {
            // Create a new mock server for each test to avoid state pollution
            let mock_server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(header("Content-Type", "application/json"))
                .respond_with(ResponseTemplate::new(status_code))
                .expect(1)
                .mount(&mock_server)
                .await;

            let mut client =
                HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

            // Send notification - should fail with non-200/202 status
            let result = client
                .send_notification("notifications/initialized", None)
                .await;
            assert!(
                result.is_err(),
                "Expected error for status code {}",
                status_code
            );

            // Verify it's an HTTP error with the correct status code
            let err = result.unwrap_err();
            let err_string = err.to_string();
            assert!(
                err_string.contains(&format!("HTTP {}", status_code)),
                "Expected HTTP {} error, got: {}",
                status_code,
                err_string
            );
            assert!(
                err_string.contains("Expected 200 OK or 202 Accepted"),
                "Error message should explain expected status codes, got: {}",
                err_string
            );
        }
    }

    #[tokio::test]
    async fn test_initialization_notification_failure_clears_state() {
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // First request: initialize (succeeds and returns session ID)
        let init_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "test-id",
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "serverInfo": {
                    "name": "test-server",
                    "version": "1.0.0"
                }
            }
        });

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(serde_json::json!({
                "method": "initialize"
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&init_response)
                    .insert_header("Mcp-Session-Id", "test-session-123"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        // Second request: notifications/initialized (fails with 500)
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(serde_json::json!({
                "method": "notifications/initialized"
            })))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Attempt initialization - should fail during notification
        let result = client.initialize().await;
        assert!(result.is_err(), "Expected initialization to fail");

        // Verify client state is cleared
        assert!(
            client.init_result.is_none(),
            "init_result should be cleared after notification failure"
        );
        assert!(
            client.session_id.is_none(),
            "session_id should be cleared after notification failure"
        );

        // Verify error is from notification failure
        let err_string = result.unwrap_err().to_string();
        assert!(
            err_string.contains("HTTP 500"),
            "Error should indicate HTTP 500 from notification, got: {}",
            err_string
        );
    }

    #[tokio::test]
    async fn test_initialization_retry_after_notification_failure() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Track how many notification calls we've received
        let notification_count = Arc::new(AtomicUsize::new(0));
        let notification_count_clone = notification_count.clone();

        // First attempt: initialize succeeds, notification fails
        let init_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "test-id",
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "serverInfo": {
                    "name": "test-server",
                    "version": "1.0.0"
                }
            }
        });

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(serde_json::json!({
                "method": "initialize"
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&init_response)
                    .insert_header("Mcp-Session-Id", "test-session-456"),
            )
            .expect(2) // Expect 2 calls: first attempt and retry
            .mount(&mock_server)
            .await;

        // Notification handler: fail first time, succeed second time
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(serde_json::json!({
                "method": "notifications/initialized"
            })))
            .respond_with(move |_req: &wiremock::Request| {
                let count = notification_count_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    // First call: fail with 500
                    ResponseTemplate::new(500)
                } else {
                    // Second call: succeed with 202
                    ResponseTemplate::new(202)
                }
            })
            .expect(2)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // First attempt should fail
        let first_result = client.initialize().await;
        assert!(first_result.is_err(), "First initialization should fail");
        assert!(client.init_result.is_none(), "State should be cleared");

        // Second attempt should succeed
        let second_result = client.initialize().await;
        assert!(
            second_result.is_ok(),
            "Second initialization should succeed after retry: {:?}",
            second_result.as_ref().err()
        );
        assert!(
            client.init_result.is_some(),
            "init_result should be set after successful initialization"
        );
        assert!(
            client.session_id.is_some(),
            "session_id should be set after successful initialization"
        );
    }

    // Negative test cases for error scenarios

    #[tokio::test]
    async fn test_http_500_error() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server returns 500 Internal Server Error
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Attempt to send request - should fail with HTTP 500
        let result = client.send_request("test/method", None).await;
        assert!(result.is_err(), "Expected error for HTTP 500");

        let err = result.unwrap_err();
        let mcp_err = err.downcast_ref::<McpError>().expect("Should be McpError");

        // Verify error type and classification
        assert!(
            mcp_err.is_server_error(),
            "Should be classified as server error"
        );
        assert!(mcp_err.is_retryable(), "HTTP 500 should be retryable");
        assert!(!mcp_err.is_client_error(), "Should not be client error");

        // Verify error message is descriptive
        let err_str = err.to_string();
        assert!(
            err_str.contains("HTTP 500"),
            "Error message should include status code"
        );
    }

    #[tokio::test]
    async fn test_http_404_error() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server returns 404 Not Found
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Attempt to send request - should fail with HTTP 404
        let result = client.send_request("test/method", None).await;
        assert!(result.is_err(), "Expected error for HTTP 404");

        let err = result.unwrap_err();
        let mcp_err = err.downcast_ref::<McpError>().expect("Should be McpError");

        // Verify error type and classification
        assert!(
            mcp_err.is_client_error(),
            "Should be classified as client error"
        );
        assert!(!mcp_err.is_retryable(), "HTTP 404 should not be retryable");
        assert!(!mcp_err.is_server_error(), "Should not be server error");

        // Verify error message is descriptive
        let err_str = err.to_string();
        assert!(
            err_str.contains("HTTP 404"),
            "Error message should include status code"
        );
        assert!(
            err_str.contains("Not Found"),
            "Error message should include reason"
        );
    }

    #[tokio::test]
    async fn test_invalid_json_response() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server returns malformed JSON
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw("{ invalid json }", "application/json"),
            )
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Attempt to send request - should fail with JSON parsing error
        let result = client.send_request("test/method", None).await;
        assert!(result.is_err(), "Expected error for invalid JSON");

        let err = result.unwrap_err();
        let err_str = err.to_string();

        // Verify it's an McpError
        let mcp_err = err.downcast_ref::<McpError>().expect("Should be McpError");

        // Verify error message is descriptive
        assert!(
            err_str.contains("Failed to parse JSON response") || err_str.contains("JSON"),
            "Error message should indicate JSON parsing failure: {}",
            err_str
        );

        // Verify it's not retryable (malformed responses shouldn't be retried)
        assert!(
            !mcp_err.is_retryable(),
            "JSON parsing errors should not be retryable"
        );
    }

    #[tokio::test]
    async fn test_jsonrpc_error_response() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server returns JSON-RPC error response
        let error_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "test-id",
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": {
                    "detail": "Missing required parameter 'name'"
                }
            }
        });

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&error_response))
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Attempt to send request - should fail with JSON-RPC error
        let result = client.send_request("test/method", None).await;
        assert!(
            result.is_err(),
            "Expected error for JSON-RPC error response"
        );

        let err = result.unwrap_err();
        let mcp_err = err.downcast_ref::<McpError>().expect("Should be McpError");

        // Verify error type and classification
        assert!(
            !mcp_err.is_retryable(),
            "JSON-RPC errors should not be retryable"
        );
        assert!(
            !mcp_err.is_client_error(),
            "JSON-RPC errors are not HTTP client errors"
        );
        assert!(
            !mcp_err.is_server_error(),
            "JSON-RPC errors are not HTTP server errors"
        );

        // Verify error message is descriptive
        let err_str = err.to_string();
        assert!(
            err_str.contains("JSON-RPC error"),
            "Should indicate JSON-RPC error"
        );
        assert!(err_str.contains("-32602"), "Should include error code");
        assert!(
            err_str.contains("Invalid params"),
            "Should include error message"
        );
    }

    #[tokio::test]
    async fn test_timeout() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server delays response beyond timeout
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(
                ResponseTemplate::new(200).set_delay(Duration::from_secs(5)), // Delay longer than client timeout
            )
            .mount(&mock_server)
            .await;

        // Create client with very short timeout
        let mut client = HttpClient::new(
            mock_server.uri(),
            None,
            Duration::from_millis(100), // 100ms timeout
        )
        .unwrap();

        // Attempt to send request - should timeout
        let result = client.send_request("test/method", None).await;
        assert!(result.is_err(), "Expected timeout error");

        let err = result.unwrap_err();
        let mcp_err = err.downcast_ref::<McpError>().expect("Should be McpError");

        // Verify error type and classification
        assert!(mcp_err.is_retryable(), "Timeout errors should be retryable");
        assert!(!mcp_err.is_client_error(), "Timeout is not a client error");
        assert!(!mcp_err.is_server_error(), "Timeout is not a server error");

        // Verify error is a network error (reqwest wraps timeouts as network errors)
        let err_str = err.to_string();
        assert!(
            err_str.contains("timeout") || err_str.contains("Network"),
            "Error message should indicate timeout or network issue: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_missing_session_id_header() {
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server returns initialization response WITHOUT session ID header
        // This tests that the client can handle missing session ID gracefully
        let init_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "test-id",
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "serverInfo": {
                    "name": "test-server",
                    "version": "1.0.0"
                }
            }
        });

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(serde_json::json!({
                "method": "initialize"
            })))
            .respond_with(
                // Note: No Mcp-Session-Id header included
                ResponseTemplate::new(200).set_body_json(&init_response),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        // Mock notifications/initialized to succeed
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(serde_json::json!({
                "method": "notifications/initialized"
            })))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Attempt to initialize - should succeed even without session ID
        // (session ID is optional per spec, used for multiplexing)
        let result = client.initialize().await;
        assert!(
            result.is_ok(),
            "Initialization should succeed without session ID: {:?}",
            result.err()
        );

        // Verify session_id is None when not provided
        assert!(
            client.session_id.is_none(),
            "session_id should be None when server doesn't provide it"
        );

        // Verify init_result is still cached
        assert!(
            client.init_result.is_some(),
            "init_result should be cached even without session ID"
        );
    }

    #[tokio::test]
    async fn test_network_error() {
        // Attempt to connect to a server that's not running
        // Use a port that's very unlikely to have a service running
        let mut client = HttpClient::new(
            "http://localhost:59999/mcp".to_string(),
            None,
            Duration::from_secs(1),
        )
        .unwrap();

        // Attempt to send request - should fail with network error
        let result = client.send_request("test/method", None).await;
        assert!(
            result.is_err(),
            "Expected network error for connection refused"
        );

        let err = result.unwrap_err();
        let mcp_err = err.downcast_ref::<McpError>().expect("Should be McpError");

        // Verify error type and classification
        assert!(mcp_err.is_retryable(), "Network errors should be retryable");
        assert!(
            !mcp_err.is_client_error(),
            "Network error is not a client error"
        );
        assert!(
            !mcp_err.is_server_error(),
            "Network error is not a server error"
        );

        // Verify error message is descriptive
        let err_str = err.to_string();
        assert!(
            err_str.contains("Network") || err_str.contains("connection"),
            "Error message should indicate network issue: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_http_503_service_unavailable() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server returns 503 Service Unavailable
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Attempt to send request - should fail with HTTP 503
        let result = client.send_request("test/method", None).await;
        assert!(result.is_err(), "Expected error for HTTP 503");

        let err = result.unwrap_err();
        let mcp_err = err.downcast_ref::<McpError>().expect("Should be McpError");

        // Verify error type and classification
        assert!(
            mcp_err.is_server_error(),
            "Should be classified as server error"
        );
        assert!(mcp_err.is_retryable(), "HTTP 503 should be retryable");
        assert!(!mcp_err.is_client_error(), "Should not be client error");

        // Verify error message is descriptive
        let err_str = err.to_string();
        assert!(
            err_str.contains("HTTP 503"),
            "Error message should include status code"
        );
        assert!(
            err_str.contains("Service Unavailable"),
            "Error message should include reason"
        );
    }

    #[tokio::test]
    async fn test_http_401_unauthorized() {
        use wiremock::matchers::{header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock server returns 401 Unauthorized
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        // Attempt to send request - should fail with HTTP 401
        let result = client.send_request("test/method", None).await;
        assert!(result.is_err(), "Expected error for HTTP 401");

        let err = result.unwrap_err();
        let mcp_err = err.downcast_ref::<McpError>().expect("Should be McpError");

        // Verify error type and classification
        assert!(
            mcp_err.is_client_error(),
            "Should be classified as client error"
        );
        assert!(!mcp_err.is_retryable(), "HTTP 401 should not be retryable");
        assert!(!mcp_err.is_server_error(), "Should not be server error");

        // Verify error message is descriptive
        let err_str = err.to_string();
        assert!(
            err_str.contains("HTTP 401"),
            "Error message should include status code"
        );
        assert!(
            err_str.contains("Unauthorized"),
            "Error message should include reason"
        );
    }

    #[tokio::test]
    async fn test_list_tools_pagination_multiple_pages() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock initialize
        let init_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "test-id",
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "serverInfo": { "name": "test", "version": "1.0.0" }
            }
        });

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "initialize" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&init_response)
                    .insert_header("Mcp-Session-Id", "test-session"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "notifications/initialized" }),
            ))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Mock tools/list with 2 pages
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(serde_json::json!({ "method": "tools/list" })))
            .respond_with(move |_req: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    // Page 1: 2 tools + cursor
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": "test-id",
                        "result": {
                            "tools": [
                                { "name": "tool1", "description": "First", "inputSchema": {"type": "object"} },
                                { "name": "tool2", "description": "Second", "inputSchema": {"type": "object"} }
                            ],
                            "nextCursor": "page2"
                        }
                    }))
                } else {
                    // Page 2: 1 tool, no cursor
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": "test-id",
                        "result": {
                            "tools": [
                                { "name": "tool3", "description": "Third", "inputSchema": {"type": "object"} }
                            ]
                        }
                    }))
                }
            })
            .expect(2)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].name, "tool1");
        assert_eq!(tools[1].name, "tool2");
        assert_eq!(tools[2].name, "tool3");
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_list_tools_no_pagination() {
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock initialize
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "initialize" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": "test-id",
                        "result": {
                            "protocolVersion": PROTOCOL_VERSION,
                            "capabilities": {},
                            "serverInfo": { "name": "test", "version": "1.0.0" }
                        }
                    }))
                    .insert_header("Mcp-Session-Id", "test-session"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "notifications/initialized" }),
            ))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Mock tools/list with single page, no cursor
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "tools/list" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": "test-id",
                "result": {
                    "tools": [
                        { "name": "tool1", "inputSchema": {"type": "object"} }
                    ]
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "tool1");
    }

    #[tokio::test]
    async fn test_list_tools_empty_page_with_cursor_fails() {
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock initialize
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "initialize" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": "test-id",
                        "result": {
                            "protocolVersion": PROTOCOL_VERSION,
                            "capabilities": {},
                            "serverInfo": { "name": "test", "version": "1.0.0" }
                        }
                    }))
                    .insert_header("Mcp-Session-Id", "test-session"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "notifications/initialized" }),
            ))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Mock tools/list returning empty tools with a cursor (buggy server)
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "tools/list" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": "test-id",
                "result": {
                    "tools": [],
                    "nextCursor": "bogus-cursor"
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        let result = client.list_tools().await;
        assert!(result.is_err(), "Expected error for empty page with cursor");
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("empty page with cursor"),
            "Error should mention empty page: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_list_tools_pagination_limit_exceeded() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock initialize
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "initialize" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": "test-id",
                        "result": {
                            "protocolVersion": PROTOCOL_VERSION,
                            "capabilities": {},
                            "serverInfo": { "name": "test", "version": "1.0.0" }
                        }
                    }))
                    .insert_header("Mcp-Session-Id", "test-session"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "notifications/initialized" }),
            ))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Mock tools/list that always returns a tool AND a nextCursor (never terminates)
        let page_count = Arc::new(AtomicUsize::new(0));
        let page_count_clone = page_count.clone();

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "tools/list" }),
            ))
            .respond_with(move |_req: &wiremock::Request| {
                let page = page_count_clone.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": "test-id",
                    "result": {
                        "tools": [
                            { "name": format!("tool-page-{}", page), "inputSchema": {"type": "object"} }
                        ],
                        "nextCursor": format!("cursor-{}", page + 1)
                    }
                }))
            })
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();

        let result = client.list_tools().await;
        assert!(
            result.is_err(),
            "Expected error when exceeding pagination limit"
        );
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("Exceeded maximum pagination limit"),
            "Error should mention pagination limit: {}",
            err_str
        );

        // Verify we actually hit the limit
        assert_eq!(page_count.load(Ordering::SeqCst), MAX_PAGINATION_PAGES);
    }

    #[tokio::test]
    async fn test_list_resources_pagination() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock initialize
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "initialize" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "jsonrpc": "2.0", "id": "test-id",
                        "result": {
                            "protocolVersion": PROTOCOL_VERSION,
                            "capabilities": { "resources": {} },
                            "serverInfo": { "name": "test", "version": "1.0.0" }
                        }
                    }))
                    .insert_header("Mcp-Session-Id", "test-session"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_partial_json(
                serde_json::json!({ "method": "notifications/initialized" }),
            ))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Mock resources/list with 2 pages
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("POST"))
            .and(body_partial_json(
                serde_json::json!({ "method": "resources/list" }),
            ))
            .respond_with(move |_req: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "jsonrpc": "2.0", "id": "test-id",
                        "result": {
                            "resources": [
                                { "uri": "file:///a.txt", "name": "a.txt" },
                                { "uri": "file:///b.txt", "name": "b.txt" }
                            ],
                            "nextCursor": "page2"
                        }
                    }))
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "jsonrpc": "2.0", "id": "test-id",
                        "result": {
                            "resources": [
                                { "uri": "file:///c.txt", "name": "c.txt" }
                            ]
                        }
                    }))
                }
            })
            .expect(2)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();
        let resources = client.list_resources().await.unwrap();
        assert_eq!(resources.len(), 3);
        assert_eq!(resources[0].uri, "file:///a.txt");
        assert_eq!(resources[2].uri, "file:///c.txt");
    }

    #[tokio::test]
    async fn test_read_resource() {
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock initialize
        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "initialize" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "jsonrpc": "2.0", "id": "test-id",
                        "result": {
                            "protocolVersion": PROTOCOL_VERSION,
                            "capabilities": { "resources": {} },
                            "serverInfo": { "name": "test", "version": "1.0.0" }
                        }
                    }))
                    .insert_header("Mcp-Session-Id", "test-session"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_partial_json(
                serde_json::json!({ "method": "notifications/initialized" }),
            ))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Mock resources/read
        Mock::given(method("POST"))
            .and(body_partial_json(serde_json::json!({ "method": "resources/read" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": "test-id",
                "result": {
                    "contents": [{ "uri": "file:///test.txt", "mimeType": "text/plain", "text": "hello world" }]
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();
        let result = client.read_resource("file:///test.txt").await.unwrap();
        assert_eq!(result.contents.len(), 1);
        assert_eq!(result.contents[0].text, Some("hello world".to_string()));
    }

    #[tokio::test]
    async fn test_list_prompts_pagination() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "initialize" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "jsonrpc": "2.0", "id": "test-id",
                        "result": {
                            "protocolVersion": PROTOCOL_VERSION,
                            "capabilities": { "prompts": {} },
                            "serverInfo": { "name": "test", "version": "1.0.0" }
                        }
                    }))
                    .insert_header("Mcp-Session-Id", "test-session"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_partial_json(
                serde_json::json!({ "method": "notifications/initialized" }),
            ))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&mock_server)
            .await;

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("POST"))
            .and(body_partial_json(
                serde_json::json!({ "method": "prompts/list" }),
            ))
            .respond_with(move |_req: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "jsonrpc": "2.0", "id": "test-id",
                        "result": {
                            "prompts": [{ "name": "prompt1" }, { "name": "prompt2" }],
                            "nextCursor": "page2"
                        }
                    }))
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "jsonrpc": "2.0", "id": "test-id",
                        "result": { "prompts": [{ "name": "prompt3" }] }
                    }))
                }
            })
            .expect(2)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();
        let prompts = client.list_prompts().await.unwrap();
        assert_eq!(prompts.len(), 3);
        assert_eq!(prompts[0].name, "prompt1");
        assert_eq!(prompts[2].name, "prompt3");
    }

    #[tokio::test]
    async fn test_get_prompt() {
        use wiremock::matchers::{body_partial_json, header, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(header("Content-Type", "application/json"))
            .and(body_partial_json(
                serde_json::json!({ "method": "initialize" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "jsonrpc": "2.0", "id": "test-id",
                        "result": {
                            "protocolVersion": PROTOCOL_VERSION,
                            "capabilities": { "prompts": {} },
                            "serverInfo": { "name": "test", "version": "1.0.0" }
                        }
                    }))
                    .insert_header("Mcp-Session-Id", "test-session"),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_partial_json(
                serde_json::json!({ "method": "notifications/initialized" }),
            ))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(body_partial_json(serde_json::json!({ "method": "prompts/get" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": "test-id",
                "result": {
                    "description": "A test prompt",
                    "messages": [
                        { "role": "user", "content": { "type": "text", "text": "Review this code" } }
                    ]
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let mut client = HttpClient::new(mock_server.uri(), None, Duration::from_secs(30)).unwrap();
        let result = client
            .get_prompt(
                "code_review",
                Some(serde_json::json!({"code": "fn main() {}"})),
            )
            .await
            .unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].role, "user");
    }
}
