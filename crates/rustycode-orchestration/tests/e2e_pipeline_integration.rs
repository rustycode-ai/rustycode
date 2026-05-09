#![allow(clippy::unwrap_used, clippy::float_cmp)]

//! End-to-end integration tests for the full orchestration pipeline.
//!
//! Tests the complete flow: pipeline creation → task submission → step
//! execution → trace recording → event publishing → result extraction.

use rustycode_orchestration::bus::OrchestrationEvent;
use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::error_signal::{ErrorCategory, ErrorSignal};
use rustycode_orchestration::execution_trace::{ExecutionTrace, TraceEntry};
use rustycode_orchestration::pipeline::{OrchestrationPipeline, TaskResult};
use rustycode_orchestration::task_context::{TaskComplexity, TaskContext, TaskPhase};

// ─── 1. Full Pipeline Lifecycle ─────────────────────────────────────────────

mod full_pipeline_lifecycle {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_simple_task_completes() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let result = pipeline
            .conduct("task-e2e-1".into(), "echo hello world".into())
            .await
            .unwrap();

        match result {
            TaskResult::Success {
                output,
                tier_used,
                steps_completed,
                ..
            } => {
                assert!(steps_completed > 0, "Should complete at least 1 step");
                assert!(tier_used >= 2, "Should use at least tier 2");
                let _ = output;
            }
            TaskResult::Failed { reason, .. } => {
                panic!("Simple task should not fail: {reason}");
            }
        }
    }

    #[tokio::test]
    async fn test_pipeline_publishes_completion_event() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
        let mut rx = pipeline.bus_handle().subscribe();

        let _ = pipeline
            .conduct("task-e2e-2".into(), "echo test".into())
            .await;

        let mut found = false;
        while let Ok(event) = rx.try_recv() {
            if let OrchestrationEvent::TaskCompleted {
                task_id,
                tier_used,
                cost_usd,
            } = event
            {
                if task_id == "task-e2e-2" {
                    found = true;
                    assert!(tier_used >= 2);
                    assert!(cost_usd >= 0.0);
                }
            }
        }
        assert!(found, "Should publish TaskCompleted event");
    }

    #[tokio::test]
    async fn test_pipeline_multiple_sequential_tasks() {
        let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());

        for i in 0..3 {
            let task_id = format!("seq-{i}");
            let result = pipeline
                .conduct(task_id.clone(), format!("echo task_{i}"))
                .await
                .unwrap();
            assert!(
                matches!(result, TaskResult::Success { .. }),
                "Task {task_id} should succeed"
            );
        }
    }
}

// ─── 2. Task Context State Transitions ──────────────────────────────────────

mod task_context_lifecycle {
    use super::*;

    #[test]
    fn test_task_context_starts_in_planning() {
        let ctx = TaskContext::new("t1".into(), "task".into());
        assert_eq!(ctx.current_phase, TaskPhase::Planning);
        assert_eq!(ctx.current_tier, 2);
    }

    #[test]
    fn test_task_context_escalate_increments_tier() {
        let mut ctx = TaskContext::new("t2".into(), "task".into());
        assert_eq!(ctx.current_tier, 2);
        ctx.escalate();
        assert_eq!(ctx.current_tier, 3);
        ctx.escalate();
        assert_eq!(ctx.current_tier, 4);
        ctx.escalate();
        assert_eq!(ctx.current_tier, 5);
    }

    #[test]
    fn test_task_context_escalate_grows_beyond_5() {
        let mut ctx = TaskContext::new("t3".into(), "task".into());
        for _ in 0..10 {
            ctx.escalate();
        }
        // escalate() uses saturating_add, no cap at 5
        assert_eq!(ctx.current_tier, 12);
    }

    #[test]
    fn test_task_context_complete_transitions_phase() {
        let mut ctx = TaskContext::new("t4".into(), "task".into());
        ctx.complete(TaskPhase::Completed);
        assert_eq!(ctx.current_phase, TaskPhase::Completed);
        assert!(ctx.current_phase.is_terminal());
    }

    #[test]
    fn test_task_context_budget_tracking() {
        let mut ctx = TaskContext::new("t5".into(), "task".into());
        let initial_remaining = ctx.budget_remaining();
        assert!(initial_remaining > 0.0);

        ctx.cost_used += 0.01;
        assert!(ctx.budget_remaining() < initial_remaining);
    }

    #[test]
    fn test_task_phase_tier_mapping() {
        assert_eq!(TaskPhase::Tier2Execution.tier(), 2);
        assert_eq!(TaskPhase::Tier3Review.tier(), 3);
        assert_eq!(TaskPhase::Tier4Recomposition.tier(), 4);
        assert_eq!(TaskPhase::Tier5Thinking.tier(), 5);
        assert_eq!(TaskPhase::Planning.tier(), 0);
        assert_eq!(TaskPhase::Completed.tier(), 0);
    }

    #[test]
    fn test_task_phase_terminal_states() {
        assert!(TaskPhase::Completed.is_terminal());
        assert!(TaskPhase::Failed.is_terminal());
        assert!(TaskPhase::Cancelled.is_terminal());
        assert!(TaskPhase::Killed.is_terminal());
        assert!(!TaskPhase::Planning.is_terminal());
        assert!(!TaskPhase::Tier2Execution.is_terminal());
    }

    #[test]
    fn test_task_phase_display() {
        assert_eq!(TaskPhase::Planning.to_string(), "planning");
        assert_eq!(TaskPhase::Tier2Execution.to_string(), "tier2_execution");
        assert_eq!(TaskPhase::Completed.to_string(), "completed");
        assert_eq!(TaskPhase::Failed.to_string(), "failed");
    }

    #[test]
    fn test_task_context_with_complexity() {
        let mut ctx = TaskContext::new("t6".into(), "complex task".into());
        ctx.constraints.complexity = TaskComplexity::Expert;
        assert_eq!(ctx.constraints.complexity, TaskComplexity::Expert);
    }
}

// ─── 3. Execution Trace Recording ───────────────────────────────────────────

mod execution_trace_recording {
    use super::*;

    #[test]
    fn test_trace_records_success_entry() {
        let mut trace = ExecutionTrace::new("trace-1".into());
        trace.append(TraceEntry::new_success(
            "step-1".into(),
            0,
            2,
            "Bash".into(),
            serde_json::json!({"desc": "echo hello"}),
            "hello".into(),
            Some(0),
            0.001,
        ));

        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.steps[0].step_id, "step-1");
        assert_eq!(trace.steps[0].tier, 2);
        assert_eq!(trace.steps[0].exit_code, Some(0));
        assert!(trace.steps[0].error_signal.is_none());
    }

    #[test]
    fn test_trace_records_failure_entry() {
        let mut trace = ExecutionTrace::new("trace-2".into());
        let error = ErrorSignal::new(
            ErrorCategory::LogicError,
            Some(1),
            "assertion failed".into(),
            "step-2".into(),
            "Bash".into(),
        );
        trace.append(TraceEntry::new_failure(
            "step-2".into(),
            0,
            2,
            "Bash".into(),
            serde_json::json!({}),
            "error output".into(),
            Some(1),
            error,
            0.005,
        ));

        assert_eq!(trace.steps.len(), 1);
        assert!(trace.steps[0].error_signal.is_some());
    }

    #[test]
    fn test_trace_multiple_steps() {
        let mut trace = ExecutionTrace::new("trace-3".into());
        for i in 0..5 {
            trace.append(TraceEntry::new_success(
                format!("step-{i}"),
                i as u8,
                2,
                "Bash".into(),
                serde_json::json!({}),
                format!("output-{i}"),
                Some(0),
                0.001,
            ));
        }
        assert_eq!(trace.steps.len(), 5);
        assert_eq!(trace.failures().len(), 0);
    }

    #[test]
    fn test_trace_failures_filter() {
        let mut trace = ExecutionTrace::new("trace-4".into());

        trace.append(TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "Bash".into(),
            serde_json::json!({}),
            "ok".into(),
            Some(0),
            0.001,
        ));

        let err = ErrorSignal::new(
            ErrorCategory::SyntaxError,
            Some(1),
            "parse error".into(),
            "s2".into(),
            "Bash".into(),
        );
        trace.append(TraceEntry::new_failure(
            "s2".into(),
            1,
            2,
            "Bash".into(),
            serde_json::json!({}),
            "fail".into(),
            Some(1),
            err,
            0.002,
        ));

        assert_eq!(trace.steps.len(), 2);
        assert_eq!(trace.failures().len(), 1);
        assert_eq!(trace.failures()[0].step_id, "s2");
    }
}

// ─── 4. Error Signal Classification ─────────────────────────────────────────

mod error_classification {
    use super::*;
    use rustycode_orchestration::error_signal::ErrorClassifier;

    #[test]
    fn test_classifier_identifies_patterns() {
        let classifier = ErrorClassifier::new();

        let cases = [
            ("syntax error at line 5", ErrorCategory::SyntaxError),
            (
                "error[E0308]: mismatched types",
                ErrorCategory::CompileError,
            ),
            ("TypeError: undefined", ErrorCategory::TypeError),
            (
                "permission denied: /etc/shadow",
                ErrorCategory::PermissionDenied,
            ),
            ("no space left on device", ErrorCategory::DiskFull),
            ("tool timed out after 30s", ErrorCategory::ToolTimeout),
        ];

        for (input, expected) in &cases {
            assert_eq!(
                classifier.classify(input, 1),
                *expected,
                "Failed to classify: {input}"
            );
        }
    }

    #[test]
    fn test_classifier_falls_back_to_exit_code() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("unknown", 13),
            ErrorCategory::PermissionDenied
        );
        assert_eq!(
            classifier.classify("unknown", 124),
            ErrorCategory::ToolTimeout
        );
    }

    #[test]
    fn test_error_signal_truncation_boundary() {
        // Exactly at boundary should not truncate
        let msg_2048 = "x".repeat(2048);
        let signal = ErrorSignal::new(
            ErrorCategory::Internal,
            None,
            msg_2048,
            "s1".into(),
            "Bash".into(),
        );
        assert!(!signal.message.contains("truncated"));

        // Just over boundary should truncate
        let msg_2049 = "x".repeat(2049);
        let signal = ErrorSignal::new(
            ErrorCategory::Internal,
            None,
            msg_2049,
            "s1".into(),
            "Bash".into(),
        );
        assert!(signal.message.contains("truncated"));
    }
}
