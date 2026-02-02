use crate::config::APP_NAME;
use crate::config::ServerConfig;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

use super::ServerWithSource;

/// Normalized result from loading a single config source.
///
/// Each agent produces this from their native config format, then the shared
/// algorithms operate on it to merge servers across sources, apply changes, etc.
pub(crate) struct LoadedSource {
    /// Active servers (already converted to ServerConfig), excluding "mcpg"
    pub servers: HashMap<String, ServerConfig>,
    /// Server names that are disabled (for cross-source disabling)
    pub disabled_names: Vec<String>,
    /// Whether this source contains a "mcpg" server entry
    pub has_mcpg: bool,
    /// Whether mcpg is explicitly disabled in this source
    pub mcpg_disabled: bool,
}

/// Collect servers from multiple prioritized config sources.
///
/// Sources are in priority order (highest first). Lower priority sources are
/// processed first so higher priority sources can overwrite. Disabled server
/// names from any source remove the server from the final result.
pub(crate) fn collect_servers(
    sources: &[PathBuf],
    load: impl Fn(&Path) -> Result<LoadedSource>,
) -> Result<HashMap<String, ServerWithSource>> {
    let mut servers: HashMap<String, ServerWithSource> = HashMap::new();
    let mut all_disabled: Vec<String> = Vec::new();

    debug!("Checking {} config sources for MCP servers", sources.len());

    // Process in reverse order (lowest priority first)
    for source in sources.iter().rev() {
        let exists = source.exists();
        debug!(
            source = %source.display(),
            exists = exists,
            "Checking config source"
        );

        if !exists {
            continue;
        }

        match load(source) {
            Ok(loaded) => {
                debug!(
                    source = %source.display(),
                    server_count = loaded.servers.len(),
                    disabled_count = loaded.disabled_names.len(),
                    servers = ?loaded.servers.keys().collect::<Vec<_>>(),
                    "Loaded config from source"
                );

                all_disabled.extend(loaded.disabled_names);

                for (name, config) in loaded.servers {
                    servers.insert(
                        name,
                        ServerWithSource {
                            config,
                            source: source.clone(),
                        },
                    );
                }
            }
            Err(e) => {
                debug!(
                    source = %source.display(),
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

/// Apply changes across multiple config sources.
///
/// Groups servers_to_disable by their source file, determines the highest
/// priority source to add mcpg to, then calls `update_fn` for each source
/// that needs modifications.
pub(crate) fn apply_changes_to_sources(
    sources: &[PathBuf],
    servers: &HashMap<String, ServerWithSource>,
    servers_to_disable: &[String],
    update_fn: impl Fn(&Path, &[String], bool) -> Result<()>,
) -> Result<()> {
    // Group servers by their source file
    let mut sources_to_update: HashMap<PathBuf, Vec<String>> = HashMap::new();

    for server_name in servers_to_disable {
        if let Some(server_with_source) = servers.get(server_name) {
            sources_to_update
                .entry(server_with_source.source.clone())
                .or_default()
                .push(server_name.clone());
        }
    }

    // Determine the highest priority source to add mcpg to:
    // - If there are servers to disable, use the highest priority source with servers
    // - Otherwise, use the highest priority existing config file
    let mcpg_target_source = if !sources_to_update.is_empty() {
        sources
            .iter()
            .find(|s| sources_to_update.contains_key(*s))
            .cloned()
    } else {
        sources.iter().find(|s| s.exists()).cloned()
    };

    // Update each source file
    for (source_path, servers_in_source) in &sources_to_update {
        let should_add_mcpg = mcpg_target_source
            .as_ref()
            .map(|s| s == source_path)
            .unwrap_or(false);

        update_fn(source_path, servers_in_source, should_add_mcpg)?;
    }

    // If no servers were disabled but we have a target for mcpg, add it there
    if sources_to_update.is_empty() {
        if let Some(target) = mcpg_target_source {
            update_fn(&target, &[], true)?;
        }
    }

    Ok(())
}

/// Find the config source where mcpg is configured.
///
/// Returns None if mcpg is not found in any source, or if mcpg is explicitly
/// disabled in any source.
pub(crate) fn find_mcpg_source(
    sources: &[PathBuf],
    load: impl Fn(&Path) -> Result<LoadedSource>,
) -> Option<PathBuf> {
    let mut all_disabled_names: Vec<String> = Vec::new();
    let mut any_mcpg_disabled = false;
    let mut mcpg_source: Option<PathBuf> = None;

    for source in sources {
        if !source.exists() {
            continue;
        }

        if let Ok(loaded) = load(source) {
            all_disabled_names.extend(loaded.disabled_names);
            if loaded.mcpg_disabled {
                any_mcpg_disabled = true;
            }
            if loaded.has_mcpg && mcpg_source.is_none() {
                mcpg_source = Some(source.clone());
            }
        }
    }

    // Return None if mcpg is disabled
    if any_mcpg_disabled || all_disabled_names.contains(&APP_NAME.to_string()) {
        return None;
    }

    mcpg_source
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TransportConfig;
    use anyhow::bail;
    use std::cell::RefCell;
    use std::fs;
    use tempfile::TempDir;

    fn stdio_server_config(command: &str) -> ServerConfig {
        ServerConfig {
            transport: TransportConfig::Stdio {
                command: command.to_string(),
                args: None,
                env: None,
            },
            timeout_secs: 30,
        }
    }

    /// Create a temp dir with source files that exist on disk.
    /// Returns (temp_dir, vec of source paths).
    fn make_sources(names: &[&str]) -> (TempDir, Vec<PathBuf>) {
        let tmp = TempDir::new().unwrap();
        let paths: Vec<PathBuf> = names
            .iter()
            .map(|name| {
                let p = tmp.path().join(name);
                if let Some(parent) = p.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&p, "{}").unwrap();
                p
            })
            .collect();
        (tmp, paths)
    }

    // ---- collect_servers tests ----

    #[test]
    fn test_collect_empty_sources() {
        let result = collect_servers(&[], |_| bail!("should not be called")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_all_sources_nonexistent() {
        let sources = vec![
            PathBuf::from("/nonexistent/a.json"),
            PathBuf::from("/nonexistent/b.json"),
        ];
        let result = collect_servers(&sources, |_| bail!("should not be called")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_load_errors_skipped() {
        let (_tmp, sources) = make_sources(&["a.json"]);
        let result = collect_servers(&sources, |_| bail!("parse error")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_single_source() {
        let (_tmp, sources) = make_sources(&["a.json"]);
        let result = collect_servers(&sources, |_| {
            Ok(LoadedSource {
                servers: [("github".to_string(), stdio_server_config("gh"))].into(),
                disabled_names: vec![],
                has_mcpg: false,
                mcpg_disabled: false,
            })
        })
        .unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("github"));
    }

    #[test]
    fn test_collect_higher_priority_overwrites() {
        let (_tmp, sources) = make_sources(&["high.json", "low.json"]);
        let high = sources[0].clone();
        let low = sources[1].clone();

        let result = collect_servers(&sources, |path| {
            if path == high {
                Ok(LoadedSource {
                    servers: [("shared".to_string(), stdio_server_config("high-cmd"))].into(),
                    disabled_names: vec![],
                    has_mcpg: false,
                    mcpg_disabled: false,
                })
            } else if path == low {
                Ok(LoadedSource {
                    servers: [("shared".to_string(), stdio_server_config("low-cmd"))].into(),
                    disabled_names: vec![],
                    has_mcpg: false,
                    mcpg_disabled: false,
                })
            } else {
                bail!("unexpected source")
            }
        })
        .unwrap();

        assert_eq!(result.len(), 1);
        let shared = &result["shared"];
        assert_eq!(shared.source, high);
        match &shared.config.transport {
            TransportConfig::Stdio { command, .. } => assert_eq!(command, "high-cmd"),
            _ => panic!("expected stdio"),
        }
    }

    #[test]
    fn test_collect_disabled_removes_server() {
        let (_tmp, sources) = make_sources(&["high.json", "low.json"]);
        let high = sources[0].clone();
        let low = sources[1].clone();

        let result = collect_servers(&sources, |path| {
            if path == high {
                Ok(LoadedSource {
                    servers: [("github".to_string(), stdio_server_config("gh"))].into(),
                    disabled_names: vec![],
                    has_mcpg: false,
                    mcpg_disabled: false,
                })
            } else if path == low {
                Ok(LoadedSource {
                    servers: HashMap::new(),
                    disabled_names: vec!["github".to_string()],
                    has_mcpg: false,
                    mcpg_disabled: false,
                })
            } else {
                bail!("unexpected source")
            }
        })
        .unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_merges_across_sources() {
        let (_tmp, sources) = make_sources(&["high.json", "low.json"]);
        let high = sources[0].clone();
        let low = sources[1].clone();

        let result = collect_servers(&sources, |path| {
            if path == high {
                Ok(LoadedSource {
                    servers: [("github".to_string(), stdio_server_config("gh"))].into(),
                    disabled_names: vec![],
                    has_mcpg: false,
                    mcpg_disabled: false,
                })
            } else if path == low {
                Ok(LoadedSource {
                    servers: [("slack".to_string(), stdio_server_config("slack"))].into(),
                    disabled_names: vec![],
                    has_mcpg: false,
                    mcpg_disabled: false,
                })
            } else {
                bail!("unexpected source")
            }
        })
        .unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result["github"].source, high);
        assert_eq!(result["slack"].source, low);
    }

    // ---- apply_changes_to_sources tests ----

    #[test]
    fn test_apply_empty_disable_list_adds_mcpg_to_first_existing() {
        let (_tmp, sources) = make_sources(&["high.json", "low.json"]);
        let high = sources[0].clone();

        let calls: RefCell<Vec<(PathBuf, Vec<String>, bool)>> = RefCell::new(vec![]);
        apply_changes_to_sources(&sources, &HashMap::new(), &[], |path, servers, add_mcpg| {
            calls
                .borrow_mut()
                .push((path.to_path_buf(), servers.to_vec(), add_mcpg));
            Ok(())
        })
        .unwrap();

        let calls = calls.into_inner();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, high);
        assert!(calls[0].1.is_empty());
        assert!(calls[0].2); // should_add_mcpg
    }

    #[test]
    fn test_apply_groups_by_source() {
        let (_tmp, sources) = make_sources(&["high.json", "low.json"]);
        let high = sources[0].clone();
        let low = sources[1].clone();

        let servers: HashMap<String, ServerWithSource> = [
            (
                "github".to_string(),
                ServerWithSource {
                    config: stdio_server_config("gh"),
                    source: high.clone(),
                },
            ),
            (
                "slack".to_string(),
                ServerWithSource {
                    config: stdio_server_config("slack"),
                    source: low.clone(),
                },
            ),
        ]
        .into();

        let calls: RefCell<Vec<(PathBuf, Vec<String>, bool)>> = RefCell::new(vec![]);
        apply_changes_to_sources(
            &sources,
            &servers,
            &["github".to_string(), "slack".to_string()],
            |path, disabled, add_mcpg| {
                calls
                    .borrow_mut()
                    .push((path.to_path_buf(), disabled.to_vec(), add_mcpg));
                Ok(())
            },
        )
        .unwrap();

        let calls = calls.into_inner();
        assert_eq!(calls.len(), 2);

        let high_call = calls.iter().find(|(p, _, _)| p == &high).unwrap();
        assert!(high_call.1.contains(&"github".to_string()));
        assert!(high_call.2); // mcpg added to highest priority source

        let low_call = calls.iter().find(|(p, _, _)| p == &low).unwrap();
        assert!(low_call.1.contains(&"slack".to_string()));
        assert!(!low_call.2); // mcpg NOT added to lower priority
    }

    #[test]
    fn test_apply_unknown_server_ignored() {
        let (_tmp, sources) = make_sources(&["a.json"]);

        let calls: RefCell<Vec<(PathBuf, Vec<String>, bool)>> = RefCell::new(vec![]);
        apply_changes_to_sources(
            &sources,
            &HashMap::new(),
            &["nonexistent".to_string()],
            |path, disabled, add_mcpg| {
                calls
                    .borrow_mut()
                    .push((path.to_path_buf(), disabled.to_vec(), add_mcpg));
                Ok(())
            },
        )
        .unwrap();

        // No servers matched, so falls through to "add mcpg to first existing"
        let calls = calls.into_inner();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].1.is_empty());
        assert!(calls[0].2);
    }

    // ---- find_mcpg_source tests ----

    #[test]
    fn test_find_mcpg_not_present() {
        let (_tmp, sources) = make_sources(&["a.json"]);
        let result = find_mcpg_source(&sources, |_| {
            Ok(LoadedSource {
                servers: HashMap::new(),
                disabled_names: vec![],
                has_mcpg: false,
                mcpg_disabled: false,
            })
        });
        assert!(result.is_none());
    }

    #[test]
    fn test_find_mcpg_in_single_source() {
        let (_tmp, sources) = make_sources(&["a.json"]);
        let result = find_mcpg_source(&sources, |_| {
            Ok(LoadedSource {
                servers: HashMap::new(),
                disabled_names: vec![],
                has_mcpg: true,
                mcpg_disabled: false,
            })
        });
        assert_eq!(result, Some(sources[0].clone()));
    }

    #[test]
    fn test_find_mcpg_returns_highest_priority() {
        let (_tmp, sources) = make_sources(&["high.json", "low.json"]);
        let result = find_mcpg_source(&sources, |_| {
            Ok(LoadedSource {
                servers: HashMap::new(),
                disabled_names: vec![],
                has_mcpg: true,
                mcpg_disabled: false,
            })
        });
        assert_eq!(result, Some(sources[0].clone()));
    }

    #[test]
    fn test_find_mcpg_disabled_returns_none() {
        let (_tmp, sources) = make_sources(&["a.json"]);
        let result = find_mcpg_source(&sources, |_| {
            Ok(LoadedSource {
                servers: HashMap::new(),
                disabled_names: vec![],
                has_mcpg: true,
                mcpg_disabled: true,
            })
        });
        assert!(result.is_none());
    }

    #[test]
    fn test_find_mcpg_in_disabled_names_returns_none() {
        let (_tmp, sources) = make_sources(&["a.json"]);
        let result = find_mcpg_source(&sources, |_| {
            Ok(LoadedSource {
                servers: HashMap::new(),
                disabled_names: vec!["mcpg".to_string()],
                has_mcpg: true,
                mcpg_disabled: false,
            })
        });
        assert!(result.is_none());
    }

    #[test]
    fn test_find_mcpg_disabled_in_lower_priority_overrides() {
        let (_tmp, sources) = make_sources(&["high.json", "low.json"]);
        let high = sources[0].clone();

        let result = find_mcpg_source(&sources, |path| {
            if path == high {
                Ok(LoadedSource {
                    servers: HashMap::new(),
                    disabled_names: vec![],
                    has_mcpg: true,
                    mcpg_disabled: false,
                })
            } else {
                Ok(LoadedSource {
                    servers: HashMap::new(),
                    disabled_names: vec![],
                    has_mcpg: false,
                    mcpg_disabled: true,
                })
            }
        });
        assert!(result.is_none());
    }

    #[test]
    fn test_find_mcpg_all_nonexistent_sources() {
        let sources = vec![
            PathBuf::from("/nonexistent/a.json"),
            PathBuf::from("/nonexistent/b.json"),
        ];
        let result = find_mcpg_source(&sources, |_| bail!("should not be called"));
        assert!(result.is_none());
    }

    #[test]
    fn test_find_mcpg_load_error_skipped() {
        let (_tmp, sources) = make_sources(&["a.json"]);
        let result = find_mcpg_source(&sources, |_| bail!("parse error"));
        assert!(result.is_none());
    }
}
