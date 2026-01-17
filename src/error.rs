use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum McpgError {
    #[error("Configuration file not found. Searched paths: {searched_paths:?}")]
    ConfigNotFound { searched_paths: Vec<PathBuf> },
    #[error("Configuration validation error in file {path:?}: {message}")]
    ConfigValidation { path: PathBuf, message: String },
    #[error("Failed to parse configuration file {path:?}: {source}")]
    ConfigParse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("Environment variable '{var_name}' not found in context '{context}'")]
    EnvVarNotFound { var_name: String, context: String },
    #[error("Server '{name}' not found. Available servers: {available:?}")]
    ServerNotFound {
        name: String,
        available: Vec<String>,
    },
    #[error("Tool '{name}' not found on server '{server}'. Available tools: {available:?}")]
    ToolNotFound {
        server: String,
        name: String,
        available: Vec<String>,
    },
    #[error("MCP connection error to server '{server}': {message}")]
    McpConnection { server: String, message: String },
    #[error("MCP protocol error to server '{server}': {message}")]
    McpProtocol { server: String, message: String },
    #[error("MCP timeout connecting to server '{server}' after {duration_secs} seconds")]
    McpTimeout { server: String, duration_secs: u64 },
    #[error("Failed to spawn process '{command}': {source}")]
    ProcessSpawn {
        command: String,
        source: std::io::Error,
    },
    #[error("Failed to read cache file {path:?}: {source}")]
    CacheRead {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to write cache file {path:?}: {source}")]
    CacheWrite {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Invalid JSON in context '{context}': {source}")]
    InvalidJson {
        context: String,
        source: serde_json::Error,
    },
    #[error("HTTP error for URL '{url}': {source}")]
    Http { url: String, source: reqwest::Error },
}

pub type Result<T> = std::result::Result<T, McpgError>;
