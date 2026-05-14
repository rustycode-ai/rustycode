//! Integration adapter connecting [`ExecutionRouter`] to orchestration's [`TaskToolExecutor`].

use crate::musician::TaskToolExecutor;
use crate::types::StepResult;
use async_trait::async_trait;
use rustycode_executable::{ExecutionContext, ExecutionInput, ExecutionRouter};
use std::sync::Arc;

/// Adapts [`ExecutionRouter`] to implement the orchestration [`TaskToolExecutor`] trait.
pub struct ExecutableToolExecutor {
    router: Arc<ExecutionRouter>,
}

impl ExecutableToolExecutor {
    pub const fn new(router: Arc<ExecutionRouter>) -> Self {
        Self { router }
    }
}

#[async_trait]
impl TaskToolExecutor for ExecutableToolExecutor {
    async fn execute(
        &self,
        _task_id: &str,
        tool_name: &str,
        input: &str,
        _allowed_tools: &[&'static str],
        _model: &str,
    ) -> crate::error::Result<StepResult> {
        let exec_input = ExecutionInput {
            data: serde_json::from_str(input).unwrap_or_else(|_| {
                serde_json::json!({
                    "raw": input
                })
            }),
            caller_info: None,
            session_context: None,
        };

        let context = ExecutionContext::DirectTool {
            immediate_result: true,
            timeout_ms: Some(30_000),
        };

        match self.router.execute(tool_name, exec_input, context).await {
            Ok(output) => Ok(StepResult {
                output: serde_json::to_string(&output.data).unwrap_or_default(),
                exit_code: Some(0),
            }),
            Err(e) => Ok(StepResult {
                output: format!("error: {e}"),
                exit_code: Some(1),
            }),
        }
    }
}
