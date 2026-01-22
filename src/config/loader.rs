use anyhow::{Context, Result, bail};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ConfigSource {
    Provided(PathBuf),
    Env(PathBuf),
    Project(PathBuf),
    User(PathBuf),
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigSource::Provided(p) => write!(f, "{}", p.display()),
            ConfigSource::Env(p) => write!(f, "{} (from $MCPG_CONFIG)", p.display()),
            ConfigSource::Project(p) => write!(f, "{}", p.display()),
            ConfigSource::User(p) => write!(f, "{}", p.display()),
        }
    }
}

#[derive(Debug)]
pub struct LoadedConfig {
    pub config: super::Config,
    pub source: ConfigSource,
}

const MCPG_CONFIG_VAR: &str = "MCPG_CONFIG";
const PROJECT_MCP_CONFIG_FILE: &str = "mcp.json";

impl LoadedConfig {
    pub fn load(path: Option<PathBuf>) -> Result<LoadedConfig> {
        // 1. If a path is provided, use that
        if let Some(provided_path) = path {
            let config = Self::from_file(&provided_path)?;
            return Ok(LoadedConfig {
                config,
                source: ConfigSource::Provided(provided_path),
            });
        }

        // 2. Check environment variable
        if let Ok(env_path) = std::env::var(MCPG_CONFIG_VAR) {
            let path = PathBuf::from(env_path);
            let config = Self::from_file(&path)?;
            return Ok(LoadedConfig {
                config,
                source: ConfigSource::Env(path),
            });
        }

        // 3. Check project directory
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

        // 4. Fallback to user config
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
        let config: super::Config = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        // Validate each server config
        for (name, server_config) in &config.servers {
            server_config
                .validate()
                .with_context(|| format!("Invalid config for server '{}'", name))?;
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config_json() -> String {
        r#"{
            "servers": {
                "test-server": {
                    "command": "npx",
                    "args": ["@modelcontextprotocol/server-brave-search"]
                }
            }
        }"#
        .to_string()
    }

    fn create_invalid_config_json() -> String {
        r#"{
            "servers": {
                "test-server": {
                    "invalid": "config"
                }
            }
        }"#
        .to_string()
    }

    #[test]
    fn test_load_from_provided_path() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        fs::write(&config_path, create_test_config_json()).unwrap();

        let result = LoadedConfig::load(Some(config_path.clone()));
        assert!(result.is_ok());

        let loaded = result.unwrap();
        assert!(loaded.config.get_server("test-server").is_some());

        match loaded.source {
            ConfigSource::Provided(path) => assert_eq!(path, config_path),
            _ => panic!("Expected Provided source"),
        }
    }

    #[test]
    fn test_load_from_provided_path_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.json");

        let result = LoadedConfig::load(Some(config_path));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to read config file")
        );
    }

    #[test]
    fn test_load_from_provided_path_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid.json");
        fs::write(&config_path, "not valid json").unwrap();

        let result = LoadedConfig::load(Some(config_path));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse config file")
        );
    }

    #[test]
    #[serial]
    fn test_load_from_env_variable() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("env_config.json");
        fs::write(&config_path, create_test_config_json()).unwrap();

        // Set environment variable
        unsafe {
            std::env::set_var(MCPG_CONFIG_VAR, config_path.to_str().unwrap());
        }

        let result = LoadedConfig::load(None);
        assert!(result.is_ok());

        let loaded = result.unwrap();
        assert!(loaded.config.get_server("test-server").is_some());

        match loaded.source {
            ConfigSource::Env(path) => assert_eq!(path, config_path),
            _ => panic!("Expected Env source"),
        }

        // Clean up
        unsafe {
            std::env::remove_var(MCPG_CONFIG_VAR);
        }
    }

    #[test]
    #[serial]
    fn test_load_from_project_directory() {
        // Save current directory
        let original_dir = std::env::current_dir().unwrap();

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(PROJECT_MCP_CONFIG_FILE);
        fs::write(&config_path, create_test_config_json()).unwrap();

        // Change to temp directory
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Ensure no env var is set
        unsafe {
            std::env::remove_var(MCPG_CONFIG_VAR);
        }

        let result = LoadedConfig::load(None);
        assert!(result.is_ok());

        let loaded = result.unwrap();
        assert!(loaded.config.get_server("test-server").is_some());

        match loaded.source {
            ConfigSource::Project(path) => {
                // Use canonicalized paths for comparison to handle symlinks
                let canonical_path = path.canonicalize().unwrap();
                let canonical_expected = config_path.canonicalize().unwrap();
                assert_eq!(canonical_path, canonical_expected);
            }
            _ => panic!("Expected Project source"),
        }

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    #[serial]
    fn test_load_priority_provided_over_env() {
        let temp_dir = TempDir::new().unwrap();

        // Create two different configs
        let provided_path = temp_dir.path().join("provided.json");
        fs::write(
            &provided_path,
            r#"{
            "servers": {
                "provided-server": {
                    "command": "provided"
                }
            }
        }"#,
        )
        .unwrap();

        let env_path = temp_dir.path().join("env.json");
        fs::write(
            &env_path,
            r#"{
            "servers": {
                "env-server": {
                    "command": "env"
                }
            }
        }"#,
        )
        .unwrap();

        // Set env var
        unsafe {
            std::env::set_var(MCPG_CONFIG_VAR, env_path.to_str().unwrap());
        }

        let result = LoadedConfig::load(Some(provided_path.clone()));
        assert!(result.is_ok());

        let loaded = result.unwrap();
        assert!(loaded.config.get_server("provided-server").is_some());
        assert!(loaded.config.get_server("env-server").is_none());

        match loaded.source {
            ConfigSource::Provided(_) => {}
            _ => panic!("Expected Provided source to take priority"),
        }

        // Clean up
        unsafe {
            std::env::remove_var(MCPG_CONFIG_VAR);
        }
    }

    #[test]
    #[serial]
    fn test_load_priority_env_over_project() {
        let original_dir = std::env::current_dir().unwrap();
        let temp_dir = TempDir::new().unwrap();

        // Create project config
        let project_path = temp_dir.path().join(PROJECT_MCP_CONFIG_FILE);
        fs::write(
            &project_path,
            r#"{
            "servers": {
                "project-server": {
                    "command": "project"
                }
            }
        }"#,
        )
        .unwrap();

        // Create env config
        let env_path = temp_dir.path().join("env.json");
        fs::write(
            &env_path,
            r#"{
            "servers": {
                "env-server": {
                    "command": "env"
                }
            }
        }"#,
        )
        .unwrap();

        // Get absolute path before changing directory
        let absolute_env_path = env_path.canonicalize().unwrap();

        // Change to temp directory and set env var
        std::env::set_current_dir(temp_dir.path()).unwrap();
        unsafe {
            std::env::set_var(MCPG_CONFIG_VAR, absolute_env_path.to_str().unwrap());
        }

        let result = LoadedConfig::load(None);

        // Restore environment first to avoid affecting other tests
        unsafe {
            std::env::remove_var(MCPG_CONFIG_VAR);
        }
        std::env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_ok());

        let loaded = result.unwrap();
        assert!(loaded.config.get_server("env-server").is_some());
        assert!(loaded.config.get_server("project-server").is_none());

        match loaded.source {
            ConfigSource::Env(_) => {}
            _ => panic!("Expected Env source to take priority"),
        }
    }

    #[test]
    #[serial]
    fn test_load_no_config_found() {
        let original_dir = std::env::current_dir().unwrap();
        let temp_dir = TempDir::new().unwrap();

        // Change to empty temp directory
        std::env::set_current_dir(temp_dir.path()).unwrap();
        unsafe {
            std::env::remove_var(MCPG_CONFIG_VAR);
        }

        let result = LoadedConfig::load(None);

        // Restore original directory first to avoid affecting other tests
        std::env::set_current_dir(&original_dir).unwrap();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No configuration file found")
        );
    }

    #[test]
    fn test_from_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.json");
        fs::write(&config_path, create_test_config_json()).unwrap();

        let result = LoadedConfig::from_file(&config_path);
        assert!(result.is_ok());

        let config = result.unwrap();
        assert!(config.get_server("test-server").is_some());
    }

    #[test]
    fn test_from_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.json");

        let result = LoadedConfig::from_file(&config_path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to read config file")
        );
    }

    #[test]
    fn test_from_file_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid.json");
        fs::write(&config_path, "{ invalid json }").unwrap();

        let result = LoadedConfig::from_file(&config_path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse config file")
        );
    }

    #[test]
    fn test_from_file_invalid_config_structure() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid_structure.json");
        fs::write(&config_path, create_invalid_config_json()).unwrap();

        let result = LoadedConfig::from_file(&config_path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse config file")
        );
    }

    #[test]
    fn test_config_source_display() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.json");

        // Just verify that the enum variants exist and can be constructed
        let _provided = ConfigSource::Provided(path.clone());
        let _env = ConfigSource::Env(path.clone());
        let _project = ConfigSource::Project(path.clone());
        let _user = ConfigSource::User(path.clone());
    }
}
