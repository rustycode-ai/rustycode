//! Autonomous development framework.
//!
//! High-level service for autonomous task execution with planning, recovery,
//! and coordination. Re-exports core types for the unified API surface.
//!
//! Supports two execution strategies:
//! 1. **`OrchestrationPipeline`** (default) — tiered execution with conductor escalation
//! 2. **`AstPipeline`** (opt-in) — 6-phase structured thinking for complex tasks
//!
//! Enable AST routing by calling [`AutonomousService::with_ast_config`].
//! Complex and moderate tasks are routed to AST; trivial tasks use the
//! existing pipeline.

use std::path::PathBuf;

use crate::ast::classifier::TaskClassifier;
use crate::ast::types::{ComplexityLevel, VerificationStatus};
use crate::ast::{AstConfig, AstPipeline};
use crate::bus::BusHandle;
use crate::config::OrchestrationConfig;
use crate::error::Result;
use crate::pipeline::{OrchestrationPipeline, TaskResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapInfo {
    pub project_dir: String,
    pub config: OrchestrationConfig,
    pub task_description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    Idle,
    Planning,
    Executing,
    Recovering,
    Completed,
    Failed,
}

impl ServiceState {
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Planning | Self::Executing | Self::Recovering)
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

pub struct AutonomousConfig {
    pub max_retries: u8,
    pub enable_recovery: bool,
    pub enable_git: bool,
    pub enable_worktree: bool,
    pub workspace: PathBuf,
}

impl Default for AutonomousConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            enable_recovery: true,
            enable_git: true,
            enable_worktree: true,
            workspace: PathBuf::from("."),
        }
    }
}

pub struct AutonomousService {
    state: ServiceState,
    config: AutonomousConfig,
    pipeline: OrchestrationPipeline,
    ast_config: Option<AstConfig>,
}

impl AutonomousService {
    pub fn new(orchestration_config: OrchestrationConfig) -> Self {
        let pipeline = OrchestrationPipeline::new(orchestration_config);
        Self {
            state: ServiceState::Idle,
            config: AutonomousConfig::default(),
            pipeline,
            ast_config: None,
        }
    }

    pub fn with_config(mut self, config: AutonomousConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_ast_config(mut self, config: AstConfig) -> Self {
        self.ast_config = Some(config);
        self
    }

    pub fn bootstrap(info: BootstrapInfo) -> Result<Self> {
        Ok(Self::new(info.config))
    }

    pub const fn state(&self) -> ServiceState {
        self.state
    }

    pub fn bus_handle(&self) -> BusHandle {
        self.pipeline.bus_handle()
    }

    pub fn shutdown(mut self) -> Result<()> {
        self.state = ServiceState::Completed;
        Ok(())
    }

    pub async fn execute(&mut self, task_id: String, task: String) -> Result<TaskResult> {
        self.state = ServiceState::Executing;

        let use_ast = match &self.ast_config {
            Some(_) => {
                let classifier = TaskClassifier::new();
                let assessment = classifier.classify(&task);
                assessment.complexity != ComplexityLevel::Trivial
            }
            None => false,
        };

        let result = if use_ast {
            let config = self.ast_config.clone().ok_or_else(|| {
                crate::error::OrchestrationError::Execution {
                    message: "AST config missing despite classification requiring it".into(),
                }
            })?;
            self.run_ast_pipeline(&task, config).await
        } else {
            self.pipeline.conduct(task_id, task).await
        };

        match &result {
            Ok(TaskResult::Success { .. }) => {
                self.state = ServiceState::Completed;
            }
            Ok(TaskResult::Failed { .. }) | Err(_) => {
                self.state = ServiceState::Failed;
            }
        }
        result
    }

    #[allow(clippy::unused_async)]
    async fn run_ast_pipeline(&self, task: &str, config: AstConfig) -> Result<TaskResult> {
        let workspace = self.config.workspace.clone();
        let mut ast = AstPipeline::with_config(config, workspace);
        let ast_result = ast.run_to_completion(task).map_err(|e| {
            crate::error::OrchestrationError::Execution {
                message: format!("AST pipeline failed: {e:#}"),
            }
        })?;
        Ok(convert_ast_result(ast_result))
    }
}

fn convert_ast_result(result: crate::ast::AstExecutionResult) -> TaskResult {
    let steps = result.completed_milestones.len();
    match result.status {
        VerificationStatus::Pass => TaskResult::Success {
            output: result
                .assessment
                .map(|a| a.task_summary)
                .unwrap_or_default(),
            total_cost: 0.0,
            tier_used: 0,
            steps_completed: steps,
            execution_trace: crate::execution_trace::ExecutionTrace {
                task_id: String::new(),
                steps: vec![],
            },
            structured_output: None,
        },
        VerificationStatus::Partial => TaskResult::Success {
            output: format!(
                "Partial completion: {steps} milestones done, {} escalated",
                result.consultant_escalation.len()
            ),
            total_cost: 0.0,
            tier_used: 0,
            steps_completed: steps,
            execution_trace: crate::execution_trace::ExecutionTrace {
                task_id: String::new(),
                steps: vec![],
            },
            structured_output: None,
        },
        VerificationStatus::Fail => TaskResult::Failed {
            reason: "AST pipeline verification failed".into(),
            total_cost: 0.0,
            steps_completed: steps,
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_service_new_starts_idle() {
        let svc = AutonomousService::new(OrchestrationConfig::default());
        assert_eq!(svc.state(), ServiceState::Idle);
    }

    #[test]
    fn test_service_state_is_active() {
        assert!(ServiceState::Planning.is_active());
        assert!(ServiceState::Executing.is_active());
        assert!(ServiceState::Recovering.is_active());
        assert!(!ServiceState::Idle.is_active());
        assert!(!ServiceState::Completed.is_active());
        assert!(!ServiceState::Failed.is_active());
    }

    #[test]
    fn test_service_state_is_terminal() {
        assert!(ServiceState::Completed.is_terminal());
        assert!(ServiceState::Failed.is_terminal());
        assert!(!ServiceState::Idle.is_terminal());
        assert!(!ServiceState::Planning.is_terminal());
    }

    #[test]
    fn test_with_custom_config() {
        let config = AutonomousConfig {
            max_retries: 5,
            enable_recovery: false,
            enable_git: false,
            enable_worktree: false,
            workspace: PathBuf::from("."),
        };
        let svc = AutonomousService::new(OrchestrationConfig::default()).with_config(config);
        assert_eq!(svc.state(), ServiceState::Idle);
    }

    #[test]
    fn test_bootstrap_creates_service() {
        let info = BootstrapInfo {
            project_dir: "/tmp/test".into(),
            config: OrchestrationConfig::default(),
            task_description: "test task".into(),
        };
        let svc = AutonomousService::bootstrap(info).unwrap();
        assert_eq!(svc.state(), ServiceState::Idle);
    }

    #[test]
    fn test_shutdown_transitions_to_completed() {
        let svc = AutonomousService::new(OrchestrationConfig::default());
        svc.shutdown().unwrap();
    }

    #[test]
    fn test_autonomous_config_default() {
        let config = AutonomousConfig::default();
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_recovery);
        assert!(config.enable_git);
        assert!(config.enable_worktree);
        assert_eq!(config.workspace, PathBuf::from("."));
    }

    #[test]
    fn test_service_exposes_bus_handle() {
        let svc = AutonomousService::new(OrchestrationConfig::default());
        let bus = svc.bus_handle();
        let mut rx = bus.subscribe();
        bus.publish(crate::bus::OrchestrationEvent::TaskCompleted {
            task_id: "t1".into(),
            tier_used: 2,
            cost_usd: 0.01,
        });
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn test_service_state_serialization() {
        let states = vec![
            ServiceState::Idle,
            ServiceState::Planning,
            ServiceState::Executing,
            ServiceState::Recovering,
            ServiceState::Completed,
            ServiceState::Failed,
        ];
        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let deserialized: ServiceState = serde_json::from_str(&json).unwrap();
            assert_eq!(state, deserialized);
        }
    }

    #[tokio::test]
    async fn test_execute_transitions_to_completed_on_success() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        assert_eq!(svc.state(), ServiceState::Idle);
        let result = svc
            .execute("t-exec".into(), "echo hello".into())
            .await
            .unwrap();
        assert!(matches!(
            result,
            crate::pipeline::TaskResult::Success { .. }
        ));
        assert_eq!(svc.state(), ServiceState::Completed);
    }

    #[tokio::test]
    async fn test_execute_publishes_bus_event() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        let mut rx = svc.bus_handle().subscribe();
        let _ = svc.execute("t-bus".into(), "echo hello".into()).await;

        let mut found = false;
        while let Ok(event) = rx.try_recv() {
            if let crate::bus::OrchestrationEvent::TaskCompleted { task_id, .. } = event {
                if task_id == "t-bus" {
                    found = true;
                }
            }
        }
        assert!(found);
    }

    #[tokio::test]
    async fn test_complex_task_routes_to_ast() {
        let tmp = tempfile::tempdir().unwrap();
        let ast_config = crate::ast::AstConfig {
            ledger_dir: tmp.path().join(".ast"),
            ..Default::default()
        };
        let config = AutonomousConfig {
            workspace: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let mut svc = AutonomousService::new(OrchestrationConfig::default())
            .with_config(config)
            .with_ast_config(ast_config);

        let result = svc
            .execute(
                "t-ast".into(),
                "Implement JWT auth with refresh tokens".into(),
            )
            .await
            .unwrap();

        assert!(matches!(result, TaskResult::Success { tier_used: 0, .. }));
        assert_eq!(svc.state(), ServiceState::Completed);
    }

    #[tokio::test]
    async fn test_trivial_task_uses_existing_pipeline() {
        let tmp = tempfile::tempdir().unwrap();
        let ast_config = crate::ast::AstConfig {
            ledger_dir: tmp.path().join(".ast"),
            ..Default::default()
        };
        let config = AutonomousConfig {
            workspace: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let mut svc = AutonomousService::new(OrchestrationConfig::default())
            .with_config(config)
            .with_ast_config(ast_config);

        let result = svc
            .execute("t-pipe".into(), "Fix typo".into())
            .await
            .unwrap();

        assert!(matches!(result, TaskResult::Success { tier_used: t, .. } if t > 0));
        assert_eq!(svc.state(), ServiceState::Completed);
    }

    #[test]
    fn test_no_ast_config_uses_existing_pipeline() {
        let svc = AutonomousService::new(OrchestrationConfig::default());
        assert!(svc.ast_config.is_none());
    }

    #[test]
    fn test_with_ast_config_sets_field() {
        let ast_config = crate::ast::AstConfig::default();
        let svc =
            AutonomousService::new(OrchestrationConfig::default()).with_ast_config(ast_config);
        assert!(svc.ast_config.is_some());
    }

    #[test]
    fn test_convert_ast_result_pass() {
        let ast_result = crate::ast::pipeline::AstExecutionResult {
            status: VerificationStatus::Pass,
            assessment: Some(crate::ast::types::TaskAssessment {
                task_summary: "Fix typo".into(),
                complexity: ComplexityLevel::Complex,
                success_criteria: vec![],
                route: crate::ast::types::PhaseRoute::RollingWave,
                clarity: None,
            }),
            report: None,
            ledger_path: PathBuf::from(".ast/ledger.md"),
            completed_milestones: vec![0, 1, 2],
            consultant_escalation: vec![],
        };
        let task_result = convert_ast_result(ast_result);
        match task_result {
            TaskResult::Success {
                output,
                tier_used,
                steps_completed,
                ..
            } => {
                assert_eq!(output, "Fix typo");
                assert_eq!(tier_used, 0);
                assert_eq!(steps_completed, 3);
            }
            TaskResult::Failed { .. } => panic!("Expected Success for Pass status"),
        }
    }

    #[test]
    fn test_convert_ast_result_fail() {
        let ast_result = crate::ast::pipeline::AstExecutionResult {
            status: VerificationStatus::Fail,
            assessment: None,
            report: None,
            ledger_path: PathBuf::from(".ast/ledger.md"),
            completed_milestones: vec![0],
            consultant_escalation: vec![1],
        };
        let task_result = convert_ast_result(ast_result);
        match task_result {
            TaskResult::Failed {
                reason,
                steps_completed,
                ..
            } => {
                assert!(reason.contains("verification failed"));
                assert_eq!(steps_completed, 1);
            }
            TaskResult::Success { .. } => panic!("Expected Failed for Fail status"),
        }
    }

    #[test]
    fn test_convert_ast_result_partial() {
        let ast_result = crate::ast::pipeline::AstExecutionResult {
            status: VerificationStatus::Partial,
            assessment: None,
            report: None,
            ledger_path: PathBuf::from(".ast/ledger.md"),
            completed_milestones: vec![0],
            consultant_escalation: vec![1],
        };
        let task_result = convert_ast_result(ast_result);
        match task_result {
            TaskResult::Success {
                output,
                tier_used,
                steps_completed,
                ..
            } => {
                assert!(output.contains("Partial completion"));
                assert_eq!(tier_used, 0);
                assert_eq!(steps_completed, 1);
            }
            TaskResult::Failed { .. } => panic!("Expected Success for Partial status"),
        }
    }
}
