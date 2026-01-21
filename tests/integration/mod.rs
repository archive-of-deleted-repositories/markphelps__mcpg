//! Integration tests for MCP stdio client
//!
//! These tests require Node.js and npx to be available in the PATH.
//! They use the official @modelcontextprotocol/server-everything test server.
//!
//! To skip these tests if Node.js is not available:
//! ```bash
//! SKIP_INTEGRATION_TESTS=1 cargo test
//! ```

mod helpers;
mod stdio_tests;
