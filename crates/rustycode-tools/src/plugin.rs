// Plugin trait and related types for dynamic tool extension

use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::Result;
use serde_json::Value;
use std::any::Any;

/// Maps a `ToolPermission` to an explicit privilege level for comparison.
///
/// This avoids relying on enum discriminant ordering, which would silently
/// break if variants were reordered.
fn permission_level(p: ToolPermission) -> u8 {
    match p {
        ToolPermission::None => 0,
        ToolPermission::Read => 1,
        ToolPermission::Write => 2,
        ToolPermission::Execute => 3,
        ToolPermission::Network => 4,
        _ => 0,
    }
}

/// Returns `true` if `permission` represents a higher privilege level than `other`.
fn permission_exceeds(permission: ToolPermission, other: ToolPermission) -> bool {
    permission_level(permission) > permission_level(other)
}

/// Capabilities that a plugin can request
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginCapabilities {
    /// Maximum permission level required
    pub max_permission: ToolPermission,
    /// Whether the plugin needs filesystem access
    pub filesystem: bool,
    /// Whether the plugin needs network access
    pub network: bool,
    /// Whether the plugin can execute subprocesses
    pub execute: bool,
    /// Maximum memory allocation in MB (None = unlimited)
    pub max_memory_mb: Option<usize>,
    /// Maximum CPU time per execution in seconds (None = unlimited)
    pub max_cpu_secs: Option<u64>,
}

impl Default for PluginCapabilities {
    fn default() -> Self {
        Self {
            max_permission: ToolPermission::Read,
            filesystem: false,
            network: false,
            execute: false,
            max_memory_mb: Some(100),
            max_cpu_secs: Some(30),
        }
    }
}

impl PluginCapabilities {
    /// Create capabilities for a read-only plugin
    pub fn read_only() -> Self {
        Self {
            max_permission: ToolPermission::Read,
            filesystem: true,
            ..Default::default()
        }
    }

    /// Create capabilities for a full-access plugin
    pub const fn full_access() -> Self {
        Self {
            max_permission: ToolPermission::Network,
            filesystem: true,
            network: true,
            execute: true,
            max_memory_mb: None,
            max_cpu_secs: None,
        }
    }

    /// Validate that requested capabilities are within allowed limits
    pub fn validate(&self, allowed: &Self) -> Result<()> {
        if permission_exceeds(self.max_permission, allowed.max_permission) {
            anyhow::bail!(
                "Plugin requires {:?} permission but only {:?} is allowed",
                self.max_permission,
                allowed.max_permission
            );
        }

        if self.filesystem && !allowed.filesystem {
            anyhow::bail!("Plugin requires filesystem access but it is not allowed");
        }

        if self.network && !allowed.network {
            anyhow::bail!("Plugin requires network access but it is not allowed");
        }

        if self.execute && !allowed.execute {
            anyhow::bail!("Plugin requires execute permission but it is not allowed");
        }

        if let (Some(requested), Some(allowed_max)) = (self.max_memory_mb, allowed.max_memory_mb) {
            if requested > allowed_max {
                anyhow::bail!(
                    "Plugin requests {requested}MB memory but only {allowed_max}MB is allowed"
                );
            }
        }

        if let (Some(requested), Some(allowed_max)) = (self.max_cpu_secs, allowed.max_cpu_secs) {
            if requested > allowed_max {
                anyhow::bail!(
                    "Plugin requests {requested}s CPU time but only {allowed_max}s is allowed"
                );
            }
        }

        Ok(())
    }
}

/// Plugin state that persists across tool invocations
pub trait PluginState: Send + Sync + Any {
    /// Called when plugin is being unloaded, allowing cleanup
    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}

// Implementation for unit type (stateless plugins)
impl PluginState for () {}

// Blanket implementation for Box<dyn PluginState>
impl<T: PluginState + ?Sized> PluginState for Box<T> {}

/// Trait that all plugins must implement
pub trait ToolPlugin: Send + Sync {
    /// Get the plugin name (used for namespacing)
    fn name(&self) -> &str;

    /// Get the plugin version
    fn version(&self) -> &'static str {
        "0.1.0"
    }

    /// Get the plugin description
    fn description(&self) -> &'static str {
        ""
    }

    /// Get the capabilities this plugin requires
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::default()
    }

    /// Initialize the plugin and return its state
    ///
    /// This is called once when the plugin is loaded.
    /// The returned state will be passed to all tool invocations.
    fn init(&self) -> Result<Box<dyn PluginState>> {
        Ok(Box::new(()))
    }

    /// Get all tools provided by this plugin
    ///
    /// Each tool name will be namespaced as "`plugin_name::tool_name`"
    fn tools(&self) -> Vec<Box<dyn Tool>>;

    /// Called when the plugin is unloaded
    ///
    /// This gives the plugin a chance to clean up resources.
    /// The default implementation does nothing.
    fn on_unload(&self) -> Result<()> {
        Ok(())
    }
}

/// Wrapper that adds namespacing to plugin tools
pub struct NamespacedTool {
    /// The namespaced tool name (`plugin::tool`)
    namespaced_name: String,
    /// The underlying tool
    tool: Box<dyn Tool>,
}

impl NamespacedTool {
    pub fn new(plugin_name: String, tool: Box<dyn Tool>) -> Self {
        let namespaced_name = format!("{}::{}", plugin_name, tool.name());
        Self {
            namespaced_name,
            tool,
        }
    }

    /// Get the namespaced tool name (`plugin::tool`)
    pub fn namespaced_name(&self) -> &str {
        &self.namespaced_name
    }

    /// Get the underlying tool
    pub fn inner(&self) -> &dyn Tool {
        self.tool.as_ref()
    }
}

impl Tool for NamespacedTool {
    fn name(&self) -> &str {
        &self.namespaced_name
    }

    fn description(&self) -> &str {
        self.tool.description()
    }

    fn permission(&self) -> ToolPermission {
        self.tool.permission()
    }

    fn parameters_schema(&self) -> Value {
        self.tool.parameters_schema()
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        self.tool.execute(params, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_capabilities() {
        let caps = PluginCapabilities::default();
        assert_eq!(caps.max_permission, ToolPermission::Read);
        assert!(!caps.filesystem);
        assert!(!caps.network);
        assert!(!caps.execute);
        assert_eq!(caps.max_memory_mb, Some(100));
        assert_eq!(caps.max_cpu_secs, Some(30));
    }

    #[test]
    fn test_read_only_capabilities() {
        let caps = PluginCapabilities::read_only();
        assert_eq!(caps.max_permission, ToolPermission::Read);
        assert!(caps.filesystem);
        assert!(!caps.network);
        assert!(!caps.execute);
    }

    #[test]
    fn test_full_access_capabilities() {
        let caps = PluginCapabilities::full_access();
        assert_eq!(caps.max_permission, ToolPermission::Network);
        assert!(caps.filesystem);
        assert!(caps.network);
        assert!(caps.execute);
        assert_eq!(caps.max_memory_mb, None);
        assert_eq!(caps.max_cpu_secs, None);
    }

    #[test]
    fn test_capability_validation_success() {
        let requested = PluginCapabilities::read_only();
        let allowed = PluginCapabilities::full_access();
        assert!(requested.validate(&allowed).is_ok());
    }

    #[test]
    fn test_capability_validation_permission_denied() {
        let requested = PluginCapabilities::full_access();
        let allowed = PluginCapabilities::read_only();
        let result = requested.validate(&allowed);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("permission"));
    }

    #[test]
    fn test_capability_validation_filesystem_denied() {
        let requested = PluginCapabilities {
            filesystem: true,
            ..Default::default()
        };
        let allowed = PluginCapabilities {
            filesystem: false,
            ..Default::default()
        };
        let result = requested.validate(&allowed);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("filesystem"));
    }

    #[test]
    fn test_capability_validation_memory_limit() {
        let requested = PluginCapabilities {
            max_memory_mb: Some(200),
            ..Default::default()
        };
        let allowed = PluginCapabilities {
            max_memory_mb: Some(100),
            ..Default::default()
        };
        let result = requested.validate(&allowed);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("memory"));
    }
}
