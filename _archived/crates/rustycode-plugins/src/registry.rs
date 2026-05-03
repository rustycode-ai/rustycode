//! Plugin registry for managing and accessing plugins
//!
//! The registry maintains plugin state, handles lifecycle events, and provides
//! thread-safe access to registered plugins.

use anyhow::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::PluginError;
use crate::status::PluginStatus;
use crate::traits::{AgentPlugin, LLMProviderPlugin, ToolPlugin};

/// Type alias for tool plugin storage
type ToolPluginMap = Arc<RwLock<HashMap<String, Arc<Box<dyn ToolPlugin>>>>>;
/// Type alias for agent plugin storage
type AgentPluginMap = Arc<RwLock<HashMap<String, Arc<Box<dyn AgentPlugin>>>>>;
/// Type alias for LLM provider plugin storage
type ProviderPluginMap = Arc<RwLock<HashMap<String, Arc<Box<dyn LLMProviderPlugin>>>>>;

/// Registry for managing plugins
///
/// The registry provides thread-safe storage and lifecycle management
/// for plugins. It tracks plugin status and enables enabling/disabling plugins.
pub struct PluginRegistry {
    /// Tool plugins by name
    tools: ToolPluginMap,
    /// Agent plugins by name
    agents: AgentPluginMap,
    /// LLM provider plugins by name
    providers: ProviderPluginMap,
    /// Status of each plugin
    status: Arc<RwLock<HashMap<String, PluginStatus>>>,
}

impl PluginRegistry {
    /// Create a new empty plugin registry
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            agents: Arc::new(RwLock::new(HashMap::new())),
            providers: Arc::new(RwLock::new(HashMap::new())),
            status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // ── Tool Plugin Methods ────────────────────────────────────────────────────

    /// Register a tool plugin
    pub fn register_tool(&self, plugin: Box<dyn ToolPlugin>) -> Result<()> {
        let name = plugin.name().to_string();

        // Check if already registered
        {
            let tools = self.tools.read();
            if tools.contains_key(&name) {
                return Err(PluginError::already_registered(&name).into());
            }
        }

        // Initialize the plugin
        plugin.init().map_err(|e| {
            anyhow::anyhow!(PluginError::initialization_failed(&name, e.to_string()))
        })?;

        // Store in registry
        {
            let mut tools = self.tools.write();
            let mut status = self.status.write();
            tools.insert(name.clone(), Arc::new(plugin));
            status.insert(name, PluginStatus::Active);
        }

        Ok(())
    }

    /// Get a tool plugin by name
    pub fn get_tool(&self, name: &str) -> Option<Arc<Box<dyn ToolPlugin>>> {
        let tools = self.tools.read();
        tools.get(name).cloned()
    }

    /// List all tool plugins with their status
    pub fn list_tools(&self) -> Vec<(String, PluginStatus)> {
        let tools = self.tools.read();
        let status = self.status.read();
        tools
            .keys()
            .map(|name| {
                let s = status
                    .get(name)
                    .cloned()
                    .unwrap_or(PluginStatus::Failed("unknown status".to_string()));
                (name.clone(), s)
            })
            .collect()
    }

    /// Enable a tool plugin (make it active)
    pub fn enable_tool(&self, name: &str) -> Result<()> {
        let tools = self.tools.read();
        if !tools.contains_key(name) {
            return Err(PluginError::not_found(name).into());
        }

        let mut status = self.status.write();
        status.insert(name.to_string(), PluginStatus::Active);
        Ok(())
    }

    /// Disable a tool plugin
    pub fn disable_tool(&self, name: &str) -> Result<()> {
        let tools = self.tools.read();
        if !tools.contains_key(name) {
            return Err(PluginError::not_found(name).into());
        }

        let mut status = self.status.write();
        status.insert(name.to_string(), PluginStatus::Disabled);
        Ok(())
    }

    /// Unload a tool plugin (calls shutdown)
    pub fn unload_tool(&self, name: &str) -> Result<()> {
        let plugin = {
            let mut tools = self.tools.write();
            tools
                .remove(name)
                .ok_or_else(|| PluginError::not_found(name))?
        };

        // Call shutdown on the plugin
        if let Err(e) = plugin.shutdown() {
            let mut status = self.status.write();
            status.insert(
                name.to_string(),
                PluginStatus::Failed(format!("shutdown failed: {e}")),
            );
            return Err(PluginError::shutdown_failed(name, e.to_string()).into());
        }

        // Remove from status tracking
        {
            let mut status = self.status.write();
            status.remove(name);
        }

        Ok(())
    }

    // ── Agent Plugin Methods ───────────────────────────────────────────────────

    /// Register an agent plugin
    pub fn register_agent(&self, plugin: Box<dyn AgentPlugin>) -> Result<()> {
        let name = plugin.name().to_string();

        // Check if already registered
        {
            let agents = self.agents.read();
            if agents.contains_key(&name) {
                return Err(PluginError::already_registered(&name).into());
            }
        }

        // Initialize the plugin
        plugin.init().map_err(|e| {
            anyhow::anyhow!(PluginError::initialization_failed(&name, e.to_string()))
        })?;

        // Store in registry
        {
            let mut agents = self.agents.write();
            let mut status = self.status.write();
            agents.insert(name.clone(), Arc::new(plugin));
            status.insert(name, PluginStatus::Active);
        }

        Ok(())
    }

    /// Get an agent plugin by name
    pub fn get_agent(&self, name: &str) -> Option<Arc<Box<dyn AgentPlugin>>> {
        let agents = self.agents.read();
        agents.get(name).cloned()
    }

    /// List all agent plugins with their status
    pub fn list_agents(&self) -> Vec<(String, PluginStatus)> {
        let agents = self.agents.read();
        let status = self.status.read();
        agents
            .keys()
            .map(|name| {
                let s = status
                    .get(name)
                    .cloned()
                    .unwrap_or(PluginStatus::Failed("unknown status".to_string()));
                (name.clone(), s)
            })
            .collect()
    }

    /// Enable an agent plugin
    pub fn enable_agent(&self, name: &str) -> Result<()> {
        let agents = self.agents.read();
        if !agents.contains_key(name) {
            return Err(PluginError::not_found(name).into());
        }

        let mut status = self.status.write();
        status.insert(name.to_string(), PluginStatus::Active);
        Ok(())
    }

    /// Disable an agent plugin
    pub fn disable_agent(&self, name: &str) -> Result<()> {
        let agents = self.agents.read();
        if !agents.contains_key(name) {
            return Err(PluginError::not_found(name).into());
        }

        let mut status = self.status.write();
        status.insert(name.to_string(), PluginStatus::Disabled);
        Ok(())
    }

    /// Unload an agent plugin
    pub fn unload_agent(&self, name: &str) -> Result<()> {
        let plugin = {
            let mut agents = self.agents.write();
            agents
                .remove(name)
                .ok_or_else(|| PluginError::not_found(name))?
        };

        // Call shutdown on the plugin
        if let Err(e) = plugin.shutdown() {
            let mut status = self.status.write();
            status.insert(
                name.to_string(),
                PluginStatus::Failed(format!("shutdown failed: {e}")),
            );
            return Err(PluginError::shutdown_failed(name, e.to_string()).into());
        }

        let mut status = self.status.write();
        status.remove(name);
        Ok(())
    }

    // ── LLM Provider Methods ───────────────────────────────────────────────────

    /// Register an LLM provider plugin
    pub fn register_provider(&self, plugin: Box<dyn LLMProviderPlugin>) -> Result<()> {
        let name = plugin.name().to_string();

        // Check if already registered
        {
            let providers = self.providers.read();
            if providers.contains_key(&name) {
                return Err(PluginError::already_registered(&name).into());
            }
        }

        // Initialize the plugin
        plugin.init().map_err(|e| {
            anyhow::anyhow!(PluginError::initialization_failed(&name, e.to_string()))
        })?;

        // Store in registry
        {
            let mut providers = self.providers.write();
            let mut status = self.status.write();
            providers.insert(name.clone(), Arc::new(plugin));
            status.insert(name, PluginStatus::Active);
        }

        Ok(())
    }

    /// Get an LLM provider plugin by name
    pub fn get_provider(&self, name: &str) -> Option<Arc<Box<dyn LLMProviderPlugin>>> {
        let providers = self.providers.read();
        providers.get(name).cloned()
    }

    /// List all LLM provider plugins with their status
    pub fn list_providers(&self) -> Vec<(String, PluginStatus)> {
        let providers = self.providers.read();
        let status = self.status.read();
        providers
            .keys()
            .map(|name| {
                let s = status
                    .get(name)
                    .cloned()
                    .unwrap_or(PluginStatus::Failed("unknown status".to_string()));
                (name.clone(), s)
            })
            .collect()
    }

    /// Enable an LLM provider plugin
    pub fn enable_provider(&self, name: &str) -> Result<()> {
        let providers = self.providers.read();
        if !providers.contains_key(name) {
            return Err(PluginError::not_found(name).into());
        }

        let mut status = self.status.write();
        status.insert(name.to_string(), PluginStatus::Active);
        Ok(())
    }

    /// Disable an LLM provider plugin
    pub fn disable_provider(&self, name: &str) -> Result<()> {
        let providers = self.providers.read();
        if !providers.contains_key(name) {
            return Err(PluginError::not_found(name).into());
        }

        let mut status = self.status.write();
        status.insert(name.to_string(), PluginStatus::Disabled);
        Ok(())
    }

    /// Unload an LLM provider plugin
    pub fn unload_provider(&self, name: &str) -> Result<()> {
        let plugin = {
            let mut providers = self.providers.write();
            providers
                .remove(name)
                .ok_or_else(|| PluginError::not_found(name))?
        };

        // Call shutdown on the plugin
        if let Err(e) = plugin.shutdown() {
            let mut status = self.status.write();
            status.insert(
                name.to_string(),
                PluginStatus::Failed(format!("shutdown failed: {e}")),
            );
            return Err(PluginError::shutdown_failed(name, e.to_string()).into());
        }

        let mut status = self.status.write();
        status.remove(name);
        Ok(())
    }

    // ── General Methods ────────────────────────────────────────────────────────

    /// Get the status of a plugin
    pub fn get_status(&self, name: &str) -> Option<PluginStatus> {
        let status = self.status.read();
        status.get(name).cloned()
    }

    /// List all plugins (tools, agents, and providers)
    pub fn list_all(&self) -> Vec<(String, &'static str, PluginStatus)> {
        let mut result = vec![];

        for (name, status) in self.list_tools() {
            result.push((name, "tool", status));
        }

        for (name, status) in self.list_agents() {
            result.push((name, "agent", status));
        }

        for (name, status) in self.list_providers() {
            result.push((name, "provider", status));
        }

        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// Clear all plugins from the registry
    pub fn clear(&self) -> Result<()> {
        // Unload all plugins
        let all_plugins = self.list_all();
        for (name, plugin_type, _) in all_plugins {
            match plugin_type {
                "tool" => self.unload_tool(&name).ok(),
                "agent" => self.unload_agent(&name).ok(),
                "provider" => self.unload_provider(&name).ok(),
                _ => None,
            };
        }

        // Clear all data
        self.tools.write().clear();
        self.agents.write().clear();
        self.providers.write().clear();
        self.status.write().clear();

        Ok(())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{AgentPlugin, LLMProviderPlugin, ToolPlugin};

    struct MockToolPlugin {
        name: String,
        fail_init: bool,
    }

    impl ToolPlugin for MockToolPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "Mock tool plugin"
        }

        fn init(&self) -> Result<()> {
            if self.fail_init {
                Err(anyhow::anyhow!("init failed"))
            } else {
                Ok(())
            }
        }
    }

    struct MockAgentPlugin {
        name: String,
    }

    impl AgentPlugin for MockAgentPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "Mock agent plugin"
        }
    }

    struct MockProviderPlugin {
        name: String,
    }

    impl LLMProviderPlugin for MockProviderPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn description(&self) -> &str {
            "Mock provider plugin"
        }
    }

    #[test]
    fn test_register_tool_plugin() {
        let registry = PluginRegistry::new();
        let plugin = Box::new(MockToolPlugin {
            name: "test_tool".to_string(),
            fail_init: false,
        });
        assert!(registry.register_tool(plugin).is_ok());
        assert!(registry.get_tool("test_tool").is_some());
    }

    #[test]
    fn test_register_duplicate_tool_plugin() {
        let registry = PluginRegistry::new();
        let plugin1 = Box::new(MockToolPlugin {
            name: "test_tool".to_string(),
            fail_init: false,
        });
        let plugin2 = Box::new(MockToolPlugin {
            name: "test_tool".to_string(),
            fail_init: false,
        });
        assert!(registry.register_tool(plugin1).is_ok());
        assert!(registry.register_tool(plugin2).is_err());
    }

    #[test]
    fn test_register_tool_plugin_init_fails() {
        let registry = PluginRegistry::new();
        let plugin = Box::new(MockToolPlugin {
            name: "bad_tool".to_string(),
            fail_init: true,
        });
        let result = registry.register_tool(plugin);
        assert!(result.is_err());
        assert!(registry.get_tool("bad_tool").is_none());
    }

    #[test]
    fn test_list_tools() {
        let registry = PluginRegistry::new();
        registry
            .register_tool(Box::new(MockToolPlugin {
                name: "tool1".to_string(),
                fail_init: false,
            }))
            .unwrap();
        registry
            .register_tool(Box::new(MockToolPlugin {
                name: "tool2".to_string(),
                fail_init: false,
            }))
            .unwrap();

        let tools = registry.list_tools();
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().any(|(name, _)| name == "tool1"));
        assert!(tools.iter().any(|(name, _)| name == "tool2"));
    }

    #[test]
    fn test_enable_disable_tool() {
        let registry = PluginRegistry::new();
        registry
            .register_tool(Box::new(MockToolPlugin {
                name: "test_tool".to_string(),
                fail_init: false,
            }))
            .unwrap();

        assert!(registry.disable_tool("test_tool").is_ok());
        assert_eq!(
            registry.get_status("test_tool"),
            Some(PluginStatus::Disabled)
        );

        assert!(registry.enable_tool("test_tool").is_ok());
        assert_eq!(registry.get_status("test_tool"), Some(PluginStatus::Active));
    }

    #[test]
    fn test_unload_tool() {
        let registry = PluginRegistry::new();
        registry
            .register_tool(Box::new(MockToolPlugin {
                name: "test_tool".to_string(),
                fail_init: false,
            }))
            .unwrap();

        assert!(registry.unload_tool("test_tool").is_ok());
        assert!(registry.get_tool("test_tool").is_none());
    }

    #[test]
    fn test_register_agent_plugin() {
        let registry = PluginRegistry::new();
        registry
            .register_agent(Box::new(MockAgentPlugin {
                name: "test_agent".to_string(),
            }))
            .unwrap();
        assert!(registry.get_agent("test_agent").is_some());
    }

    #[test]
    fn test_register_provider_plugin() {
        let registry = PluginRegistry::new();
        registry
            .register_provider(Box::new(MockProviderPlugin {
                name: "test_provider".to_string(),
            }))
            .unwrap();
        assert!(registry.get_provider("test_provider").is_some());
    }

    #[test]
    fn test_list_all_plugins() {
        let registry = PluginRegistry::new();
        registry
            .register_tool(Box::new(MockToolPlugin {
                name: "tool1".to_string(),
                fail_init: false,
            }))
            .unwrap();
        registry
            .register_agent(Box::new(MockAgentPlugin {
                name: "agent1".to_string(),
            }))
            .unwrap();
        registry
            .register_provider(Box::new(MockProviderPlugin {
                name: "provider1".to_string(),
            }))
            .unwrap();

        let all = registry.list_all();
        assert_eq!(all.len(), 3);
        assert!(all
            .iter()
            .any(|(name, t, _)| name == "tool1" && *t == "tool"));
        assert!(all
            .iter()
            .any(|(name, t, _)| name == "agent1" && *t == "agent"));
        assert!(all
            .iter()
            .any(|(name, t, _)| name == "provider1" && *t == "provider"));
    }

    #[test]
    fn test_plugin_lifecycle() {
        let registry = PluginRegistry::new();

        // Register
        registry
            .register_tool(Box::new(MockToolPlugin {
                name: "test".to_string(),
                fail_init: false,
            }))
            .unwrap();
        assert_eq!(registry.get_status("test"), Some(PluginStatus::Active));

        // Disable
        registry.disable_tool("test").unwrap();
        assert_eq!(registry.get_status("test"), Some(PluginStatus::Disabled));

        // Enable
        registry.enable_tool("test").unwrap();
        assert_eq!(registry.get_status("test"), Some(PluginStatus::Active));

        // Unload
        registry.unload_tool("test").unwrap();
        assert_eq!(registry.get_status("test"), None);
    }

    #[test]
    fn test_clear_registry() {
        let registry = PluginRegistry::new();
        registry
            .register_tool(Box::new(MockToolPlugin {
                name: "tool1".to_string(),
                fail_init: false,
            }))
            .unwrap();
        registry
            .register_agent(Box::new(MockAgentPlugin {
                name: "agent1".to_string(),
            }))
            .unwrap();

        assert!(registry.clear().is_ok());
        assert_eq!(registry.list_tools().len(), 0);
        assert_eq!(registry.list_agents().len(), 0);
    }

    #[test]
    fn test_get_nonexistent_plugin() {
        let registry = PluginRegistry::new();
        assert!(registry.get_tool("nonexistent").is_none());
        assert!(registry.get_agent("nonexistent").is_none());
        assert!(registry.get_provider("nonexistent").is_none());
    }

    #[test]
    fn test_enable_nonexistent_plugin() {
        let registry = PluginRegistry::new();
        let result = registry.enable_tool("nonexistent");
        assert!(result.is_err());
    }
}
