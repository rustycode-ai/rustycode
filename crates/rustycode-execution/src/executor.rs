//! Core execution traits and types
//!
//! This module defines the core execution interfaces for running plans and steps.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Configuration for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Maximum execution time
    pub timeout: Duration,

    /// Whether to continue on step failures
    pub continue_on_failure: bool,

    /// Maximum number of retries per step
    pub max_retries: u32,

    /// Whether to enable execution monitoring
    pub enable_monitoring: bool,

    /// Whether to continue on errors
    pub continue_on_error: bool,

    /// Maximum number of iterations
    pub max_iterations: usize,

    /// Maximum time per step in seconds
    pub step_timeout_secs: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_mins(5),
            continue_on_failure: false,
            max_retries: 3,
            enable_monitoring: true,
            continue_on_error: true,
            max_iterations: 10,
            step_timeout_secs: 30,
        }
    }
}

/// Result of an execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Whether the execution completed successfully
    pub success: bool,

    /// Output or result of the execution
    pub output: String,

    /// Execution time
    pub duration: Duration,

    /// Number of steps executed
    pub steps_executed: usize,

    /// Error message if execution failed
    pub error: Option<String>,
}

impl ExecutionResult {
    /// Create a successful result
    pub const fn success(output: String, duration: Duration, steps_executed: usize) -> Self {
        Self {
            success: true,
            output,
            duration,
            steps_executed,
            error: None,
        }
    }

    /// Create a failed result
    pub const fn failure(error: String, duration: Duration, steps_executed: usize) -> Self {
        Self {
            success: false,
            output: String::new(),
            duration,
            steps_executed,
            error: Some(error),
        }
    }
}

/// Core executor trait
#[async_trait]
pub trait Executor: Send + Sync {
    /// Execute a plan
    async fn execute_plan(
        &self,
        plan: &rustycode_protocol::Plan,
        config: &ExecutionConfig,
    ) -> Result<ExecutionResult>;

    /// Execute a single step
    async fn execute_step(
        &self,
        step: &PlanStep,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult>;
}

use anyhow::bail;
use chrono::Utc;
use rustycode_protocol::PlanStep;
use rustycode_protocol::{Conversation, Message, ToolCall, ToolResult};
use rustycode_tools::{ToolContext, ToolRegistry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Check if a tool is critical for plan execution.
///
/// Critical tools are those whose failure should immediately halt
/// plan execution, as subsequent steps depend on their success.
fn is_critical_tool(tool_name: &str) -> bool {
    const CRITICAL_TOOLS: &[&str] = &["read_file", "write_file", "bash"];
    let base_name = tool_name.split(':').next().unwrap_or(tool_name);
    CRITICAL_TOOLS.contains(&base_name)
}

/// Create a tool registry with all available tools registered.
///
/// Uses the shared `default_registry()` so plan execution has the same
/// tool set as CLI, TUI, and headless modes.
fn create_tool_registry() -> ToolRegistry {
    rustycode_tools::default_registry()
}

/// Configuration for plan execution limits.
#[derive(Clone)]
/// Context for plan execution with error tracking.
pub struct ExecutionContext {
    /// Configuration for execution limits.
    pub config: ExecutionConfig,
    /// Number of steps executed so far.
    pub steps_executed: usize,
    /// Errors encountered during execution.
    pub errors: Vec<String>,
    /// Whether execution should continue.
    pub should_continue: bool,
    /// Working directory for tool execution.
    pub cwd: PathBuf,
    /// Tool registry for executing tools.
    pub tool_registry: Arc<ToolRegistry>,
}

impl fmt::Debug for ExecutionContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("config", &self.config)
            .field("steps_executed", &self.steps_executed)
            .field("errors", &self.errors)
            .field("should_continue", &self.should_continue)
            .field("cwd", &self.cwd)
            .field("tool_registry", &"<registry>")
            .finish()
    }
}

impl ExecutionContext {
    /// Create a new execution context.
    pub fn new(config: ExecutionConfig, cwd: PathBuf) -> Self {
        // Create tool registry and register all available tools
        let tool_registry = create_tool_registry();

        Self {
            config,
            steps_executed: 0,
            errors: vec![],
            should_continue: true,
            cwd,
            tool_registry: Arc::new(tool_registry),
        }
    }

    /// Record an error and decide whether to continue.
    pub fn record_error(&mut self, error: String) {
        self.errors.push(error);
        // Continue on the first few errors, or always continue when configured.
        self.should_continue = self.config.continue_on_error || self.errors.len() < 3;
    }

    /// Check if max iterations exceeded.
    pub fn check_iteration_limit(&mut self) -> Result<()> {
        self.steps_executed = self.steps_executed.saturating_add(1);
        if self.steps_executed > self.config.max_iterations {
            let msg = format!(
                "Exceeded maximum iterations ({}/{})",
                self.steps_executed, self.config.max_iterations
            );
            self.record_error(msg.clone());
            bail!(msg);
        }
        Ok(())
    }

    /// Get human-readable status.
    pub fn status(&self) -> String {
        format!(
            "Execution: {}/{} steps, {} errors, continuing: {}",
            self.steps_executed,
            self.config.max_iterations,
            self.errors.len(),
            self.should_continue
        )
    }
}

/// Trait for executing a plan step.
pub trait StepExecutor: Send + Sync {
    /// Execute a step and return the updated step with results.
    fn execute(
        &self,
        step: PlanStep,
        conversation: &mut Conversation,
        ctx: &ExecutionContext,
    ) -> Result<PlanStep>;
}

/// Registry of available step executors.
pub struct StepExecutorRegistry {
    executors: HashMap<String, Arc<dyn StepExecutor>>,
}

impl StepExecutorRegistry {
    /// Create a new empty executor registry.
    pub fn new() -> Self {
        Self {
            executors: HashMap::new(),
        }
    }

    /// Register an executor for a step type.
    pub fn register(&mut self, step_type: String, executor: Arc<dyn StepExecutor>) {
        self.executors.insert(step_type, executor);
    }

    /// Get an executor by step type.
    pub fn get(&self, step_type: &str) -> Option<Arc<dyn StepExecutor>> {
        self.executors.get(step_type).cloned()
    }

    /// Get default (generic) executor for any step type.
    pub fn default_executor(&self, cwd: PathBuf) -> Arc<dyn StepExecutor> {
        // Return a generic executor that uses the tool registry
        Arc::new(GenericStepExecutor::new(cwd))
    }
}

impl Default for StepExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Generic step executor for standard steps.
///
/// This executor uses a tool registry to execute tools specified in plan steps.
/// It maintains a working directory and tool registry for executing tools.
struct GenericStepExecutor {
    cwd: PathBuf,
    tool_registry: Arc<ToolRegistry>,
}

impl GenericStepExecutor {
    /// Create a new generic step executor.
    fn new(cwd: PathBuf) -> Self {
        let tool_registry = Arc::new(create_tool_registry());
        Self { cwd, tool_registry }
    }

    /// Process step feedback through conversation loop.
    fn feedback_loop(
        step: &mut PlanStep,
        conversation: &mut Conversation,
        tool_result: Option<ToolResult>,
        tool_name: Option<&str>,
    ) {
        if let Some(result) = tool_result {
            // Get tool name for result wrapping
            let name =
                tool_name.unwrap_or_else(|| step.tools.first().map_or("generic", String::as_str));

            // Wrap tool output into message
            let msg = ToolInvocationWrapper::wrap_result(name, &result);
            conversation.add_message(msg);

            // Record tool execution in step
            step.results.push(format!("Tool executed: {name}"));
            if let Some(ref error) = result.error {
                step.errors.push(error.clone());
            }
        }
    }
}

impl StepExecutor for GenericStepExecutor {
    fn execute(
        &self,
        mut step: PlanStep,
        conversation: &mut Conversation,
        _ctx: &ExecutionContext,
    ) -> Result<PlanStep> {
        use rustycode_protocol::StepStatus;

        step.execution_status = StepStatus::InProgress;
        step.started_at = Some(Utc::now());

        // Add step start message to conversation
        conversation.add_message(Message::user(format!(
            "Executing step: {}\nDescription: {}\nTools to use: {:?}",
            step.title, step.description, step.tools
        )));

        // Execute tools specified in the step
        let mut tool_results = Vec::new();

        // Extract first tool name as owned String to avoid borrow checker issues
        let first_tool_name = step
            .tools
            .first()
            .and_then(|t| t.split(':').next())
            .unwrap_or("generic")
            .to_string();

        for tool_spec in &step.tools {
            // Parse tool specification (format: "tool_name:param1=value1,param2=value2")
            let (tool_name, params_str) = match tool_spec.split_once(':') {
                Some((name, params)) => (name, params),
                None => (tool_spec.as_str(), ""),
            };

            // Parse parameters as JSON
            let params = if params_str.is_empty() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                // Try to parse as JSON object
                match serde_json::from_str(params_str) {
                    Ok(params) => params,
                    Err(e) => {
                        // Log parsing error but continue with empty params
                        conversation.add_message(Message::assistant(format!(
                            "Warning: Failed to parse parameters for tool '{tool_name}': {e}. Using empty parameters."
                        )));
                        serde_json::json!({})
                    }
                }
            };

            // Create tool context
            let tool_ctx = ToolContext::new(&self.cwd);

            // Create tool call
            let tool_call = ToolCall {
                call_id: format!("{}-{}", tool_name, step.order),
                name: tool_name.to_string(),
                arguments: params,
            };

            // Execute tool using the tool registry from context
            let result = self.tool_registry.execute(&tool_call, &tool_ctx);

            // Record result
            tool_results.push(result.clone());

            // Log to conversation
            if let Some(ref error) = result.error {
                conversation.add_message(Message::assistant(format!(
                    "Tool '{tool_name}' failed: {error}"
                )));
                step.errors.push(error.clone());
            } else {
                conversation.add_message(Message::assistant(format!(
                    "Tool '{tool_name}' output:\n{output}",
                    output = result.output
                )));
            }

            // If this was a critical tool and it failed, stop execution
            if !result.success && is_critical_tool(tool_name) {
                conversation.add_message(Message::assistant(format!(
                    "Critical tool '{tool_name}' failed - stopping step execution"
                )));
                // Mark step as failed but continue to record results
                step.execution_status = StepStatus::Failed;
                break;
            }
        }

        // Use the first tool result for feedback loop (or create default if none)
        let first_result = tool_results
            .into_iter()
            .next()
            .unwrap_or_else(|| ToolResult {
                call_id: format!("step-{}", step.order),
                output: format!("Step '{}' completed (no tools executed)", step.title),
                error: None,
                success: true,
                exit_code: None,
                data: None,
            });

        // Process through feedback loop
        Self::feedback_loop(
            &mut step,
            conversation,
            Some(first_result),
            Some(&first_tool_name),
        );

        // Only set status to Completed if not already failed
        if step.execution_status == StepStatus::Failed {
            step.results.push(format!(
                "Step '{}' failed due to critical tool error",
                step.title
            ));
        } else {
            step.execution_status = StepStatus::Completed;
            step.results.push(format!(
                "Step '{}' executed successfully (tools: {tools:?})",
                step.title,
                tools = step.tools
            ));
        }
        step.completed_at = Some(Utc::now());

        // Add completion message to conversation
        conversation.add_message(Message::assistant(format!(
            "Step completed: {title}\n\nExpected outcome: {outcome}\n\nResults: {results}",
            title = step.title,
            outcome = step.expected_outcome,
            results = step.results.join("\n")
        )));

        Ok(step)
    }
}

/// Wraps tool invocation with output capture and message conversion.
pub struct ToolInvocationWrapper;

impl ToolInvocationWrapper {
    /// Create a new tool invocation wrapper.
    pub fn new(_tool_name: String, _args: String) -> Self {
        Self
    }

    /// Convert a tool result to a conversation message.
    pub fn result_to_message(tool_name: &str, result: &ToolResult) -> Message {
        if result.error.is_none() {
            Message::assistant(format!(
                "Tool: {}\nCall ID: {}\n\nOutput:\n{}",
                tool_name, result.call_id, result.output
            ))
        } else {
            Message::assistant(format!(
                "Tool: {} failed\nError: {}",
                tool_name,
                result.error.as_deref().unwrap_or("Unknown error")
            ))
        }
    }

    /// Wrap a tool result into a message for adding to conversation.
    /// Note: This requires the tool name to be passed separately since `ToolResult`
    /// only contains `call_id`, not the tool name.
    pub fn wrap_result(tool_name: &str, result: &ToolResult) -> Message {
        Self::result_to_message(tool_name, result)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_config_defaults() {
        let config = ExecutionConfig::default();
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.step_timeout_secs, 30);
        assert!(config.continue_on_error);
    }

    #[test]
    fn test_execution_context_new() {
        let config = ExecutionConfig::default();
        let cwd = PathBuf::from(".");
        let ctx = ExecutionContext::new(config, cwd);
        assert_eq!(ctx.steps_executed, 0);
        assert!(ctx.errors.is_empty());
        assert!(ctx.should_continue);
    }

    #[test]
    fn test_execution_context_record_error() {
        let config = ExecutionConfig::default();
        let cwd = PathBuf::from(".");
        let mut ctx = ExecutionContext::new(config, cwd);
        ctx.record_error("Test error".to_string());
        assert_eq!(ctx.errors.len(), 1);
        assert_eq!(ctx.errors[0], "Test error");
    }

    #[test]
    fn test_execution_context_iteration_limit() {
        let config = ExecutionConfig {
            max_iterations: 2,
            ..Default::default()
        };
        let cwd = PathBuf::from(".");
        let mut ctx = ExecutionContext::new(config, cwd);

        // First iteration should succeed
        assert!(ctx.check_iteration_limit().is_ok());
        assert_eq!(ctx.steps_executed, 1);

        // Second iteration should succeed
        assert!(ctx.check_iteration_limit().is_ok());
        assert_eq!(ctx.steps_executed, 2);

        // Third iteration should fail
        assert!(ctx.check_iteration_limit().is_err());
        assert_eq!(ctx.steps_executed, 3);
    }

    #[test]
    fn test_step_executor_registry_new() {
        let registry = StepExecutorRegistry::new();
        assert!(registry.get("test").is_none());
    }

    #[test]
    fn test_step_executor_registry_default() {
        let registry = StepExecutorRegistry::new();
        let _executor = registry.default_executor(PathBuf::from("."));
        // Note: Full executor test requires PlanStep with all fields,
        // which we can't easily construct here without Default
    }

    #[test]
    fn test_is_critical_tool() {
        // Critical tools should be detected
        assert!(is_critical_tool("read_file"));
        assert!(is_critical_tool("write_file"));
        assert!(is_critical_tool("bash"));
        assert!(is_critical_tool("bash:some command"));

        // Non-critical tools should not be detected
        assert!(!is_critical_tool("grep"));
        assert!(!is_critical_tool("glob"));
        assert!(!is_critical_tool("git_status"));
    }

    // --- ExecutionResult constructors ---

    #[test]
    fn test_execution_result_success() {
        let result = ExecutionResult::success("done".to_string(), Duration::from_secs(1), 3);
        assert!(result.success);
        assert_eq!(result.output, "done");
        assert_eq!(result.steps_executed, 3);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_execution_result_failure() {
        let result =
            ExecutionResult::failure("something broke".to_string(), Duration::from_secs(2), 5);
        assert!(!result.success);
        assert!(result.output.is_empty());
        assert_eq!(result.steps_executed, 5);
        assert_eq!(result.error.as_deref(), Some("something broke"));
    }

    // --- ExecutionContext edge cases ---

    #[test]
    fn test_context_record_many_errors_stops() {
        let config = ExecutionConfig {
            continue_on_error: false,
            ..Default::default()
        };
        let cwd = PathBuf::from(".");
        let mut ctx = ExecutionContext::new(config, cwd);
        for i in 0..5 {
            ctx.record_error(format!("error {i}"));
        }
        // After 3 errors, should_continue should be false
        assert!(!ctx.should_continue);
        assert_eq!(ctx.errors.len(), 5);
    }

    #[test]
    fn test_context_continue_on_error_always_continues() {
        let config = ExecutionConfig {
            continue_on_error: true,
            ..Default::default()
        };
        let cwd = PathBuf::from(".");
        let mut ctx = ExecutionContext::new(config, cwd);
        for i in 0..10 {
            ctx.record_error(format!("error {i}"));
        }
        assert!(
            ctx.should_continue,
            "continue_on_error=true should always continue"
        );
    }

    #[test]
    fn test_context_status_format() {
        let config = ExecutionConfig {
            max_iterations: 20,
            ..Default::default()
        };
        let cwd = PathBuf::from(".");
        let mut ctx = ExecutionContext::new(config, cwd);
        ctx.steps_executed = 5;
        ctx.record_error("oops".to_string());
        let status = ctx.status();
        assert!(status.contains("5/20"));
        assert!(status.contains("1 errors"));
    }

    // --- StepExecutorRegistry ---

    #[test]
    fn test_step_executor_registry_register_and_get() {
        let mut registry = StepExecutorRegistry::new();
        let executor = Arc::new(MockStepExecutor);
        registry.register("test_step".to_string(), executor);
        assert!(registry.get("test_step").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    // --- ToolInvocationWrapper ---

    #[test]
    fn test_tool_invocation_wrapper_success_result() {
        let result = ToolResult {
            call_id: "call-1".to_string(),
            output: "file contents".to_string(),
            error: None,
            success: true,
            exit_code: Some(0),
            data: None,
        };
        let msg = ToolInvocationWrapper::wrap_result("read_file", &result);
        let content = &msg.content;
        assert!(content.contains("read_file"));
        assert!(content.contains("file contents"));
    }

    #[test]
    fn test_tool_invocation_wrapper_error_result() {
        let result = ToolResult {
            call_id: "call-2".to_string(),
            output: String::new(),
            error: Some("file not found".to_string()),
            success: false,
            exit_code: Some(1),
            data: None,
        };
        let msg = ToolInvocationWrapper::wrap_result("write_file", &result);
        let content = &msg.content;
        assert!(content.contains("write_file"));
        assert!(content.contains("file not found"));
    }

    // --- ExecutionConfig custom ---

    #[test]
    fn test_execution_config_custom() {
        let config = ExecutionConfig {
            timeout: Duration::from_mins(10),
            continue_on_failure: true,
            max_retries: 5,
            enable_monitoring: false,
            continue_on_error: false,
            max_iterations: 100,
            step_timeout_secs: 120,
        };
        assert_eq!(config.timeout, Duration::from_mins(10));
        assert!(config.continue_on_failure);
        assert_eq!(config.max_retries, 5);
        assert!(!config.enable_monitoring);
        assert!(!config.continue_on_error);
        assert_eq!(config.max_iterations, 100);
    }

    // --- Serialization roundtrips ---

    #[test]
    fn test_execution_config_serde_roundtrip() {
        let config = ExecutionConfig {
            timeout: Duration::from_secs(42),
            continue_on_failure: true,
            max_retries: 7,
            enable_monitoring: false,
            continue_on_error: false,
            max_iterations: 50,
            step_timeout_secs: 99,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: ExecutionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.timeout, config.timeout);
        assert_eq!(decoded.continue_on_failure, config.continue_on_failure);
        assert_eq!(decoded.max_retries, config.max_retries);
        assert_eq!(decoded.max_iterations, config.max_iterations);
    }

    #[test]
    fn test_execution_result_serde_roundtrip() {
        let result = ExecutionResult::success("output text".to_string(), Duration::from_secs(3), 7);
        let json = serde_json::to_string(&result).unwrap();
        let decoded: ExecutionResult = serde_json::from_str(&json).unwrap();
        assert!(decoded.success);
        assert_eq!(decoded.output, "output text");
        assert_eq!(decoded.steps_executed, 7);
    }

    #[test]
    fn test_execution_result_failure_serde_roundtrip() {
        let result =
            ExecutionResult::failure("error msg".to_string(), Duration::from_millis(500), 2);
        let json = serde_json::to_string(&result).unwrap();
        let decoded: ExecutionResult = serde_json::from_str(&json).unwrap();
        assert!(!decoded.success);
        assert_eq!(decoded.error.as_deref(), Some("error msg"));
    }

    // --- ExecutionContext debug ---

    #[test]
    fn test_execution_context_debug_format() {
        let config = ExecutionConfig::default();
        let cwd = PathBuf::from("/tmp/test");
        let ctx = ExecutionContext::new(config, cwd);
        let debug_str = format!("{ctx:?}");
        assert!(debug_str.contains("ExecutionContext"));
        assert!(debug_str.contains("<registry>"));
    }

    // --- GenericStepExecutor via StepExecutorRegistry ---

    #[test]
    fn test_default_executor_is_created() {
        let registry = StepExecutorRegistry::new();
        let _executor = registry.default_executor(PathBuf::from("."));
    }

    // --- ToolInvocationWrapper edge cases ---

    #[test]
    fn test_tool_invocation_wrapper_result_to_message_success() {
        let result = ToolResult {
            call_id: "c1".to_string(),
            output: "hello world".to_string(),
            error: None,
            success: true,
            exit_code: None,
            data: None,
        };
        let msg = ToolInvocationWrapper::result_to_message("bash", &result);
        assert!(msg.content.contains("bash"));
        assert!(msg.content.contains("hello world"));
        assert!(msg.content.contains("c1"));
    }

    #[test]
    fn test_tool_invocation_wrapper_result_to_message_error() {
        let result = ToolResult {
            call_id: "c2".to_string(),
            output: String::new(),
            error: Some("permission denied".to_string()),
            success: false,
            exit_code: Some(1),
            data: None,
        };
        let msg = ToolInvocationWrapper::result_to_message("write_file", &result);
        assert!(msg.content.contains("write_file failed"));
        assert!(msg.content.contains("permission denied"));
    }

    #[test]
    fn test_tool_invocation_wrapper_new() {
        let _wrapper = ToolInvocationWrapper::new("tool".to_string(), "args".to_string());
    }

    // --- is_critical_tool edge cases ---

    #[test]
    fn test_is_critical_tool_with_colon_suffix() {
        assert!(is_critical_tool("read_file:some/path"));
        assert!(is_critical_tool("write_file:some/path"));
        assert!(is_critical_tool("bash:ls -la"));
    }

    #[test]
    fn test_is_critical_tool_empty_and_unknown() {
        assert!(!is_critical_tool(""));
        assert!(!is_critical_tool("unknown_tool"));
        assert!(!is_critical_tool("read")); // partial match should NOT match
        assert!(!is_critical_tool("bash_script"));
    }

    // Helper mock for StepExecutor
    struct MockStepExecutor;

    impl StepExecutor for MockStepExecutor {
        fn execute(
            &self,
            step: PlanStep,
            _conversation: &mut Conversation,
            _ctx: &ExecutionContext,
        ) -> Result<PlanStep> {
            Ok(step)
        }
    }
}
