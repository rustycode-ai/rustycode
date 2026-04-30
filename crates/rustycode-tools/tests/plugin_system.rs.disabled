//! Integration tests for the plugin system
//!
//! These tests verify:
//! - Plugin metadata and lifecycle
//! - Plugin registration and tool registration
//! - Plugin capability validation
//! - Plugin loading and unloading

use anyhow::Result;
use rustycode_tools::{
    PluginCapabilities, PluginManager, PluginState, Tool, ToolContext, ToolOutput, ToolPermission,
    ToolPlugin, ToolRegistry,
};
use serde_json::{json, Value};
use std::path::PathBuf;

// Test Plugin 1: Simple plugin with no state
struct TestPlugin;

impl ToolPlugin for TestPlugin {
    fn name(&self) -> &str {
        "test_plugin"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "A test plugin"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::read_only()
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![Box::new(TestTool)]
    }
}

struct TestTool;

impl Tool for TestTool {
    fn name(&self) -> &str {
        "test_tool"
    }

    fn description(&self) -> &str {
        "A test tool"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": {"type": "string"}
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let input = params
            .get("input")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        Ok(ToolOutput::text(format!("Test: {}", input)))
    }
}

// Test Plugin 2: Plugin with state
struct StatefulPlugin {
    initial_value: i32,
}

impl ToolPlugin for StatefulPlugin {
    fn name(&self) -> &str {
        "stateful_plugin"
    }

    fn version(&self) -> &str {
        "2.0.0"
    }

    fn description(&self) -> &str {
        "A plugin with persistent state"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::default()
    }

    fn init(&self) -> Result<Box<dyn PluginState>> {
        Ok(Box::new(TestState {
            value: self.initial_value,
        }))
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![Box::new(GetValueTool)]
    }

    fn on_unload(&self) -> Result<()> {
        println!("StatefulPlugin unloading");
        Ok(())
    }
}

struct TestState {
    value: i32,
}

impl PluginState for TestState {
    fn cleanup(&mut self) -> Result<()> {
        self.value = 0;
        Ok(())
    }
}

struct GetValueTool;

impl Tool for GetValueTool {
    fn name(&self) -> &str {
        "get_value"
    }

    fn description(&self) -> &str {
        "Get the stored value"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::text("Value retrieved"))
    }
}

// Test Plugin 3: Plugin with excessive capabilities (should fail registration)
struct DangerousPlugin;

impl ToolPlugin for DangerousPlugin {
    fn name(&self) -> &str {
        "dangerous_plugin"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "A plugin that requires too many capabilities"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::full_access()
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![]
    }
}

#[test]
fn test_plugin_registration() {
    let mut registry = ToolRegistry::new();
    let mut manager = PluginManager::new();

    // Register a plugin
    let result = manager.register_plugin(TestPlugin, &mut registry);
    assert!(result.is_ok(), "Plugin registration should succeed");

    // Verify plugin is loaded
    assert!(manager.is_loaded("test_plugin"));
    assert_eq!(manager.plugin_count(), 1);

    // Verify tool is registered
    let tool_names: Vec<String> = registry
        .list()
        .iter()
        .filter(|t| t.name.starts_with("test_plugin::"))
        .map(|t| t.name.clone())
        .collect();

    assert_eq!(tool_names, vec!["test_plugin::test_tool"]);
}

#[test]
fn test_duplicate_plugin_registration_fails() {
    let mut registry = ToolRegistry::new();
    let mut manager = PluginManager::new();

    // Register plugin first time
    manager.register_plugin(TestPlugin, &mut registry).unwrap();

    // Try to register again
    let result = manager.register_plugin(TestPlugin, &mut registry);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("already registered"));
}

#[test]
fn test_plugin_capability_validation() {
    let mut registry = ToolRegistry::new();

    // Create manager with read-only capabilities
    let caps = PluginCapabilities::read_only();
    let mut manager = PluginManager::with_capabilities(caps);

    // Try to register a plugin that requires full access
    let result = manager.register_plugin(DangerousPlugin, &mut registry);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("capability validation failed"));
}

#[test]
fn test_plugin_info_retrieval() {
    let mut registry = ToolRegistry::new();
    let mut manager = PluginManager::new();

    manager.register_plugin(TestPlugin, &mut registry).unwrap();

    // Get plugin info
    let info = manager.get_plugin("test_plugin");
    assert!(info.is_some());

    let info = info.unwrap();
    assert_eq!(info.name, "test_plugin");
    assert_eq!(info.version, "1.0.0");
    assert_eq!(info.description, "A test plugin");
    assert_eq!(info.tools, vec!["test_tool"]);
}

#[test]
fn test_list_plugins() {
    let mut registry = ToolRegistry::new();
    let mut manager = PluginManager::new();

    // Register multiple plugins
    manager.register_plugin(TestPlugin, &mut registry).unwrap();
    manager
        .register_plugin(StatefulPlugin { initial_value: 42 }, &mut registry)
        .unwrap();

    // List plugins
    let plugins = manager.list_plugins();
    assert_eq!(plugins.len(), 2);

    let plugin_names: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
    assert!(plugin_names.contains(&"test_plugin"));
    assert!(plugin_names.contains(&"stateful_plugin"));
}

#[test]
fn test_plugin_unload() {
    let mut registry = ToolRegistry::new();
    let mut manager = PluginManager::new();

    // Register plugin
    manager.register_plugin(TestPlugin, &mut registry).unwrap();
    assert!(manager.is_loaded("test_plugin"));

    // Unload plugin
    let result = manager.unload_plugin("test_plugin");
    assert!(result.is_ok());
    assert!(!manager.is_loaded("test_plugin"));
    assert_eq!(manager.plugin_count(), 0);
}

#[test]
fn test_unload_nonexistent_plugin_fails() {
    let mut manager = PluginManager::new();

    let result = manager.unload_plugin("nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_plugin_state_initialization() {
    let mut registry = ToolRegistry::new();
    let mut manager = PluginManager::new();

    // Register stateful plugin
    let plugin = StatefulPlugin { initial_value: 42 };
    let result = manager.register_plugin(plugin, &mut registry);
    assert!(result.is_ok());

    // Verify plugin is loaded
    assert!(manager.is_loaded("stateful_plugin"));
}

#[test]
fn test_plugin_tool_execution() {
    let mut registry = ToolRegistry::new();
    let mut manager = PluginManager::new();

    // Register plugin
    manager.register_plugin(TestPlugin, &mut registry).unwrap();

    // Execute the plugin tool
    let ctx = ToolContext::new(PathBuf::from("/tmp"));
    let call = rustycode_protocol::ToolCall {
        call_id: "test-1".to_string(),
        name: "test_plugin::test_tool".to_string(),
        arguments: json!({"input": "hello"}),
    };

    let result = registry.execute(&call, &ctx);
    assert!(result.success);
    assert!(result.output.contains("hello"));
}

#[test]
fn test_plugin_metadata() {
    let mut registry = ToolRegistry::new();
    let mut manager = PluginManager::new();

    manager.register_plugin(TestPlugin, &mut registry).unwrap();

    let info = manager.get_plugin("test_plugin").unwrap();

    // Verify metadata
    assert_eq!(info.name, "test_plugin");
    assert_eq!(info.version, "1.0.0");
    assert_eq!(info.description, "A test plugin");

    // Verify capabilities
    assert_eq!(info.capabilities.max_permission, ToolPermission::Read);
    assert!(info.capabilities.filesystem);
    assert!(!info.capabilities.network);
    assert!(!info.capabilities.execute);
}

#[test]
fn test_plugin_manager_default() {
    let manager = PluginManager::default();
    assert_eq!(manager.plugin_count(), 0);
}

#[test]
fn test_multiple_plugins_with_same_tools() {
    let mut registry = ToolRegistry::new();
    let mut manager = PluginManager::new();

    // Create two plugins with similar tools
    manager.register_plugin(TestPlugin, &mut registry).unwrap();
    manager
        .register_plugin(StatefulPlugin { initial_value: 1 }, &mut registry)
        .unwrap();

    // Both should be registered
    assert_eq!(manager.plugin_count(), 2);

    // Tools should be namespaced
    let tool_names: Vec<String> = registry
        .list()
        .iter()
        .filter(|t| t.name.contains("::"))
        .map(|t| t.name.clone())
        .collect();

    assert!(tool_names.contains(&"test_plugin::test_tool".to_string()));
    assert!(tool_names.contains(&"stateful_plugin::get_value".to_string()));
}
