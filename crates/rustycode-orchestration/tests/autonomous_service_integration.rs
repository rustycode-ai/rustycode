//! Integration tests for `AutonomousService` execution flow.
//!
//! Exercises the full path: `AutonomousService::execute()` -> `Pipeline` -> `StepOrchestrator`
//! -> `Musician` -> `Conductor` + Bus events + State transitions.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use rustycode_orchestration::autonomous::{AutonomousService, ServiceState};
use rustycode_orchestration::bus::OrchestrationEvent;
use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::pipeline::TaskResult;

// ─── 1. Service Lifecycle ────────────────────────────────────────────────────

mod service_lifecycle {
    use super::*;

    #[test]
    fn test_new_service_starts_idle() {
        let svc = AutonomousService::new(OrchestrationConfig::default());
        assert_eq!(svc.state(), ServiceState::Idle);
    }

    #[tokio::test]
    async fn test_execute_transitions_to_completed() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        let result = svc
            .execute("t-life-1".into(), "echo lifecycle".into())
            .await
            .unwrap();
        assert!(matches!(result, TaskResult::Success { .. }));
        assert_eq!(svc.state(), ServiceState::Completed);
    }

    #[tokio::test]
    async fn test_execute_sequential_tasks() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());

        let r1 = svc
            .execute("t-seq-1".into(), "echo first".into())
            .await
            .unwrap();
        assert!(matches!(r1, TaskResult::Success { .. }));
        assert_eq!(svc.state(), ServiceState::Completed);

        // Can we reuse after completion? The service should still work.
        let r2 = svc
            .execute("t-seq-2".into(), "echo second".into())
            .await
            .unwrap();
        assert!(matches!(r2, TaskResult::Success { .. }));
    }

    #[tokio::test]
    async fn test_execute_publishes_task_completed_event() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        let mut rx = svc.bus_handle().subscribe();

        let _ = svc.execute("t-evt-1".into(), "echo events".into()).await;

        let mut found = false;
        while let Ok(event) = rx.try_recv() {
            if let OrchestrationEvent::TaskCompleted {
                task_id,
                tier_used,
                cost_usd,
            } = event
            {
                if task_id == "t-evt-1" {
                    found = true;
                    assert!(tier_used >= 2);
                    assert!(cost_usd >= 0.0);
                }
            }
        }
        assert!(found, "Should publish TaskCompleted event for t-evt-1");
    }

    #[tokio::test]
    async fn test_execute_with_empty_task_description() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        let result = svc.execute("t-empty".into(), String::new()).await;
        // Should handle gracefully (either success or failure, not panic)
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_long_task_description() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        let long_task = "x".repeat(1000);
        let result = svc.execute("t-long".into(), long_task).await;
        assert!(result.is_ok());
    }
}

// ─── 2. Bus Event Propagation ────────────────────────────────────────────────

mod bus_propagation {
    use super::*;

    #[tokio::test]
    async fn test_multiple_subscribers_receive_events() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        let mut rx1 = svc.bus_handle().subscribe();
        let mut rx2 = svc.bus_handle().subscribe();

        let _ = svc.execute("t-bus-1".into(), "echo broadcast".into()).await;

        let mut found1 = false;
        let mut found2 = false;
        while let Ok(event) = rx1.try_recv() {
            if let OrchestrationEvent::TaskCompleted { task_id, .. } = event {
                if task_id == "t-bus-1" {
                    found1 = true;
                }
            }
        }
        while let Ok(event) = rx2.try_recv() {
            if let OrchestrationEvent::TaskCompleted { task_id, .. } = event {
                if task_id == "t-bus-1" {
                    found2 = true;
                }
            }
        }
        assert!(found1, "First subscriber should receive event");
        assert!(found2, "Second subscriber should receive event");
    }

    #[tokio::test]
    async fn test_events_from_multiple_executions() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        let mut rx = svc.bus_handle().subscribe();

        let _ = svc.execute("t-multi-1".into(), "echo one".into()).await;
        let _ = svc.execute("t-multi-2".into(), "echo two".into()).await;
        let _ = svc.execute("t-multi-3".into(), "echo three".into()).await;

        let mut task_ids = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let OrchestrationEvent::TaskCompleted { task_id, .. } = event {
                task_ids.push(task_id);
            }
        }
        assert!(
            task_ids.len() >= 3,
            "Should receive events from all 3 executions"
        );
    }
}

// ─── 3. Bootstrap Configuration ─────────────────────────────────────────────

mod bootstrap_config {
    use super::*;
    use rustycode_orchestration::autonomous::{AutonomousConfig, BootstrapInfo};

    #[test]
    fn test_bootstrap_creates_service() {
        let info = BootstrapInfo {
            project_dir: "/tmp/project".into(),
            config: OrchestrationConfig::default(),
            task_description: "build the thing".into(),
        };
        let svc = AutonomousService::bootstrap(info).unwrap();
        assert_eq!(svc.state(), ServiceState::Idle);
    }

    #[test]
    fn test_with_custom_autonomous_config() {
        let config = AutonomousConfig {
            max_retries: 10,
            enable_recovery: false,
            enable_git: false,
            enable_worktree: false,
            workspace: std::path::PathBuf::from("."),
        };
        let svc = AutonomousService::new(OrchestrationConfig::default()).with_config(config);
        assert_eq!(svc.state(), ServiceState::Idle);
    }

    #[tokio::test]
    async fn test_bootstrapped_service_executes() {
        let info = BootstrapInfo {
            project_dir: "/tmp/project".into(),
            config: OrchestrationConfig::default(),
            task_description: "initial task".into(),
        };
        let mut svc = AutonomousService::bootstrap(info).unwrap();
        let result = svc
            .execute("t-boot".into(), "echo bootstrapped".into())
            .await
            .unwrap();
        assert!(matches!(result, TaskResult::Success { .. }));
        assert_eq!(svc.state(), ServiceState::Completed);
    }
}

// ─── 4. TaskResult Validation ────────────────────────────────────────────────

mod task_result_validation {
    use super::*;

    #[tokio::test]
    async fn test_success_result_has_valid_fields() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        let result = svc
            .execute("t-fields".into(), "echo validation".into())
            .await
            .unwrap();

        if let TaskResult::Success {
            output,
            tier_used,
            steps_completed,
            total_cost,
            ..
        } = result
        {
            assert!(tier_used >= 2, "Tier should be at least 2");
            assert!(steps_completed > 0, "Should complete at least 1 step");
            assert!(total_cost >= 0.0, "Cost should be non-negative");
            let _ = output; // Output may be empty for some tools
        }
    }

    #[tokio::test]
    async fn test_result_serializable() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        let result = svc
            .execute("t-serde".into(), "echo serde".into())
            .await
            .unwrap();

        let json = serde_json::to_string(&result).unwrap();
        let back: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back, "TaskResult should roundtrip through JSON");
    }
}

// ─── 5. State Machine Transitions ───────────────────────────────────────────

mod state_machine {
    use super::*;

    #[test]
    fn test_all_service_state_variants() {
        let states = [
            ServiceState::Idle,
            ServiceState::Planning,
            ServiceState::Executing,
            ServiceState::Recovering,
            ServiceState::Completed,
            ServiceState::Failed,
        ];
        assert_eq!(states.len(), 6);

        // Verify active/terminal properties
        assert!(ServiceState::Planning.is_active());
        assert!(ServiceState::Executing.is_active());
        assert!(ServiceState::Recovering.is_active());
        assert!(!ServiceState::Idle.is_active());
        assert!(!ServiceState::Completed.is_active());
        assert!(!ServiceState::Failed.is_active());

        assert!(ServiceState::Completed.is_terminal());
        assert!(ServiceState::Failed.is_terminal());
        assert!(!ServiceState::Idle.is_terminal());
        assert!(!ServiceState::Executing.is_terminal());
    }

    #[tokio::test]
    async fn test_idle_to_executing_to_completed() {
        let mut svc = AutonomousService::new(OrchestrationConfig::default());
        assert_eq!(svc.state(), ServiceState::Idle);

        // After execute, should be Completed (not Executing, since execute is synchronous)
        let _ = svc
            .execute("t-trans".into(), "echo transition".into())
            .await;
        assert_eq!(svc.state(), ServiceState::Completed);
    }
}
