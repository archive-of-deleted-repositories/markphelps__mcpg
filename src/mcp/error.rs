use serde_json::Value;
use thiserror::Error;

/// Structured error type for MCP operations
#[derive(Debug, Error)]
#[allow(dead_code)] // Some variants are currently unused but part of the public API
pub enum McpError {
    /// Network error (e.g., connection refused, DNS failure)
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// HTTP error with status code and message
    #[error("HTTP {status} error: {message}")]
    Http { status: u16, message: String },

    /// JSON-RPC protocol error
    #[error("JSON-RPC error (code {code}): {message}")]
    JsonRpc {
        code: i32,
        message: String,
        data: Option<Value>,
    },

    /// Request timeout
    #[error("Timeout after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    /// Invalid or missing session ID
    #[error("Invalid session ID: {0}")]
    InvalidSessionId(String),

    /// Client not initialized
    #[error("Not initialized")]
    NotInitialized,

    /// JSON serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Other/unexpected errors
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

#[allow(dead_code)] // Some methods are currently unused but part of the public API
impl McpError {
    /// Returns true if this error is retryable
    ///
    /// Retryable errors include:
    /// - Network errors (connection issues, DNS failures)
    /// - Timeout errors
    /// - HTTP 5xx server errors
    pub fn is_retryable(&self) -> bool {
        match self {
            McpError::Network(_) | McpError::Timeout { .. } => true,
            McpError::Http { status, .. } => (500..=599).contains(status),
            _ => false,
        }
    }

    /// Returns true if this is a client error (HTTP 4xx)
    ///
    /// Client errors indicate that the request was malformed or invalid
    /// and should not be retried without modification.
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            McpError::Http {
                status: 400..=499,
                ..
            }
        )
    }

    /// Returns true if this is a server error (HTTP 5xx)
    pub fn is_server_error(&self) -> bool {
        matches!(
            self,
            McpError::Http {
                status: 500..=599,
                ..
            }
        )
    }

    /// Create an HTTP error from status code and message
    pub fn http(status: u16, message: impl Into<String>) -> Self {
        McpError::Http {
            status,
            message: message.into(),
        }
    }

    /// Create a JSON-RPC error
    pub fn jsonrpc(code: i32, message: impl Into<String>, data: Option<Value>) -> Self {
        McpError::JsonRpc {
            code,
            message: message.into(),
            data,
        }
    }

    /// Create a timeout error
    pub fn timeout(timeout_secs: u64) -> Self {
        McpError::Timeout { timeout_secs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_network_error_is_retryable() {
        let err = McpError::Network(
            reqwest::Client::new()
                .get("http://invalid-domain-that-does-not-exist-12345.com")
                .send()
                .await
                .unwrap_err(),
        );
        assert!(err.is_retryable());
        assert!(!err.is_client_error());
        assert!(!err.is_server_error());
    }

    #[test]
    fn test_timeout_error_is_retryable() {
        let err = McpError::timeout(30);
        assert!(err.is_retryable());
        assert!(!err.is_client_error());
        assert!(!err.is_server_error());
        assert_eq!(err.to_string(), "Timeout after 30s");
    }

    #[test]
    fn test_http_4xx_is_client_error() {
        let err = McpError::http(404, "Not Found");
        assert!(!err.is_retryable());
        assert!(err.is_client_error());
        assert!(!err.is_server_error());
        assert_eq!(err.to_string(), "HTTP 404 error: Not Found");
    }

    #[test]
    fn test_http_400_is_client_error() {
        let err = McpError::http(400, "Bad Request");
        assert!(err.is_client_error());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_http_499_is_client_error() {
        let err = McpError::http(499, "Client Error");
        assert!(err.is_client_error());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_http_5xx_is_retryable() {
        let err = McpError::http(500, "Internal Server Error");
        assert!(err.is_retryable());
        assert!(!err.is_client_error());
        assert!(err.is_server_error());
        assert_eq!(err.to_string(), "HTTP 500 error: Internal Server Error");
    }

    #[test]
    fn test_http_502_is_retryable() {
        let err = McpError::http(502, "Bad Gateway");
        assert!(err.is_retryable());
        assert!(err.is_server_error());
    }

    #[test]
    fn test_http_599_is_retryable() {
        let err = McpError::http(599, "Server Error");
        assert!(err.is_retryable());
        assert!(err.is_server_error());
    }

    #[test]
    fn test_jsonrpc_error_not_retryable() {
        let err = McpError::jsonrpc(-32600, "Invalid Request", None);
        assert!(!err.is_retryable());
        assert!(!err.is_client_error());
        assert!(!err.is_server_error());
        assert_eq!(
            err.to_string(),
            "JSON-RPC error (code -32600): Invalid Request"
        );
    }

    #[test]
    fn test_jsonrpc_error_with_data() {
        let data = serde_json::json!({"detail": "Missing required parameter"});
        let err = McpError::jsonrpc(-32602, "Invalid params", Some(data));
        assert!(!err.is_retryable());

        // Verify the error displays correctly
        let err_str = err.to_string();
        assert!(err_str.contains("JSON-RPC error"));
        assert!(err_str.contains("-32602"));
        assert!(err_str.contains("Invalid params"));
    }

    #[test]
    fn test_invalid_session_id_not_retryable() {
        let err = McpError::InvalidSessionId("bad-session".to_string());
        assert!(!err.is_retryable());
        assert_eq!(err.to_string(), "Invalid session ID: bad-session");
    }

    #[test]
    fn test_not_initialized_not_retryable() {
        let err = McpError::NotInitialized;
        assert!(!err.is_retryable());
        assert_eq!(err.to_string(), "Not initialized");
    }

    #[test]
    fn test_serialization_error() {
        // Create an invalid JSON serialization scenario
        let err = McpError::Serialization(
            serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err(),
        );
        assert!(!err.is_retryable());
        assert!(err.to_string().contains("Serialization error"));
    }

    #[test]
    fn test_other_error() {
        let err = McpError::Other(anyhow::anyhow!("Unexpected error"));
        assert!(!err.is_retryable());
        assert_eq!(err.to_string(), "Unexpected error");
    }

    #[test]
    fn test_error_classification_mutually_exclusive() {
        // HTTP 4xx
        let err = McpError::http(400, "Bad Request");
        assert!(err.is_client_error());
        assert!(!err.is_server_error());
        assert!(!err.is_retryable());

        // HTTP 5xx
        let err = McpError::http(500, "Server Error");
        assert!(!err.is_client_error());
        assert!(err.is_server_error());
        assert!(err.is_retryable());

        // Other errors
        let err = McpError::NotInitialized;
        assert!(!err.is_client_error());
        assert!(!err.is_server_error());
        assert!(!err.is_retryable());
    }
}
