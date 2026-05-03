//! Plugin lifecycle management for loading, initializing, and shutting down plugins

use anyhow::Result;
use std::path::Path;

use crate::error::PluginError;
use crate::traits::{AgentPlugin, LLMProviderPlugin, ToolPlugin};

/// Manages the lifecycle of plugins (loading, initialization, shutdown)
pub struct PluginLifecycleManager;

impl PluginLifecycleManager {
    /// Load a plugin from a dynamic library file
    ///
    /// This is a placeholder for future dynamic loading capability.
    /// Currently returns an error as dynamic loading requires libloading integration.
    pub fn load_plugin<P: AsRef<Path>>(_path: P) -> Result<Box<dyn ToolPlugin>> {
        Err(anyhow::anyhow!(PluginError::loading_failed(
            "Dynamic plugin loading is not yet implemented. Use static registration."
        )))
    }

    /// Initialize a tool plugin
    ///
    /// Calls the plugin's `init()` method and returns an error if initialization fails.
    pub fn initialize_tool_plugin(plugin: &dyn ToolPlugin) -> Result<()> {
        plugin.init().map_err(|e| {
            anyhow::anyhow!(PluginError::initialization_failed(
                plugin.name(),
                e.to_string()
            ))
        })
    }

    /// Initialize an agent plugin
    pub fn initialize_agent_plugin(plugin: &dyn AgentPlugin) -> Result<()> {
        plugin.init().map_err(|e| {
            anyhow::anyhow!(PluginError::initialization_failed(
                plugin.name(),
                e.to_string()
            ))
        })
    }

    /// Initialize an LLM provider plugin
    pub fn initialize_provider_plugin(plugin: &dyn LLMProviderPlugin) -> Result<()> {
        plugin.init().map_err(|e| {
            anyhow::anyhow!(PluginError::initialization_failed(
                plugin.name(),
                e.to_string()
            ))
        })
    }

    /// Shutdown a tool plugin
    ///
    /// Calls the plugin's `shutdown()` method to clean up resources.
    pub fn shutdown_tool_plugin(plugin: &dyn ToolPlugin) -> Result<()> {
        plugin.shutdown().map_err(|e| {
            anyhow::anyhow!(PluginError::shutdown_failed(plugin.name(), e.to_string()))
        })
    }

    /// Shutdown an agent plugin
    pub fn shutdown_agent_plugin(plugin: &dyn AgentPlugin) -> Result<()> {
        plugin.shutdown().map_err(|e| {
            anyhow::anyhow!(PluginError::shutdown_failed(plugin.name(), e.to_string()))
        })
    }

    /// Shutdown an LLM provider plugin
    pub fn shutdown_provider_plugin(plugin: &dyn LLMProviderPlugin) -> Result<()> {
        plugin.shutdown().map_err(|e| {
            anyhow::anyhow!(PluginError::shutdown_failed(plugin.name(), e.to_string()))
        })
    }

    /// Validate plugin metadata
    ///
    /// Performs basic validation on plugin metadata to ensure it's well-formed.
    pub fn validate_plugin_metadata(name: &str, version: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow::anyhow!(PluginError::configuration_error(
                "Plugin name cannot be empty"
            )));
        }

        // Reject excessively long names (filesystem / memory DoS)
        if name.len() > 256 {
            return Err(anyhow::anyhow!(PluginError::configuration_error(
                "Plugin name exceeds 256 characters"
            )));
        }

        // Reject names with null bytes or path separators
        if name.contains('\0') || name.contains('/') || name.contains('\\') {
            return Err(anyhow::anyhow!(PluginError::configuration_error(
                "Plugin name contains invalid characters"
            )));
        }

        if version.is_empty() {
            return Err(anyhow::anyhow!(PluginError::configuration_error(format!(
                "Plugin '{name}' version cannot be empty"
            ))));
        }

        if version.len() > 64 {
            return Err(anyhow::anyhow!(PluginError::configuration_error(format!(
                "Plugin '{name}' version exceeds 64 characters"
            ))));
        }

        // Basic semver validation
        if !is_valid_semver(version) {
            return Err(anyhow::anyhow!(PluginError::version_mismatch(
                name,
                format!("'{version}' is not a valid semver version (expected: major.minor.patch)")
            )));
        }

        Ok(())
    }

    /// Check if a plugin's dependencies are satisfied
    ///
    /// Returns an error if any required dependencies are missing.
    pub fn check_dependencies(
        plugin_name: &str,
        dependencies: &[String],
        available_plugins: &[String],
    ) -> Result<()> {
        for dep in dependencies {
            if !available_plugins.contains(dep) {
                return Err(anyhow::anyhow!(PluginError::missing_dependency(
                    plugin_name,
                    dep
                )));
            }
        }
        Ok(())
    }
}

/// Check if a version string is valid semver
fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return false;
    }

    for part in parts {
        if part.parse::<u32>().is_err() {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{AgentPlugin, LLMProviderPlugin, ToolPlugin};

    struct TestToolPlugin {
        name: String,
        init_fails: bool,
        shutdown_fails: bool,
    }

    impl ToolPlugin for TestToolPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "Test plugin"
        }

        fn init(&self) -> Result<()> {
            if self.init_fails {
                Err(anyhow::anyhow!("init failed"))
            } else {
                Ok(())
            }
        }

        fn shutdown(&self) -> Result<()> {
            if self.shutdown_fails {
                Err(anyhow::anyhow!("shutdown failed"))
            } else {
                Ok(())
            }
        }
    }

    struct TestAgentPlugin {
        name: String,
    }

    impl AgentPlugin for TestAgentPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "Test agent"
        }
    }

    struct TestProviderPlugin {
        name: String,
    }

    impl LLMProviderPlugin for TestProviderPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "Test provider"
        }
    }

    #[test]
    fn test_initialize_tool_plugin_success() {
        let plugin = TestToolPlugin {
            name: "test".to_string(),
            init_fails: false,
            shutdown_fails: false,
        };
        assert!(PluginLifecycleManager::initialize_tool_plugin(&plugin).is_ok());
    }

    #[test]
    fn test_initialize_tool_plugin_fails() {
        let plugin = TestToolPlugin {
            name: "test".to_string(),
            init_fails: true,
            shutdown_fails: false,
        };
        let result = PluginLifecycleManager::initialize_tool_plugin(&plugin);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("init failed"));
    }

    #[test]
    fn test_shutdown_tool_plugin_success() {
        let plugin = TestToolPlugin {
            name: "test".to_string(),
            init_fails: false,
            shutdown_fails: false,
        };
        assert!(PluginLifecycleManager::shutdown_tool_plugin(&plugin).is_ok());
    }

    #[test]
    fn test_shutdown_tool_plugin_fails() {
        let plugin = TestToolPlugin {
            name: "test".to_string(),
            init_fails: false,
            shutdown_fails: true,
        };
        let result = PluginLifecycleManager::shutdown_tool_plugin(&plugin);
        assert!(result.is_err());
    }

    #[test]
    fn test_initialize_agent_plugin() {
        let plugin = TestAgentPlugin {
            name: "test".to_string(),
        };
        assert!(PluginLifecycleManager::initialize_agent_plugin(&plugin).is_ok());
    }

    #[test]
    fn test_initialize_provider_plugin() {
        let plugin = TestProviderPlugin {
            name: "test".to_string(),
        };
        assert!(PluginLifecycleManager::initialize_provider_plugin(&plugin).is_ok());
    }

    #[test]
    fn test_validate_plugin_metadata_valid() {
        assert!(PluginLifecycleManager::validate_plugin_metadata("test", "1.0.0").is_ok());
        assert!(PluginLifecycleManager::validate_plugin_metadata("plugin", "0.2.5").is_ok());
    }

    #[test]
    fn test_validate_plugin_metadata_empty_name() {
        let result = PluginLifecycleManager::validate_plugin_metadata("", "1.0.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_plugin_metadata_empty_version() {
        let result = PluginLifecycleManager::validate_plugin_metadata("test", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_plugin_metadata_invalid_semver() {
        let result = PluginLifecycleManager::validate_plugin_metadata("test", "1.0");
        assert!(result.is_err());

        let result = PluginLifecycleManager::validate_plugin_metadata("test", "1.0.0.0");
        assert!(result.is_err());

        let result = PluginLifecycleManager::validate_plugin_metadata("test", "1.a.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_plugin_metadata_name_too_long() {
        let long_name = "x".repeat(300);
        let result = PluginLifecycleManager::validate_plugin_metadata(&long_name, "1.0.0");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("256 characters"));
    }

    #[test]
    fn test_validate_plugin_metadata_name_with_null_bytes() {
        let result = PluginLifecycleManager::validate_plugin_metadata("bad\0name", "1.0.0");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid characters"));
    }

    #[test]
    fn test_validate_plugin_metadata_name_with_path_separator() {
        let result = PluginLifecycleManager::validate_plugin_metadata("../evil", "1.0.0");
        assert!(result.is_err());
        let result = PluginLifecycleManager::validate_plugin_metadata("evil\\plugin", "1.0.0");
        assert!(result.is_err());
    }

    #[test]
    fn test_check_dependencies_satisfied() {
        let available = vec!["plugin_a".to_string(), "plugin_b".to_string()];
        let deps = vec!["plugin_a".to_string()];
        assert!(PluginLifecycleManager::check_dependencies("test", &deps, &available).is_ok());
    }

    #[test]
    fn test_check_dependencies_missing() {
        let available = vec!["plugin_a".to_string()];
        let deps = vec!["plugin_a".to_string(), "plugin_b".to_string()];
        let result = PluginLifecycleManager::check_dependencies("test", &deps, &available);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("plugin_b"));
    }

    #[test]
    fn test_check_dependencies_empty() {
        let available = vec![];
        let deps = vec![];
        assert!(PluginLifecycleManager::check_dependencies("test", &deps, &available).is_ok());
    }

    #[test]
    fn test_valid_semver() {
        assert!(is_valid_semver("0.0.0"));
        assert!(is_valid_semver("1.2.3"));
        assert!(is_valid_semver("10.20.30"));
    }

    #[test]
    fn test_invalid_semver() {
        assert!(!is_valid_semver("1.0"));
        assert!(!is_valid_semver("1.0.0.0"));
        assert!(!is_valid_semver("1.a.0"));
        assert!(!is_valid_semver(""));
    }

    #[test]
    fn test_load_plugin_not_implemented() {
        let result = PluginLifecycleManager::load_plugin("nonexistent.so");
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("not yet implemented"));
    }
}
