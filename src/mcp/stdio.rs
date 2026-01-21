use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

use super::client::McpClient;
use super::protocol::{CallToolResult, InitializeResult, JsonRpcRequest, JsonRpcResponse, Tool};

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

    // Request ID counter
    next_id: AtomicI64,

    // Connection state
    initialized: bool,
}

impl StdioClient {
    /// Create a new stdio client (doesn't spawn the process yet)
    pub fn new(
        command: String,
        args: Option<Vec<String>>,
        env: Option<HashMap<String, String>>,
    ) -> Self {
        Self {
            command,
            args,
            env,
            child: None,
            stdin: None,
            stdout: None,
            next_id: AtomicI64::new(1),
            initialized: false,
        }
    }

    /// Spawn the subprocess and capture stdin/stdout
    ///
    /// TODO: Implement this method
    /// - Use tokio::process::Command
    /// - Set stdin(Stdio::piped()) and stdout(Stdio::piped())
    /// - Apply environment variables
    /// - Call .spawn()
    /// - Extract and store stdin/stdout handles
    fn spawn(&mut self) -> Result<()> {
        todo!("Implement process spawning")
    }

    /// Send a JSON-RPC request to the server
    ///
    /// TODO: Implement this method
    /// - Create JsonRpcRequest with next ID
    /// - Serialize to JSON string
    /// - Add newline character
    /// - Write to stdin
    /// - Flush stdin
    async fn send_request(&mut self, method: &str, params: Option<serde_json::Value>) -> Result<i64> {
        todo!("Implement request sending")
    }

    /// Read a JSON-RPC response from the server
    ///
    /// TODO: Implement this method
    /// - Use BufReader::read_line() to read until newline
    /// - Parse JSON into JsonRpcResponse
    /// - Check for errors in response
    /// - Return result or error
    async fn read_response(&mut self) -> Result<JsonRpcResponse> {
        todo!("Implement response reading")
    }

    /// Get the next request ID
    fn next_request_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }
}

#[async_trait]
impl McpClient for StdioClient {
    async fn initialize(&mut self) -> Result<InitializeResult> {
        // TODO: Implement initialize
        // 1. Check if already initialized
        // 2. If not spawned, call spawn()
        // 3. Send initialize request with client info
        // 4. Read response
        // 5. Parse InitializeResult from response.result
        // 6. Set initialized = true
        todo!("Implement initialize method")
    }

    async fn list_tools(&mut self) -> Result<Vec<Tool>> {
        // TODO: Implement list_tools
        // 1. Ensure initialized (call initialize if needed)
        // 2. Send "tools/list" request
        // 3. Read response
        // 4. Parse tools array from response.result
        todo!("Implement list_tools method")
    }

    async fn call_tool(&mut self, name: &str, args: serde_json::Value) -> Result<CallToolResult> {
        // TODO: Implement call_tool
        // 1. Ensure initialized
        // 2. Send "tools/call" request with CallToolParams
        // 3. Read response
        // 4. Parse CallToolResult from response.result
        todo!("Implement call_tool method")
    }

    async fn shutdown(&mut self) -> Result<()> {
        // TODO: Implement shutdown
        // 1. Send any cleanup messages if needed
        // 2. Kill child process
        // 3. Wait for process to exit
        todo!("Implement shutdown method")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_client_creation() {
        let client = StdioClient::new(
            "test-command".to_string(),
            Some(vec!["arg1".to_string()]),
            None,
        );
        assert_eq!(client.command, "test-command");
        assert!(!client.initialized);
    }

    #[test]
    fn test_request_id_increment() {
        let client = StdioClient::new("test".to_string(), None, None);
        let id1 = client.next_request_id();
        let id2 = client.next_request_id();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    // TODO: Add more tests once methods are implemented
    // - Test spawning a simple echo process
    // - Test sending/receiving JSON-RPC messages
    // - Test error handling
}
