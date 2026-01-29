use once_cell::sync::Lazy;
use tracing::{debug, trace};

use super::config_file::ConfigFile;
use super::{registry, Agent, ConfigLevel, ConfigSource, ServerWithSource};
use crate::config::{ServerConfig, TransportConfig};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

static REGISTER: Lazy<()> = Lazy::new(|| {
    registry::register_agent(|| Box::new(ClaudeCodeAgent::new()));
});

pub fn register() {
    Lazy::force(&REGISTER);
}

/// Project-level config structure (e.g., .mcp.json, .claude/settings.local.json)
/// Has mcpServers at the top level
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ProjectLevelConfig {
    #[serde(default)]
    mcp_servers: HashMap<String, ClaudeMcpServer>,
    #[serde(default)]
    disabled_mcp_servers: Vec<String>,
    #[serde(flatten)]
    other: HashMap<String, serde_json::Value>,
}

/// User-level config structure (e.g., ~/.claude.json)
/// Has mcpServers nested under projects.<project_path>
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct UserLevelConfig {
    #[serde(default)]
    projects: HashMap<String, ProjectSettings>,
    #[serde(flatten)]
    other: HashMap<String, serde_json::Value>,
}

/// Per-project settings within user-level config
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ProjectSettings {
    #[serde(default)]
    mcp_servers: HashMap<String, ClaudeMcpServer>,
    #[serde(default)]
    disabled_mcp_servers: Vec<String>,
    #[serde(flatten)]
    other: HashMap<String, serde_json::Value>,
}

/// MCP server config as stored in Claude Code
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClaudeMcpServer {
    #[serde(rename = "type", default = "default_transport_type")]
    transport_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
}

fn default_transport_type() -> String {
    "stdio".to_string()
}

pub struct ClaudeCodeAgent;

impl ClaudeCodeAgent {
    pub fn new() -> Self {
        Self
    }

    /// Get config sources with their level (project vs user)
    fn config_sources_with_level(&self, project_path: &Path) -> Vec<ConfigSource> {
        let home = dirs::home_dir().unwrap_or_default();

        vec![
            // Project-scoped (highest priority)
            ConfigSource {
                path: project_path.join(".mcp.json"),
                level: ConfigLevel::Project,
            },
            ConfigSource {
                path: project_path.join(".claude/settings.local.json"),
                level: ConfigLevel::Project,
            },
            // User-scoped
            ConfigSource {
                path: home.join(".claude/settings.local.json"),
                level: ConfigLevel::User,
            },
            ConfigSource {
                path: home.join(".claude/settings.json"),
                level: ConfigLevel::User,
            },
            ConfigSource {
                path: home.join(".claude.json"),
                level: ConfigLevel::User,
            },
            ConfigSource {
                path: home.join(".claude/mcp_servers.json"),
                level: ConfigLevel::User,
            },
        ]
    }

    fn convert_to_server_config(
        &self,
        name: &str,
        server: &ClaudeMcpServer,
    ) -> Result<ServerConfig> {
        let transport = match server.transport_type.as_str() {
            "stdio" => {
                let command = server
                    .command
                    .clone()
                    .with_context(|| format!("Server '{}' is stdio but missing 'command'", name))?;
                TransportConfig::Stdio {
                    command,
                    args: server.args.clone(),
                    env: server.env.clone(),
                }
            }
            "http" | "sse" => {
                let url = server
                    .url
                    .clone()
                    .with_context(|| format!("Server '{}' is http but missing 'url'", name))?;
                TransportConfig::Http {
                    url,
                    headers: server.headers.clone(),
                }
            }
            other => bail!("Unknown transport type '{}' for server '{}'", other, name),
        };

        Ok(ServerConfig {
            transport,
            timeout_secs: 30,
        })
    }

    /// Find the project key in a user-level config, handling trailing slash variants.
    ///
    /// Returns the matching key if the project is found, or the canonical key
    /// (without trailing slash) for creating new entries.
    fn find_project_key(project_path: &Path, projects: &HashMap<String, ProjectSettings>) -> String {
        let project_key = project_path.to_string_lossy().to_string();

        if projects.contains_key(&project_key) {
            return project_key;
        }

        let with_slash = format!("{}/", project_key);
        if projects.contains_key(&with_slash) {
            return with_slash;
        }

        project_key
    }

    /// Load servers from a single config file based on its level
    fn load_servers_from_source(
        &self,
        source: &ConfigSource,
        project_path: &Path,
    ) -> Result<(HashMap<String, ClaudeMcpServer>, Vec<String>)> {
        let content = std::fs::read_to_string(&source.path)
            .with_context(|| format!("Failed to read config file: {}", source.path.display()))?;

        match source.level {
            ConfigLevel::Project => {
                let config: ProjectLevelConfig = serde_json::from_str(&content)
                    .with_context(|| {
                        format!("Failed to parse project config: {}", source.path.display())
                    })?;
                debug!(
                    source = %source.path.display(),
                    servers = config.mcp_servers.len(),
                    "Parsed project-level config"
                );
                Ok((config.mcp_servers, config.disabled_mcp_servers))
            }
            ConfigLevel::User => {
                let config: UserLevelConfig = serde_json::from_str(&content).with_context(|| {
                    format!("Failed to parse user config: {}", source.path.display())
                })?;

                let key = Self::find_project_key(project_path, &config.projects);

                if let Some(project_settings) = config.projects.get(&key) {
                    debug!(
                        source = %source.path.display(),
                        project = %key,
                        servers = project_settings.mcp_servers.len(),
                        "Found project in user-level config"
                    );
                    return Ok((
                        project_settings.mcp_servers.clone(),
                        project_settings.disabled_mcp_servers.clone(),
                    ));
                }

                // Log available projects for debugging
                if !config.projects.is_empty() {
                    trace!(
                        source = %source.path.display(),
                        available_projects = ?config.projects.keys().collect::<Vec<_>>(),
                        looking_for = %key,
                        "Project not found in user-level config"
                    );
                }

                Ok((HashMap::new(), Vec::new()))
            }
        }
    }

    /// Update a config file to disable servers and optionally add mcpg
    fn update_config_file(
        &self,
        source_path: &Path,
        level: &ConfigLevel,
        project_path: &Path,
        servers_to_disable: &[String],
        add_mcpg: bool,
    ) -> Result<()> {
        match level {
            ConfigLevel::Project => {
                let config_file = ConfigFile::<ProjectLevelConfig>::new(source_path.to_path_buf());
                config_file.create_backup()?;
                let mut config = config_file.load()?;

                // Disable the specified servers
                for server in servers_to_disable {
                    if !config.disabled_mcp_servers.contains(server) {
                        config.disabled_mcp_servers.push(server.clone());
                    }
                }

                // Add mcpg if requested
                if add_mcpg && !config.mcp_servers.contains_key("mcpg") {
                    config.mcp_servers.insert("mcpg".to_string(), Self::mcpg_server_config());
                }

                config_file.save(&config)
            }
            ConfigLevel::User => {
                let config_file = ConfigFile::<UserLevelConfig>::new(source_path.to_path_buf());
                config_file.create_backup()?;
                let mut config = config_file.load()?;

                let key_to_use = Self::find_project_key(project_path, &config.projects);
                let project_settings = config.projects.entry(key_to_use).or_default();

                // Disable the specified servers
                for server in servers_to_disable {
                    if !project_settings.disabled_mcp_servers.contains(server) {
                        project_settings.disabled_mcp_servers.push(server.clone());
                    }
                }

                // Add mcpg if requested
                if add_mcpg && !project_settings.mcp_servers.contains_key("mcpg") {
                    project_settings
                        .mcp_servers
                        .insert("mcpg".to_string(), Self::mcpg_server_config());
                }

                config_file.save(&config)
            }
        }
    }

    /// Create the mcpg server configuration
    fn mcpg_server_config() -> ClaudeMcpServer {
        ClaudeMcpServer {
            transport_type: "stdio".to_string(),
            command: Some("mcpg".to_string()),
            args: Some(vec![]),
            env: None,
            url: None,
            headers: None,
        }
    }
}

impl Agent for ClaudeCodeAgent {
    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn config_sources(&self, project_path: &Path) -> Vec<PathBuf> {
        self.config_sources_with_level(project_path)
            .into_iter()
            .map(|s| s.path)
            .collect()
    }

    // Use default detect() implementation which checks if any config source exists

    fn get_servers(&self, project_path: &Path) -> Result<HashMap<String, ServerWithSource>> {
        let sources = self.config_sources_with_level(project_path);
        let mut servers: HashMap<String, ServerWithSource> = HashMap::new();
        let mut all_disabled: Vec<String> = Vec::new();

        debug!("Checking {} config sources for MCP servers", sources.len());

        // Process sources in reverse order (lowest priority first)
        // so higher priority sources can override
        for source in sources.iter().rev() {
            let exists = source.path.exists();
            debug!(
                source = %source.path.display(),
                level = ?source.level,
                exists = exists,
                "Checking config source"
            );

            if !exists {
                continue;
            }

            match self.load_servers_from_source(source, project_path) {
                Ok((source_servers, disabled)) => {
                    debug!(
                        source = %source.path.display(),
                        server_count = source_servers.len(),
                        disabled_count = disabled.len(),
                        servers = ?source_servers.keys().collect::<Vec<_>>(),
                        disabled_servers = ?disabled,
                        "Loaded config from source"
                    );

                    // Track all disabled servers
                    all_disabled.extend(disabled);

                    // Add servers from this source (will be overwritten by higher priority)
                    for (name, claude_server) in source_servers {
                        // Skip mcpg itself
                        if name == "mcpg" {
                            trace!(server = %name, "Skipping mcpg server");
                            continue;
                        }

                        match self.convert_to_server_config(&name, &claude_server) {
                            Ok(server_config) => {
                                trace!(server = %name, "Converted server config");
                                servers.insert(
                                    name,
                                    ServerWithSource {
                                        config: server_config,
                                        source: source.path.clone(),
                                    },
                                );
                            }
                            Err(e) => {
                                debug!(server = %name, error = %e, "Failed to convert server config");
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!(
                        source = %source.path.display(),
                        error = %e,
                        "Failed to load config from source"
                    );
                    continue;
                }
            }
        }

        // Remove disabled servers
        for disabled in &all_disabled {
            if servers.remove(disabled).is_some() {
                debug!(server = %disabled, "Removed disabled server");
            }
        }

        debug!(
            total_servers = servers.len(),
            server_names = ?servers.keys().collect::<Vec<_>>(),
            "Final server list"
        );

        Ok(servers)
    }

    fn apply_changes(
        &self,
        project_path: &Path,
        servers_to_disable: &[String],
    ) -> Result<()> {
        // Group servers by their source file
        let servers = self.get_servers(project_path)?;
        let mut sources_to_update: HashMap<PathBuf, Vec<String>> = HashMap::new();

        for server_name in servers_to_disable {
            if let Some(server_with_source) = servers.get(server_name) {
                sources_to_update
                    .entry(server_with_source.source.clone())
                    .or_default()
                    .push(server_name.clone());
            }
        }

        let sources_with_level = self.config_sources_with_level(project_path);

        // Determine the highest priority source to add mcpg to:
        // - If there are servers to disable, use the highest priority source with servers
        // - Otherwise, use the highest priority existing config file
        let mcpg_target_source = if !sources_to_update.is_empty() {
            sources_with_level
                .iter()
                .find(|s| sources_to_update.contains_key(&s.path))
                .cloned()
        } else {
            // No servers to disable - find the first existing config file
            sources_with_level.iter().find(|s| s.path.exists()).cloned()
        };

        // Update each source file - disable servers and add mcpg to highest priority source
        for (source_path, servers_to_disable_in_source) in &sources_to_update {
            // Determine the config level for this source
            let source_level = sources_with_level
                .iter()
                .find(|s| s.path == *source_path)
                .map(|s| s.level)
                .unwrap_or(ConfigLevel::Project);

            let should_add_mcpg = mcpg_target_source
                .as_ref()
                .map(|s| s.path == *source_path)
                .unwrap_or(false);

            self.update_config_file(
                source_path,
                &source_level,
                project_path,
                servers_to_disable_in_source,
                should_add_mcpg,
            )?;
        }

        // If no servers were disabled but we have a target for mcpg, add it there
        if sources_to_update.is_empty()
            && let Some(target) = mcpg_target_source
        {
            self.update_config_file(&target.path, &target.level, project_path, &[], true)?;
        }

        Ok(())
    }

    fn mcpg_configured_at(&self, project_path: &Path) -> Option<PathBuf> {
        let sources = self.config_sources_with_level(project_path);
        let mut all_disabled: Vec<String> = Vec::new();
        let mut mcpg_source: Option<PathBuf> = None;

        for source in &sources {
            if !source.path.exists() {
                continue;
            }

            if let Ok((servers, disabled)) = self.load_servers_from_source(source, project_path) {
                all_disabled.extend(disabled);
                if servers.contains_key("mcpg") {
                    mcpg_source = Some(source.path.clone());
                }
            }
        }

        // Return None if mcpg is disabled
        if all_disabled.contains(&"mcpg".to_string()) {
            return None;
        }

        mcpg_source
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config_file(project_path: &Path) -> PathBuf {
        let config_path = project_path.join(".mcp.json");
        let config = ProjectLevelConfig {
            mcp_servers: [
                (
                    "github".to_string(),
                    ClaudeMcpServer {
                        transport_type: "stdio".to_string(),
                        command: Some("npx".to_string()),
                        args: Some(vec!["@modelcontextprotocol/server-github".to_string()]),
                        env: Some([("GITHUB_TOKEN".to_string(), "test".to_string())].into()),
                        url: None,
                        headers: None,
                    },
                ),
                (
                    "slack".to_string(),
                    ClaudeMcpServer {
                        transport_type: "stdio".to_string(),
                        command: Some("npx".to_string()),
                        args: Some(vec!["@anthropic/mcp-server-slack".to_string()]),
                        env: None,
                        url: None,
                        headers: None,
                    },
                ),
            ]
            .into(),
            disabled_mcp_servers: vec![],
            other: HashMap::new(),
        };

        fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
        config_path
    }

    #[test]
    fn test_detect_returns_false_for_nonexistent_path() {
        let agent = ClaudeCodeAgent::new();
        assert!(!agent.detect(&PathBuf::from("/nonexistent/path")));
    }

    #[test]
    fn test_detect_returns_true_when_config_exists() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config_file(temp_dir.path());

        let agent = ClaudeCodeAgent::new();
        assert!(agent.detect(temp_dir.path()));
    }

    #[test]
    fn test_config_sources_returns_expected_paths() {
        let agent = ClaudeCodeAgent::new();
        let project_path = PathBuf::from("/test/project");
        let sources = agent.config_sources(&project_path);

        assert_eq!(sources.len(), 6);
        assert_eq!(sources[0], PathBuf::from("/test/project/.mcp.json"));
        assert_eq!(
            sources[1],
            PathBuf::from("/test/project/.claude/settings.local.json")
        );
    }

    #[test]
    fn test_get_servers_returns_configured_servers() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config_file(temp_dir.path());

        let agent = ClaudeCodeAgent::new();
        let servers = agent.get_servers(temp_dir.path()).unwrap();

        assert_eq!(servers.len(), 2);
        assert!(servers.contains_key("github"));
        assert!(servers.contains_key("slack"));

        // Verify source tracking
        let github = servers.get("github").unwrap();
        assert_eq!(github.source, temp_dir.path().join(".mcp.json"));
    }

    #[test]
    fn test_get_servers_skips_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_file(temp_dir.path());

        // Manually update config to disable slack
        let mut config: ProjectLevelConfig =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        config.disabled_mcp_servers.push("slack".to_string());
        fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

        let agent = ClaudeCodeAgent::new();
        let servers = agent.get_servers(temp_dir.path()).unwrap();

        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("github"));
        assert!(!servers.contains_key("slack"));
    }

    #[test]
    fn test_apply_changes_disables_servers() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_file(temp_dir.path());

        let agent = ClaudeCodeAgent::new();
        agent
            .apply_changes(temp_dir.path(), &["github".to_string()])
            .unwrap();

        // Verify the config was updated
        let updated: ProjectLevelConfig =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();

        assert!(updated.disabled_mcp_servers.contains(&"github".to_string()));
        assert!(updated.mcp_servers.contains_key("mcpg"));
    }

    #[test]
    fn test_apply_changes_creates_backup() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_file(temp_dir.path());

        let agent = ClaudeCodeAgent::new();
        agent
            .apply_changes(temp_dir.path(), &["github".to_string()])
            .unwrap();

        // Verify backup was created
        let backup_path = config_path.with_extension("json.backup");
        assert!(backup_path.exists());
    }

    #[test]
    fn test_name() {
        let agent = ClaudeCodeAgent::new();
        assert_eq!(agent.name(), "Claude Code");
    }

    #[test]
    fn test_mcpg_configured_at_returns_none_when_not_present() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config_file(temp_dir.path());

        let agent = ClaudeCodeAgent::new();
        assert!(agent.mcpg_configured_at(temp_dir.path()).is_none());
    }

    #[test]
    fn test_mcpg_configured_at_returns_path_after_apply_changes() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_file(temp_dir.path());

        let agent = ClaudeCodeAgent::new();

        // Before apply_changes, mcpg is not configured
        assert!(agent.mcpg_configured_at(temp_dir.path()).is_none());

        // Apply changes adds mcpg to the same file where servers were found
        agent
            .apply_changes(temp_dir.path(), &["github".to_string()])
            .unwrap();

        // After apply_changes, mcpg is configured at the same source file
        let configured_at = agent.mcpg_configured_at(temp_dir.path());
        assert!(configured_at.is_some());
        assert_eq!(configured_at.unwrap(), config_path);
    }

    #[test]
    fn test_mcpg_configured_at_returns_none_when_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_file(temp_dir.path());

        // Add mcpg but also disable it
        let mut config: ProjectLevelConfig =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        config.mcp_servers.insert(
            "mcpg".to_string(),
            ClaudeMcpServer {
                transport_type: "stdio".to_string(),
                command: Some("mcpg".to_string()),
                args: None,
                env: None,
                url: None,
                headers: None,
            },
        );
        config.disabled_mcp_servers.push("mcpg".to_string());
        fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

        let agent = ClaudeCodeAgent::new();
        // mcpg is present but disabled, so should return None
        assert!(agent.mcpg_configured_at(temp_dir.path()).is_none());
    }

    #[test]
    fn test_multi_source_priority() {
        let temp_dir = TempDir::new().unwrap();

        // Create a lower priority config with server "low"
        let claude_dir = temp_dir.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let low_priority_path = claude_dir.join("settings.local.json");
        let low_config = ProjectLevelConfig {
            mcp_servers: [(
                "shared".to_string(),
                ClaudeMcpServer {
                    transport_type: "stdio".to_string(),
                    command: Some("low-priority".to_string()),
                    args: None,
                    env: None,
                    url: None,
                    headers: None,
                },
            )]
            .into(),
            disabled_mcp_servers: vec![],
            other: HashMap::new(),
        };
        fs::write(
            &low_priority_path,
            serde_json::to_string_pretty(&low_config).unwrap(),
        )
        .unwrap();

        // Create a higher priority config with same server name
        let high_priority_path = temp_dir.path().join(".mcp.json");
        let high_config = ProjectLevelConfig {
            mcp_servers: [(
                "shared".to_string(),
                ClaudeMcpServer {
                    transport_type: "stdio".to_string(),
                    command: Some("high-priority".to_string()),
                    args: None,
                    env: None,
                    url: None,
                    headers: None,
                },
            )]
            .into(),
            disabled_mcp_servers: vec![],
            other: HashMap::new(),
        };
        fs::write(
            &high_priority_path,
            serde_json::to_string_pretty(&high_config).unwrap(),
        )
        .unwrap();

        let agent = ClaudeCodeAgent::new();
        let servers = agent.get_servers(temp_dir.path()).unwrap();

        // Should have one server, from the higher priority source
        assert_eq!(servers.len(), 1);
        let shared = servers.get("shared").unwrap();
        assert_eq!(shared.source, high_priority_path);

        // Verify the config is from high priority (command = "high-priority")
        match &shared.config.transport {
            TransportConfig::Stdio { command, .. } => {
                assert_eq!(command, "high-priority");
            }
            _ => panic!("Expected stdio transport"),
        }
    }

    #[test]
    fn test_user_level_config_nested_format() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();

        // Create a user-level config with nested projects structure
        // Simulates ~/.claude.json format
        let user_config_path = temp_dir.path().join(".mcp.json");
        let project_key = project_path.to_string_lossy().to_string();

        let user_config = UserLevelConfig {
            projects: [(
                project_key,
                ProjectSettings {
                    mcp_servers: [(
                        "nested-server".to_string(),
                        ClaudeMcpServer {
                            transport_type: "stdio".to_string(),
                            command: Some("nested-command".to_string()),
                            args: None,
                            env: None,
                            url: None,
                            headers: None,
                        },
                    )]
                    .into(),
                    disabled_mcp_servers: vec![],
                    other: HashMap::new(),
                },
            )]
            .into(),
            other: HashMap::new(),
        };

        fs::write(
            &user_config_path,
            serde_json::to_string_pretty(&user_config).unwrap(),
        )
        .unwrap();

        let agent = ClaudeCodeAgent::new();
        let servers = agent.get_servers(project_path).unwrap();

        // Should find the server from nested format
        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("nested-server"));

        let server = servers.get("nested-server").unwrap();
        match &server.config.transport {
            TransportConfig::Stdio { command, .. } => {
                assert_eq!(command, "nested-command");
            }
            _ => panic!("Expected stdio transport"),
        }
    }
}
