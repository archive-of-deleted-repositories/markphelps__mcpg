use once_cell::sync::Lazy;
use tracing::{debug, trace};

use super::config_file::ConfigFile;
use super::{registry, Agent, ServerWithSource};
use crate::config::{ServerConfig, TransportConfig};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

static REGISTER: Lazy<()> = Lazy::new(|| {
    registry::register_agent(|| Box::new(OpenCodeAgent::new()));
});

pub fn register() {
    Lazy::force(&REGISTER);
}

/// OpenCode config structure
/// Both project-level (./opencode.json) and global (~/.config/opencode/opencode.json)
/// share this same flat format.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct OpenCodeConfig {
    #[serde(default)]
    mcp: HashMap<String, OpenCodeMcpServer>,
    #[serde(flatten)]
    other: HashMap<String, serde_json::Value>,
}

/// MCP server config as stored in OpenCode
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenCodeMcpServer {
    #[serde(rename = "type")]
    server_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    /// Preserves unknown fields (oauth, timeout, etc.) through round-trip serialization
    #[serde(flatten)]
    other: HashMap<String, serde_json::Value>,
}

pub struct OpenCodeAgent {
    home_dir: Option<PathBuf>,
}

impl OpenCodeAgent {
    pub fn new() -> Self {
        Self { home_dir: None }
    }

    #[cfg(test)]
    pub fn with_home_dir(home: PathBuf) -> Self {
        Self {
            home_dir: Some(home),
        }
    }

    fn convert_to_server_config(
        &self,
        name: &str,
        server: &OpenCodeMcpServer,
    ) -> Result<ServerConfig> {
        let transport = match server.server_type.as_str() {
            "local" => {
                let command = server
                    .command
                    .as_ref()
                    .with_context(|| format!("Server '{}' is local but missing 'command'", name))?;
                if command.is_empty() {
                    bail!("Server '{}' has empty command array", name);
                }
                if command[0].is_empty() {
                    bail!("Server '{}' has empty command string", name);
                }
                TransportConfig::Stdio {
                    command: command[0].clone(),
                    args: if command.len() > 1 {
                        Some(command[1..].to_vec())
                    } else {
                        None
                    },
                    env: server.environment.clone(),
                }
            }
            "remote" => {
                let url = server
                    .url
                    .clone()
                    .with_context(|| format!("Server '{}' is remote but missing 'url'", name))?;
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

    /// Load servers from a single config file
    fn load_servers_from_source(
        &self,
        source_path: &Path,
    ) -> Result<HashMap<String, OpenCodeMcpServer>> {
        let content = std::fs::read_to_string(source_path)
            .with_context(|| format!("Failed to read config file: {}", source_path.display()))?;

        let config: OpenCodeConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config: {}", source_path.display()))?;

        debug!(
            source = %source_path.display(),
            servers = config.mcp.len(),
            "Parsed OpenCode config"
        );

        Ok(config.mcp)
    }

    /// Create the mcpg server configuration for OpenCode
    fn mcpg_server_config() -> OpenCodeMcpServer {
        OpenCodeMcpServer {
            server_type: "local".to_string(),
            command: Some(vec!["mcpg".to_string()]),
            environment: None,
            url: None,
            headers: None,
            enabled: None,
            other: HashMap::new(),
        }
    }

    /// Update a config file to disable servers and optionally add mcpg
    fn update_config_file(
        &self,
        source_path: &Path,
        servers_to_disable: &[String],
        add_mcpg: bool,
    ) -> Result<()> {
        let config_file = ConfigFile::<OpenCodeConfig>::new(source_path.to_path_buf());
        config_file.create_backup()?;
        let mut config = config_file.load()?;

        // Disable the specified servers by setting enabled: false
        for server_name in servers_to_disable {
            if let Some(server) = config.mcp.get_mut(server_name) {
                server.enabled = Some(false);
            }
        }

        // Add mcpg if requested
        if add_mcpg && !config.mcp.contains_key("mcpg") {
            config
                .mcp
                .insert("mcpg".to_string(), Self::mcpg_server_config());
        }

        config_file.save(&config)
    }
}

impl Agent for OpenCodeAgent {
    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn config_sources(&self, project_path: &Path) -> Vec<PathBuf> {
        let config_dir = self
            .home_dir
            .as_ref()
            .map(|h| h.join(".config"))
            .unwrap_or_else(|| dirs::config_dir().unwrap_or_default());

        vec![
            // Project-scoped (highest priority)
            project_path.join("opencode.json"),
            // User/global (respects $XDG_CONFIG_HOME)
            config_dir.join("opencode/opencode.json"),
        ]
    }

    // Use default detect() implementation

    fn get_servers(&self, project_path: &Path) -> Result<HashMap<String, ServerWithSource>> {
        let sources = self.config_sources(project_path);
        let mut servers: HashMap<String, ServerWithSource> = HashMap::new();

        debug!("Checking {} OpenCode config sources", sources.len());

        // Process sources in reverse order (lowest priority first)
        // so higher priority sources can override
        for source_path in sources.iter().rev() {
            let exists = source_path.exists();
            debug!(
                source = %source_path.display(),
                exists = exists,
                "Checking OpenCode config source"
            );

            if !exists {
                continue;
            }

            match self.load_servers_from_source(source_path) {
                Ok(source_servers) => {
                    debug!(
                        source = %source_path.display(),
                        server_count = source_servers.len(),
                        servers = ?source_servers.keys().collect::<Vec<_>>(),
                        "Loaded OpenCode config from source"
                    );

                    for (name, oc_server) in source_servers {
                        // Skip mcpg itself
                        if name == "mcpg" {
                            trace!(server = %name, "Skipping mcpg server");
                            continue;
                        }

                        // Skip disabled servers
                        if oc_server.enabled == Some(false) {
                            debug!(server = %name, "Skipping disabled server");
                            continue;
                        }

                        match self.convert_to_server_config(&name, &oc_server) {
                            Ok(server_config) => {
                                trace!(server = %name, "Converted OpenCode server config");
                                servers.insert(
                                    name,
                                    ServerWithSource {
                                        config: server_config,
                                        source: source_path.clone(),
                                    },
                                );
                            }
                            Err(e) => {
                                debug!(server = %name, error = %e, "Failed to convert OpenCode server config");
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!(
                        source = %source_path.display(),
                        error = %e,
                        "Failed to load OpenCode config from source"
                    );
                    continue;
                }
            }
        }

        debug!(
            total_servers = servers.len(),
            server_names = ?servers.keys().collect::<Vec<_>>(),
            "Final OpenCode server list"
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

        let all_sources = self.config_sources(project_path);

        // Determine the highest priority source to add mcpg to
        let mcpg_target_source = if !sources_to_update.is_empty() {
            all_sources
                .iter()
                .find(|s| sources_to_update.contains_key(*s))
                .cloned()
        } else {
            // No servers to disable - find the first existing config file
            all_sources.iter().find(|s| s.exists()).cloned()
        };

        // Update each source file
        for (source_path, servers_in_source) in &sources_to_update {
            let should_add_mcpg = mcpg_target_source
                .as_ref()
                .map(|s| s == source_path)
                .unwrap_or(false);

            self.update_config_file(source_path, servers_in_source, should_add_mcpg)?;
        }

        // If no servers were disabled but we have a target for mcpg, add it there
        if sources_to_update.is_empty()
            && let Some(target) = mcpg_target_source
        {
            self.update_config_file(&target, &[], true)?;
        }

        Ok(())
    }

    fn mcpg_configured_at(&self, project_path: &Path) -> Option<PathBuf> {
        let sources = self.config_sources(project_path);

        for source_path in &sources {
            if !source_path.exists() {
                continue;
            }

            if let Ok(source_servers) = self.load_servers_from_source(source_path)
                && let Some(mcpg_server) = source_servers.get("mcpg")
            {
                // Return None if mcpg is explicitly disabled
                if mcpg_server.enabled == Some(false) {
                    return None;
                }
                return Some(source_path.clone());
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ---- Serde round-trip tests ----

    #[test]
    fn test_parse_local_server() {
        let json = r#"{
            "mcp": {
                "github": {
                    "type": "local",
                    "command": ["npx", "-y", "@modelcontextprotocol/server-github"],
                    "environment": { "GITHUB_TOKEN": "test" }
                }
            }
        }"#;

        let config: OpenCodeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mcp.len(), 1);
        let github = &config.mcp["github"];
        assert_eq!(github.server_type, "local");
        assert_eq!(
            github.command.as_ref().unwrap(),
            &vec![
                "npx".to_string(),
                "-y".to_string(),
                "@modelcontextprotocol/server-github".to_string()
            ]
        );
        assert_eq!(
            github.environment.as_ref().unwrap().get("GITHUB_TOKEN").unwrap(),
            "test"
        );
    }

    #[test]
    fn test_parse_remote_server() {
        let json = r#"{
            "mcp": {
                "jira": {
                    "type": "remote",
                    "url": "https://jira.example.com/mcp",
                    "headers": { "Authorization": "Bearer token" }
                }
            }
        }"#;

        let config: OpenCodeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mcp.len(), 1);
        let jira = &config.mcp["jira"];
        assert_eq!(jira.server_type, "remote");
        assert_eq!(jira.url.as_ref().unwrap(), "https://jira.example.com/mcp");
        assert_eq!(
            jira.headers.as_ref().unwrap().get("Authorization").unwrap(),
            "Bearer token"
        );
    }

    #[test]
    fn test_unknown_fields_preserved() {
        let json = r#"{
            "mcp": {
                "server": {
                    "type": "remote",
                    "url": "https://example.com/mcp",
                    "oauth": false,
                    "timeout": 5000
                }
            },
            "$schema": "https://opencode.ai/config.json",
            "theme": "catppuccin"
        }"#;

        let config: OpenCodeConfig = serde_json::from_str(json).unwrap();

        // Top-level unknown fields preserved
        assert!(config.other.contains_key("$schema"));
        assert!(config.other.contains_key("theme"));

        // Server-level unknown fields preserved
        let server = &config.mcp["server"];
        assert!(server.other.contains_key("oauth"));
        assert!(server.other.contains_key("timeout"));

        // Round-trip: serialize back and verify
        let serialized = serde_json::to_string_pretty(&config).unwrap();
        assert!(serialized.contains("$schema"));
        assert!(serialized.contains("catppuccin"));
        assert!(serialized.contains("oauth"));
        assert!(serialized.contains("5000"));
    }

    #[test]
    fn test_parse_disabled_server() {
        let json = r#"{
            "mcp": {
                "disabled-server": {
                    "type": "local",
                    "command": ["echo"],
                    "enabled": false
                }
            }
        }"#;

        let config: OpenCodeConfig = serde_json::from_str(json).unwrap();
        let server = &config.mcp["disabled-server"];
        assert_eq!(server.enabled, Some(false));
    }

    #[test]
    fn test_parse_empty_mcp() {
        let json = r#"{ "theme": "dark" }"#;
        let config: OpenCodeConfig = serde_json::from_str(json).unwrap();
        assert!(config.mcp.is_empty());
    }

    #[test]
    fn test_load_servers_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("opencode.json");
        fs::write(&config_path, "{ invalid json }").unwrap();

        let agent = OpenCodeAgent::new();
        let result = agent.load_servers_from_source(&config_path);
        assert!(result.is_err());
    }

    // ---- Conversion tests ----

    #[test]
    fn test_convert_local_to_stdio() {
        let agent = OpenCodeAgent::new();
        let server = OpenCodeMcpServer {
            server_type: "local".to_string(),
            command: Some(vec!["npx".to_string(), "-y".to_string(), "server".to_string()]),
            environment: Some([("KEY".to_string(), "val".to_string())].into()),
            url: None,
            headers: None,
            enabled: None,
            other: HashMap::new(),
        };

        let config = agent.convert_to_server_config("test", &server).unwrap();
        match &config.transport {
            TransportConfig::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(args.as_ref().unwrap(), &vec!["-y".to_string(), "server".to_string()]);
                assert_eq!(env.as_ref().unwrap().get("KEY").unwrap(), "val");
            }
            _ => panic!("Expected Stdio transport"),
        }
    }

    #[test]
    fn test_convert_local_single_command() {
        let agent = OpenCodeAgent::new();
        let server = OpenCodeMcpServer {
            server_type: "local".to_string(),
            command: Some(vec!["mcpg".to_string()]),
            environment: None,
            url: None,
            headers: None,
            enabled: None,
            other: HashMap::new(),
        };

        let config = agent.convert_to_server_config("test", &server).unwrap();
        match &config.transport {
            TransportConfig::Stdio { command, args, .. } => {
                assert_eq!(command, "mcpg");
                assert!(args.is_none());
            }
            _ => panic!("Expected Stdio transport"),
        }
    }

    #[test]
    fn test_convert_remote_to_http() {
        let agent = OpenCodeAgent::new();
        let server = OpenCodeMcpServer {
            server_type: "remote".to_string(),
            command: None,
            environment: None,
            url: Some("https://example.com/mcp".to_string()),
            headers: Some([("Auth".to_string(), "Bearer tok".to_string())].into()),
            enabled: None,
            other: HashMap::new(),
        };

        let config = agent.convert_to_server_config("test", &server).unwrap();
        match &config.transport {
            TransportConfig::Http { url, headers } => {
                assert_eq!(url, "https://example.com/mcp");
                assert_eq!(headers.as_ref().unwrap().get("Auth").unwrap(), "Bearer tok");
            }
            _ => panic!("Expected Http transport"),
        }
    }

    #[test]
    fn test_convert_local_missing_command_fails() {
        let agent = OpenCodeAgent::new();
        let server = OpenCodeMcpServer {
            server_type: "local".to_string(),
            command: None,
            environment: None,
            url: None,
            headers: None,
            enabled: None,
            other: HashMap::new(),
        };

        assert!(agent.convert_to_server_config("test", &server).is_err());
    }

    #[test]
    fn test_convert_local_empty_command_fails() {
        let agent = OpenCodeAgent::new();
        let server = OpenCodeMcpServer {
            server_type: "local".to_string(),
            command: Some(vec![]),
            environment: None,
            url: None,
            headers: None,
            enabled: None,
            other: HashMap::new(),
        };

        assert!(agent.convert_to_server_config("test", &server).is_err());
    }

    #[test]
    fn test_convert_local_empty_command_string_fails() {
        let agent = OpenCodeAgent::new();
        let server = OpenCodeMcpServer {
            server_type: "local".to_string(),
            command: Some(vec!["".to_string(), "arg1".to_string()]),
            environment: None,
            url: None,
            headers: None,
            enabled: None,
            other: HashMap::new(),
        };

        assert!(agent.convert_to_server_config("test", &server).is_err());
    }

    #[test]
    fn test_convert_remote_missing_url_fails() {
        let agent = OpenCodeAgent::new();
        let server = OpenCodeMcpServer {
            server_type: "remote".to_string(),
            command: None,
            environment: None,
            url: None,
            headers: None,
            enabled: None,
            other: HashMap::new(),
        };

        assert!(agent.convert_to_server_config("test", &server).is_err());
    }

    #[test]
    fn test_convert_unknown_type_fails() {
        let agent = OpenCodeAgent::new();
        let server = OpenCodeMcpServer {
            server_type: "unknown".to_string(),
            command: None,
            environment: None,
            url: None,
            headers: None,
            enabled: None,
            other: HashMap::new(),
        };

        assert!(agent.convert_to_server_config("test", &server).is_err());
    }

    // ---- Agent trait tests ----

    #[test]
    fn test_name() {
        let agent = OpenCodeAgent::new();
        assert_eq!(agent.name(), "OpenCode");
    }

    #[test]
    fn test_config_sources() {
        let agent = OpenCodeAgent::with_home_dir(PathBuf::from("/fake/home"));
        let sources = agent.config_sources(&PathBuf::from("/test/project"));

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0], PathBuf::from("/test/project/opencode.json"));
        assert_eq!(
            sources[1],
            PathBuf::from("/fake/home/.config/opencode/opencode.json")
        );
    }

    #[test]
    fn test_detect_false_for_nonexistent() {
        let agent = OpenCodeAgent::new();
        assert!(!agent.detect(&PathBuf::from("/nonexistent/path")));
    }

    #[test]
    fn test_detect_true_when_project_config_exists() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("opencode.json"), "{}").unwrap();

        let agent = OpenCodeAgent::new();
        assert!(agent.detect(temp_dir.path()));
    }

    #[test]
    fn test_detect_true_when_global_config_exists() {
        let temp_dir = TempDir::new().unwrap();
        let fake_home = temp_dir.path().join("home");
        let config_dir = fake_home.join(".config/opencode");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("opencode.json"), "{}").unwrap();

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        // Project path doesn't have config, but global does
        let project_path = temp_dir.path().join("project");
        fs::create_dir_all(&project_path).unwrap();
        assert!(agent.detect(&project_path));
    }

    // ---- get_servers tests ----

    fn create_opencode_config(path: &Path, servers: HashMap<String, OpenCodeMcpServer>) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let config = OpenCodeConfig {
            mcp: servers,
            other: HashMap::new(),
        };
        fs::write(path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    }

    fn local_server(command: &[&str]) -> OpenCodeMcpServer {
        OpenCodeMcpServer {
            server_type: "local".to_string(),
            command: Some(command.iter().map(|s| s.to_string()).collect()),
            environment: None,
            url: None,
            headers: None,
            enabled: None,
            other: HashMap::new(),
        }
    }

    fn remote_server(url: &str) -> OpenCodeMcpServer {
        OpenCodeMcpServer {
            server_type: "remote".to_string(),
            command: None,
            environment: None,
            url: Some(url.to_string()),
            headers: None,
            enabled: None,
            other: HashMap::new(),
        }
    }

    fn disabled_local_server(command: &[&str]) -> OpenCodeMcpServer {
        let mut s = local_server(command);
        s.enabled = Some(false);
        s
    }

    #[test]
    fn test_get_servers_from_project_config() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        create_opencode_config(
            &project_path.join("opencode.json"),
            [
                ("github".to_string(), local_server(&["npx", "-y", "server-github"])),
                ("jira".to_string(), remote_server("https://jira.example.com/mcp")),
            ]
            .into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        let servers = agent.get_servers(&project_path).unwrap();

        assert_eq!(servers.len(), 2);
        assert!(servers.contains_key("github"));
        assert!(servers.contains_key("jira"));
        assert_eq!(servers["github"].source, project_path.join("opencode.json"));
    }

    #[test]
    fn test_get_servers_from_global_config() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        let global_config_path = fake_home.join(".config/opencode/opencode.json");
        create_opencode_config(
            &global_config_path,
            [("github".to_string(), local_server(&["npx", "server-github"]))].into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        let servers = agent.get_servers(&project_path).unwrap();

        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("github"));
        assert_eq!(servers["github"].source, global_config_path);
    }

    #[test]
    fn test_get_servers_project_overrides_global() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        // Global: shared with global-cmd
        create_opencode_config(
            &fake_home.join(".config/opencode/opencode.json"),
            [("shared".to_string(), local_server(&["global-cmd"]))].into(),
        );

        // Project: shared with project-cmd
        create_opencode_config(
            &project_path.join("opencode.json"),
            [("shared".to_string(), local_server(&["project-cmd"]))].into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        let servers = agent.get_servers(&project_path).unwrap();

        assert_eq!(servers.len(), 1);
        let shared = &servers["shared"];
        assert_eq!(shared.source, project_path.join("opencode.json"));
        match &shared.config.transport {
            TransportConfig::Stdio { command, .. } => assert_eq!(command, "project-cmd"),
            _ => panic!("Expected Stdio transport"),
        }
    }

    #[test]
    fn test_get_servers_merges_across_sources() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        // Global: slack
        let global_path = fake_home.join(".config/opencode/opencode.json");
        create_opencode_config(
            &global_path,
            [("slack".to_string(), local_server(&["slack-cmd"]))].into(),
        );

        // Project: github
        create_opencode_config(
            &project_path.join("opencode.json"),
            [("github".to_string(), local_server(&["github-cmd"]))].into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        let servers = agent.get_servers(&project_path).unwrap();

        assert_eq!(servers.len(), 2);
        assert!(servers.contains_key("github"));
        assert!(servers.contains_key("slack"));
        assert_eq!(servers["github"].source, project_path.join("opencode.json"));
        assert_eq!(servers["slack"].source, global_path);
    }

    #[test]
    fn test_get_servers_skips_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        create_opencode_config(
            &project_path.join("opencode.json"),
            [
                ("github".to_string(), local_server(&["github-cmd"])),
                ("disabled".to_string(), disabled_local_server(&["disabled-cmd"])),
            ]
            .into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        let servers = agent.get_servers(&project_path).unwrap();

        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("github"));
        assert!(!servers.contains_key("disabled"));
    }

    #[test]
    fn test_get_servers_skips_mcpg() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        create_opencode_config(
            &project_path.join("opencode.json"),
            [
                ("github".to_string(), local_server(&["github-cmd"])),
                ("mcpg".to_string(), local_server(&["mcpg"])),
            ]
            .into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        let servers = agent.get_servers(&project_path).unwrap();

        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("github"));
    }

    #[test]
    fn test_get_servers_empty_when_no_config() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        let servers = agent.get_servers(&project_path).unwrap();
        assert!(servers.is_empty());
    }

    // ---- apply_changes tests ----

    #[test]
    fn test_apply_changes_disables_and_adds_mcpg() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        let config_path = project_path.join("opencode.json");
        create_opencode_config(
            &config_path,
            [
                ("github".to_string(), local_server(&["github-cmd"])),
                ("slack".to_string(), local_server(&["slack-cmd"])),
            ]
            .into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        agent
            .apply_changes(&project_path, &["github".to_string()])
            .unwrap();

        // Verify config was updated
        let updated: OpenCodeConfig =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();

        // github should be disabled
        assert_eq!(updated.mcp["github"].enabled, Some(false));
        // slack should be untouched
        assert_ne!(updated.mcp["slack"].enabled, Some(false));
        // mcpg should be added
        assert!(updated.mcp.contains_key("mcpg"));
        assert_eq!(updated.mcp["mcpg"].server_type, "local");
    }

    #[test]
    fn test_apply_changes_creates_backup() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        let config_path = project_path.join("opencode.json");
        create_opencode_config(
            &config_path,
            [("github".to_string(), local_server(&["github-cmd"]))].into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        agent
            .apply_changes(&project_path, &["github".to_string()])
            .unwrap();

        let backup_path = config_path.with_extension("json.backup");
        assert!(backup_path.exists());
    }

    #[test]
    fn test_apply_multi_source() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        // Project: github
        let project_config = project_path.join("opencode.json");
        create_opencode_config(
            &project_config,
            [("github".to_string(), local_server(&["github-cmd"]))].into(),
        );

        // Global: slack
        let global_config = fake_home.join(".config/opencode/opencode.json");
        create_opencode_config(
            &global_config,
            [("slack".to_string(), local_server(&["slack-cmd"]))].into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        agent
            .apply_changes(&project_path, &["github".to_string(), "slack".to_string()])
            .unwrap();

        // Project config: github disabled, mcpg added (highest priority)
        let project_updated: OpenCodeConfig =
            serde_json::from_str(&fs::read_to_string(&project_config).unwrap()).unwrap();
        assert_eq!(project_updated.mcp["github"].enabled, Some(false));
        assert!(project_updated.mcp.contains_key("mcpg"));

        // Global config: slack disabled, mcpg NOT added
        let global_updated: OpenCodeConfig =
            serde_json::from_str(&fs::read_to_string(&global_config).unwrap()).unwrap();
        assert_eq!(global_updated.mcp["slack"].enabled, Some(false));
        assert!(!global_updated.mcp.contains_key("mcpg"));
    }

    #[test]
    fn test_apply_preserves_unknown_fields() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        let config_path = project_path.join("opencode.json");
        // Write config with extra fields
        fs::write(
            &config_path,
            r#"{
                "mcp": {
                    "github": {
                        "type": "local",
                        "command": ["github-cmd"],
                        "timeout": 5000
                    }
                },
                "$schema": "https://opencode.ai/config.json",
                "theme": "catppuccin"
            }"#,
        )
        .unwrap();

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        agent
            .apply_changes(&project_path, &["github".to_string()])
            .unwrap();

        let raw = fs::read_to_string(&config_path).unwrap();
        // Unknown fields should be preserved
        assert!(raw.contains("$schema"));
        assert!(raw.contains("catppuccin"));
        assert!(raw.contains("5000")); // timeout on server
    }

    // ---- mcpg_configured_at tests ----

    #[test]
    fn test_mcpg_configured_at_none_when_not_present() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        create_opencode_config(
            &project_path.join("opencode.json"),
            [("github".to_string(), local_server(&["github-cmd"]))].into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        assert!(agent.mcpg_configured_at(&project_path).is_none());
    }

    #[test]
    fn test_mcpg_configured_at_returns_path_after_apply() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        let config_path = project_path.join("opencode.json");
        create_opencode_config(
            &config_path,
            [("github".to_string(), local_server(&["github-cmd"]))].into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        assert!(agent.mcpg_configured_at(&project_path).is_none());

        agent
            .apply_changes(&project_path, &["github".to_string()])
            .unwrap();

        let configured_at = agent.mcpg_configured_at(&project_path);
        assert!(configured_at.is_some());
        assert_eq!(configured_at.unwrap(), config_path);
    }

    #[test]
    fn test_mcpg_configured_at_none_when_disabled() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("project");
        let fake_home = temp_dir.path().join("home");
        fs::create_dir_all(&project_path).unwrap();
        fs::create_dir_all(&fake_home).unwrap();

        let mut mcpg = local_server(&["mcpg"]);
        mcpg.enabled = Some(false);

        create_opencode_config(
            &project_path.join("opencode.json"),
            [("mcpg".to_string(), mcpg)].into(),
        );

        let agent = OpenCodeAgent::with_home_dir(fake_home);
        assert!(agent.mcpg_configured_at(&project_path).is_none());
    }
}
