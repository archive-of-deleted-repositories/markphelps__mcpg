mod loader;
mod types;

// Re-export types and constants for easier access
pub use loader::PROJECT_CONFIG_FILE;
pub use types::APP_NAME;
#[allow(unused_imports)] // Used in tests and other modules
pub use types::{Config, ServerConfig, TransportConfig};
