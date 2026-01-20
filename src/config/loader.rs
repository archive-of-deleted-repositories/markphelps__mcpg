use anyhow::{Context, Result, bail};
use std::path::PathBuf;

pub enum ConfigSource {
    Env(PathBuf),
    Project(PathBuf),
    User(PathBuf),
}

pub struct LoadedConfig {
    pub config: super::Config,
    pub source: ConfigSource,
}

const MCPG_CONFIG_VAR: &str = "MCPG_CONFIG";
const PROJECT_MCP_CONFIG_FILE: &str = "mcp.json";

impl LoadedConfig {
    pub fn load() -> Result<LoadedConfig> {
        // 1. Check environment variable
        if let Ok(env_path) = std::env::var(MCPG_CONFIG_VAR) {
            let path = PathBuf::from(env_path);
            let config = Self::from_file(&path)?;
            return Ok(LoadedConfig {
                config,
                source: ConfigSource::Env(path),
            });
        }

        // 2. Check project directory
        if let Ok(current_dir) = std::env::current_dir() {
            let project_path = current_dir.join(PROJECT_MCP_CONFIG_FILE);
            if project_path.exists() {
                let config = Self::from_file(&project_path)?;
                return Ok(LoadedConfig {
                    config,
                    source: ConfigSource::Project(project_path),
                });
            }
        }

        // 3. Fallback to user config
        let user_config_path = dirs::config_dir()
            .context("Could not determine user config directory")?
            .join("mcp")
            .join("servers.json");
        if user_config_path.exists() {
            let config = Self::from_file(&user_config_path)?;
            return Ok(LoadedConfig {
                config,
                source: ConfigSource::User(user_config_path),
            });
        }

        bail!("No configuration file found")
    }

    fn from_file(path: &PathBuf) -> Result<super::Config> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }
}
