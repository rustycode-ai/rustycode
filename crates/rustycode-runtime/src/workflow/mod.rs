//! Workflow and Meta Tools System
//!
//! This module provides workflow orchestration and meta tool capabilities:
//! - Reusable workflow definitions
//! - Multi-step tool composition
//! - Conditional execution
//! - Loop support
//! - Error handling and rollback

pub mod definition;
pub mod engine;
pub mod meta_tool;
pub mod parser;
pub mod registry;

pub use definition::{Workflow, WorkflowState, WorkflowStatus, WorkflowStep};
pub use engine::WorkflowExecutor;
pub use meta_tool::MetaTool;
pub use registry::WorkflowRegistry;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Re-export commonly used types
pub use rustycode_protocol::{SessionId, ToolCall, ToolResult};

/// Workflow execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    /// Whether workflow completed successfully
    pub success: bool,
    /// Final workflow state
    pub final_state: WorkflowState,
    /// Results from each step (by step ID)
    pub step_results: HashMap<String, StepResult>,
    /// Output data
    pub output: serde_json::Value,
    /// Execution duration
    pub duration_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
}

/// Result of a single workflow step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    /// Whether step succeeded
    pub success: bool,
    /// Step output data
    pub output: Option<serde_json::Value>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub duration_ms: u64,
}

/// Workflow execution error
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum WorkflowError {
    #[error("Workflow validation failed: {0}")]
    Validation(String),

    #[error("Step execution failed: {step} - {error}")]
    StepExecution { step: String, error: String },

    #[error("Workflow not found: {0}")]
    NotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Loop detected: {0}")]
    LoopDetected(String),

    #[error("Workflow execution timeout")]
    Timeout,

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Tool execution failed: {tool} - {error}")]
    ToolExecutionFailed { tool: String, error: String },
}

/// Result type for workflow operations
pub type Result<T> = std::result::Result<T, WorkflowError>;

/// Trait for executing tools within a workflow.
///
/// Implementations can wrap a `ToolRegistry`, mock results for testing,
/// or dispatch to any external tool execution backend.
pub trait WorkflowToolExecutor: Send + Sync {
    /// Execute a tool by name with the given parameters.
    ///
    /// Returns the tool output as a `serde_json::Value`.
    /// On failure, return an `Err` with a descriptive message.
    fn execute_tool(
        &self,
        tool_name: &str,
        parameters: &HashMap<String, String>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<serde_json::Value, String>>
                + Send
                + '_,
        >,
    >;
}

/// Default mock executor that records but does not execute tools.
pub struct MockToolExecutor {
    /// Whether to record calls for inspection in tests
    pub record: bool,
    calls: std::sync::Mutex<Vec<(String, HashMap<String, String>)>>,
}

impl MockToolExecutor {
    pub fn new() -> Self {
        Self {
            record: false,
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Retrieve recorded calls (only useful when `record` is true)
    pub fn recorded_calls(&self) -> Vec<(String, HashMap<String, String>)> {
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Default for MockToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowToolExecutor for MockToolExecutor {
    fn execute_tool(
        &self,
        tool_name: &str,
        parameters: &HashMap<String, String>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<serde_json::Value, String>>
                + Send
                + '_,
        >,
    > {
        if self.record {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((tool_name.to_string(), parameters.clone()));
        }
        let tool = tool_name.to_string();
        let params = parameters.clone();
        Box::pin(async move {
            Ok(serde_json::json!({
                "tool": tool,
                "result": format!("Executed {}", tool),
                "parameters": params,
            }))
        })
    }
}

/// Real tool executor that dispatches through a boxed function.
///
/// This bridges the workflow engine to any external tool execution backend
/// (e.g., `ToolExecutor` from rustycode-tools) without coupling to concrete types.
///
/// # Example
///
/// ```rust
/// use std::collections::HashMap;
/// use std::sync::Arc;
/// use rustycode_runtime::workflow::{RegistryToolExecutor, WorkflowToolExecutor};
///
/// let executor = RegistryToolExecutor::new(|tool_name: &str, params: &HashMap<String, String>| {
///     // In production, dispatch to ToolExecutor::execute() here
///     Ok(serde_json::json!({ "tool": tool_name, "status": "executed" }))
/// });
type ToolHandler = dyn Fn(&str, &HashMap<String, String>) -> std::result::Result<serde_json::Value, String>
    + Send
    + Sync;

/// A `ToolHandler` dispatcher that routes tool calls through the registry.
pub struct RegistryToolExecutor {
    handler: Box<ToolHandler>,
}

impl RegistryToolExecutor {
    pub fn new(
        handler: impl Fn(&str, &HashMap<String, String>) -> std::result::Result<serde_json::Value, String>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            handler: Box::new(handler),
        }
    }
}

impl WorkflowToolExecutor for RegistryToolExecutor {
    fn execute_tool(
        &self,
        tool_name: &str,
        parameters: &HashMap<String, String>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<serde_json::Value, String>>
                + Send
                + '_,
        >,
    > {
        let result = (self.handler)(tool_name, parameters);
        Box::pin(async move { result })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- WorkflowResult serde ---

    #[test]
    fn workflow_result_serde_roundtrip() {
        let result = WorkflowResult {
            success: true,
            final_state: WorkflowState::default(),
            step_results: HashMap::new(),
            output: serde_json::json!({"key": "value"}),
            duration_ms: 500,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: WorkflowResult = serde_json::from_str(&json).unwrap();
        assert!(decoded.success);
        assert_eq!(decoded.duration_ms, 500);
    }

    #[test]
    fn workflow_result_with_error_serde() {
        let result = WorkflowResult {
            success: false,
            final_state: WorkflowState::default(),
            step_results: HashMap::new(),
            output: serde_json::Value::Null,
            duration_ms: 1000,
            error: Some("Step failed".into()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let decoded: WorkflowResult = serde_json::from_str(&json).unwrap();
        assert!(!decoded.success);
        assert_eq!(decoded.error, Some("Step failed".into()));
    }

    // --- StepResult serde ---

    #[test]
    fn step_result_serde_roundtrip() {
        let sr = StepResult {
            step_id: "s1".into(),
            success: true,
            output: Some(serde_json::json!("done")),
            error: None,
            duration_ms: 100,
        };
        let json = serde_json::to_string(&sr).unwrap();
        let decoded: StepResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.step_id, "s1");
        assert!(decoded.success);
    }

    // --- WorkflowError display ---

    #[test]
    fn workflow_error_display_messages() {
        assert!(format!("{}", WorkflowError::Validation("bad".into())).contains("bad"));
        assert!(format!("{}", WorkflowError::NotFound("wf1".into())).contains("wf1"));
        assert!(format!("{}", WorkflowError::Timeout).contains("timeout"));
        assert!(format!("{}", WorkflowError::ToolNotFound("bash".into())).contains("bash"));
    }

    #[test]
    fn workflow_error_step_execution() {
        let err = WorkflowError::StepExecution {
            step: "s1".into(),
            error: "crashed".into(),
        };
        assert!(format!("{}", err).contains("s1"));
        assert!(format!("{}", err).contains("crashed"));
    }

    // --- MockToolExecutor ---

    #[tokio::test]
    async fn mock_tool_executor_returns_ok() {
        let mock = MockToolExecutor::new();
        let mut params = HashMap::new();
        params.insert("key".into(), "value".into());
        let result = mock.execute_tool("test_tool", &params).await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["tool"], "test_tool");
    }

    #[tokio::test]
    async fn mock_tool_executor_recording() {
        let mut mock = MockToolExecutor::new();
        mock.record = true;
        let params = HashMap::new();
        let _ = mock.execute_tool("recorded_tool", &params).await;
        let calls = mock.recorded_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "recorded_tool");
    }

    #[tokio::test]
    async fn mock_tool_executor_no_record_by_default() {
        let mock = MockToolExecutor::new();
        let params = HashMap::new();
        let _ = mock.execute_tool("unrecorded", &params).await;
        assert!(mock.recorded_calls().is_empty());
    }

    #[test]
    fn mock_tool_executor_default() {
        let mock = MockToolExecutor::default();
        assert!(!mock.record);
    }

    // --- RegistryToolExecutor ---

    #[tokio::test]
    async fn registry_tool_executor_success() {
        let executor =
            RegistryToolExecutor::new(|name, _params| Ok(serde_json::json!({"executed": name})));
        let result = executor.execute_tool("my_tool", &HashMap::new()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["executed"], "my_tool");
    }

    #[tokio::test]
    async fn registry_tool_executor_error() {
        let executor = RegistryToolExecutor::new(|_name, _params| Err("tool not available".into()));
        let result = executor.execute_tool("missing", &HashMap::new()).await;
        assert!(result.is_err());
    }
}
