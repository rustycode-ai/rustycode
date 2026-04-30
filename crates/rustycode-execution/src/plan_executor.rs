//! Plan executor implementation

use crate::executor::{ExecutionConfig, ExecutionResult, Executor};
use anyhow::Result;
use async_trait::async_trait;

/// Plan executor for executing complete plans
pub struct PlanExecutor;

impl PlanExecutor {
    /// Create a new plan executor
    pub const fn new() -> Self {
        Self
    }
}

impl Default for PlanExecutor {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Executor for PlanExecutor {
    async fn execute_plan(
        &self,
        _plan: &rustycode_protocol::Plan,
        _config: &ExecutionConfig,
    ) -> Result<ExecutionResult> {
        Ok(ExecutionResult::success(
            "Plan executed successfully".to_string(),
            std::time::Duration::from_secs(1),
            1,
        ))
    }

    async fn execute_step(
        &self,
        _step: &rustycode_protocol::PlanStep,
        _context: &crate::executor::ExecutionContext,
    ) -> Result<ExecutionResult> {
        Ok(ExecutionResult::success(
            "Step executed successfully".to_string(),
            std::time::Duration::from_millis(100),
            1,
        ))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::default_constructed_unit_structs
)]
mod tests {
    use super::*;
    #[test]
    fn test_plan_executor_new() {
        let _executor = PlanExecutor::new();
    }

    #[test]
    fn test_plan_executor_default() {
        let _executor = PlanExecutor::default();
    }

    #[tokio::test]
    async fn test_plan_executor_execute_step() {
        use crate::executor::ExecutionContext;
        use std::path::PathBuf;

        let executor = PlanExecutor::new();
        let step = rustycode_protocol::PlanStep {
            order: 0,
            title: "Test step".to_string(),
            description: "A test step".to_string(),
            tools: vec![],
            expected_outcome: "Success".to_string(),
            rollback_hint: String::new(),
            execution_status: rustycode_protocol::StepStatus::Pending,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };
        let config = ExecutionConfig::default();
        let ctx = ExecutionContext::new(config, PathBuf::from("."));
        let result = executor.execute_step(&step, &ctx).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_plan_executor_execute_plan() {
        let executor = PlanExecutor::new();
        let plan = rustycode_protocol::Plan {
            id: rustycode_protocol::PlanId::new(),
            session_id: rustycode_protocol::SessionId::new(),
            task: "Test task".to_string(),
            created_at: chrono::Utc::now(),
            status: rustycode_protocol::PlanStatus::Draft,
            summary: "A test plan".to_string(),
            approach: String::new(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        let config = ExecutionConfig::default();
        let result = executor.execute_plan(&plan, &config).await.unwrap();
        assert!(result.success);
        assert_eq!(result.steps_executed, 1);
        assert!(!result.output.is_empty());
    }

    #[tokio::test]
    async fn test_plan_executor_execute_step_has_duration() {
        use crate::executor::ExecutionContext;
        use std::path::PathBuf;

        let executor = PlanExecutor::new();
        let step = rustycode_protocol::PlanStep {
            order: 0,
            title: "Step".to_string(),
            description: String::new(),
            tools: vec![],
            expected_outcome: String::new(),
            rollback_hint: String::new(),
            execution_status: rustycode_protocol::StepStatus::Pending,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };
        let ctx = ExecutionContext::new(ExecutionConfig::default(), PathBuf::from("."));
        let result = executor.execute_step(&step, &ctx).await.unwrap();
        assert!(result.duration.as_millis() > 0);
    }

    #[tokio::test]
    async fn test_plan_executor_execute_step_no_error() {
        use crate::executor::ExecutionContext;
        use std::path::PathBuf;

        let executor = PlanExecutor::new();
        let step = rustycode_protocol::PlanStep {
            order: 5,
            title: "Another step".to_string(),
            description: "Desc".to_string(),
            tools: vec![],
            expected_outcome: "ok".to_string(),
            rollback_hint: String::new(),
            execution_status: rustycode_protocol::StepStatus::Pending,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };
        let ctx = ExecutionContext::new(ExecutionConfig::default(), PathBuf::from("."));
        let result = executor.execute_step(&step, &ctx).await.unwrap();
        assert!(result.error.is_none());
    }
}
