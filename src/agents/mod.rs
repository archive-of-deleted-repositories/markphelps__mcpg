//! Agent detection and configuration management
//!
//! This module provides a registry-based system for discovering and configuring
//! AI agents that use MCP servers. Agents self-register using the `register_agents!`
//! macro, making it easy to add support for new agents.
//!
//! # Adding a New Agent
//!
//! 1. Create a new module (e.g., `cursor.rs`) implementing the `Agent` trait
//! 2. Add a `register()` function that calls `registry::register_agent()`
//! 3. Add the module name to `register_agents!(claude_code, cursor);`
//!
//! # Example
//!
//! ```rust,ignore
//! // In src/agents/cursor.rs
//! use once_cell::sync::Lazy;
//! use super::{registry, Agent};
//!
//! static REGISTER: Lazy<()> = Lazy::new(|| {
//!     registry::register_agent(|| Box::new(CursorAgent::new()));
//! });
//!
//! pub fn register() {
//!     Lazy::force(&REGISTER);
//! }
//! ```

use crate::config::ServerConfig;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Tracks where a server config came from
#[derive(Debug, Clone)]
pub struct ServerWithSource {
    pub config: ServerConfig,
    pub source: PathBuf,
}

mod config_file;
mod registry;
pub(crate) mod shared;

/// Macro to register agent modules
/// Usage: register_agents!(claude_code, cursor);
macro_rules! register_agents {
    ($($module:ident),* $(,)?) => {
        $(
            mod $module;
        )*

        fn init_all_agents() {
            $(
                $module::register();
            )*
        }
    };
}

// Register agent modules here
register_agents!(claude_code, opencode);

static INIT: Once = Once::new();

/// Represents an AI agent that can be configured to use MCP servers
pub trait Agent {
    /// Human-readable name of the agent (e.g., "Claude Code")
    fn name(&self) -> &'static str;

    /// Config sources this agent checks, in priority order (highest first)
    fn config_sources(&self, project_path: &Path) -> Vec<PathBuf>;

    /// Check if this agent is configured for the given project
    /// Default implementation returns true if any config source exists
    fn detect(&self, project_path: &Path) -> bool {
        self.config_sources(project_path).iter().any(|p| p.exists())
    }

    /// Get MCP servers configured for this project, with source tracking
    /// Higher priority sources override lower priority for same server name
    fn get_servers(&self, project_path: &Path) -> Result<HashMap<String, ServerWithSource>>;

    /// Apply changes to the agent's configuration
    /// - Disable the specified servers in their respective source files
    /// - Add mcpg to the preferred project-scoped config
    fn apply_changes(&self, project_path: &Path, servers_to_disable: &[String]) -> Result<()>;

    /// Check if mcpg is already configured for this project
    /// Returns the path where mcpg is configured, or None if not configured
    fn mcpg_configured_at(&self, project_path: &Path) -> Option<PathBuf>;
}

/// Discover which agents are available and configured for a project
pub fn discover_agents(project_path: &Path) -> Vec<Box<dyn Agent>> {
    INIT.call_once(init_all_agents);

    registry::get_registered_agents()
        .into_iter()
        .filter(|agent| agent.detect(project_path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    fn setup_claude_project() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        // Create .mcp.json (highest priority project-scoped config for Claude Code)
        fs::write(temp_dir.path().join(".mcp.json"), "{}").unwrap();
        temp_dir
    }

    #[test]
    #[serial]
    fn test_discover_agents_for_empty_project() {
        let temp_dir = TempDir::new().unwrap();
        // Empty project with no agent configs
        let agents = discover_agents(temp_dir.path());
        // Any agents found are from user-level configs (e.g., ~/.claude.json),
        // not from the project itself. Verify the function runs without errors.
        for agent in &agents {
            assert!(!agent.name().is_empty());
        }
    }

    #[test]
    #[serial]
    fn test_discover_agents_finds_claude() {
        let temp_dir = setup_claude_project();

        let agents = discover_agents(temp_dir.path());
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name(), "Claude Code");
    }

    #[test]
    #[serial]
    fn test_discover_agents_multiple_calls() {
        // Ensure registry can be called multiple times
        let temp_dir = setup_claude_project();

        let agents1 = discover_agents(temp_dir.path());
        let agents2 = discover_agents(temp_dir.path());

        assert_eq!(agents1.len(), agents2.len());
        assert_eq!(agents1[0].name(), agents2[0].name());
    }

    fn setup_opencode_project() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("opencode.json"), "{}").unwrap();
        temp_dir
    }

    #[test]
    #[serial]
    fn test_discover_agents_finds_opencode() {
        let temp_dir = setup_opencode_project();

        let agents = discover_agents(temp_dir.path());
        let names: Vec<&str> = agents.iter().map(|a| a.name()).collect();
        assert!(names.contains(&"OpenCode"));
    }

    #[test]
    #[serial]
    fn test_discover_agents_finds_both() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join(".mcp.json"), "{}").unwrap();
        fs::write(temp_dir.path().join("opencode.json"), "{}").unwrap();

        let agents = discover_agents(temp_dir.path());
        assert!(agents.len() >= 2);
        let names: Vec<&str> = agents.iter().map(|a| a.name()).collect();
        assert!(names.contains(&"Claude Code"));
        assert!(names.contains(&"OpenCode"));
    }
}
