use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
    pub fn exists(&self) -> bool {
        self.path().exists()
    }

    /// Get the path to the config file
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load and deserialize the config file
    pub fn load(&self) -> Result<T> {
        let content = fs::read_to_string(self.path())
            .with_context(|| format!("Failed to read config: {}", self.path().display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config: {}", self.path().display()))
    }

    /// Serialize and save the config file
    pub fn save(&self, config: &T) -> Result<()> {
        let content = serde_json::to_string_pretty(config).context("Failed to serialize config")?;
        fs::write(self.path(), content)
            .with_context(|| format!("Failed to write config: {}", self.path().display()))
    }

    /// Create a timestamped backup of the config file
    ///
    /// Returns the path to the backup file.
    /// If .json.backup already exists, adds a timestamp.
    pub fn create_backup(&self) -> Result<PathBuf> {
        if !self.path().exists() {
            anyhow::bail!("Cannot backup non-existent file: {}", self.path().display());
        }

        let backup_path = self.path().with_extension("json.backup");
        // If backup exists, add timestamp
        let backup_path = if backup_path.exists() {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            self.path()
                .with_extension(format!("json.backup.{}", timestamp))
        } else {
            backup_path
        };

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
    fn test_backup_adds_timestamp_when_backup_exists() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test.json");

        fs::write(&config_path, "{}").unwrap();

        let config_file = ConfigFile::<TestConfig>::new(config_path.clone());

        // Create first backup
        let backup1 = config_file.create_backup().unwrap();
        assert_eq!(backup1, config_path.with_extension("json.backup"));

        // Create second backup - should have timestamp
        let backup2 = config_file.create_backup().unwrap();
        assert_ne!(backup1, backup2);
        assert!(backup2.to_string_lossy().contains(".json.backup."));
    }

    #[test]
    fn test_backup_fails_for_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.json");

        let config_file = ConfigFile::<TestConfig>::new(config_path);
        assert!(config_file.create_backup().is_err());
    }
}
