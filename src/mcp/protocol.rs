use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Maximum number of pagination pages to fetch before aborting.
/// Prevents infinite loops from buggy upstream servers.
pub const MAX_PAGINATION_PAGES: usize = 100;

/// MCP request methods
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpMethod {
    Initialize,
    ToolsList,
    ToolsCall,
    ResourcesList,
    ResourcesRead,
    PromptsList,
    PromptsGet,
}

impl McpMethod {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Initialize => "initialize",
            Self::ToolsList => "tools/list",
            Self::ToolsCall => "tools/call",
            Self::ResourcesList => "resources/list",
            Self::ResourcesRead => "resources/read",
            Self::PromptsList => "prompts/list",
            Self::PromptsGet => "prompts/get",
        }
    }
}

impl From<McpMethod> for String {
    fn from(method: McpMethod) -> String {
        method.as_str().to_string()
    }
}

/// MCP notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpNotification {
    Initialized,
}

impl McpNotification {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Initialized => "notifications/initialized",
        }
    }
}

impl From<McpNotification> for String {
    fn from(notification: McpNotification) -> String {
        notification.as_str().to_string()
    }
}

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

/// JSON-RPC 2.0 standard error code: method not found
pub const JSONRPC_METHOD_NOT_FOUND: i32 = -32601;
/// JSON-RPC 2.0 standard error code: invalid params
pub const JSONRPC_INVALID_PARAMS: i32 = -32602;
/// Application-defined server error code: entity not found
pub const JSONRPC_NOT_FOUND: i32 = -32002;

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
    /// Instructions describing how to use the server
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
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
    /// Whether the server supports resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    /// Whether the server supports prompts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
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
    /// Opaque cursor for pagination; if present, more results are available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
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

/// A resource definition from an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    /// Unique resource URI
    pub uri: String,
    /// Resource name
    pub name: String,
    /// Human-readable title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Description of the resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type of the resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Contents of a resource returned by resources/read
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContents {
    /// Resource URI
    pub uri: String,
    /// MIME type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Text content (mutually exclusive with blob)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Base64-encoded binary content (mutually exclusive with text)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

/// Response to resources/list request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourcesResult {
    /// Available resources
    pub resources: Vec<Resource>,
    /// Pagination cursor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Response to resources/read request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceResult {
    /// Resource contents
    pub contents: Vec<ResourceContents>,
}

/// Resources capability details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    /// Whether the server supports resource subscriptions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
    /// Whether the resource list can change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
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

/// A prompt definition from an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// Unique prompt identifier
    pub name: String,
    /// Human-readable title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Description of the prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Arguments the prompt accepts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// An argument definition for a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
    /// Argument name
    pub name: String,
    /// Description of the argument
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this argument is required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// A message in a prompt response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMessage {
    /// Role: "user" or "assistant"
    pub role: String,
    /// Message content
    pub content: Content,
}

/// Response to prompts/list request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPromptsResult {
    /// Available prompts
    pub prompts: Vec<Prompt>,
    /// Pagination cursor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Response to prompts/get request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptResult {
    /// Description of the prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Prompt messages
    pub messages: Vec<PromptMessage>,
}

/// Prompts capability details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsCapability {
    /// Whether the prompts list can change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
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
        let request = JsonRpcRequest::new(
            1,
            McpMethod::Initialize.as_str(),
            Some(serde_json::json!({})),
        );
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
                name: crate::config::APP_NAME.to_string(),
                version: "0.1.0".to_string(),
            },
            capabilities: ClientCapabilities::default(),
        };

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains(r#""protocolVersion":"0.1.0""#));
        assert!(json.contains(&format!(r#""name":"{}""#, crate::config::APP_NAME)));
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

    #[test]
    fn test_mcp_method_as_str() {
        assert_eq!(McpMethod::Initialize.as_str(), "initialize");
        assert_eq!(McpMethod::ToolsList.as_str(), "tools/list");
        assert_eq!(McpMethod::ToolsCall.as_str(), "tools/call");
        assert_eq!(McpMethod::ResourcesList.as_str(), "resources/list");
        assert_eq!(McpMethod::ResourcesRead.as_str(), "resources/read");
        assert_eq!(McpMethod::PromptsList.as_str(), "prompts/list");
        assert_eq!(McpMethod::PromptsGet.as_str(), "prompts/get");
    }

    #[test]
    fn test_mcp_method_to_string() {
        assert_eq!(String::from(McpMethod::Initialize), "initialize");
        assert_eq!(String::from(McpMethod::ToolsList), "tools/list");
        assert_eq!(String::from(McpMethod::ToolsCall), "tools/call");
        assert_eq!(String::from(McpMethod::ResourcesList), "resources/list");
        assert_eq!(String::from(McpMethod::ResourcesRead), "resources/read");
        assert_eq!(String::from(McpMethod::PromptsList), "prompts/list");
        assert_eq!(String::from(McpMethod::PromptsGet), "prompts/get");
    }

    #[test]
    fn test_mcp_notification_as_str() {
        assert_eq!(
            McpNotification::Initialized.as_str(),
            "notifications/initialized"
        );
    }

    #[test]
    fn test_mcp_notification_to_string() {
        assert_eq!(
            String::from(McpNotification::Initialized),
            "notifications/initialized"
        );
    }

    #[test]
    fn test_list_tools_result_without_next_cursor() {
        let json = r#"{"tools": []}"#;
        let result: ListToolsResult = serde_json::from_str(json).unwrap();
        assert!(result.tools.is_empty());
        assert!(result.next_cursor.is_none());
    }

    #[test]
    fn test_list_tools_result_with_next_cursor() {
        let json = r#"{"tools": [], "nextCursor": "abc123"}"#;
        let result: ListToolsResult = serde_json::from_str(json).unwrap();
        assert!(result.tools.is_empty());
        assert_eq!(result.next_cursor, Some("abc123".to_string()));
    }

    #[test]
    fn test_list_tools_result_serialization_omits_null_cursor() {
        let result = ListToolsResult {
            tools: vec![],
            next_cursor: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("nextCursor"));
    }

    #[test]
    fn test_list_tools_result_serialization_includes_cursor() {
        let result = ListToolsResult {
            tools: vec![],
            next_cursor: Some("cursor123".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""nextCursor":"cursor123""#));
    }

    #[test]
    fn test_resource_serialization() {
        let resource = Resource {
            uri: "file:///test.txt".to_string(),
            name: "test.txt".to_string(),
            title: Some("Test File".to_string()),
            description: None,
            mime_type: Some("text/plain".to_string()),
        };
        let json = serde_json::to_string(&resource).unwrap();
        assert!(json.contains(r#""uri":"file:///test.txt""#));
        assert!(json.contains(r#""mimeType":"text/plain""#));
        assert!(!json.contains("description")); // skipped when None
    }

    #[test]
    fn test_resource_deserialization_minimal() {
        let json = r#"{"uri": "file:///test.txt", "name": "test.txt"}"#;
        let resource: Resource = serde_json::from_str(json).unwrap();
        assert_eq!(resource.uri, "file:///test.txt");
        assert!(resource.title.is_none());
        assert!(resource.mime_type.is_none());
    }

    #[test]
    fn test_resource_contents_text() {
        let contents = ResourceContents {
            uri: "file:///test.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
            text: Some("hello".to_string()),
            blob: None,
        };
        let json = serde_json::to_string(&contents).unwrap();
        assert!(json.contains(r#""text":"hello""#));
        assert!(!json.contains("blob"));
    }

    #[test]
    fn test_resource_contents_blob() {
        let contents = ResourceContents {
            uri: "file:///image.png".to_string(),
            mime_type: Some("image/png".to_string()),
            text: None,
            blob: Some("base64data".to_string()),
        };
        let json = serde_json::to_string(&contents).unwrap();
        assert!(json.contains(r#""blob":"base64data""#));
        assert!(!json.contains(r#""text""#));
    }

    #[test]
    fn test_list_resources_result_without_cursor() {
        let json = r#"{"resources": []}"#;
        let result: ListResourcesResult = serde_json::from_str(json).unwrap();
        assert!(result.resources.is_empty());
        assert!(result.next_cursor.is_none());
    }

    #[test]
    fn test_list_resources_result_with_cursor() {
        let json = r#"{"resources": [], "nextCursor": "page2"}"#;
        let result: ListResourcesResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.next_cursor, Some("page2".to_string()));
    }

    #[test]
    fn test_read_resource_result_deserialization() {
        let json = r#"{"contents": [{"uri": "file:///test.txt", "text": "hello"}]}"#;
        let result: ReadResourceResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.contents.len(), 1);
        assert_eq!(result.contents[0].text, Some("hello".to_string()));
    }

    #[test]
    fn test_prompt_serialization() {
        let prompt = Prompt {
            name: "code_review".to_string(),
            title: Some("Code Review".to_string()),
            description: Some("Review code quality".to_string()),
            arguments: Some(vec![PromptArgument {
                name: "code".to_string(),
                description: Some("Code to review".to_string()),
                required: Some(true),
            }]),
        };
        let json = serde_json::to_string(&prompt).unwrap();
        assert!(json.contains(r#""name":"code_review""#));
        assert!(json.contains(r#""required":true"#));
    }

    #[test]
    fn test_prompt_deserialization_minimal() {
        let json = r#"{"name": "test_prompt"}"#;
        let prompt: Prompt = serde_json::from_str(json).unwrap();
        assert_eq!(prompt.name, "test_prompt");
        assert!(prompt.arguments.is_none());
    }

    #[test]
    fn test_list_prompts_result_without_cursor() {
        let json = r#"{"prompts": []}"#;
        let result: ListPromptsResult = serde_json::from_str(json).unwrap();
        assert!(result.prompts.is_empty());
        assert!(result.next_cursor.is_none());
    }

    #[test]
    fn test_list_prompts_result_with_cursor() {
        let json = r#"{"prompts": [], "nextCursor": "page2"}"#;
        let result: ListPromptsResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.next_cursor, Some("page2".to_string()));
    }

    #[test]
    fn test_get_prompt_result_deserialization() {
        let json = r#"{
            "description": "A test prompt",
            "messages": [
                {"role": "user", "content": {"type": "text", "text": "Hello"}}
            ]
        }"#;
        let result: GetPromptResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.description, Some("A test prompt".to_string()));
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].role, "user");
    }

    #[test]
    fn test_prompt_message_with_resource_content() {
        let json = r#"{
            "role": "user",
            "content": {
                "type": "resource",
                "uri": "file:///test.txt",
                "text": "file contents"
            }
        }"#;
        let msg: PromptMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "user");
    }

    #[test]
    fn test_server_capabilities_with_resources_and_prompts() {
        let caps = ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(false),
            }),
            resources: Some(ResourcesCapability {
                subscribe: None,
                list_changed: Some(false),
            }),
            prompts: Some(PromptsCapability {
                list_changed: Some(false),
            }),
        };
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("tools"));
        assert!(json.contains("resources"));
        assert!(json.contains("prompts"));
    }

    #[test]
    fn test_server_capabilities_default_omits_optional() {
        let caps = ServerCapabilities::default();
        let json = serde_json::to_string(&caps).unwrap();
        assert!(!json.contains("resources"));
        assert!(!json.contains("prompts"));
    }
}
