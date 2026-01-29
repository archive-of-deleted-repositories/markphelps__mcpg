use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Map of server name → server configuration
    pub servers: HashMap<String, ServerConfig>,
}

/// Configuration for a single MCP server
///
/// Supports two transport types:
/// - stdio: Run a subprocess (requires `command`)
/// - HTTP: Connect to an HTTP endpoint (requires `url`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// The transport-specific configuration (stdio or HTTP)
    #[serde(flatten)]
    pub transport: TransportConfig,

    /// Timeout in seconds for operations (default: 30)
    /// - For stdio: timeout reading from stdout
    /// - For HTTP: timeout for HTTP requests
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    30
}

/// Transport-specific configuration
///
/// This enum is flattened when serializing/deserializing JSON,
/// so the fields appear directly in the ServerConfig object.
///
/// When serializing, the `type` field is always included.
/// When deserializing, the `type` field is optional and will be inferred:
/// - If `command` is present, it's stdio
/// - If `url` is present, it's http
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TransportConfig {
    /// Stdio transport: runs a subprocess
    Stdio {
        /// The executable to run (e.g., "uvx", "npx")
        command: String,
        /// Arguments to pass to the command
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Vec<String>>,
        /// Environment variables to set
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    /// HTTP transport: connects to an HTTP endpoint
    Http {
        /// The HTTP endpoint URL
        url: String,
        /// HTTP headers to send
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
}

impl<'de> Deserialize<'de> for TransportConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        use serde_json::Value;

        let mut value = Value::deserialize(deserializer)?;

        // If type field is missing, infer it from the fields present
        if let Value::Object(ref mut map) = value
            && !map.contains_key("type")
        {
            let inferred_type = if map.contains_key("command") {
                "stdio"
            } else if map.contains_key("url") {
                "http"
            } else {
                return Err(D::Error::custom(
                    "Cannot infer transport type: must have either 'command' or 'url' field",
                ));
            };
            map.insert("type".to_string(), Value::String(inferred_type.to_string()));
        }

        // Now deserialize with the type field present
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "lowercase")]
        enum TransportConfigHelper {
            Stdio {
                command: String,
                #[serde(skip_serializing_if = "Option::is_none")]
                args: Option<Vec<String>>,
                #[serde(skip_serializing_if = "Option::is_none")]
                env: Option<HashMap<String, String>>,
            },
            Http {
                url: String,
                #[serde(skip_serializing_if = "Option::is_none")]
                headers: Option<HashMap<String, String>>,
            },
        }

        let helper = TransportConfigHelper::deserialize(value).map_err(D::Error::custom)?;

        Ok(match helper {
            TransportConfigHelper::Stdio { command, args, env } => {
                TransportConfig::Stdio { command, args, env }
            }
            TransportConfigHelper::Http { url, headers } => TransportConfig::Http { url, headers },
        })
    }
}

impl ServerConfig {
    /// Validate the server configuration
    ///
    /// Returns Ok(()) if valid, or an error describing the validation failure.
    pub fn validate(&self) -> Result<()> {
        match &self.transport {
            TransportConfig::Stdio { command, .. } => {
                if command.trim().is_empty() {
                    bail!("Command for stdio transport cannot be empty");
                }
            }
            TransportConfig::Http { url, .. } => {
                if url.trim().is_empty() {
                    bail!("URL for HTTP transport cannot be empty");
                }
            }
        }
        Ok(())
    }
}

impl Config {
    pub fn get_server(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.get(name)
    }

    pub fn servers(&self) -> impl Iterator<Item = (&String, &ServerConfig)> {
        self.servers.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_server_config_validation() {
        let config = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "npx".to_string(),
                args: Some(vec![
                    "@modelcontextprotocol/server-brave-search".to_string(),
                ]),
                env: Some(HashMap::from([(
                    "BRAVE_API_KEY".to_string(),
                    "test-key".to_string(),
                )])),
            },
            timeout_secs: 30,
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_stdio_server_config_empty_command() {
        let config = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "".to_string(),
                args: None,
                env: None,
            },
            timeout_secs: 30,
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_stdio_server_config_whitespace_command() {
        let config = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "   ".to_string(),
                args: None,
                env: None,
            },
            timeout_secs: 30,
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_http_server_config_validation() {
        let config = ServerConfig {
            transport: TransportConfig::Http {
                url: "http://localhost:8080".to_string(),
                headers: Some(HashMap::from([(
                    "Authorization".to_string(),
                    "Bearer token".to_string(),
                )])),
            },
            timeout_secs: 30,
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_http_server_config_empty_url() {
        let config = ServerConfig {
            transport: TransportConfig::Http {
                url: "".to_string(),
                headers: None,
            },
            timeout_secs: 30,
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_http_server_config_whitespace_url() {
        let config = ServerConfig {
            transport: TransportConfig::Http {
                url: "   ".to_string(),
                headers: None,
            },
            timeout_secs: 30,
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_config_get_server() {
        let mut servers = HashMap::new();
        servers.insert(
            "brave-search".to_string(),
            ServerConfig {
                transport: TransportConfig::Stdio {
                    command: "npx".to_string(),
                    args: None,
                    env: None,
                },
                timeout_secs: 30,
            },
        );

        let config = Config { servers };

        assert!(config.get_server("brave-search").is_some());
        assert!(config.get_server("nonexistent").is_none());
    }

    #[test]
    fn test_config_servers_iterator() {
        let mut servers = HashMap::new();
        servers.insert(
            "server1".to_string(),
            ServerConfig {
                transport: TransportConfig::Stdio {
                    command: "cmd1".to_string(),
                    args: None,
                    env: None,
                },
                timeout_secs: 30,
            },
        );
        servers.insert(
            "server2".to_string(),
            ServerConfig {
                transport: TransportConfig::Http {
                    url: "http://localhost".to_string(),
                    headers: None,
                },
                timeout_secs: 30,
            },
        );

        let config = Config { servers };
        let server_names: Vec<&String> = config.servers().map(|(name, _)| name).collect();

        assert_eq!(server_names.len(), 2);
        assert!(server_names.contains(&&"server1".to_string()));
        assert!(server_names.contains(&&"server2".to_string()));
    }

    #[test]
    fn test_stdio_config_serialization() {
        let config = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "npx".to_string(),
                args: Some(vec!["server".to_string()]),
                env: Some(HashMap::from([("KEY".to_string(), "value".to_string())])),
            },
            timeout_secs: 30,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ServerConfig = serde_json::from_str(&json).unwrap();

        match deserialized.transport {
            TransportConfig::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(args, Some(vec!["server".to_string()]));
                assert_eq!(env.unwrap().get("KEY").unwrap(), "value");
            }
            _ => panic!("Expected Stdio transport"),
        }
    }

    #[test]
    fn test_http_config_serialization() {
        let config = ServerConfig {
            transport: TransportConfig::Http {
                url: "http://localhost:8080".to_string(),
                headers: Some(HashMap::from([(
                    "Auth".to_string(),
                    "Bearer token".to_string(),
                )])),
            },
            timeout_secs: 30,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ServerConfig = serde_json::from_str(&json).unwrap();

        match deserialized.transport {
            TransportConfig::Http { url, headers } => {
                assert_eq!(url, "http://localhost:8080");
                assert_eq!(headers.unwrap().get("Auth").unwrap(), "Bearer token");
            }
            _ => panic!("Expected Http transport"),
        }
    }

    #[test]
    fn test_stdio_config_minimal_serialization() {
        let json = r#"{"command":"npx"}"#;
        let config: ServerConfig = serde_json::from_str(json).unwrap();

        match config.transport {
            TransportConfig::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(args, None);
                assert_eq!(env, None);
            }
            _ => panic!("Expected Stdio transport"),
        }
    }

    #[test]
    fn test_http_config_minimal_serialization() {
        let json = r#"{"url":"http://localhost"}"#;
        let config: ServerConfig = serde_json::from_str(json).unwrap();

        match config.transport {
            TransportConfig::Http { url, headers } => {
                assert_eq!(url, "http://localhost");
                assert_eq!(headers, None);
            }
            _ => panic!("Expected Http transport"),
        }
    }

    #[test]
    fn test_server_config_default_timeout() {
        let json = r#"{"command":"npx"}"#;
        let config: ServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_server_config_custom_timeout() {
        let json = r#"{"command":"npx","timeout_secs":60}"#;
        let config: ServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_stdio_config_with_explicit_type() {
        let json = r#"{"type":"stdio","command":"npx","args":["server"]}"#;
        let config: ServerConfig = serde_json::from_str(json).unwrap();

        match config.transport {
            TransportConfig::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(args, Some(vec!["server".to_string()]));
                assert_eq!(env, None);
            }
            _ => panic!("Expected Stdio transport"),
        }
    }

    #[test]
    fn test_http_config_with_explicit_type() {
        let json = r#"{"type":"http","url":"http://localhost:8080"}"#;
        let config: ServerConfig = serde_json::from_str(json).unwrap();

        match config.transport {
            TransportConfig::Http { url, headers } => {
                assert_eq!(url, "http://localhost:8080");
                assert_eq!(headers, None);
            }
            _ => panic!("Expected Http transport"),
        }
    }

    #[test]
    fn test_serialization_includes_type_field() {
        let stdio_config = ServerConfig {
            transport: TransportConfig::Stdio {
                command: "npx".to_string(),
                args: Some(vec!["server".to_string()]),
                env: None,
            },
            timeout_secs: 30,
        };

        let json = serde_json::to_string(&stdio_config).unwrap();
        assert!(json.contains(r#""type":"stdio""#));

        let http_config = ServerConfig {
            transport: TransportConfig::Http {
                url: "http://localhost:8080".to_string(),
                headers: None,
            },
            timeout_secs: 30,
        };

        let json = serde_json::to_string(&http_config).unwrap();
        assert!(json.contains(r#""type":"http""#));
    }
}
