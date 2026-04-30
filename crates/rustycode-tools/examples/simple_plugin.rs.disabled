//! Simple plugin example demonstrating the plugin system
//!
//! This example shows how to:
//! 1. Create a simple plugin with custom tools
//! 2. Register the plugin with the tool registry
//! 3. Execute plugin tools

use anyhow::Result;
use rustycode_tools::{
    PluginCapabilities, PluginManager, PluginState, Tool, ToolContext, ToolOutput, ToolPermission,
    ToolPlugin, ToolRegistry,
};
use serde_json::{json, Value};
use std::path::PathBuf;

/// A simple plugin that provides greeting tools
struct GreetingPlugin;

impl ToolPlugin for GreetingPlugin {
    fn name(&self) -> &str {
        "greeting"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn description(&self) -> &str {
        "A simple plugin that provides greeting tools"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::read_only()
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![Box::new(SayHelloTool), Box::new(SayGoodbyeTool)]
    }
}

/// A tool that says hello
struct SayHelloTool;

impl Tool for SayHelloTool {
    fn name(&self) -> &str {
        "say_hello"
    }

    fn description(&self) -> &str {
        "Says hello to someone"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name to greet"
                }
            },
            "required": ["name"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("World");

        let message = format!("Hello, {}!", name);
        Ok(ToolOutput::text(message))
    }
}

/// A tool that says goodbye
struct SayGoodbyeTool;

impl Tool for SayGoodbyeTool {
    fn name(&self) -> &str {
        "say_goodbye"
    }

    fn description(&self) -> &str {
        "Says goodbye to someone"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name to say goodbye to"
                }
            },
            "required": ["name"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("World");

        let message = format!("Goodbye, {}!", name);
        Ok(ToolOutput::text(message))
    }
}

/// An advanced plugin with state
struct CounterPlugin {
    initial_count: u32,
}

impl ToolPlugin for CounterPlugin {
    fn name(&self) -> &str {
        "counter"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn description(&self) -> &str {
        "A plugin with persistent state for counting"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::default()
    }

    fn init(&self) -> Result<Box<dyn PluginState>> {
        Ok(Box::new(CounterState {
            count: self.initial_count,
        }))
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![Box::new(GetCountTool), Box::new(IncrementCountTool)]
    }
}

/// State for the counter plugin
struct CounterState {
    count: u32,
}

impl PluginState for CounterState {
    fn cleanup(&mut self) -> Result<()> {
        println!("Cleaning up counter with final count: {}", self.count);
        Ok(())
    }
}

/// A tool that gets the current count
struct GetCountTool;

impl Tool for GetCountTool {
    fn name(&self) -> &str {
        "get_count"
    }

    fn description(&self) -> &str {
        "Get the current count value"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        // In a real implementation, this would access the plugin state
        // For now, we'll just return a placeholder
        Ok(ToolOutput::text("Current count: 0"))
    }
}

/// A tool that increments the count
struct IncrementCountTool;

impl Tool for IncrementCountTool {
    fn name(&self) -> &str {
        "increment_count"
    }

    fn description(&self) -> &str {
        "Increment the count by 1"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        // In a real implementation, this would modify the plugin state
        Ok(ToolOutput::text("Count incremented"))
    }
}

fn main() -> Result<()> {
    println!("=== Simple Plugin Example ===\n");

    // Create a tool registry
    let mut registry = ToolRegistry::new();

    // Create a plugin manager
    let mut manager = PluginManager::new();

    // ========================================================================
    // Example 1: Register and use the greeting plugin
    // ========================================================================
    println!("1. Registering GreetingPlugin...");

    manager.register_plugin(GreetingPlugin, &mut registry)?;
    println!("   ✓ Plugin registered\n");

    // List all tools
    println!("   Available tools:");
    for tool in registry.list() {
        println!("   - {}", tool.name);
    }
    println!();

    // Execute a plugin tool
    println!("2. Executing greeting::say_hello...");

    let ctx = ToolContext::new(PathBuf::from("/tmp"));
    let call = rustycode_protocol::ToolCall {
        call_id: "test-1".to_string(),
        name: "greeting::say_hello".to_string(),
        arguments: json!({"name": "Claude"}),
    };

    let result = registry.execute(&call, &ctx);
    if result.error.is_none() {
        println!("   ✓ Output: {}", result.output);
    } else {
        println!("   ✗ Error: {:?}", result.error);
    }
    println!();

    // ========================================================================
    // Example 2: List loaded plugins
    // ========================================================================
    println!("3. Listing loaded plugins...");

    for plugin in manager.list_plugins() {
        println!("   Plugin: {} (v{})", plugin.name, plugin.version);
        println!("   Description: {}", plugin.description);
        println!("   Tools: {}", plugin.tools.join(", "));
        println!();
    }

    // ========================================================================
    // Example 3: Plugin with state
    // ========================================================================
    println!("4. Registering CounterPlugin with state...");

    let counter_plugin = CounterPlugin { initial_count: 42 };
    manager.register_plugin(counter_plugin, &mut registry)?;
    println!("   ✓ Counter plugin registered");

    println!("\n   Total plugins loaded: {}", manager.plugin_count());

    // ========================================================================
    // Example 4: Unload a plugin
    // ========================================================================
    println!("\n5. Unloading greeting plugin...");

    manager.unload_plugin("greeting")?;
    println!("   ✓ Plugin unloaded");

    println!("\n   Active plugins: {}", manager.plugin_count());

    println!("\n=== Example Complete ===");

    Ok(())
}
