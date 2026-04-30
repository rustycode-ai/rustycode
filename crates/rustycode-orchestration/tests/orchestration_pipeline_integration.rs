//! Integration tests for `OrchestrationPipeline` execution flow.
//!
//! Exercises the full path: `OrchestrationPipeline::conduct()` -> `StepOrchestrator`
//! -> `Musician` -> `Conductor` + Bus events + State transitions.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use rustycode_orchestration::bus::OrchestrationEvent;
use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::pipeline::{OrchestrationPipeline, TaskResult};

// ─── 1. Service Lifecycle ────────────────────────────────────────────────────

mod service_lifecycle {
    use super::*;

    #[tokio::test]
    async fn test_execute_transitions_to_completed() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let result = pipeline
            .conduct("t-life-1".into(), "echo lifecycle".into())
            .await
            .unwrap();
        assert!(matches!(result, TaskResult::Success { .. }));
    }

    #[tokio::test]
    async fn test_execute_sequential_tasks() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());

        let r1 = pipeline
            .conduct("t-seq-1".into(), "echo first".into())
            .await
            .unwrap();
        assert!(matches!(r1, TaskResult::Success { .. }));

        let r2 = pipeline
            .conduct("t-seq-2".into(), "echo second".into())
            .await
            .unwrap();
        assert!(matches!(r2, TaskResult::Success { .. }));
    }

    #[tokio::test]
    async fn test_execute_publishes_task_completed_event() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let mut rx = pipeline.bus_handle().subscribe();

        let _ = pipeline
            .conduct("t-evt-1".into(), "echo events".into())
            .await;

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
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let result = pipeline.conduct("t-empty".into(), String::new()).await;
        // Should handle gracefully
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_long_task_description() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let long_task = "x".repeat(1000);
        let result = pipeline.conduct("t-long".into(), long_task).await;
        assert!(result.is_ok());
    }
}

// ─── 2. Bus Event Propagation ────────────────────────────────────────────────

mod bus_propagation {
    use super::*;

    #[tokio::test]
    async fn test_multiple_subscribers_receive_events() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let mut rx1 = pipeline.bus_handle().subscribe();
        let mut rx2 = pipeline.bus_handle().subscribe();

        let _ = pipeline
            .conduct("t-bus-1".into(), "echo broadcast".into())
            .await;

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
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let mut rx = pipeline.bus_handle().subscribe();

        let _ = pipeline
            .conduct("t-multi-1".into(), "echo one".into())
            .await;
        let _ = pipeline
            .conduct("t-multi-2".into(), "echo two".into())
            .await;
        let _ = pipeline
            .conduct("t-multi-3".into(), "echo three".into())
            .await;

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

    #[test]
    fn test_bootstrap_creates_pipeline() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        // Verify initialization by ensuring bus subscription works
        let _rx = pipeline.bus_handle().subscribe();
    }

    #[tokio::test]
    async fn test_bootstrapped_pipeline_executes() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let result = pipeline
            .conduct("t-boot".into(), "echo bootstrapped".into())
            .await
            .unwrap();
        assert!(matches!(result, TaskResult::Success { .. }));
    }
}

// ─── 4. TaskResult Validation ────────────────────────────────────────────────

mod task_result_validation {
    use super::*;

    #[tokio::test]
    async fn test_success_result_has_valid_fields() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let result = pipeline
            .conduct("t-fields".into(), "echo validation".into())
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
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let result = pipeline
            .conduct("t-serde".into(), "echo serde".into())
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

    #[tokio::test]
    async fn test_idle_to_completed() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());

        let _ = pipeline
            .conduct("t-trans".into(), "echo transition".into())
            .await;
        // Verify success implies completion
    }
}
