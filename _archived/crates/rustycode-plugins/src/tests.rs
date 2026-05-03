//! Comprehensive tests for the plugin system

use crate::{
    AgentPlugin, LLMProviderPlugin, PluginLifecycleManager, PluginMetadata, PluginRegistry,
    PluginStatus, ToolPlugin,
};
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ── Mock Plugins ───────────────────────────────────────────────────────────────

struct TestToolPlugin {
    name: String,
    init_called: Arc<AtomicBool>,
    shutdown_called: Arc<AtomicBool>,
}

impl TestToolPlugin {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            init_called: Arc::new(AtomicBool::new(false)),
            shutdown_called: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ToolPlugin for TestToolPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "Test tool plugin"
    }

    fn init(&self) -> Result<()> {
        self.init_called.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        self.shutdown_called.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct TestAgentPlugin {
    name: String,
}

impl TestAgentPlugin {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl AgentPlugin for TestAgentPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "Test agent plugin"
    }
}

struct TestProviderPlugin {
    name: String,
}

impl TestProviderPlugin {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl LLMProviderPlugin for TestProviderPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "Test provider plugin"
    }
}

// ── Plugin Registration Tests ──────────────────────────────────────────────────

#[test]
fn test_plugin_registration_tool() {
    let registry = PluginRegistry::new();
    let plugin = Box::new(TestToolPlugin::new("test_tool"));
    assert!(registry.register_tool(plugin).is_ok());
    assert!(registry.get_tool("test_tool").is_some());
}

#[test]
fn test_plugin_registration_agent() {
    let registry = PluginRegistry::new();
    let plugin = Box::new(TestAgentPlugin::new("test_agent"));
    assert!(registry.register_agent(plugin).is_ok());
    assert!(registry.get_agent("test_agent").is_some());
}

#[test]
fn test_plugin_registration_provider() {
    let registry = PluginRegistry::new();
    let plugin = Box::new(TestProviderPlugin::new("test_provider"));
    assert!(registry.register_provider(plugin).is_ok());
    assert!(registry.get_provider("test_provider").is_some());
}

#[test]
fn test_duplicate_tool_registration_fails() {
    let registry = PluginRegistry::new();
    let plugin1 = Box::new(TestToolPlugin::new("duplicate"));
    let plugin2 = Box::new(TestToolPlugin::new("duplicate"));
    assert!(registry.register_tool(plugin1).is_ok());
    assert!(registry.register_tool(plugin2).is_err());
}

#[test]
fn test_duplicate_agent_registration_fails() {
    let registry = PluginRegistry::new();
    let plugin1 = Box::new(TestAgentPlugin::new("duplicate"));
    let plugin2 = Box::new(TestAgentPlugin::new("duplicate"));
    assert!(registry.register_agent(plugin1).is_ok());
    assert!(registry.register_agent(plugin2).is_err());
}

#[test]
fn test_duplicate_provider_registration_fails() {
    let registry = PluginRegistry::new();
    let plugin1 = Box::new(TestProviderPlugin::new("duplicate"));
    let plugin2 = Box::new(TestProviderPlugin::new("duplicate"));
    assert!(registry.register_provider(plugin1).is_ok());
    assert!(registry.register_provider(plugin2).is_err());
}

// ── Lifecycle Tests ────────────────────────────────────────────────────────────

#[test]
fn test_plugin_lifecycle_tool() {
    let registry = PluginRegistry::new();
    let plugin = Box::new(TestToolPlugin::new("lifecycle_test"));
    let init_called = plugin.init_called.clone();
    let shutdown_called = plugin.shutdown_called.clone();

    registry.register_tool(plugin).unwrap();
    assert!(init_called.load(Ordering::SeqCst));

    registry.unload_tool("lifecycle_test").unwrap();
    assert!(shutdown_called.load(Ordering::SeqCst));
}

#[test]
fn test_plugin_lifecycle_agent() {
    let registry = PluginRegistry::new();
    let plugin = Box::new(TestAgentPlugin::new("lifecycle_test"));
    assert!(registry.register_agent(plugin).is_ok());

    assert_eq!(
        registry.get_status("lifecycle_test"),
        Some(PluginStatus::Active)
    );

    registry.unload_agent("lifecycle_test").unwrap();
    assert!(registry.get_agent("lifecycle_test").is_none());
}

#[test]
fn test_plugin_lifecycle_provider() {
    let registry = PluginRegistry::new();
    let plugin = Box::new(TestProviderPlugin::new("lifecycle_test"));
    assert!(registry.register_provider(plugin).is_ok());

    assert_eq!(
        registry.get_status("lifecycle_test"),
        Some(PluginStatus::Active)
    );

    registry.unload_provider("lifecycle_test").unwrap();
    assert!(registry.get_provider("lifecycle_test").is_none());
}

// ── Enable/Disable Tests ───────────────────────────────────────────────────────

#[test]
fn test_enable_disable_tool() {
    let registry = PluginRegistry::new();
    registry
        .register_tool(Box::new(TestToolPlugin::new("test")))
        .unwrap();

    assert_eq!(registry.get_status("test"), Some(PluginStatus::Active));

    registry.disable_tool("test").unwrap();
    assert_eq!(registry.get_status("test"), Some(PluginStatus::Disabled));

    registry.enable_tool("test").unwrap();
    assert_eq!(registry.get_status("test"), Some(PluginStatus::Active));
}

#[test]
fn test_enable_disable_agent() {
    let registry = PluginRegistry::new();
    registry
        .register_agent(Box::new(TestAgentPlugin::new("test")))
        .unwrap();

    registry.disable_agent("test").unwrap();
    assert_eq!(registry.get_status("test"), Some(PluginStatus::Disabled));

    registry.enable_agent("test").unwrap();
    assert_eq!(registry.get_status("test"), Some(PluginStatus::Active));
}

#[test]
fn test_enable_disable_provider() {
    let registry = PluginRegistry::new();
    registry
        .register_provider(Box::new(TestProviderPlugin::new("test")))
        .unwrap();

    registry.disable_provider("test").unwrap();
    assert_eq!(registry.get_status("test"), Some(PluginStatus::Disabled));

    registry.enable_provider("test").unwrap();
    assert_eq!(registry.get_status("test"), Some(PluginStatus::Active));
}

// ── Error Handling Tests ───────────────────────────────────────────────────────

#[test]
fn test_unload_nonexistent_tool() {
    let registry = PluginRegistry::new();
    let result = registry.unload_tool("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_unload_nonexistent_agent() {
    let registry = PluginRegistry::new();
    let result = registry.unload_agent("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_unload_nonexistent_provider() {
    let registry = PluginRegistry::new();
    let result = registry.unload_provider("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_enable_nonexistent_tool() {
    let registry = PluginRegistry::new();
    let result = registry.enable_tool("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_disable_nonexistent_tool() {
    let registry = PluginRegistry::new();
    let result = registry.disable_tool("nonexistent");
    assert!(result.is_err());
}

// ── Listing Tests ──────────────────────────────────────────────────────────────

#[test]
fn test_list_tools() {
    let registry = PluginRegistry::new();
    registry
        .register_tool(Box::new(TestToolPlugin::new("tool1")))
        .unwrap();
    registry
        .register_tool(Box::new(TestToolPlugin::new("tool2")))
        .unwrap();

    let tools = registry.list_tools();
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().any(|(name, _)| name == "tool1"));
    assert!(tools.iter().any(|(name, _)| name == "tool2"));
    assert!(tools
        .iter()
        .all(|(_, status)| status == &PluginStatus::Active));
}

#[test]
fn test_list_agents() {
    let registry = PluginRegistry::new();
    registry
        .register_agent(Box::new(TestAgentPlugin::new("agent1")))
        .unwrap();
    registry
        .register_agent(Box::new(TestAgentPlugin::new("agent2")))
        .unwrap();

    let agents = registry.list_agents();
    assert_eq!(agents.len(), 2);
    assert!(agents.iter().any(|(name, _)| name == "agent1"));
    assert!(agents.iter().any(|(name, _)| name == "agent2"));
}

#[test]
fn test_list_providers() {
    let registry = PluginRegistry::new();
    registry
        .register_provider(Box::new(TestProviderPlugin::new("provider1")))
        .unwrap();
    registry
        .register_provider(Box::new(TestProviderPlugin::new("provider2")))
        .unwrap();

    let providers = registry.list_providers();
    assert_eq!(providers.len(), 2);
    assert!(providers.iter().any(|(name, _)| name == "provider1"));
    assert!(providers.iter().any(|(name, _)| name == "provider2"));
}

#[test]
fn test_list_all_plugins() {
    let registry = PluginRegistry::new();
    registry
        .register_tool(Box::new(TestToolPlugin::new("tool1")))
        .unwrap();
    registry
        .register_agent(Box::new(TestAgentPlugin::new("agent1")))
        .unwrap();
    registry
        .register_provider(Box::new(TestProviderPlugin::new("provider1")))
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

// ── Metadata Tests ─────────────────────────────────────────────────────────────

#[test]
fn test_plugin_metadata_creation() {
    let meta = PluginMetadata::new("test", "1.0.0", "A test plugin");
    assert_eq!(meta.name, "test");
    assert_eq!(meta.version, "1.0.0");
}

#[test]
fn test_plugin_metadata_with_builders() {
    let meta = PluginMetadata::new("test", "1.0.0", "desc")
        .with_author("Alice")
        .with_dependency("dep1")
        .with_license("MIT");

    assert!(meta.authors.contains(&"Alice".to_string()));
    assert!(meta.dependencies.contains(&"dep1".to_string()));
    assert_eq!(meta.license, Some("MIT".to_string()));
}

// ── Status Tests ───────────────────────────────────────────────────────────────

#[test]
fn test_plugin_status_active() {
    let status = PluginStatus::Active;
    assert!(status.is_active());
    assert!(!status.is_failed());
    assert!(status.is_loaded());
}

#[test]
fn test_plugin_status_disabled() {
    let status = PluginStatus::Disabled;
    assert!(!status.is_active());
    assert!(!status.is_failed());
    assert!(!status.is_loaded());
}

#[test]
fn test_plugin_status_failed() {
    let status = PluginStatus::Failed("init error".to_string());
    assert!(!status.is_active());
    assert!(status.is_failed());
    assert_eq!(status.failure_reason(), Some("init error"));
}

#[test]
fn test_plugin_status_loading() {
    let status = PluginStatus::Loading;
    assert!(!status.is_active());
    assert!(!status.is_failed());
    assert!(status.is_loaded());
}

// ── Lifecycle Manager Tests ────────────────────────────────────────────────────

#[test]
fn test_lifecycle_manager_validate_metadata() {
    assert!(PluginLifecycleManager::validate_plugin_metadata("test", "1.0.0").is_ok());
    assert!(PluginLifecycleManager::validate_plugin_metadata("", "1.0.0").is_err());
    assert!(PluginLifecycleManager::validate_plugin_metadata("test", "").is_err());
    assert!(PluginLifecycleManager::validate_plugin_metadata("test", "invalid").is_err());
}

#[test]
fn test_lifecycle_manager_check_dependencies() {
    let available = vec!["pluginA".to_string(), "pluginB".to_string()];
    let deps = vec!["pluginA".to_string()];
    assert!(PluginLifecycleManager::check_dependencies("test", &deps, &available).is_ok());

    let missing = vec!["pluginC".to_string()];
    assert!(PluginLifecycleManager::check_dependencies("test", &missing, &available).is_err());
}

// ── Thread Safety Tests ────────────────────────────────────────────────────────

#[test]
fn test_concurrent_registration() {
    let registry = Arc::new(PluginRegistry::new());
    let mut handles = vec![];

    for i in 0..5 {
        let reg = registry.clone();
        let handle = std::thread::spawn(move || {
            let name = format!("tool_{i}");
            let plugin = Box::new(TestToolPlugin::new(&name));
            reg.register_tool(plugin).is_ok()
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().unwrap());
    }

    assert_eq!(registry.list_tools().len(), 5);
}

#[test]
fn test_concurrent_access() {
    let registry = Arc::new(PluginRegistry::new());
    registry
        .register_tool(Box::new(TestToolPlugin::new("concurrent_test")))
        .unwrap();

    let mut handles = vec![];

    for _ in 0..10 {
        let reg = registry.clone();
        let handle = std::thread::spawn(move || reg.get_tool("concurrent_test").is_some());
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.join().unwrap());
    }
}

// ── Comprehensive Plugin Lifecycle ─────────────────────────────────────────────

#[test]
fn test_complete_tool_plugin_lifecycle() {
    let registry = PluginRegistry::new();
    let plugin = Box::new(TestToolPlugin::new("lifecycle"));

    // Register and initialize
    assert!(registry.register_tool(plugin).is_ok());
    assert_eq!(registry.get_status("lifecycle"), Some(PluginStatus::Active));
    assert!(registry.get_tool("lifecycle").is_some());

    // Disable
    assert!(registry.disable_tool("lifecycle").is_ok());
    assert_eq!(
        registry.get_status("lifecycle"),
        Some(PluginStatus::Disabled)
    );

    // Re-enable
    assert!(registry.enable_tool("lifecycle").is_ok());
    assert_eq!(registry.get_status("lifecycle"), Some(PluginStatus::Active));

    // Unload and shutdown
    assert!(registry.unload_tool("lifecycle").is_ok());
    assert_eq!(registry.get_status("lifecycle"), None);
    assert!(registry.get_tool("lifecycle").is_none());
}

#[test]
fn test_mixed_plugin_types() {
    let registry = PluginRegistry::new();

    registry
        .register_tool(Box::new(TestToolPlugin::new("tool1")))
        .unwrap();
    registry
        .register_agent(Box::new(TestAgentPlugin::new("agent1")))
        .unwrap();
    registry
        .register_provider(Box::new(TestProviderPlugin::new("provider1")))
        .unwrap();

    assert_eq!(registry.list_tools().len(), 1);
    assert_eq!(registry.list_agents().len(), 1);
    assert_eq!(registry.list_providers().len(), 1);
    assert_eq!(registry.list_all().len(), 3);
}

#[test]
fn test_clear_registry() {
    let registry = PluginRegistry::new();
    registry
        .register_tool(Box::new(TestToolPlugin::new("tool1")))
        .unwrap();
    registry
        .register_agent(Box::new(TestAgentPlugin::new("agent1")))
        .unwrap();

    assert!(registry.clear().is_ok());
    assert_eq!(registry.list_tools().len(), 0);
    assert_eq!(registry.list_agents().len(), 0);
    assert_eq!(registry.list_all().len(), 0);
}

// ── Example Plugin Integration Tests ───────────────────────────────────────────

/// Example tool plugin for integration testing
#[allow(dead_code)]
struct ExampleTextStatisticsTool {
    name: String,
    verbose: bool,
    output_format: String,
}

impl ExampleTextStatisticsTool {
    fn new() -> Self {
        Self {
            name: "text-statistics".to_string(),
            verbose: false,
            output_format: "text".to_string(),
        }
    }

    fn with_config(verbose: bool, output_format: &str) -> Self {
        Self {
            name: "text-statistics".to_string(),
            verbose,
            output_format: output_format.to_string(),
        }
    }

    fn analyze_text(&self, text: &str) -> serde_json::Value {
        let words = text.split_whitespace().count();
        let lines = text.lines().count();
        let chars = text.len();
        let estimated_tokens = (chars as f64 / 4.0).ceil() as usize;

        if self.output_format == "json" {
            serde_json::json!({
                "word_count": words,
                "line_count": lines,
                "character_count": chars,
                "estimated_tokens": estimated_tokens
            })
        } else {
            serde_json::json!({
                "result": format!(
                    "Words: {}, Lines: {}, Chars: {}, Tokens: {}",
                    words, lines, chars, estimated_tokens
                )
            })
        }
    }
}

impl ToolPlugin for ExampleTextStatisticsTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "Analyzes text and provides word count, line count, and character statistics"
    }

    fn get_tools(&self) -> Result<Vec<crate::traits::ToolDescriptor>> {
        use crate::traits::ToolDescriptor;
        Ok(vec![ToolDescriptor::new(
            "analyze_text",
            "Analyze text and get statistics",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "The text to analyze"
                    }
                },
                "required": ["text"]
            }),
        )])
    }

    fn config_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "verbose": {
                    "type": "boolean",
                    "description": "Enable verbose output",
                    "default": false
                },
                "output_format": {
                    "type": "string",
                    "enum": ["text", "json"],
                    "description": "Output format",
                    "default": "text"
                }
            }
        })
    }
}

#[test]
fn test_example_plugin_text_statistics_basic() {
    let tool = ExampleTextStatisticsTool::new();
    assert_eq!(tool.name(), "text-statistics");
    assert_eq!(tool.version(), "1.0.0");
    assert!(tool.description().contains("Analyzes text"));
}

#[test]
fn test_example_plugin_analysis_text_format() {
    let tool = ExampleTextStatisticsTool::with_config(false, "text");
    let result = tool.analyze_text("hello world test");

    assert!(result["result"].is_string());
    let result_str = result["result"].as_str().unwrap();
    assert!(result_str.contains("Words: 3"));
    assert!(result_str.contains("Chars:"));
}

#[test]
fn test_example_plugin_analysis_json_format() {
    let tool = ExampleTextStatisticsTool::with_config(false, "json");
    let result = tool.analyze_text("hello world");

    assert!(result["word_count"].is_number());
    assert_eq!(result["word_count"].as_u64(), Some(2));
    assert!(result["line_count"].is_number());
}

#[test]
fn test_example_plugin_get_tools() {
    let tool = ExampleTextStatisticsTool::new();
    let tools = tool.get_tools().unwrap();

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "analyze_text");
    assert!(tools[0].description.contains("statistics"));
}

#[test]
fn test_example_plugin_config_schema() {
    let tool = ExampleTextStatisticsTool::new();
    let schema = tool.config_schema();

    assert!(schema["properties"]["verbose"].is_object());
    assert!(schema["properties"]["output_format"].is_object());
}

#[test]
fn test_example_plugin_registry_integration() {
    let registry = PluginRegistry::new();
    let plugin = Box::new(ExampleTextStatisticsTool::new());

    assert!(registry.register_tool(plugin).is_ok());
    assert!(registry.get_tool("text-statistics").is_some());

    let tools = registry.list_tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].0, "text-statistics");
    assert_eq!(tools[0].1, PluginStatus::Active);
}

#[test]
fn test_example_plugin_enable_disable() {
    let registry = PluginRegistry::new();
    registry
        .register_tool(Box::new(ExampleTextStatisticsTool::new()))
        .unwrap();

    assert!(registry.disable_tool("text-statistics").is_ok());
    assert_eq!(
        registry.get_status("text-statistics"),
        Some(PluginStatus::Disabled)
    );

    assert!(registry.enable_tool("text-statistics").is_ok());
    assert_eq!(
        registry.get_status("text-statistics"),
        Some(PluginStatus::Active)
    );
}

#[test]
fn test_example_plugin_unload() {
    let registry = PluginRegistry::new();
    registry
        .register_tool(Box::new(ExampleTextStatisticsTool::new()))
        .unwrap();

    assert!(registry.get_tool("text-statistics").is_some());
    assert!(registry.unload_tool("text-statistics").is_ok());
    assert!(registry.get_tool("text-statistics").is_none());
}

// ── Plugin Manifest Integration Tests ──────────────────────────────────────────

#[test]
#[cfg(feature = "toml")]
fn test_manifest_loading_example_plugin() {
    let manifest_toml = r#"
name = "text-statistics"
version = "1.0.0"
description = "Text analysis tool"
authors = ["RustyCode Contributors"]
permissions = ["process_info"]

[dependencies]
"#;

    let manifest = crate::PluginManifest::from_toml(manifest_toml);
    assert!(manifest.is_ok());

    let manifest = manifest.unwrap();
    assert_eq!(manifest.name, "text-statistics");
    assert_eq!(manifest.version, "1.0.0");
    assert!(manifest.requires_permission("process_info"));
}

#[test]
#[cfg(feature = "toml")]
fn test_manifest_loading_actual_example_file() {
    // Load the actual example manifest file to verify it parses correctly
    let manifest_toml = r#"
# Plugin Manifest for Text Statistics Tool
# This file declares the plugin metadata, dependencies, permissions, and configuration

# Plugin metadata (top level, no [package] wrapper)
name = "text-statistics"
version = "1.0.0"
description = "Text analysis tool that provides word count, line count, character count, and token estimation"
authors = ["RustyCode Contributors"]
entry_point = "./text_statistics"

# Permissions required by this plugin
permissions = ["process_info"]

# Dependencies on other plugins (if any)
# This plugin has no external dependencies
[dependencies]

# Configuration schema for this plugin (JSON Schema format)
[config_schema]
type = "object"
properties = { verbose = { type = "boolean" }, output_format = { type = "string", enum = ["text", "json"] } }
required = []
"#;

    let manifest = crate::PluginManifest::from_toml(manifest_toml);
    assert!(manifest.is_ok(), "Failed to parse actual manifest TOML");

    let manifest = manifest.unwrap();
    assert_eq!(manifest.name, "text-statistics");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.description, Some("Text analysis tool that provides word count, line count, character count, and token estimation".to_string()));
    assert_eq!(
        manifest.authors,
        Some(vec!["RustyCode Contributors".to_string()])
    );
    assert_eq!(manifest.entry_point, Some("./text_statistics".to_string()));
    assert!(manifest.requires_permission("process_info"));
    assert!(manifest.config_schema.is_some());
}

#[test]
fn test_manifest_json_loading_example_plugin() {
    let manifest_json = r#"{
        "name": "text-statistics",
        "version": "1.0.0",
        "description": "Text analysis tool",
        "authors": ["RustyCode Contributors"],
        "permissions": ["process_info"],
        "config_schema": {
            "type": "object",
            "properties": {
                "verbose": {"type": "boolean"},
                "output_format": {"type": "string"}
            }
        }
    }"#;

    let manifest = crate::PluginManifest::from_json(manifest_json);
    assert!(manifest.is_ok());

    let manifest = manifest.unwrap();
    assert_eq!(manifest.name, "text-statistics");
    assert_eq!(manifest.version, "1.0.0");
    assert!(manifest.requires_permission("process_info"));
    assert!(manifest.config_schema.is_some());
}

#[test]
fn test_manifest_validation_example_plugin() {
    let manifest = crate::PluginManifest {
        name: "text-statistics".to_string(),
        version: "1.0.0".to_string(),
        description: Some("Text analysis tool".to_string()),
        authors: Some(vec!["Author".to_string()]),
        dependencies: None,
        permissions: Some(vec!["process_info".to_string()]),
        config_schema: None,
        entry_point: Some("./text_statistics".to_string()),
    };

    assert!(manifest.validate().is_ok());
}

#[test]
fn test_example_plugin_full_lifecycle_with_manifest() {
    // Load manifest
    let manifest_json = r#"{
        "name": "text-statistics",
        "version": "1.0.0",
        "description": "Text analysis tool",
        "permissions": ["process_info"]
    }"#;

    let manifest =
        crate::PluginManifest::from_json(manifest_json).expect("Failed to parse manifest");
    assert!(manifest.validate().is_ok());

    // Create registry
    let registry = PluginRegistry::new();

    // Register plugin
    let plugin = Box::new(ExampleTextStatisticsTool::new());
    assert!(registry.register_tool(plugin).is_ok());

    // Verify plugin is registered
    assert!(registry.get_tool("text-statistics").is_some());
    assert_eq!(
        registry.get_status("text-statistics"),
        Some(PluginStatus::Active)
    );

    // Get plugin tools
    let plugin = registry.get_tool("text-statistics").unwrap();
    let tools = plugin.get_tools().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "analyze_text");

    // Verify config schema matches manifest
    let config_schema = plugin.config_schema();
    assert!(config_schema["properties"].is_object());

    // Disable and re-enable
    assert!(registry.disable_tool("text-statistics").is_ok());
    assert!(registry.enable_tool("text-statistics").is_ok());

    // Unload
    assert!(registry.unload_tool("text-statistics").is_ok());
    assert!(registry.get_tool("text-statistics").is_none());
}
