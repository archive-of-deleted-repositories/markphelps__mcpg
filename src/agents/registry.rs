use once_cell::sync::Lazy;
use std::sync::Mutex;

use super::Agent;

type AgentFactory = Box<dyn Fn() -> Box<dyn Agent> + Send + Sync>;

static REGISTRY: Lazy<Mutex<Vec<AgentFactory>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Register an agent factory function
pub fn register_agent<F>(factory: F)
where
    F: Fn() -> Box<dyn Agent> + Send + Sync + 'static,
{
    REGISTRY.lock().unwrap().push(Box::new(factory));
}

/// Get all registered agents as instances
pub fn get_registered_agents() -> Vec<Box<dyn Agent>> {
    REGISTRY
        .lock()
        .unwrap()
        .iter()
        .map(|factory| factory())
        .collect()
}

#[cfg(test)]
pub fn clear_registry_for_tests() {
    REGISTRY.lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::ServerWithSource;
    use anyhow::Result;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    struct TestAgent;

    impl Agent for TestAgent {
        fn name(&self) -> &'static str {
            "Test Agent"
        }

        fn config_sources(&self, _project_path: &Path) -> Vec<PathBuf> {
            // Return empty - detect() will use default impl and return false
            // unless overridden
            vec![]
        }

        fn detect(&self, _project_path: &Path) -> bool {
            // Override to always return true for tests
            true
        }

        fn get_servers(&self, _project_path: &Path) -> Result<HashMap<String, ServerWithSource>> {
            Ok(HashMap::new())
        }

        fn apply_changes(
            &self,
            _project_path: &Path,
            _servers_to_disable: &[String],
        ) -> Result<()> {
            Ok(())
        }

        fn mcpg_configured_at(&self, _project_path: &Path) -> Option<PathBuf> {
            None
        }
    }

    #[test]
    fn test_register_and_get_agents() {
        // Clean slate for this test
        clear_registry_for_tests();

        // Register our test agent
        register_agent(|| Box::new(TestAgent));

        let agents = get_registered_agents();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name(), "Test Agent");

        // Clean up after ourselves
        clear_registry_for_tests();
    }
}
