use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// Always "2.0"
    pub jsonrpc: String,
    /// Request identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    /// Method name
    pub method: String,
    /// Method parameters (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// Always "2.0"
    pub jsonrpc: String,
    /// Matches the request ID
    pub id: RequestId,
    /// Success result (mutually exclusive with error)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error details (mutually exclusive with result)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Human-readable error message
    pub message: String,
    /// Additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Request ID can be a number or string
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

impl From<i64> for RequestId {
    fn from(n: i64) -> Self {
        RequestId::Number(n)
    }
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        RequestId::String(s)
    }
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC request
    #[allow(dead_code)] // Used in tests
    pub fn new(id: impl Into<RequestId>, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id.into()),
            method: method.into(),
            params,
        }
    }
}

// MCP-specific types

/// MCP initialization parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// MCP protocol version
    pub protocol_version: String,
    /// Information about the client
    pub client_info: ClientInfo,
    /// Client capabilities
    pub capabilities: ClientCapabilities,
}

/// Information about the MCP client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    /// Client name
    pub name: String,
    /// Client version
    pub version: String,
}

/// Client capabilities (what the client supports)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    // For now, empty - can be extended later
}

/// MCP initialization response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Server's protocol version
    pub protocol_version: String,
    /// Information about the server
    pub server_info: ServerInfo,
    /// Server capabilities
    pub capabilities: ServerCapabilities,
}

/// Information about the MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    /// Server name
    pub name: String,
    /// Server version
    pub version: String,
}

/// Server capabilities (what the server supports)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// Whether the server supports tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
}

/// Tools capability details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    /// Whether tools list can change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Response to tools/list request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResult {
    /// Available tools
    pub tools: Vec<Tool>,
}

/// A tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// Unique tool identifier
    pub name: String,
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for tool parameters
    pub input_schema: Value,
}

impl Tool {
    /// Extract required parameter names from the input schema
    pub fn required_params(&self) -> Vec<String> {
        self.input_schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract optional parameter names from the input schema
    pub fn optional_params(&self) -> Vec<String> {
        let required = self.required_params();
        self.input_schema
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|props| {
                props
                    .keys()
                    .filter(|k| !required.contains(k))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Parameters for calling a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolParams {
    /// Tool name to call
    pub name: String,
    /// Tool arguments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// Result from calling a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    /// Result content blocks
    pub content: Vec<Content>,
    /// Whether this is an error result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Content block in a result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
    /// Plain text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content (base64 encoded)
    #[serde(rename = "image", rename_all = "camelCase")]
    Image { data: String, mime_type: String },
    /// Resource reference
    #[serde(rename = "resource", rename_all = "camelCase")]
    Resource {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_number() {
        let id = RequestId::from(42);
        assert_eq!(id, RequestId::Number(42));
    }

    #[test]
    fn test_request_id_string() {
        let id = RequestId::from("test-id".to_string());
        assert_eq!(id, RequestId::String("test-id".to_string()));
    }

    #[test]
    fn test_jsonrpc_request_creation() {
        let request = JsonRpcRequest::new(1, "test_method", None);
        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, Some(RequestId::Number(1)));
        assert_eq!(request.method, "test_method");
        assert!(request.params.is_none());
    }

    #[test]
    fn test_jsonrpc_request_serialization() {
        let request = JsonRpcRequest::new(1, "initialize", Some(serde_json::json!({})));
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""method":"initialize""#));
    }

    #[test]
    fn test_jsonrpc_response_deserialization_success() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"status": "ok"}
        }"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, RequestId::Number(1));
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_jsonrpc_response_deserialization_error() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32600,
                "message": "Invalid Request"
            }
        }"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, RequestId::Number(1));
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32600);
        assert_eq!(error.message, "Invalid Request");
    }

    #[test]
    fn test_tool_required_params() {
        let tool = Tool {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "param1": {"type": "string"},
                    "param2": {"type": "number"},
                    "param3": {"type": "boolean"}
                },
                "required": ["param1", "param2"]
            }),
        };

        let required = tool.required_params();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&"param1".to_string()));
        assert!(required.contains(&"param2".to_string()));
    }

    #[test]
    fn test_tool_optional_params() {
        let tool = Tool {
            name: "test_tool".to_string(),
            description: Some("A test tool".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "param1": {"type": "string"},
                    "param2": {"type": "number"},
                    "param3": {"type": "boolean"}
                },
                "required": ["param1"]
            }),
        };

        let optional = tool.optional_params();
        assert_eq!(optional.len(), 2);
        assert!(optional.contains(&"param2".to_string()));
        assert!(optional.contains(&"param3".to_string()));
    }

    #[test]
    fn test_initialize_params_serialization() {
        let params = InitializeParams {
            protocol_version: "0.1.0".to_string(),
            client_info: ClientInfo {
                name: "mcpg".to_string(),
                version: "0.1.0".to_string(),
            },
            capabilities: ClientCapabilities::default(),
        };

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains(r#""protocolVersion":"0.1.0""#));
        assert!(json.contains(r#""name":"mcpg""#));
    }

    #[test]
    fn test_content_text_serialization() {
        let content = Content::Text {
            text: "Hello, world!".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains(r#""type":"text""#));
        assert!(json.contains(r#""text":"Hello, world!""#));
    }

    #[test]
    fn test_content_image_serialization() {
        let content = Content::Image {
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains(r#""type":"image""#));
        assert!(json.contains(r#""mimeType":"image/png""#));
    }
}
