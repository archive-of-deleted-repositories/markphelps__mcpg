//! Integration tests for the MCP Gateway Server
//!
//! These tests verify the gateway server works end-to-end with real upstream servers.

use super::helpers::should_skip_integration_tests;
use mcpg::config::{Config, ServerConfig, TransportConfig};
use mcpg::mcp::protocol::{
    CallToolParams, CallToolResult, Content, GetPromptResult, InitializeParams, InitializeResult,
    JsonRpcRequest, JsonRpcResponse, ListPromptsResult, ListResourcesResult, ListToolsResult,
    PROTOCOL_VERSION, ReadResourceResult,
};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use uuid::Uuid;

/// Helper macro to skip tests if SKIP_INTEGRATION_TESTS is set
macro_rules! skip_if_no_nodejs {
    () => {
        if should_skip_integration_tests() {
            eprintln!("Skipping integration test - SKIP_INTEGRATION_TESTS is set");
            return;
        }
    };
}

/// Gateway test client wrapping the mcpg process
struct GatewayClient {
    process: Child,
}

impl GatewayClient {
    /// Spawn the mcpg gateway server with the given config
    fn spawn(config: Config) -> Self {
        // Write config to temp file
        let config_json = serde_json::to_string(&config).unwrap();
        let temp_dir = std::env::temp_dir();
        let config_path = temp_dir.join(format!("mcpg-test-config-{}.json", std::process::id()));
        std::fs::write(&config_path, config_json).unwrap();

        // Spawn mcpg process with config
        let process = Command::new("cargo")
            .arg("run")
            .arg("--quiet")
            .env("MCPG_CONFIG", config_path.to_str().unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to spawn mcpg process");

        // Give process a moment to start
        std::thread::sleep(Duration::from_millis(1000));

        Self { process }
    }

    /// Send a JSON-RPC request and receive response
    fn send_request(&mut self, method: &str, params: Option<serde_json::Value>) -> JsonRpcResponse {
        let request_id = Uuid::new_v4().to_string();
        let request = JsonRpcRequest::new(request_id, method, params);

        // Write request
        let stdin = self.process.stdin.as_mut().unwrap();
        let request_json = serde_json::to_string(&request).unwrap();
        writeln!(stdin, "{}", request_json).unwrap();
        stdin.flush().unwrap();

        // Read response
        let stdout = self.process.stdout.as_mut().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();

        serde_json::from_str(&line).expect("Failed to parse response")
    }

    fn initialize(&mut self) -> InitializeResult {
        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            client_info: mcpg::mcp::protocol::ClientInfo {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            },
            capabilities: mcpg::mcp::protocol::ClientCapabilities::default(),
        };

        let response = self.send_request("initialize", Some(serde_json::to_value(params).unwrap()));
        assert!(
            response.error.is_none(),
            "Initialize failed: {:?}",
            response.error
        );
        serde_json::from_value(response.result.unwrap()).unwrap()
    }

    fn list_tools(&mut self) -> ListToolsResult {
        let response = self.send_request("tools/list", None);
        assert!(
            response.error.is_none(),
            "List tools failed: {:?}",
            response.error
        );
        serde_json::from_value(response.result.unwrap()).unwrap()
    }

    fn list_resources(&mut self) -> ListResourcesResult {
        let response = self.send_request("resources/list", None);
        assert!(
            response.error.is_none(),
            "List resources failed: {:?}",
            response.error
        );
        serde_json::from_value(response.result.unwrap()).unwrap()
    }

    fn read_resource(&mut self, uri: &str) -> ReadResourceResult {
        let response = self.send_request("resources/read", Some(json!({"uri": uri})));
        assert!(
            response.error.is_none(),
            "Read resource failed: {:?}",
            response.error
        );
        serde_json::from_value(response.result.unwrap()).unwrap()
    }

    fn list_prompts(&mut self) -> ListPromptsResult {
        let response = self.send_request("prompts/list", None);
        assert!(
            response.error.is_none(),
            "List prompts failed: {:?}",
            response.error
        );
        serde_json::from_value(response.result.unwrap()).unwrap()
    }

    fn get_prompt(&mut self, name: &str, arguments: Option<serde_json::Value>) -> GetPromptResult {
        let mut params = json!({"name": name});
        if let Some(args) = arguments {
            params
                .as_object_mut()
                .unwrap()
                .insert("arguments".to_string(), args);
        }
        let response = self.send_request("prompts/get", Some(params));
        assert!(
            response.error.is_none(),
            "Get prompt failed: {:?}",
            response.error
        );
        serde_json::from_value(response.result.unwrap()).unwrap()
    }

    fn call_tool(&mut self, name: &str, arguments: serde_json::Value) -> CallToolResult {
        let params = CallToolParams {
            name: name.to_string(),
            arguments: Some(arguments),
        };

        let response = self.send_request("tools/call", Some(serde_json::to_value(params).unwrap()));
        assert!(
            response.error.is_none(),
            "Call tool failed: {:?}",
            response.error
        );
        serde_json::from_value(response.result.unwrap()).unwrap()
    }
}

impl Drop for GatewayClient {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn create_test_config() -> Config {
    let mut servers = HashMap::new();
    servers.insert(
        "everything".to_string(),
        ServerConfig {
            transport: TransportConfig::Stdio {
                command: "npx".to_string(),
                args: Some(vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-everything".to_string(),
                ]),
                env: None,
            },
            timeout_secs: 30,
        },
    );

    Config { servers }
}

#[test]
fn test_gateway_initialize() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    let result = client.initialize();
    assert_eq!(result.protocol_version, PROTOCOL_VERSION);
    assert_eq!(result.server_info.name, "mcpg");
    assert!(result.capabilities.tools.is_some());
}

#[test]
fn test_gateway_list_tools() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();
    let result = client.list_tools();

    assert_eq!(result.tools.len(), 4, "Should have exactly 4 gateway tools");
    assert!(
        result.next_cursor.is_none(),
        "Gateway should not paginate its own tools"
    );
    let tool_names: Vec<&str> = result.tools.iter().map(|t| t.name.as_str()).collect();
    assert!(tool_names.contains(&"search"));
    assert!(tool_names.contains(&"inspect"));
    assert!(tool_names.contains(&"call"));
    assert!(tool_names.contains(&"list"));
}

#[test]
fn test_gateway_list_servers() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();
    let result = client.call_tool("list", json!({}));

    assert_eq!(result.is_error, None);
    assert_eq!(result.content.len(), 1);
    if let Content::Text { text } = &result.content[0] {
        assert!(
            text.contains("everything"),
            "Should list the everything server"
        );
    } else {
        panic!("Expected text content");
    }
}

#[test]
fn test_gateway_search_tools() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();

    // Search should trigger cache population
    let result = client.call_tool("search", json!({"query": "echo"}));

    assert_eq!(result.is_error, None);
    assert_eq!(result.content.len(), 1);
    if let Content::Text { text } = &result.content[0] {
        assert!(
            text.contains("[everything]"),
            "Should show tools from everything server"
        );
    } else {
        panic!("Expected text content");
    }
}

#[test]
fn test_gateway_list_server_tools() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();

    // First trigger cache population with search
    let _ = client.call_tool("search", json!({"query": "echo"}));

    // Then list tools for the specific server
    let result = client.call_tool("list", json!({"server": "everything"}));

    assert_eq!(result.is_error, None);
    assert_eq!(result.content.len(), 1);
    if let Content::Text { text } = &result.content[0] {
        assert!(
            text.contains("[everything]"),
            "Should show tools from everything server"
        );
    } else {
        panic!("Expected text content");
    }
}

#[test]
fn test_gateway_call_upstream_tool() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();

    // Call the echo tool on the everything server
    let result = client.call_tool(
        "call",
        json!({
            "server": "everything",
            "tool": "echo",
            "args": {"message": "hello from gateway"}
        }),
    );

    // Should succeed (echo returns the message)
    assert_eq!(result.is_error, None);
    assert_eq!(result.content.len(), 1);
    if let Content::Text { text } = &result.content[0] {
        assert!(
            text.contains("hello from gateway"),
            "Echo should return our message"
        );
    } else {
        panic!("Expected text content");
    }
}

#[test]
fn test_gateway_inspect_tool() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();

    // First trigger cache population
    let _ = client.call_tool("search", json!({"query": "echo"}));

    // Inspect the echo tool
    let result = client.call_tool(
        "inspect",
        json!({
            "server": "everything",
            "tool": "echo"
        }),
    );

    assert_eq!(result.is_error, None);
    if let Content::Text { text } = &result.content[0] {
        assert!(text.contains("Tool:"));
        assert!(text.contains("Server:"));
        assert!(text.contains("everything"));
    } else {
        panic!("Expected text content");
    }
}

#[test]
fn test_gateway_error_handling_nonexistent_server() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();

    // Call with nonexistent server
    let result = client.call_tool(
        "call",
        json!({
            "server": "nonexistent",
            "tool": "test",
            "args": {}
        }),
    );

    assert_eq!(result.is_error, Some(true));
    if let Content::Text { text } = &result.content[0] {
        assert!(
            text.to_lowercase().contains("not found") || text.to_lowercase().contains("failed"),
            "Should report server not found"
        );
    } else {
        panic!("Expected text content");
    }
}

#[test]
fn test_gateway_initialize_advertises_capabilities() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    let result = client.initialize();
    assert!(result.capabilities.tools.is_some());
    assert!(result.capabilities.resources.is_some());
    assert!(result.capabilities.prompts.is_some());
}

#[test]
fn test_gateway_list_resources() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();
    let result = client.list_resources();

    // The everything server exposes resources
    assert!(
        !result.resources.is_empty(),
        "Everything server should expose resources"
    );
    assert!(result.next_cursor.is_none());
}

#[test]
fn test_gateway_read_resource() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();
    let resources = client.list_resources();
    assert!(
        !resources.resources.is_empty(),
        "Need at least one resource to test read"
    );

    // Read the first resource
    let uri = &resources.resources[0].uri;
    let result = client.read_resource(uri);
    assert!(!result.contents.is_empty(), "Resource should have contents");
}

#[test]
fn test_gateway_list_prompts() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();
    let result = client.list_prompts();

    // The everything server exposes prompts
    assert!(
        !result.prompts.is_empty(),
        "Everything server should expose prompts"
    );
    assert!(result.next_cursor.is_none());
}

#[test]
fn test_gateway_get_prompt() {
    skip_if_no_nodejs!();

    let config = create_test_config();
    let mut client = GatewayClient::spawn(config);

    client.initialize();
    let prompts = client.list_prompts();
    assert!(
        !prompts.prompts.is_empty(),
        "Need at least one prompt to test get"
    );

    // Get the first prompt
    let name = &prompts.prompts[0].name;
    let result = client.get_prompt(name, None);
    assert!(!result.messages.is_empty(), "Prompt should have messages");
}
