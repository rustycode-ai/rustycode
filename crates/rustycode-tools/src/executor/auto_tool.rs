use crate::{Tool, ToolContext, ToolOutput, ToolRegistry};
use anyhow::{anyhow, Result};
use rustycode_protocol::{ToolCall, ToolResult};
use serde_json::Value;
use std::sync::Arc;

/// Configuration for programmatic tool calling
#[derive(Debug, Clone)]
pub struct AutoToolConfig {
    /// Maximum depth of nested tool calls
    pub max_depth: usize,
    /// Enable automatic tool chaining
    pub enable_chaining: bool,
    /// Allow tools to call themselves (be careful with recursion)
    pub allow_recursive_calls: bool,
}

impl Default for AutoToolConfig {
    fn default() -> Self {
        Self {
            max_depth: 5,
            enable_chaining: true,
            allow_recursive_calls: false,
        }
    }
}

/// Context for programmatic tool calling
pub struct AutoToolContext {
    /// Tool registry for finding and calling tools
    pub registry: Arc<ToolRegistry>,
    /// Execution context for tool calls
    pub tool_context: ToolContext,
    /// Configuration for auto tool behavior
    pub config: AutoToolConfig,
    /// Current depth of nested tool calls
    pub current_depth: usize,
    /// Tool call history for debugging
    pub call_history: Vec<String>,
}

impl AutoToolContext {
    pub fn new(registry: Arc<ToolRegistry>, tool_context: ToolContext) -> Self {
        Self {
            registry,
            tool_context,
            config: AutoToolConfig::default(),
            current_depth: 0,
            call_history: Vec::new(),
        }
    }

    /// Create with custom configuration
    pub const fn with_config(
        registry: Arc<ToolRegistry>,
        tool_context: ToolContext,
        config: AutoToolConfig,
    ) -> Self {
        Self {
            registry,
            tool_context,
            config,
            current_depth: 0,
            call_history: Vec::new(),
        }
    }

    /// Execute a tool programmatically
    ///
    /// This allows tools to call other tools directly without going through
    /// the model. This is useful for tool chaining and automation.
    ///
    pub fn call_tool(&mut self, tool_name: &str, params: Value) -> Result<ToolOutput> {
        // Check depth limit
        if self.current_depth >= self.config.max_depth {
            return Err(anyhow!(
                "maximum tool call depth reached: {}",
                self.config.max_depth
            ));
        }

        // Check if tool exists
        let tool = self
            .registry
            .get(tool_name)
            .ok_or_else(|| anyhow!("tool not found: {tool_name}"))?;

        // Check for recursive calls
        if !self.config.allow_recursive_calls {
            if let Some(last_call) = self.call_history.last() {
                if last_call == tool_name {
                    return Err(anyhow!(
                        "recursive tool call detected: {tool_name} calling itself"
                    ));
                }
            }
        }

        // Record call in history
        self.call_history.push(tool_name.to_string());

        // Increment depth
        self.current_depth = self.current_depth.saturating_add(1);

        // Execute the tool
        let result = tool.execute(params, &self.tool_context);

        // Decrement depth
        self.current_depth -= 1;

        result
    }

    /// Execute a tool call using a `ToolCall` protocol object
    pub fn execute_tool_call(&mut self, call: &ToolCall) -> ToolResult {
        // Check depth limit
        if self.current_depth >= self.config.max_depth {
            return ToolResult {
                call_id: call.call_id.clone(),
                output: String::new(),
                error: Some(format!(
                    "maximum tool call depth reached: {}",
                    self.config.max_depth
                )),
                success: false,
                exit_code: None,
                data: None,
                new_cwd: None,
            };
        }

        // Record call in history
        self.call_history.push(call.name.clone());

        // Increment depth
        self.current_depth = self.current_depth.saturating_add(1);

        // Execute via registry
        let result = self.registry.execute(call, &self.tool_context);

        // Decrement depth
        self.current_depth -= 1;

        result
    }

    /// Get the call history (for debugging)
    pub fn call_history(&self) -> &[String] {
        &self.call_history
    }

    /// Clear the call history
    pub fn clear_history(&mut self) {
        self.call_history.clear();
    }

    /// Get current depth
    pub const fn current_depth(&self) -> usize {
        self.current_depth
    }
}

/// Extension trait for tools that support programmatic calling
pub trait AutoTool: Tool {
    /// Called when this tool is invoked programmatically by another tool
    ///
    /// This allows tools to customize their behavior when called programmatically
    /// vs when called by the model. For example, a tool might skip certain
    /// validations or use different defaults.
    ///
    fn execute_auto(
        &self,
        params: Value,
        ctx: &ToolContext,
        _auto_ctx: &mut AutoToolContext,
    ) -> Result<ToolOutput> {
        // Default implementation just uses regular execute
        self.execute(params, ctx)
    }
}

/// Blanket implementation for all tools
impl<T: Tool> AutoTool for T {}

/// Create an auto tool context from a tool executor
impl From<crate::ToolExecutor> for AutoToolContext {
    fn from(executor: crate::ToolExecutor) -> Self {
        Self::new(executor.registry, executor.context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use anyhow::Result;
    use serde_json::Value;
    use tempfile::tempdir;

    /// Mock tool for testing that doesn't execute anything
    struct MockTool;

    impl Tool for MockTool {
        fn name(&self) -> &str {
            "mock"
        }

        fn description(&self) -> &str {
            "A mock tool for testing"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
            Ok(ToolOutput::text("mock output"))
        }
    }

    #[test]
    fn test_auto_tool_context_creation() {
        let registry = ToolRegistry::new();
        let workspace = tempdir().expect("workspace tempdir");
        let ctx = ToolContext::new(workspace.path());

        let auto_ctx = AutoToolContext::new(Arc::new(registry), ctx);
        assert_eq!(auto_ctx.current_depth(), 0);
        assert_eq!(auto_ctx.call_history().len(), 0);
    }

    #[test]
    fn test_auto_tool_config_default() {
        let config = AutoToolConfig::default();
        assert_eq!(config.max_depth, 5);
        assert!(config.enable_chaining);
        assert!(!config.allow_recursive_calls);
    }

    #[test]
    fn test_auto_tool_call_depth_limit() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);

        let workspace = tempdir().expect("workspace tempdir");
        let ctx = ToolContext::new(workspace.path());

        let mut auto_ctx = AutoToolContext::with_config(
            Arc::new(registry),
            ctx,
            AutoToolConfig {
                max_depth: 2,
                ..Default::default()
            },
        );

        // First call should succeed
        let result = auto_ctx.call_tool("mock", serde_json::json!({}));
        assert!(result.is_ok());

        // Check depth increased
        assert_eq!(auto_ctx.current_depth(), 0); // Should be reset after call
        assert_eq!(auto_ctx.call_history().len(), 1);
    }

    #[test]
    fn test_auto_tool_recursive_detection() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);

        let workspace = tempdir().expect("workspace tempdir");
        let ctx = ToolContext::new(workspace.path());

        let mut auto_ctx = AutoToolContext::with_config(
            Arc::new(registry),
            ctx,
            AutoToolConfig {
                allow_recursive_calls: false,
                ..Default::default()
            },
        );

        // First call
        let _ = auto_ctx.call_tool("mock", serde_json::json!({}));

        // Second call to same tool should fail (recursive)
        let result = auto_ctx.call_tool("mock", serde_json::json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("recursive"));
    }

    #[test]
    fn test_auto_tool_clear_history() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);

        let workspace = tempdir().expect("workspace tempdir");
        let ctx = ToolContext::new(workspace.path());
        let config = AutoToolConfig {
            allow_recursive_calls: true,
            ..Default::default()
        };
        let mut auto_ctx = AutoToolContext::with_config(Arc::new(registry), ctx, config);

        let _ = auto_ctx.call_tool("mock", serde_json::json!({}));
        assert_eq!(auto_ctx.call_history().len(), 1);

        auto_ctx.clear_history();
        assert!(auto_ctx.call_history().is_empty());
    }

    #[test]
    fn test_auto_tool_execute_tool_call_unknown_tool() {
        let registry = ToolRegistry::new();
        let workspace = tempdir().expect("workspace tempdir");
        let ctx = ToolContext::new(workspace.path());
        let mut auto_ctx = AutoToolContext::new(Arc::new(registry), ctx);

        let call = ToolCall {
            call_id: "call_1".to_string(),
            name: "nonexistent".to_string(),
            arguments: serde_json::json!({}),
        };
        let result = auto_ctx.execute_tool_call(&call);
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_auto_tool_call_tool_not_found() {
        let registry = ToolRegistry::new();
        let workspace = tempdir().expect("workspace tempdir");
        let ctx = ToolContext::new(workspace.path());
        let mut auto_ctx = AutoToolContext::new(Arc::new(registry), ctx);

        let result = auto_ctx.call_tool("nonexistent", serde_json::json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("tool not found"));
    }

    #[test]
    fn test_auto_tool_allow_recursive_calls() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);

        let workspace = tempdir().expect("workspace tempdir");
        let ctx = ToolContext::new(workspace.path());
        let config = AutoToolConfig {
            allow_recursive_calls: true,
            ..Default::default()
        };
        let mut auto_ctx = AutoToolContext::with_config(Arc::new(registry), ctx, config);

        // Both calls should succeed when recursive is allowed
        let r1 = auto_ctx.call_tool("mock", serde_json::json!({}));
        let r2 = auto_ctx.call_tool("mock", serde_json::json!({}));
        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert_eq!(auto_ctx.call_history().len(), 2);
    }
}
