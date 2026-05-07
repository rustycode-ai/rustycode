//! Plan execution logic with step executors and error tracking.
//!
//! This module provides the core execution framework for running plans,
//! including configuration, context management, and step execution.

use crate::checkpoint::{CheckpointSnapshot, ExecutionPhase};
use crate::PlanStep;
use anyhow::{bail, Result};
use chrono::Utc;
use rustycode_orchestration::delegation::RetryState;
pub use rustycode_protocol::execution::StepExecutor;
use rustycode_protocol::{Conversation, Message, ToolCall, ToolResult};
use rustycode_tools::{ToolContext, ToolRegistry};
use rustycode_tools_api::MessageSender;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Thread-safe in-memory store for execution checkpoints.
///
/// Saves checkpoints in insertion order so callers can inspect pre/post pairs.
#[derive(Default)]
pub struct ExecutionCheckpointStore {
    checkpoints: std::sync::Mutex<Vec<CheckpointSnapshot>>,
}

impl ExecutionCheckpointStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn save(&self, checkpoint: CheckpointSnapshot) {
        self.checkpoints
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(checkpoint);
    }

    pub fn len(&self) -> usize {
        self.checkpoints
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn all(&self) -> Vec<CheckpointSnapshot> {
        self.checkpoints
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn latest(&self) -> Option<CheckpointSnapshot> {
        self.checkpoints
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last()
            .cloned()
    }
}

/// Check if a tool is critical for plan execution.
///
/// Critical tools are those whose failure should immediately halt
/// plan execution, as subsequent steps depend on their success.
pub fn is_critical_tool(tool_name: &str) -> bool {
    // Tools that are essential for plan continuation
    const CRITICAL_TOOLS: &[&str] = &[
        "read_file",  // Can't proceed without reading files
        "write_file", // Can't save results without writing
        "bash",       // Command execution is critical
    ];

    // Extract base tool name (without parameters)
    // Handle both "tool_name:params" and "tool_name(params)" formats
    let base_name = tool_name
        .split('(')
        .next()
        .unwrap_or(tool_name)
        .split(':')
        .next()
        .unwrap_or(tool_name);

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
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    /// Maximum number of steps to execute per plan (prevents infinite loops).
    pub max_iterations: usize,
    /// Timeout per step in seconds (prevents hanging).
    pub step_timeout_secs: u64,
    /// Whether to continue on step failure (graceful degradation).
    pub continue_on_error: bool,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            step_timeout_secs: 30,
            continue_on_error: true,
        }
    }
}

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
    /// Session ID for checkpoint correlation.
    pub session_id: String,
    /// Optional checkpoint store for saving snapshots around step execution.
    pub checkpoint_store: Option<Arc<ExecutionCheckpointStore>>,
    /// Optional message sender for inter-agent communication.
    pub message_sender: Option<Arc<dyn MessageSender>>,
    /// Mutable retry state for transient error handling and backoff.
    pub retry_state: RetryState,
}

impl ExecutionContext {
    pub fn new(config: ExecutionConfig, cwd: PathBuf) -> Self {
        let tool_registry = create_tool_registry();

        Self {
            config,
            steps_executed: 0,
            errors: vec![],
            should_continue: true,
            cwd,
            tool_registry: Arc::new(tool_registry),
            session_id: String::new(),
            checkpoint_store: None,
            message_sender: None,
            retry_state: RetryState::new(),
        }
    }

    /// Attach a session ID for checkpoint correlation.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = session_id.into();
        self
    }

    /// Attach a checkpoint store for saving snapshots during execution.
    pub fn with_checkpoint_store(mut self, store: Arc<ExecutionCheckpointStore>) -> Self {
        self.checkpoint_store = Some(store);
        self
    }

    /// Attach a message sender for inter-agent communication.
    pub fn with_message_sender(mut self, sender: Arc<dyn MessageSender>) -> Self {
        self.message_sender = Some(sender);
        self
    }

    /// Record an error and decide whether to continue.
    pub fn record_error(&mut self, error: String) {
        self.errors.push(error);
        // Continue on the first few errors, or always continue when configured.
        self.should_continue = self.config.continue_on_error || self.errors.len() < 3;
    }

    /// Check if max iterations exceeded.
    pub fn check_iteration_limit(&mut self) -> Result<()> {
        self.steps_executed += 1;
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

/// Registry of available step executors.
pub struct StepExecutorRegistry {
    executors: HashMap<String, Arc<dyn StepExecutor<ExecutionContext>>>,
}

impl StepExecutorRegistry {
    pub fn new() -> Self {
        Self {
            executors: HashMap::new(),
        }
    }

    /// Register an executor for a step type.
    pub fn register(&mut self, step_type: String, executor: Arc<dyn StepExecutor<ExecutionContext>>) {
        self.executors.insert(step_type, executor);
    }

    /// Get an executor by step type.
    pub fn get(&self, step_type: &str) -> Option<Arc<dyn StepExecutor<ExecutionContext>>> {
        self.executors.get(step_type).cloned()
    }

    /// Get default (generic) executor for any step type.
    pub fn default_executor(&self, cwd: PathBuf) -> Arc<dyn StepExecutor<ExecutionContext>> {
        // Return a generic executor that uses the tool registry
        Arc::new(GenericStepExecutor::new(cwd))
    }
}

impl Default for StepExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper that saves checkpoints around step execution.
///
/// Saves a PRE-EXECUTION checkpoint on entry. On success saves a
/// POST-EXECUTION checkpoint. On failure the pre-checkpoint is
/// preserved for recovery (no overwrite).
pub struct CheckpointingStepExecutor {
    inner: Arc<dyn StepExecutor<ExecutionContext>>,
}

impl CheckpointingStepExecutor {
    pub fn new(inner: Arc<dyn StepExecutor<ExecutionContext>>) -> Self {
        Self { inner }
    }
}

impl StepExecutor<ExecutionContext> for CheckpointingStepExecutor {
    fn execute(
        &self,
        step: PlanStep,
        conversation: &mut Conversation,
        ctx: &ExecutionContext,
    ) -> Result<PlanStep> {
        let pre_checkpoint = CheckpointSnapshot::generate(&ctx.session_id, ExecutionPhase::Act);
        if let Some(store) = &ctx.checkpoint_store {
            store.save(pre_checkpoint.clone());
        }

        let result = self.inner.execute(step, conversation, ctx);

        match &result {
            Ok(_) => {
                let post_checkpoint =
                    CheckpointSnapshot::with_context(&ctx.session_id, ExecutionPhase::Act, vec![]);
                if let Some(store) = &ctx.checkpoint_store {
                    store.save(post_checkpoint);
                }
            }
            Err(_) => {
                tracing::debug!("Preserving checkpoint {} for recovery", pre_checkpoint.id);
            }
        }

        result
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
            let name = tool_name
                .unwrap_or_else(|| step.tools.first().map(|s| s.as_str()).unwrap_or("generic"));

            // Wrap tool output into message
            let msg = ToolInvocationWrapper::wrap_result(name, &result);
            conversation.add_message(msg);

            // Record tool execution in step
            step.results.push(format!("Tool executed: {}", name));
            if let Some(ref error) = result.error {
                step.errors.push(error.clone());
            }
        }
    }
}

impl StepExecutor<ExecutionContext> for GenericStepExecutor {
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
                            "Warning: Failed to parse parameters for tool '{}': {}. Using empty parameters.",
                            tool_name, e
                        )));
                        serde_json::json!({})
                    }
                }
            };

            // Create tool context with optional message sender for inter-agent communication
            let mut tool_ctx =
                ToolContext::new(&self.cwd).with_registry(self.tool_registry.clone());
            if let Some(ref sender) = _ctx.message_sender {
                tool_ctx = tool_ctx.with_message_sender(sender.clone());
            }

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
                    "Tool '{}' failed: {}",
                    tool_name, error
                )));
                step.errors.push(error.clone());
            } else {
                conversation.add_message(Message::assistant(format!(
                    "Tool '{}' output:\n{}",
                    tool_name, result.output
                )));
            }

            // If this was a critical tool and it failed, stop execution
            if !result.success && is_critical_tool(tool_name) {
                conversation.add_message(Message::assistant(format!(
                    "Critical tool '{}' failed - stopping step execution",
                    tool_name
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
        if step.execution_status != StepStatus::Failed {
            step.execution_status = StepStatus::Completed;
            step.results.push(format!(
                "Step '{}' executed successfully (tools: {:?})",
                step.title, step.tools
            ));
        } else {
            step.results.push(format!(
                "Step '{}' failed due to critical tool error",
                step.title
            ));
        }
        step.completed_at = Some(Utc::now());

        // Add completion message to conversation
        conversation.add_message(Message::assistant(format!(
            "Step completed: {}\n\nExpected outcome: {}\n\nResults: {}",
            step.title,
            step.expected_outcome,
            step.results.join("\n")
        )));

        Ok(step)
    }
}

/// Wraps tool invocation with output capture and message conversion.
pub struct ToolInvocationWrapper;

impl ToolInvocationWrapper {
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
    /// Note: This requires the tool name to be passed separately since ToolResult
    /// only contains call_id, not the tool name.
    pub fn wrap_result(tool_name: &str, result: &ToolResult) -> Message {
        Self::result_to_message(tool_name, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::SessionId;

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
    }

    #[test]
    fn test_is_critical_tool() {
        assert!(is_critical_tool("read_file"));
        assert!(is_critical_tool("write_file"));
        assert!(is_critical_tool("bash"));
        assert!(is_critical_tool("bash:some command"));

        assert!(!is_critical_tool("grep"));
        assert!(!is_critical_tool("glob"));
        assert!(!is_critical_tool("git_status"));
    }

    // -- Checkpointing integration tests --

    fn make_step(title: &str) -> PlanStep {
        use rustycode_protocol::StepStatus;
        PlanStep {
            order: 0,
            title: title.to_string(),
            description: format!("desc for {title}"),
            tools: vec![],
            expected_outcome: "done".to_string(),
            rollback_hint: String::new(),
            execution_status: StepStatus::Pending,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        }
    }

    fn make_context(store: Arc<ExecutionCheckpointStore>) -> ExecutionContext {
        ExecutionContext::new(ExecutionConfig::default(), PathBuf::from("."))
            .with_session_id("test-session")
            .with_checkpoint_store(store)
    }

    fn test_conversation() -> Conversation {
        Conversation::new(SessionId::new())
    }

    struct SucceedingExecutor;
    impl StepExecutor<ExecutionContext> for SucceedingExecutor {
        fn execute(
            &self,
            step: PlanStep,
            _conversation: &mut Conversation,
            _ctx: &ExecutionContext,
        ) -> Result<PlanStep> {
            Ok(step)
        }
    }

    struct FailingExecutor;
    impl StepExecutor<ExecutionContext> for FailingExecutor {
        fn execute(
            &self,
            step: PlanStep,
            _conversation: &mut Conversation,
            _ctx: &ExecutionContext,
        ) -> Result<PlanStep> {
            bail!("intentional failure on step '{}'", step.title)
        }
    }

    #[test]
    fn execute_saves_pre_checkpoint() {
        let store = Arc::new(ExecutionCheckpointStore::new());
        let ctx = make_context(Arc::clone(&store));
        let executor = CheckpointingStepExecutor::new(Arc::new(SucceedingExecutor));
        let mut conv = test_conversation();

        executor
            .execute(make_step("pre-check"), &mut conv, &ctx)
            .unwrap();

        assert!(
            !store.all().is_empty(),
            "at least pre-checkpoint must be saved"
        );
        let first = &store.all()[0];
        assert_eq!(first.session_id, "test-session");
    }

    #[test]
    fn execute_saves_post_checkpoint_on_success() {
        let store = Arc::new(ExecutionCheckpointStore::new());
        let ctx = make_context(Arc::clone(&store));
        let executor = CheckpointingStepExecutor::new(Arc::new(SucceedingExecutor));
        let mut conv = test_conversation();

        executor
            .execute(make_step("post-check"), &mut conv, &ctx)
            .unwrap();

        assert_eq!(store.len(), 2, "pre + post checkpoints on success");
    }

    #[test]
    fn preserve_pre_checkpoint_on_failure() {
        let store = Arc::new(ExecutionCheckpointStore::new());
        let ctx = make_context(Arc::clone(&store));
        let executor = CheckpointingStepExecutor::new(Arc::new(FailingExecutor));
        let mut conv = test_conversation();

        let result = executor.execute(make_step("fail"), &mut conv, &ctx);
        assert!(result.is_err());

        assert_eq!(store.len(), 1, "only pre-checkpoint kept on failure");
    }

    #[test]
    fn checkpoint_includes_phase() {
        let store = Arc::new(ExecutionCheckpointStore::new());
        let ctx = make_context(Arc::clone(&store));
        let executor = CheckpointingStepExecutor::new(Arc::new(SucceedingExecutor));
        let mut conv = test_conversation();

        executor
            .execute(make_step("phase-check"), &mut conv, &ctx)
            .unwrap();

        let all = store.all();
        assert!(all.iter().all(|cp| cp.phase == ExecutionPhase::Act));
    }

    #[test]
    fn checkpoint_includes_context() {
        let store = Arc::new(ExecutionCheckpointStore::new());
        let ctx = make_context(Arc::clone(&store));
        let executor = CheckpointingStepExecutor::new(Arc::new(SucceedingExecutor));
        let mut conv = test_conversation();

        executor
            .execute(make_step("ctx-check"), &mut conv, &ctx)
            .unwrap();

        let all = store.all();
        assert!(all.iter().all(|cp| cp.session_id == "test-session"));
        assert!(all.iter().all(|cp| !cp.id.is_empty()));
        assert!(all.iter().all(|cp| !cp.context_hash.is_empty()));
    }
}
