//! Helper functions for integration tests

use anyhow::Result;
use std::env;

/// Check if integration tests should be skipped
pub fn should_skip_integration_tests() -> bool {
    env::var("SKIP_INTEGRATION_TESTS").is_ok()
}

/// Create a StdioClient configured to use the everything server via npx
///
/// This will spawn the official MCP everything test server using npx.
/// Requires Node.js and npx to be available in PATH.
pub fn create_everything_client() -> mcpg::mcp::stdio::StdioClient {
    use mcpg::mcp::stdio::StdioClient;
    StdioClient::new(
        "npx".to_string(),
        Some(vec![
            "-y".to_string(),
            "@modelcontextprotocol/server-everything".to_string(),
        ]),
        None,
    )
}

/// Create a StdioClient with custom command for error testing
pub fn create_test_client(
    command: String,
    args: Option<Vec<String>>,
) -> mcpg::mcp::stdio::StdioClient {
    use mcpg::mcp::stdio::StdioClient;
    StdioClient::new(command, args, None)
}

/// Initialize client and verify successful connection
///
/// This performs basic validation on the initialization result:
/// - Protocol version matches expected
/// - Server info is not empty
pub async fn init_and_verify(
    client: &mut mcpg::mcp::stdio::StdioClient,
) -> Result<mcpg::mcp::protocol::InitializeResult> {
    use mcpg::mcp::client::McpClient;
    use mcpg::mcp::protocol::PROTOCOL_VERSION;
    let result = client.initialize().await?;
    assert_eq!(result.protocol_version, PROTOCOL_VERSION);
    assert!(!result.server_info.name.is_empty());
    Ok(result)
}

/// Cleanup helper - ensures client is properly shutdown
///
/// This ignores errors during cleanup to avoid failing tests
/// due to cleanup issues when the test itself passed.
pub async fn cleanup_client(client: &mut mcpg::mcp::stdio::StdioClient) {
    use mcpg::mcp::client::McpClient;
    let _ = client.shutdown().await; // Ignore errors during cleanup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_everything_client() {
        let client = create_everything_client();
        // Just verify it can be created without panicking
        drop(client);
    }

    #[test]
    fn test_create_custom_client() {
        let client = create_test_client("echo".to_string(), Some(vec!["test".to_string()]));
        drop(client);
    }
}
