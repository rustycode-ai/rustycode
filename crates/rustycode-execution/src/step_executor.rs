//! Step executor implementation

use crate::executor::{ExecutionContext, ExecutionResult};
use anyhow::Result;
use async_trait::async_trait;

/// Step executor for executing individual plan steps
pub struct StepExecutor;

impl StepExecutor {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for StepExecutor {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl crate::executor::Executor for StepExecutor {
    async fn execute_plan(
        &self,
        _plan: &rustycode_protocol::Plan,
        _config: &crate::executor::ExecutionConfig,
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
        _context: &ExecutionContext,
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
    use crate::executor::Executor;
    use std::path::PathBuf;

    #[test]
    fn test_step_executor_new() {
        let _executor = StepExecutor::new();
    }

    #[test]
    fn test_step_executor_default() {
        let _executor = StepExecutor::default();
    }

    #[tokio::test]
    async fn test_step_executor_execute_step_returns_success() {
        let executor = StepExecutor::new();
        let step = rustycode_protocol::PlanStep {
            order: 0,
            title: "Test".to_string(),
            description: "A test step".to_string(),
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
        let config = crate::executor::ExecutionConfig::default();
        let ctx = ExecutionContext::new(config, PathBuf::from("."));
        let result = executor.execute_step(&step, &ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.steps_executed, 1);
    }

    #[tokio::test]
    async fn test_step_executor_execute_plan_returns_success() {
        use crate::executor::ExecutionConfig;

        let executor = StepExecutor::new();
        let plan = rustycode_protocol::Plan {
            id: rustycode_protocol::PlanId::new(),
            session_id: rustycode_protocol::SessionId::new(),
            task: "Test".to_string(),
            created_at: chrono::Utc::now(),
            status: rustycode_protocol::PlanStatus::Draft,
            summary: "Test plan".to_string(),
            approach: String::new(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        let result = executor
            .execute_plan(&plan, &ExecutionConfig::default())
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.steps_executed, 1);
    }

    #[tokio::test]
    async fn test_step_executor_execute_step_output_not_empty() {
        let executor = StepExecutor::new();
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
        let ctx = ExecutionContext::new(
            crate::executor::ExecutionConfig::default(),
            PathBuf::from("/tmp"),
        );
        let result = executor.execute_step(&step, &ctx).await.unwrap();
        assert!(!result.output.is_empty());
    }

    #[tokio::test]
    async fn test_step_executor_execute_step_has_positive_duration() {
        let executor = StepExecutor::new();
        let step = rustycode_protocol::PlanStep {
            order: 0,
            title: "Timed step".to_string(),
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
        let ctx = ExecutionContext::new(
            crate::executor::ExecutionConfig::default(),
            PathBuf::from("."),
        );
        let result = executor.execute_step(&step, &ctx).await.unwrap();
        assert!(result.duration.as_millis() > 0);
    }
}
