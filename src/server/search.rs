//! Search functionality for tool discovery
//!
//! Uses a reverse index (inverted index) for efficient O(k) lookups
//! where k = number of query terms, rather than O(n) linear scans.

use crate::mcp::protocol::Tool;
use std::collections::{HashMap, HashSet};

/// A tool with server context for search results
#[derive(Debug, Clone)]
pub struct CachedTool {
    /// Name of the server this tool comes from
    pub server_name: String,
    /// The tool definition
    pub tool: Tool,
}

/// Trait for searching tools
pub trait ToolSearch: Send + Sync {
    /// Index tools for efficient searching
    fn index(&mut self, tools: Vec<CachedTool>);

    /// Search for tools matching the query
    fn search(&self, query: &str) -> Vec<usize>;

    /// Get reference to indexed tools
    fn tools(&self) -> &[CachedTool];

    /// Get tool indices for a specific server (O(1) lookup)
    fn tools_for_server(&self, server_name: &str) -> Option<&[usize]>;
}

/// Tokenize text into searchable words
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

/// Simple reverse index search implementation (MVP)
#[derive(Debug, Default)]
pub struct IndexedSearch {
    /// All tools
    tools: Vec<CachedTool>,
    /// Reverse index: word -> tool indices
    word_index: HashMap<String, Vec<usize>>,
    /// Server name -> tool indices (for fast exact server lookups)
    server_index: HashMap<String, Vec<usize>>,
}

impl IndexedSearch {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ToolSearch for IndexedSearch {
    fn index(&mut self, tools: Vec<CachedTool>) {
        self.tools = tools;
        self.word_index.clear();
        self.server_index.clear();

        for (idx, tool) in self.tools.iter().enumerate() {
            // Index by server name (exact match)
            self.server_index
                .entry(tool.server_name.to_lowercase())
                .or_default()
                .push(idx);

            // Index by words in server name
            for word in tokenize(&tool.server_name) {
                self.word_index.entry(word).or_default().push(idx);
            }

            // Index by words in tool name
            for word in tokenize(&tool.tool.name) {
                self.word_index.entry(word).or_default().push(idx);
            }

            // Index by words in description
            if let Some(desc) = &tool.tool.description {
                for word in tokenize(desc) {
                    self.word_index.entry(word).or_default().push(idx);
                }
            }
        }
    }

    fn search(&self, query: &str) -> Vec<usize> {
        let query_lower = query.to_lowercase();

        // Fast path: exact server name match
        if let Some(indices) = self.server_index.get(&query_lower) {
            return indices.clone();
        }

        // Tokenize query
        let tokens = tokenize(&query_lower);
        if tokens.is_empty() {
            return vec![];
        }

        // For each token, find all matching words (prefix match)
        // This allows "issue" to match "issues", "issuing", etc.
        fn find_matching_indices(
            word_index: &HashMap<String, Vec<usize>>,
            token: &str,
        ) -> HashSet<usize> {
            let mut matches = HashSet::new();
            for (word, indices) in word_index {
                // Match if word starts with token or token starts with word
                if word.starts_with(token) || token.starts_with(word) {
                    matches.extend(indices.iter().copied());
                }
            }
            matches
        }

        // Single token: prefix matching lookup
        if tokens.len() == 1 {
            return find_matching_indices(&self.word_index, &tokens[0])
                .into_iter()
                .collect();
        }

        // Multiple tokens: intersection (AND logic) with prefix matching
        let mut result = find_matching_indices(&self.word_index, &tokens[0]);

        for token in &tokens[1..] {
            let token_matches = find_matching_indices(&self.word_index, token);
            result.retain(|idx| token_matches.contains(idx));
            if result.is_empty() {
                return vec![];
            }
        }

        result.into_iter().collect()
    }

    fn tools(&self) -> &[CachedTool] {
        &self.tools
    }

    fn tools_for_server(&self, server_name: &str) -> Option<&[usize]> {
        self.server_index
            .get(&server_name.to_lowercase())
            .map(|v| v.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_tool(server: &str, name: &str, desc: &str) -> CachedTool {
        CachedTool {
            server_name: server.to_string(),
            tool: Tool {
                name: name.to_string(),
                description: Some(desc.to_string()),
                input_schema: json!({}),
            },
        }
    }

    #[test]
    fn test_indexed_search_by_server() {
        let tools = vec![
            create_test_tool("github", "list_issues", "List issues"),
            create_test_tool("slack", "send_message", "Send message"),
        ];

        let mut search = IndexedSearch::new();
        search.index(tools);
        let results = search.search("github");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 0);
    }

    #[test]
    fn test_indexed_search_by_tool_name() {
        let tools = vec![
            create_test_tool("github", "list_issues", "List issues"),
            create_test_tool("github", "create_issue", "Create issue"),
            create_test_tool("slack", "send_message", "Send message"),
        ];

        let mut search = IndexedSearch::new();
        search.index(tools);
        let results = search.search("issue");

        assert_eq!(results.len(), 2);
        assert!(results.contains(&0));
        assert!(results.contains(&1));
    }

    #[test]
    fn test_indexed_search_by_description() {
        let tools = vec![
            create_test_tool("github", "list_issues", "List repository issues"),
            create_test_tool("slack", "send_message", "Send a message"),
        ];

        let mut search = IndexedSearch::new();
        search.index(tools);
        let results = search.search("repository");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 0);
    }

    #[test]
    fn test_indexed_search_case_insensitive() {
        let tools = vec![create_test_tool("github", "list_issues", "List issues")];

        let mut search = IndexedSearch::new();
        search.index(tools);
        let results = search.search("GITHUB");

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_indexed_search_no_matches() {
        let tools = vec![create_test_tool("github", "list_issues", "List issues")];

        let mut search = IndexedSearch::new();
        search.index(tools);
        let results = search.search("nonexistent");

        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_indexed_search_multi_token() {
        let tools = vec![
            create_test_tool("github", "list_issues", "List repository issues"),
            create_test_tool("github", "create_issue", "Create new issue"),
            create_test_tool("slack", "send_message", "Send channel message"),
        ];

        let mut search = IndexedSearch::new();
        search.index(tools);

        // Both "github" and "issue" - should match both github tools
        let results = search.search("github issue");
        assert_eq!(results.len(), 2);

        // "repository issue" - should match only first tool
        let results = search.search("repository issue");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], 0);
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("list-issues");
        assert_eq!(tokens, vec!["list", "issues"]);

        let tokens = tokenize("CamelCase");
        assert_eq!(tokens, vec!["camelcase"]);

        let tokens = tokenize("with  spaces");
        assert_eq!(tokens, vec!["with", "spaces"]);
    }
}
