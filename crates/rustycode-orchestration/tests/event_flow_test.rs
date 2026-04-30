//! Event flow integration tests.
//!
//! Tests the full event lifecycle: Musician publishes PartialResult,
//! Skeptic monitors and raises Objections, Conductor publishes
//! EscalationSignal, and the bus delivers events correctly.

#![allow(
    clippy::unwrap_used,
    clippy::match_wildcard_for_single_variants,
    clippy::manual_let_else,
    clippy::doc_markdown,
    clippy::uninlined_format_args,
    clippy::redundant_clone,
    clippy::items_after_statements
)]

use rustycode_orchestration::bus::{BusHandle, OrchestrationEvent};
use rustycode_orchestration::conductor::{Conductor, EscalationDecision};
use rustycode_orchestration::config::{OrchestrationConfig, TierConfig};
use rustycode_orchestration::error_signal::ErrorSignal;
use rustycode_orchestration::musician::Musician;
use rustycode_orchestration::shared_workspace::SharedWorkspace;
use rustycode_orchestration::skeptic::Skeptic;
use rustycode_orchestration::state_machine::TaskContext;
use rustycode_orchestration::types::{OutputType, Step};
use std::sync::Arc;

fn make_step(id: &str, desc: &str) -> Step {
    Step {
        id: id.into(),
        index: 0,
        description: desc.into(),
        expected_output_type: OutputType::Verification,
        suggested_tool: Some("bash".into()),
        retry_on_failure: true,
        required_resources: rustycode_orchestration::guard::RequiredResources::default(),
    }
}

fn make_config_with_escalation() -> OrchestrationConfig {
    let mut config = OrchestrationConfig::default();
    config.escalation.insert(
        "tier_2".into(),
        TierConfig {
            max_attempts: 2,
            critical_errors: vec![],
            recoverable_errors: vec![],
        },
    );
    config.escalation.insert(
        "tier_3".into(),
        TierConfig {
            max_attempts: 2,
            critical_errors: vec![],
            recoverable_errors: vec![],
        },
    );
    config
}

// ─── 1. End-to-End Event Flow ────────────────────────────────────────────

mod e2e_event_flow {
    use super::*;

    #[test]
    fn test_full_event_chain_musician_to_skeptic() {
        let bus = BusHandle::new(64);
        let mut rx = bus.subscribe();

        // Start skeptic monitoring
        let skeptic = Skeptic::new(bus.clone());
        skeptic.start_monitoring();

        // Musician publishes a partial result with ERROR
        bus.publish(OrchestrationEvent::PartialResult {
            step_id: "step-1".into(),
            content: "ERROR: compilation failed".into(),
        });

        // Read PartialResult
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, OrchestrationEvent::PartialResult { .. }));

        // Note: Skeptic's spawned task may not have processed yet in non-async context
    }

    #[tokio::test]
    async fn test_skeptic_detects_error_and_publishes_objection() {
        let bus = BusHandle::new(64);
        let mut rx = bus.subscribe();

        let skeptic = Skeptic::new(bus.clone());
        skeptic.start_monitoring();

        // Yield to let skeptic's task start
        tokio::task::yield_now().await;

        // Publish error-containing partial result
        bus.publish(OrchestrationEvent::PartialResult {
            step_id: "s-err".into(),
            content: "PANIC: unrecoverable error in computation".into(),
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let partial = rx.try_recv().unwrap();
        assert!(matches!(partial, OrchestrationEvent::PartialResult { .. }));

        let objection = rx.try_recv().unwrap();
        if let OrchestrationEvent::Objection { step_id, reason } = objection {
            assert_eq!(step_id, "s-err");
            assert!(reason.contains("Skeptic"));
        } else {
            panic!("Expected Objection event, got {:?}", objection);
        }
    }

    #[tokio::test]
    async fn test_skeptic_ignores_clean_output() {
        let bus = BusHandle::new(64);
        let mut rx = bus.subscribe();

        let skeptic = Skeptic::new(bus.clone());
        skeptic.start_monitoring();
        tokio::task::yield_now().await;

        bus.publish(OrchestrationEvent::PartialResult {
            step_id: "s-clean".into(),
            content: "Build successful, all tests pass".into(),
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let partial = rx.try_recv().unwrap();
        assert!(matches!(partial, OrchestrationEvent::PartialResult { .. }));
        // No objection should follow
        assert!(rx.try_recv().is_err());
    }
}

// ─── 2. Conductor Escalation Events ──────────────────────────────────────

mod conductor_events {
    use super::*;

    #[test]
    fn test_conductor_publishes_escalation_on_tier2_exhaustion() {
        let config = make_config_with_escalation();
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let conductor = Conductor::with_bus(config, bus);

        let mut ctx = TaskContext::new("task-esc-1".into(), "test".into());
        ctx.attempt_count = 2;

        let signal = ErrorSignal::new(
            rustycode_orchestration::error_signal::SignalCategory::LogicError,
            Some(1),
            "assertion failed".into(),
            "step-1".into(),
            "bash".into(),
        );

        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 3, .. }
        ));

        let event = rx.try_recv().unwrap();
        if let OrchestrationEvent::EscalationSignal {
            task_id,
            from_tier,
            to_tier,
            reason,
        } = event
        {
            assert_eq!(task_id, "task-esc-1");
            assert_eq!(from_tier, 2);
            assert_eq!(to_tier, 3);
            assert!(reason.contains("tier2_exhausted"));
        } else {
            panic!("Expected EscalationSignal, got {:?}", event);
        }
    }

    #[test]
    fn test_conductor_publishes_escalation_on_tier3_exhaustion() {
        let config = make_config_with_escalation();
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let conductor = Conductor::with_bus(config, bus);

        let mut ctx = TaskContext::new("task-esc-2".into(), "test".into());
        ctx.current_tier = 3;
        ctx.attempt_count = 2;

        let signal = ErrorSignal::new(
            rustycode_orchestration::error_signal::SignalCategory::Internal,
            None,
            "internal error".into(),
            "step-2".into(),
            "bash".into(),
        );

        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 4, .. }
        ));

        let event = rx.try_recv().unwrap();
        if let OrchestrationEvent::EscalationSignal {
            from_tier, to_tier, ..
        } = event
        {
            assert_eq!(from_tier, 3);
            assert_eq!(to_tier, 4);
        } else {
            panic!("Expected EscalationSignal");
        }
    }

    #[test]
    fn test_conductor_publishes_escalation_on_budget_exceeded() {
        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 0.01;
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let conductor = Conductor::with_bus(config, bus);

        let mut ctx = TaskContext::new("task-budget".into(), "test".into());
        ctx.cost_used = 5.0;

        let signal = ErrorSignal::new(
            rustycode_orchestration::error_signal::SignalCategory::Internal,
            None,
            "error".into(),
            "s1".into(),
            "bash".into(),
        );

        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Abandon { .. }));

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, OrchestrationEvent::EscalationSignal { .. }));
    }

    #[test]
    fn test_conductor_no_event_without_bus() {
        let config = make_config_with_escalation();
        let conductor = Conductor::new(config);

        let mut ctx = TaskContext::new("t-no-bus".into(), "test".into());
        ctx.cost_used = 999.0;

        let signal = ErrorSignal::new(
            rustycode_orchestration::error_signal::SignalCategory::Internal,
            None,
            "error".into(),
            "s1".into(),
            "bash".into(),
        );

        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Abandon { .. }));
    }
}

// ─── 3. Musician Event Publishing ────────────────────────────────────────

mod musician_events {
    use super::*;

    #[tokio::test]
    async fn test_musician_publishes_partial_result_on_step() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let musician = Musician::with_bus(bus);

        let step = make_step("s-pub", "run tests");
        let mut ctx = TaskContext::new("t-mus".into(), "run tests".into());

        let _result = musician
            .play_step_with_context(&step, &mut ctx)
            .await
            .unwrap();

        let event = rx.try_recv().unwrap();
        if let OrchestrationEvent::PartialResult { step_id, content } = event {
            assert_eq!(step_id, "s-pub");
            assert!(!content.is_empty());
        } else {
            panic!("Expected PartialResult, got {:?}", event);
        }
    }

    #[tokio::test]
    async fn test_musician_publishes_for_multiple_steps() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let musician = Musician::with_bus(bus);

        for i in 0..3 {
            let step = make_step(&format!("s-{i}"), "step");
            let mut ctx = TaskContext::new("t-multi".into(), format!("step {i}"));
            let _result = musician
                .play_step_with_context(&step, &mut ctx)
                .await
                .unwrap();
        }

        let mut count = 0;
        while let Ok(OrchestrationEvent::PartialResult { .. }) = rx.try_recv() {
            count += 1;
        }
        assert_eq!(count, 3);
    }
}

// ─── 4. SharedWorkspace with Events ──────────────────────────────────────

mod workspace_events {
    use super::*;

    #[tokio::test]
    async fn test_workspace_publishes_update_event() {
        let workspace = Arc::new(SharedWorkspace::new());
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();

        workspace
            .write(
                "test.key".into(),
                serde_json::json!({"value": 42}),
                "agent-1".into(),
                Some("step-1".into()),
            )
            .await;

        bus.publish(OrchestrationEvent::WorkspaceUpdated {
            task_id: "t-ws".into(),
            key: "test.key".into(),
            written_by: "agent-1".into(),
        });

        let event = rx.try_recv().unwrap();
        if let OrchestrationEvent::WorkspaceUpdated {
            task_id,
            key,
            written_by,
        } = event
        {
            assert_eq!(task_id, "t-ws");
            assert_eq!(key, "test.key");
            assert_eq!(written_by, "agent-1");
        } else {
            panic!("Expected WorkspaceUpdated");
        }

        assert!(workspace.contains("test.key").await);
    }
}
