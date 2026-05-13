//! Plugin manager for loading and managing tool plugins.
//!
//! The plugin manager handles the lifecycle of plugins including:
//! - Loading plugins and validating capabilities
//! - Initializing plugin state
//! - Registering plugin tools with the tool registry
//! - Unloading plugins and cleaning up resources
//!
//! # Example
//!
//! ```rust,no_run
//! use rustycode_tools::{ToolRegistry, plugin_manager::PluginManager};
//! use std::path::PathBuf;
//!
//! # fn example() -> anyhow::Result<()> {
//! # struct MyPlugin;
//! # impl MyPlugin {
//! #     fn new() -> Self { Self }
//! # }
//! let mut registry = ToolRegistry::new();
//! let mut manager = PluginManager::new();
//!
//! // Load a plugin
//! # let my_plugin = MyPlugin::new();
//! // manager.register_plugin(my_plugin, &mut registry)?;
//!
//! // List all loaded plugins
//! for plugin in manager.list_plugins() {
//!     println!("Plugin: {} (v{})", plugin.name, plugin.version);
//! }
//! # Ok(())
//! # }
//! ```

use crate::ToolRegistry;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

use super::plugin::{PluginCapabilities, PluginState, ToolPlugin};

/// Metadata about a loaded plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: String,
    /// Capabilities required by this plugin
    pub capabilities: PluginCapabilities,
    /// Tools provided by this plugin
    pub tools: Vec<String>,
    /// Whether the plugin is currently active
    pub active: bool,
}

/// Information about a registered plugin
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Plugin name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: String,
    /// Capabilities required by this plugin
    pub capabilities: PluginCapabilities,
    /// Tools provided by this plugin
    pub tools: Vec<String>,
}

impl From<PluginMetadata> for PluginInfo {
    fn from(meta: PluginMetadata) -> Self {
        Self {
            name: meta.name,
            version: meta.version,
            description: meta.description,
            capabilities: meta.capabilities,
            tools: meta.tools,
        }
    }
}

/// Manages the lifecycle of tool plugins
pub struct PluginManager {
    /// Loaded plugins indexed by name
    plugins: HashMap<String, LoadedPlugin>,
    /// Maximum allowed capabilities for plugins
    max_capabilities: PluginCapabilities,
}

/// A loaded plugin with its state
struct LoadedPlugin {
    /// The plugin instance
    plugin: Arc<dyn ToolPlugin>,
    /// The plugin's state (created during init)
    state: Option<Box<dyn PluginState>>,
    /// Metadata about the plugin
    metadata: PluginMetadata,
    /// Whether the plugin is active
    active: bool,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            max_capabilities: PluginCapabilities::full_access(),
        }
    }

    /// Create a new plugin manager with custom capability limits
    pub fn with_capabilities(capabilities: PluginCapabilities) -> Self {
        Self {
            plugins: HashMap::new(),
            max_capabilities: capabilities,
        }
    }

    /// Register a plugin and its tools with the registry
    ///
    /// This will:
    /// 1. Validate the plugin's capabilities against allowed limits
    /// 2. Initialize the plugin (calling `init()`)
    /// 3. Register all tools from the plugin
    /// 4. Store the plugin for later management
    ///
    pub fn register_plugin(
        &mut self,
        plugin: impl ToolPlugin + 'static,
        registry: &mut ToolRegistry,
    ) -> Result<()> {
        let plugin_name = plugin.name().to_string();

        // Check for duplicate plugin names
        if self.plugins.contains_key(&plugin_name) {
            anyhow::bail!("Plugin '{plugin_name}' is already registered");
        }

        // Validate capabilities
        let capabilities = plugin.capabilities();
        capabilities.validate(&self.max_capabilities).map_err(|e| {
            anyhow::anyhow!("Plugin '{plugin_name}' capability validation failed: {e}")
        })?;

        // Initialize the plugin
        let state = plugin
            .init()
            .map_err(|e| anyhow::anyhow!("Plugin '{plugin_name}' initialization failed: {e}"))?;

        // Get tools from the plugin
        let tools = plugin.tools();
        let tool_names: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();

        // Register each tool with namespacing
        for tool in tools {
            let tool_name = tool.name().to_string();
            let namespaced_name = format!("{}::{}", plugin_name, tool_name);
            if registry.get(&namespaced_name).is_some() {
                warn!(
                    plugin = %plugin_name,
                    tool = %namespaced_name,
                    "tool already registered, skipping duplicate"
                );
                continue;
            }
            let namespaced_tool = super::plugin::NamespacedTool::new(plugin_name.clone(), tool);
            registry.register(namespaced_tool);
        }

        // Store the plugin
        let metadata = PluginMetadata {
            name: plugin_name.clone(),
            version: plugin.version().to_string(),
            description: plugin.description().to_string(),
            capabilities: capabilities.clone(),
            tools: tool_names,
            active: true,
        };

        self.plugins.insert(
            plugin_name,
            LoadedPlugin {
                plugin: Arc::new(plugin),
                state: Some(state),
                metadata,
                active: true,
            },
        );

        Ok(())
    }

    /// Unload a plugin by name
    ///
    /// This will:
    /// 1. Call the plugin's `on_unload()` hook
    /// 2. Call cleanup on the plugin state
    /// 3. Mark the plugin as inactive
    ///
    /// Note: This does not unregister tools from the registry.
    /// Tools will remain but calls to them may fail.
    ///
    pub fn unload_plugin(&mut self, name: &str) -> Result<()> {
        let loaded = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{name}' not found"))?;

        if !loaded.active {
            return Ok(()); // Already unloaded
        }

        // Call unload hook
        loaded
            .plugin
            .on_unload()
            .map_err(|e| anyhow::anyhow!("Plugin '{name}' unload failed: {e}"))?;

        // Cleanup state
        if let Some(mut state) = loaded.state.take() {
            state
                .cleanup()
                .map_err(|e| anyhow::anyhow!("Plugin '{name}' state cleanup failed: {e}"))?;
        }

        loaded.active = false;
        loaded.metadata.active = false;

        Ok(())
    }

    /// Get information about a loaded plugin
    pub fn plugin(&self, name: &str) -> Option<PluginInfo> {
        self.plugins.get(name).map(|p| PluginInfo {
            name: p.metadata.name.clone(),
            version: p.metadata.version.clone(),
            description: p.metadata.description.clone(),
            capabilities: p.metadata.capabilities.clone(),
            tools: p.metadata.tools.clone(),
        })
    }

    /// List all loaded plugins
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .values()
            .filter(|p| p.active)
            .map(|p| PluginInfo {
                name: p.metadata.name.clone(),
                version: p.metadata.version.clone(),
                description: p.metadata.description.clone(),
                capabilities: p.metadata.capabilities.clone(),
                tools: p.metadata.tools.clone(),
            })
            .collect()
    }

    /// Get the maximum allowed capabilities for plugins
    pub const fn max_capabilities(&self) -> &PluginCapabilities {
        &self.max_capabilities
    }

    /// Check if a plugin is loaded and active
    pub fn is_loaded(&self, name: &str) -> bool {
        self.plugins.get(name).map(|p| p.active).unwrap_or(false)
    }

    /// Get the number of loaded plugins
    pub fn plugin_count(&self) -> usize {
        self.plugins.values().filter(|p| p.active).count()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

// Async variant for future use with dynamic loading
pub struct AsyncPluginManager {
    inner: Arc<RwLock<PluginManager>>,
}

impl AsyncPluginManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(PluginManager::new())),
        }
    }

    /// Create a new async plugin manager with custom capability limits
    pub fn with_capabilities(capabilities: PluginCapabilities) -> Self {
        Self {
            inner: Arc::new(RwLock::new(PluginManager::with_capabilities(capabilities))),
        }
    }

    /// Get the inner plugin manager for synchronous operations
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, PluginManager> {
        self.inner.read().await
    }

    /// Get mutable access to the inner plugin manager
    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, PluginManager> {
        self.inner.write().await
    }
}

impl Default for AsyncPluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for AsyncPluginManager {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolPermission;

    #[test]
    fn test_plugin_manager_creation() {
        let manager = PluginManager::new();
        assert_eq!(manager.plugin_count(), 0);
        assert!(!manager.is_loaded("test"));
    }

    #[test]
    fn test_plugin_manager_with_capabilities() {
        let caps = PluginCapabilities::read_only();
        let manager = PluginManager::with_capabilities(caps.clone());
        assert_eq!(manager.max_capabilities(), &caps);
    }

    #[test]
    fn test_unload_nonexistent_plugin() {
        let mut manager = PluginManager::new();
        let result = manager.unload_plugin("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_nonexistent_plugin() {
        let manager = PluginManager::new();
        assert!(manager.plugin("nonexistent").is_none());
    }

    #[test]
    fn test_list_empty_plugins() {
        let manager = PluginManager::new();
        assert_eq!(manager.list_plugins().len(), 0);
    }

    #[test]
    fn test_async_plugin_manager_creation() {
        let _manager = AsyncPluginManager::new();
        // Just verify it can be created
    }

    #[test]
    fn test_async_plugin_manager_clone() {
        let manager1 = AsyncPluginManager::new();
        let manager2 = manager1.clone();
        // Both should point to the same inner manager
        assert!(Arc::ptr_eq(&manager1.inner, &manager2.inner));
    }

    #[test]
    fn test_plugin_metadata_from_info() {
        let metadata = PluginMetadata {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            capabilities: PluginCapabilities::default(),
            tools: vec!["tool1".to_string(), "tool2".to_string()],
            active: true,
        };

        let info = PluginInfo::from(metadata.clone());
        assert_eq!(info.name, metadata.name);
        assert_eq!(info.version, metadata.version);
        assert_eq!(info.description, metadata.description);
        assert_eq!(info.tools, metadata.tools);
    }

    #[test]
    fn test_plugin_info_capabilities() {
        let info = PluginInfo {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            capabilities: PluginCapabilities::read_only(),
            tools: vec![],
        };

        // Verify capabilities are accessible
        assert_eq!(info.capabilities.max_permission, ToolPermission::Read);
        assert!(info.capabilities.filesystem);
    }
}
