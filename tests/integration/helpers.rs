//! Helper functions for integration tests
//!
//! ## Usage
//!
//! Tests should call `init_test_tracing()` at the start to suppress tracing output
//! while keeping the tracing infrastructure functional. This is especially important
//! to prevent noise during `cargo test` runs.
//!
//! ```rust,no_run
//! use super::helpers::init_test_tracing;
//!
//! #[tokio::test]
//! async fn my_test() {
//!     init_test_tracing();
//!     // ... test code
//! }
//! ```

use anyhow::Result;
use std::env;
use std::sync::Once;
use std::time::Duration;

static INIT: Once = Once::new();

/// Initialize tracing for tests with a no-output subscriber
///
/// This ensures tests don't produce tracing output but the tracing
/// infrastructure is properly initialized. Uses Once to ensure
/// it's only called once per test run.
pub fn init_test_tracing() {
    INIT.call_once(|| {
        // Install a subscriber that captures but doesn't print
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer() // Captures but doesn't print during tests
            .try_init();
    });
}

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
        Duration::from_secs(30),
    )
}

/// Create a StdioClient with custom command for error testing
pub fn create_test_client(
    command: String,
    args: Option<Vec<String>>,
) -> mcpg::mcp::stdio::StdioClient {
    use mcpg::mcp::stdio::StdioClient;
    StdioClient::new(command, args, None, Duration::from_secs(30))
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
    fn test_init_test_tracing() {
        // Should not panic and can be called multiple times
        init_test_tracing();
        init_test_tracing();
    }

    #[test]
    fn test_create_everything_client() {
        init_test_tracing();
        let client = create_everything_client();
        // Just verify it can be created without panicking
        drop(client);
    }

    #[test]
    fn test_create_custom_client() {
        init_test_tracing();
        let client = create_test_client("echo".to_string(), Some(vec!["test".to_string()]));
        drop(client);
    }
}
