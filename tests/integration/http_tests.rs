//! Integration tests for HttpClient with the MCP everything server

use mcpg::config::{ServerConfig, TransportConfig};
use mcpg::mcp::client::create_client;
use mcpg::mcp::protocol::PROTOCOL_VERSION;

// Import the HTTP server helper
#[path = "../test_utils/http_server.rs"]
mod http_server;
use http_server::TestHttpServer;

/// Helper to create test config
fn create_test_config(url: String) -> ServerConfig {
    ServerConfig {
        transport: TransportConfig::Http { url, headers: None },
        timeout_secs: 30,
    }
}

#[tokio::test]
async fn test_http_initialize() {
    let server = match TestHttpServer::start() {
        Some(s) => s,
        None => {
            eprintln!("Skipping HTTP test - server not available");
            return;
        }
    };

    let config = create_test_config(server.url.clone());
    let mut client = create_client(&config).unwrap();

    let result = client.initialize().await;
    assert!(result.is_ok());

    let init_result = result.unwrap();
    assert_eq!(init_result.protocol_version, PROTOCOL_VERSION);
}

#[tokio::test]
async fn test_http_list_tools() {
    let server = match TestHttpServer::start() {
        Some(s) => s,
        None => {
            eprintln!("Skipping HTTP test - server not available");
            return;
        }
    };

    let config = create_test_config(server.url.clone());
    let mut client = create_client(&config).unwrap();

    let tools = client.list_tools().await.unwrap();
    assert!(!tools.is_empty());
}

#[tokio::test]
async fn test_http_call_tool() {
    let server = match TestHttpServer::start() {
        Some(s) => s,
        None => {
            eprintln!("Skipping HTTP test - server not available");
            return;
        }
    };

    let config = create_test_config(server.url.clone());
    let mut client = create_client(&config).unwrap();

    // Use echo tool from server-everything
    let result = client
        .call_tool("echo", serde_json::json!({"message": "test"}))
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_http_initialization_caching() {
    let server = match TestHttpServer::start() {
        Some(s) => s,
        None => {
            eprintln!("Skipping HTTP test - server not available");
            return;
        }
    };

    let config = create_test_config(server.url.clone());
    let mut client = create_client(&config).unwrap();

    // First initialize
    let result1 = client.initialize().await.unwrap();

    // Second initialize should return cached result
    let result2 = client.initialize().await.unwrap();

    assert_eq!(result1.protocol_version, result2.protocol_version);
    assert_eq!(result1.server_info.name, result2.server_info.name);
}

#[tokio::test]
async fn test_http_custom_headers() {
    let server = match TestHttpServer::start() {
        Some(s) => s,
        None => {
            eprintln!("Skipping HTTP test - server not available");
            return;
        }
    };

    use std::collections::HashMap;
    let mut headers = HashMap::new();
    headers.insert("X-Custom-Header".to_string(), "test-value".to_string());

    let config = ServerConfig {
        transport: TransportConfig::Http {
            url: server.url.clone(),
            headers: Some(headers),
        },
        timeout_secs: 30,
    };

    let mut client = create_client(&config).unwrap();

    // Should succeed even with custom headers
    let result = client.initialize().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_http_shutdown() {
    let server = match TestHttpServer::start() {
        Some(s) => s,
        None => {
            eprintln!("Skipping HTTP test - server not available");
            return;
        }
    };

    let config = create_test_config(server.url.clone());
    let mut client = create_client(&config).unwrap();

    client.initialize().await.unwrap();

    // Shutdown should succeed
    let result = client.shutdown().await;
    assert!(result.is_ok());
}
