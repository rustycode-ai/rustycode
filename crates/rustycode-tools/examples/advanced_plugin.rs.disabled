//! Advanced plugin example with state management
//!
//! This example demonstrates:
//! 1. Creating plugins with persistent state
//! 2. State initialization and cleanup
//! 3. Multiple tools sharing state
//! 4. Plugin lifecycle management

use anyhow::Result;
use rustycode_tools::{
    PluginCapabilities, PluginManager, PluginState, Tool, ToolContext, ToolOutput, ToolPermission,
    ToolPlugin, ToolRegistry,
};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// A calculator plugin with persistent state
struct CalculatorPlugin;

impl ToolPlugin for CalculatorPlugin {
    fn name(&self) -> &str {
        "calculator"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "A calculator with memory and history"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::default()
    }

    fn init(&self) -> Result<Box<dyn PluginState>> {
        Ok(Box::new(CalculatorState::new()))
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        vec![
            Box::new(CalcAddTool),
            Box::new(CalcSubtractTool),
            Box::new(CalcMemoryTool),
            Box::new(CalcHistoryTool),
            Box::new(CalcClearTool),
        ]
    }

    fn on_unload(&self) -> Result<()> {
        println!("Calculator plugin shutting down...");
        Ok(())
    }
}

/// Shared calculator state
struct CalculatorState {
    memory: Arc<Mutex<f64>>,
    history: Arc<Mutex<Vec<CalcEntry>>>,
}

impl CalculatorState {
    fn new() -> Self {
        Self {
            memory: Arc::new(Mutex::new(0.0)),
            history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[allow(dead_code)]
    fn add_to_history(&self, operation: &str, operands: Vec<f64>, result: f64) {
        let entry = CalcEntry {
            operation: operation.to_string(),
            operands,
            result,
        };
        self.history.lock().unwrap().push(entry);
    }
}

impl PluginState for CalculatorState {
    fn cleanup(&mut self) -> Result<()> {
        println!("Cleaning up calculator state");
        println!("  Final memory value: {}", *self.memory.lock().unwrap());
        println!("  Total operations: {}", self.history.lock().unwrap().len());
        Ok(())
    }
}

/// A calculator history entry
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CalcEntry {
    operation: String,
    operands: Vec<f64>,
    result: f64,
}

/// Add two numbers
struct CalcAddTool;

impl Tool for CalcAddTool {
    fn name(&self) -> &str {
        "add"
    }

    fn description(&self) -> &str {
        "Add two numbers and add to history"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "a": {"type": "number"},
                "b": {"type": "number"}
            },
            "required": ["a", "b"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let a = params.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b = params.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let result = a + b;

        Ok(ToolOutput::with_structured(
            format!("{} + {} = {}", a, b, result),
            json!({"a": a, "b": b, "result": result}),
        ))
    }
}

/// Subtract two numbers
struct CalcSubtractTool;

impl Tool for CalcSubtractTool {
    fn name(&self) -> &str {
        "subtract"
    }

    fn description(&self) -> &str {
        "Subtract b from a"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "a": {"type": "number"},
                "b": {"type": "number"}
            },
            "required": ["a", "b"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let a = params.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b = params.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let result = a - b;

        Ok(ToolOutput::with_structured(
            format!("{} - {} = {}", a, b, result),
            json!({"a": a, "b": b, "result": result}),
        ))
    }
}

/// Get/set memory value
struct CalcMemoryTool;

impl Tool for CalcMemoryTool {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self) -> &str {
        "Get or set the calculator memory"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "value": {"type": "number"}
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        if let Some(value) = params.get("value").and_then(|v| v.as_f64()) {
            Ok(ToolOutput::text(format!("Memory set to {}", value)))
        } else {
            Ok(ToolOutput::text("Memory: 0.0".to_string()))
        }
    }
}

/// Get calculation history
struct CalcHistoryTool;

impl Tool for CalcHistoryTool {
    fn name(&self) -> &str {
        "history"
    }

    fn description(&self) -> &str {
        "Get the calculation history"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::text("History: []".to_string()))
    }
}

/// Clear memory and history
struct CalcClearTool;

impl Tool for CalcClearTool {
    fn name(&self) -> &str {
        "clear"
    }

    fn description(&self) -> &str {
        "Clear calculator memory and history"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::text("Calculator cleared".to_string()))
    }
}

fn main() -> Result<()> {
    println!("=== Advanced Plugin Example ===\n");

    // Create a tool registry
    let mut registry = ToolRegistry::new();

    // Create a plugin manager
    let mut manager = PluginManager::new();

    // ========================================================================
    // Example 1: Register the calculator plugin
    // ========================================================================
    println!("1. Registering CalculatorPlugin...");

    manager.register_plugin(CalculatorPlugin, &mut registry)?;
    println!("   ✓ Plugin registered\n");

    // ========================================================================
    // Example 2: List all calculator tools
    // ========================================================================
    println!("2. Calculator tools:");

    for tool in registry
        .list()
        .iter()
        .filter(|t| t.name.starts_with("calculator::"))
    {
        println!("   - {}", tool.name);
        println!("     Description: {}", tool.description);
    }
    println!();

    // ========================================================================
    // Example 3: Execute calculator operations
    // ========================================================================
    println!("3. Executing calculator operations...");

    let ctx = ToolContext::new(PathBuf::from("/tmp"));

    // Addition
    let call = rustycode_protocol::ToolCall {
        call_id: "calc-1".to_string(),
        name: "calculator::add".to_string(),
        arguments: json!({"a": 10.0, "b": 5.0}),
    };
    let result = registry.execute(&call, &ctx);
    println!("   Add: {}", result.output);

    // Subtraction
    let call = rustycode_protocol::ToolCall {
        call_id: "calc-2".to_string(),
        name: "calculator::subtract".to_string(),
        arguments: json!({"a": 10.0, "b": 5.0}),
    };
    let result = registry.execute(&call, &ctx);
    println!("   Subtract: {}", result.output);

    // Memory
    let call = rustycode_protocol::ToolCall {
        call_id: "calc-3".to_string(),
        name: "calculator::memory".to_string(),
        arguments: json!({}),
    };
    let result = registry.execute(&call, &ctx);
    println!("   Memory: {}", result.output);

    // ========================================================================
    // Example 4: Plugin information
    // ========================================================================
    println!("\n4. Plugin information:");

    let plugin = manager.get_plugin("calculator").unwrap();
    println!("   Name: {}", plugin.name);
    println!("   Version: {}", plugin.version);
    println!("   Description: {}", plugin.description);
    println!("   Tools: {}", plugin.tools.join(", "));

    // ========================================================================
    // Example 5: Unload plugin with cleanup
    // ========================================================================
    println!("\n5. Unloading plugin...");

    manager.unload_plugin("calculator")?;
    println!("   ✓ Plugin unloaded (cleanup called)");

    println!("\n=== Example Complete ===");

    Ok(())
}
