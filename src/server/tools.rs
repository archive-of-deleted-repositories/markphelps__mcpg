use crate::mcp::protocol::Tool;
use serde_json::json;

/// Get the list of gateway tools exposed by the server
pub fn gateway_tools() -> Vec<Tool> {
    vec![search_tool(), inspect_tool(), call_tool(), list_tool()]
}

/// Search tool - find tools across all upstream servers
fn search_tool() -> Tool {
    Tool {
        name: "search".to_string(),
        description: Some("Search for tools across all configured MCP servers".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to match against tool names, descriptions, and server names"
                }
            },
            "required": ["query"]
        }),
    }
}

/// Inspect tool - get detailed schema for a specific tool
fn inspect_tool() -> Tool {
    Tool {
        name: "inspect".to_string(),
        description: Some("Get detailed schema and documentation for a specific tool".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Name of the server hosting the tool"
                },
                "tool": {
                    "type": "string",
                    "description": "Name of the tool to inspect"
                }
            },
            "required": ["server", "tool"]
        }),
    }
}

/// Call tool - forward a tool call to an upstream server
fn call_tool() -> Tool {
    Tool {
        name: "call".to_string(),
        description: Some("Call a tool on an upstream MCP server".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Name of the server hosting the tool"
                },
                "tool": {
                    "type": "string",
                    "description": "Name of the tool to call"
                },
                "args": {
                    "type": "object",
                    "description": "Arguments to pass to the tool (optional)"
                }
            },
            "required": ["server", "tool"]
        }),
    }
}

/// List tool - list available servers or tools on a server
fn list_tool() -> Tool {
    Tool {
        name: "list".to_string(),
        description: Some("List available servers or tools on a specific server".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Name of the server to list tools from (optional - if not provided, lists all servers)"
                }
            },
            "required": []
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_tools_count() {
        let tools = gateway_tools();
        assert_eq!(tools.len(), 4, "Should have exactly 4 gateway tools");
    }

    #[test]
    fn test_search_tool_schema() {
        let tool = search_tool();
        assert_eq!(tool.name, "search");
        assert!(tool.description.is_some());

        let required = tool.required_params();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "query");
    }

    #[test]
    fn test_inspect_tool_schema() {
        let tool = inspect_tool();
        assert_eq!(tool.name, "inspect");
        assert!(tool.description.is_some());

        let required = tool.required_params();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&"server".to_string()));
        assert!(required.contains(&"tool".to_string()));
    }

    #[test]
    fn test_call_tool_schema() {
        let tool = call_tool();
        assert_eq!(tool.name, "call");
        assert!(tool.description.is_some());

        let required = tool.required_params();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&"server".to_string()));
        assert!(required.contains(&"tool".to_string()));

        let optional = tool.optional_params();
        assert!(optional.contains(&"args".to_string()));
    }

    #[test]
    fn test_list_tool_schema() {
        let tool = list_tool();
        assert_eq!(tool.name, "list");
        assert!(tool.description.is_some());

        let required = tool.required_params();
        assert_eq!(required.len(), 0, "list tool has no required params");

        let optional = tool.optional_params();
        assert!(optional.contains(&"server".to_string()));
    }

    #[test]
    fn test_all_tools_have_descriptions() {
        let tools = gateway_tools();
        for tool in tools {
            assert!(
                tool.description.is_some(),
                "Tool {} should have a description",
                tool.name
            );
        }
    }
}
