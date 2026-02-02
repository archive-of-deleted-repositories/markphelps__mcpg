use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Parse a JSONC string (JSON with comments and trailing commas) into a typed value.
///
/// Strips `//` line comments, `/* */` block comments, and trailing commas before
/// parsing with serde_json. This is backwards-compatible with standard JSON.
pub fn parse_jsonc<T: for<'de> Deserialize<'de>>(content: &str) -> serde_json::Result<T> {
    let mut stripped = content.to_string();
    // Best-effort: if stripping fails (e.g., unclosed comment), try parsing as-is
    let _ = json_strip_comments::strip(&mut stripped);
    serde_json::from_str(&stripped)
}

/// Generic config file handler for JSON-based agent configurations
///
/// Provides common functionality for loading, saving, and backing up
/// JSON configuration files. Agents can use this to avoid duplicating
/// file I/O and backup logic.
pub struct ConfigFile<T> {
    path: PathBuf,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> ConfigFile<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    /// Create a new ConfigFile handler for the given path
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Check if the config file exists
    #[allow(dead_code)]
    pub fn exists(&self) -> bool {
        self.path().exists()
    }

    /// Get the path to the config file
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load and deserialize the config file (supports JSONC comments and trailing commas)
    pub fn load(&self) -> Result<T> {
        let content = fs::read_to_string(self.path())
            .with_context(|| format!("Failed to read config: {}", self.path().display()))?;
        parse_jsonc(&content)
            .with_context(|| format!("Failed to parse config: {}", self.path().display()))
    }

    /// Serialize and save the config file
    pub fn save(&self, config: &T) -> Result<()> {
        let content = serde_json::to_string_pretty(config).context("Failed to serialize config")?;
        fs::write(self.path(), content)
            .with_context(|| format!("Failed to write config: {}", self.path().display()))
    }

    /// Create a backup of the config file.
    ///
    /// The backup is always written to `.json.backup`. If a previous backup
    /// already exists at that path, it is moved to a timestamped path first
    /// (e.g. `.json.backup.1234567890`) so that `.json.backup` always
    /// represents the most recent backup.
    pub fn create_backup(&self) -> Result<PathBuf> {
        if !self.path().exists() {
            anyhow::bail!("Cannot backup non-existent file: {}", self.path().display());
        }

        let backup_path = self.path().with_extension("json.backup");

        // If a previous backup exists, move it to a timestamped path
        if backup_path.exists() {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let timestamped_path = self
                .path()
                .with_extension(format!("json.backup.{}", timestamp));
            fs::rename(&backup_path, &timestamped_path).with_context(|| {
                format!(
                    "Failed to move existing backup to {}",
                    timestamped_path.display()
                )
            })?;
        }

        fs::copy(self.path(), &backup_path)
            .with_context(|| format!("Failed to create backup at {}", backup_path.display()))?;

        Ok(backup_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    #[test]
    fn test_exists_returns_false_for_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.json");

        let config_file = ConfigFile::<TestConfig>::new(config_path);
        assert!(!config_file.exists());
    }

    #[test]
    fn test_exists_returns_true_when_file_present() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.json");
        fs::write(&config_path, "{}").unwrap();

        let config_file = ConfigFile::<TestConfig>::new(config_path);
        assert!(config_file.exists());
    }

    #[test]
    fn test_load_and_save_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.json");

        let config_file = ConfigFile::new(config_path.clone());
        let test_config = TestConfig {
            name: "test".to_string(),
            value: 42,
        };

        config_file.save(&test_config).unwrap();
        let loaded: TestConfig = config_file.load().unwrap();

        assert_eq!(loaded, test_config);
    }

    #[test]
    fn test_load_fails_for_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.json");

        let config_file = ConfigFile::<TestConfig>::new(config_path);
        assert!(config_file.load().is_err());
    }

    #[test]
    fn test_backup_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.json");

        fs::write(&config_path, "{}").unwrap();

        let config_file = ConfigFile::<TestConfig>::new(config_path.clone());
        let backup_path = config_file.create_backup().unwrap();

        assert!(backup_path.exists());
        assert!(backup_path.to_string_lossy().contains(".json.backup"));
    }

    #[test]
    fn test_backup_moves_existing_to_timestamped() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.json");

        fs::write(&config_path, r#"{"version": 1}"#).unwrap();

        let config_file = ConfigFile::<TestConfig>::new(config_path.clone());

        // Create first backup
        let backup1 = config_file.create_backup().unwrap();
        assert_eq!(backup1, config_path.with_extension("json.backup"));

        // Update the config file
        fs::write(&config_path, r#"{"version": 2}"#).unwrap();

        // Create second backup - should still be .json.backup
        let backup2 = config_file.create_backup().unwrap();
        assert_eq!(backup2, config_path.with_extension("json.backup"));

        // The old backup should have been moved to a timestamped path
        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".json.backup."))
            .collect();
        assert_eq!(entries.len(), 1, "Expected one timestamped backup");
    }

    #[test]
    fn test_backup_fails_for_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.json");

        let config_file = ConfigFile::<TestConfig>::new(config_path);
        assert!(config_file.create_backup().is_err());
    }

    // ---- parse_jsonc tests ----

    #[test]
    fn test_parse_jsonc_plain_json() {
        let json = r#"{"name": "test", "value": 42}"#;
        let config: TestConfig = parse_jsonc(json).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_parse_jsonc_line_comments() {
        let jsonc = r#"{
            // This is a comment
            "name": "test",
            "value": 42 // inline comment
        }"#;
        let config: TestConfig = parse_jsonc(jsonc).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_parse_jsonc_block_comments() {
        let jsonc = r#"{
            /* Block comment */
            "name": "test",
            "value": /* inline */ 42
        }"#;
        let config: TestConfig = parse_jsonc(jsonc).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_parse_jsonc_trailing_commas() {
        let jsonc = r#"{
            "name": "test",
            "value": 42,
        }"#;
        let config: TestConfig = parse_jsonc(jsonc).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_parse_jsonc_comments_and_trailing_commas() {
        let jsonc = r#"{
            // A JSONC config file
            "name": "test", // the name
            "value": 42, /* trailing comma */
        }"#;
        let config: TestConfig = parse_jsonc(jsonc).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_parse_jsonc_invalid_json_fails() {
        let bad = "{ not valid json }";
        let result: serde_json::Result<TestConfig> = parse_jsonc(bad);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_jsonc_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.json");
        fs::write(
            &config_path,
            r#"{
                // JSONC config
                "name": "loaded",
                "value": 99,
            }"#,
        )
        .unwrap();

        let config_file = ConfigFile::<TestConfig>::new(config_path);
        let loaded = config_file.load().unwrap();
        assert_eq!(loaded.name, "loaded");
        assert_eq!(loaded.value, 99);
    }
}
