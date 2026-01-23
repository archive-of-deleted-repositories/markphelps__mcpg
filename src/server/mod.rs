//! MCP Gateway Server implementation
//!
//! This module implements mcpg as an MCP server that acts as a gateway
//! to upstream MCP servers, providing progressive tool discovery.

pub mod handler;
pub mod search;
pub mod stdio;
pub mod tools;

pub use search::CachedTool;
pub use stdio::run;

use crate::config::Config;
use crate::mcp::client::{McpClient, create_client};
use anyhow::Result;
use lru::LruCache;
use search::{IndexedSearch, ToolSearch};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use tracing::{Instrument, debug_span, info_span, warn};

/// Tool cache with reverse index and LRU query cache
pub struct ToolCache {
    /// Whether the cache has been populated
    is_populated: bool,
    /// Search implementation with reverse index (trait-based for future extensibility)
    searcher: Box<dyn ToolSearch>,
    /// LRU cache for search results (query -> indices) - still useful for complex queries
    search_cache: LruCache<String, Vec<usize>>,
}

impl ToolCache {
    /// Create a new empty cache with default indexed search
    pub fn new() -> Self {
        Self::with_capacity(100)
    }

    /// Create a new cache with specific LRU capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            is_populated: false,
            searcher: Box::new(IndexedSearch::new()),
            search_cache: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
        }
    }

    /// Create a cache with a custom search implementation
    #[allow(dead_code)]
    pub fn with_searcher(searcher: Box<dyn ToolSearch>) -> Self {
        Self {
            is_populated: false,
            searcher,
            search_cache: LruCache::new(NonZeroUsize::new(100).unwrap()),
        }
    }

    /// Populate the cache with tools (builds reverse index)
    pub fn populate(&mut self, tools: Vec<CachedTool>) {
        self.searcher.index(tools);
        self.is_populated = true;
        self.search_cache.clear(); // Clear LRU cache on re-index
    }

    /// Find a tool by server name and tool name
    pub fn find_tool(&self, server_name: &str, tool_name: &str) -> Option<&CachedTool> {
        self.searcher
            .tools()
            .iter()
            .find(|cached| cached.server_name == server_name && cached.tool.name == tool_name)
    }

    /// Search for tools matching a query (with LRU caching + reverse index)
    pub fn search(&mut self, query: &str) -> Vec<&CachedTool> {
        // Check LRU cache first
        if let Some(indices) = self.search_cache.get(query) {
            return indices
                .iter()
                .map(|&idx| &self.searcher.tools()[idx])
                .collect();
        }

        // Use reverse index for O(k) lookup
        let indices = self.searcher.search(query);

        // Cache the result
        self.search_cache.put(query.to_string(), indices.clone());

        // Return tools
        indices
            .iter()
            .map(|&idx| &self.searcher.tools()[idx])
            .collect()
    }

    /// Get all tools for a specific server (O(1) lookup via server index)
    pub fn tools_for_server(&self, server_name: &str) -> Vec<&CachedTool> {
        if let Some(indices) = self.searcher.tools_for_server(server_name) {
            indices
                .iter()
                .map(|&idx| &self.searcher.tools()[idx])
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Clear the search cache (useful when tools are re-indexed)
    #[allow(dead_code)] // Reserved for future cache invalidation
    pub fn clear_search_cache(&mut self) {
        self.search_cache.clear();
    }
}

impl Default for ToolCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Gateway server state
pub struct GatewayServer {
    /// Configuration with upstream server definitions
    config: Config,
    /// Lazy-loaded tool cache
    pub(crate) cache: ToolCache,
    /// Active connections to upstream servers
    clients: HashMap<String, Box<dyn McpClient>>,
}

impl GatewayServer {
    /// Create a new gateway server with the given configuration
    pub fn new(config: Config) -> Self {
        Self {
            config,
            cache: ToolCache::new(),
            clients: HashMap::new(),
        }
    }

    /// Ensure the tool cache is populated
    ///
    /// This connects to all upstream servers and caches their tools.
    /// Only runs once - subsequent calls return immediately.
    pub async fn ensure_cache_populated(&mut self) -> Result<()> {
        async move {
            if self.cache.is_populated {
                return Ok(());
            }

            let mut all_tools = Vec::new();

            for (server_name, server_config) in self.config.servers() {
                let span = debug_span!("upstream_init", server = %server_name);
                let _enter = span.enter();

                // Create client using factory function
                let mut client = create_client(server_config)?;

                // Initialize and get tools
                client.initialize().await?;
                let tools = client.list_tools().await?;

                // Store client for reuse
                self.clients.insert(server_name.clone(), client);

                // Cache tools with server name
                for tool in tools {
                    all_tools.push(CachedTool {
                        server_name: server_name.clone(),
                        tool,
                    });
                }
            }

            // Populate cache with tools (builds reverse index)
            self.cache.populate(all_tools);
            Ok(())
        }
        .instrument(info_span!("cache_populate"))
        .await
    }

    /// Get or create a client for the given server
    pub async fn get_or_create_client(
        &mut self,
        server_name: &str,
    ) -> Result<&mut (dyn McpClient + '_)> {
        let span = debug_span!("get_or_create_client", server = %server_name);
        let _enter = span.enter();

        // If client exists, return it
        if self.clients.contains_key(server_name) {
            return Ok(self.clients.get_mut(server_name).unwrap().as_mut());
        }

        // Get server config
        let server_config = self.config.get_server(server_name).ok_or_else(|| {
            anyhow::anyhow!("Server '{}' not found in configuration", server_name)
        })?;

        // Create client using factory function
        let mut client = create_client(server_config)?;

        // Initialize the client
        client.initialize().await?;

        // Store and return
        self.clients.insert(server_name.to_string(), client);
        Ok(self.clients.get_mut(server_name).unwrap().as_mut())
    }

    /// Get the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Shutdown all upstream client connections
    pub async fn shutdown(&mut self) -> Result<()> {
        for (name, client) in self.clients.iter_mut() {
            if let Err(e) = client.shutdown().await {
                warn!(client = %name, error = %e, "Failed to shutdown client");
            }
        }
        self.clients.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::Tool;

    fn create_test_tool(server: &str, name: &str, desc: &str) -> CachedTool {
        CachedTool {
            server_name: server.to_string(),
            tool: Tool {
                name: name.to_string(),
                description: Some(desc.to_string()),
                input_schema: serde_json::json!({}),
            },
        }
    }

    #[test]
    fn test_tool_cache_new() {
        let cache = ToolCache::new();
        assert!(!cache.is_populated);
    }

    #[test]
    fn test_tool_cache_with_capacity() {
        let cache = ToolCache::with_capacity(50);
        assert!(!cache.is_populated);
    }

    #[test]
    fn test_tool_cache_find_tool() {
        let mut cache = ToolCache::new();
        let tools = vec![create_test_tool("test-server", "test-tool", "A test tool")];
        cache.populate(tools);

        let found = cache.find_tool("test-server", "test-tool");
        assert!(found.is_some());
        assert_eq!(found.unwrap().tool.name, "test-tool");

        let not_found = cache.find_tool("test-server", "nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_tool_cache_search() {
        let mut cache = ToolCache::new();
        let tools = vec![
            create_test_tool("github", "list_issues", "List issues in a repository"),
            create_test_tool("github", "create_issue", "Create a new issue"),
            create_test_tool("slack", "send_message", "Send a message to a channel"),
        ];
        cache.populate(tools);

        // Search by server name
        let results = cache.search("github");
        assert_eq!(results.len(), 2);

        // Search by tool name
        let results = cache.search("issue");
        assert_eq!(results.len(), 2);

        // Search by description
        let results = cache.search("message");
        assert_eq!(results.len(), 1);

        // Case insensitive
        let results = cache.search("GITHUB");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_tool_cache_search_caching() {
        let mut cache = ToolCache::new();
        let tools = vec![create_test_tool("github", "list_issues", "List issues")];
        cache.populate(tools);

        // First search - not in LRU cache, uses reverse index
        let results1 = cache.search("github");
        assert_eq!(results1.len(), 1);

        // Second search - should hit LRU cache
        let results2 = cache.search("github");
        assert_eq!(results2.len(), 1);

        // Clear LRU cache and search again - uses reverse index again
        cache.clear_search_cache();
        let results3 = cache.search("github");
        assert_eq!(results3.len(), 1);
    }

    #[test]
    fn test_tool_cache_tools_for_server() {
        let mut cache = ToolCache::new();
        let tools = vec![
            create_test_tool("github", "list_issues", "List issues"),
            create_test_tool("slack", "send_message", "Send message"),
        ];
        cache.populate(tools);

        let github_tools = cache.tools_for_server("github");
        assert_eq!(github_tools.len(), 1);

        let slack_tools = cache.tools_for_server("slack");
        assert_eq!(slack_tools.len(), 1);

        let nonexistent = cache.tools_for_server("nonexistent");
        assert_eq!(nonexistent.len(), 0);
    }

    #[test]
    fn test_gateway_server_new() {
        let config = Config {
            servers: HashMap::new(),
        };
        let server = GatewayServer::new(config);
        assert!(!server.cache.is_populated);
        assert_eq!(server.clients.len(), 0);
    }
}
