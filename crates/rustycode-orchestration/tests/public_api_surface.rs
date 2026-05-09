//! Tests verifying the public API surface exposed through lib.rs re-exports.
//!
//! These tests use ONLY the top-level `rustycode_orchestration::` paths
//! (not submodule paths) to validate the public interface consumers rely on.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use rustycode_orchestration::{
    BusHandle, EnsembleStrategy, ErrorCategory, ErrorClassifier, ErrorSignal, MessageBus,
    OrchestrationError, OrchestrationErrorCategory, OrchestrationEvent, ParticipantSpec,
    SharedWorkspace, SignalCategory, StrategyKind, StrategyOutcome, VerificationGateRegistry,
    VerificationOutcome,
};

// ─── 1. Bus Public API ─────────────────────────────────────────────────────

mod bus_api {
    use super::*;

    #[test]
    fn test_bus_handle_create_and_subscribe() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        bus.publish(OrchestrationEvent::TaskCompleted {
            task_id: "api-test".into(),
            tier_used: 2,
            cost_usd: 0.01,
        });
        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, OrchestrationEvent::TaskCompleted { task_id, .. } if task_id == "api-test")
        );
    }

    #[test]
    fn test_message_bus_create() {
        let bus = MessageBus::new(16);
        let handle = bus.handle();
        let mut rx = handle.subscribe();
        handle.publish(OrchestrationEvent::TaskCompleted {
            task_id: "mb-test".into(),
            tier_used: 3,
            cost_usd: 0.05,
        });
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn test_escalation_event_via_bus() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        bus.publish(OrchestrationEvent::EscalationSignal {
            task_id: "esc-1".into(),
            from_tier: 2,
            to_tier: 3,
            reason: "max_attempts".into(),
        });
        let event = rx.try_recv().unwrap();
        if let OrchestrationEvent::EscalationSignal {
            task_id,
            from_tier,
            to_tier,
            reason,
        } = event
        {
            assert_eq!(task_id, "esc-1");
            assert_eq!(from_tier, 2);
            assert_eq!(to_tier, 3);
            assert_eq!(reason, "max_attempts");
        } else {
            panic!("Expected EscalationSignal event");
        }
    }

    #[test]
    fn test_bus_no_event_on_empty() {
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        assert!(rx.try_recv().is_err());
    }
}

// ─── 2. Error Public API ───────────────────────────────────────────────────

mod error_api {
    use super::*;

    #[test]
    fn test_orchestration_error_convenience_constructors() {
        let err = OrchestrationError::execution("test exec");
        assert!(err.is_recoverable());
        assert_eq!(err.category(), OrchestrationErrorCategory::Execution);

        let err = OrchestrationError::thinking("test think");
        assert!(!err.is_recoverable());
        assert_eq!(err.category(), OrchestrationErrorCategory::Thinking);

        let err = OrchestrationError::config("bad cfg");
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_all_error_categories_via_public_api() {
        assert_eq!(
            OrchestrationError::tool("x").category(),
            OrchestrationErrorCategory::Tool
        );
        assert_eq!(
            OrchestrationError::session("x").category(),
            OrchestrationErrorCategory::Session
        );
        assert_eq!(
            OrchestrationError::verification("x").category(),
            OrchestrationErrorCategory::Verification
        );
        assert_eq!(
            OrchestrationError::recovery("x").category(),
            OrchestrationErrorCategory::Recovery
        );
        assert_eq!(
            OrchestrationError::llm("x").category(),
            OrchestrationErrorCategory::LLM
        );
    }
}

// ─── 3. Error Signal Public API ────────────────────────────────────────────

mod error_signal_api {
    use super::*;

    #[test]
    fn test_error_signal_creation() {
        let signal = ErrorSignal::new(
            SignalCategory::LogicError,
            Some(1),
            "logic failed".into(),
            "step-1".into(),
            "Bash".into(),
        );
        assert_eq!(signal.category, SignalCategory::LogicError);
        assert_eq!(signal.exit_code, Some(1));
        assert_eq!(signal.message, "logic failed");
    }

    #[test]
    fn test_error_classifier_via_public_api() {
        let classifier = ErrorClassifier::new();
        let category = classifier.classify("syntax error at line 5", 1);
        assert_eq!(category, SignalCategory::SyntaxError);
    }

    #[test]
    fn test_all_error_category_variants() {
        let categories = [
            ErrorCategory::SyntaxError,
            ErrorCategory::CompileError,
            ErrorCategory::TypeError,
            ErrorCategory::LogicError,
            ErrorCategory::PermissionDenied,
            ErrorCategory::DiskFull,
            ErrorCategory::ToolTimeout,
            ErrorCategory::ContextLengthExceeded,
            ErrorCategory::Fatal,
            ErrorCategory::Internal,
            ErrorCategory::Custom("test".into()),
        ];
        assert_eq!(categories.len(), 11);
    }

    #[test]
    fn test_signal_category_is_error_category_alias() {
        let cat: SignalCategory = ErrorCategory::LogicError;
        assert_eq!(cat, SignalCategory::LogicError);
    }
}

// ─── 4. Shared Workspace Public API ────────────────────────────────────────

mod workspace_api {
    use super::*;

    #[tokio::test]
    async fn test_shared_workspace_write_read() {
        let ws = SharedWorkspace::new();
        ws.write(
            "key1".into(),
            serde_json::json!("value1"),
            "agent".into(),
            None,
        )
        .await;
        let entry = ws.read("key1").await;
        assert!(entry.is_some());
    }

    #[tokio::test]
    async fn test_shared_workspace_read_value() {
        let ws = SharedWorkspace::new();
        ws.write(
            "k".into(),
            serde_json::json!(42),
            "agent".into(),
            Some("s1".into()),
        )
        .await;
        let val = ws.read_value("k").await;
        assert!(val.is_some());
    }

    #[tokio::test]
    async fn test_shared_workspace_contains() {
        let ws = SharedWorkspace::new();
        assert!(!ws.contains("missing").await);
        ws.write("k".into(), serde_json::json!("v"), "agent".into(), None)
            .await;
        assert!(ws.contains("k").await);
    }

    #[tokio::test]
    async fn test_shared_workspace_is_empty() {
        let ws = SharedWorkspace::new();
        assert!(ws.is_empty().await);
        ws.write("k".into(), serde_json::json!("v"), "agent".into(), None)
            .await;
        assert!(!ws.is_empty().await);
    }

    #[tokio::test]
    async fn test_shared_workspace_remove() {
        let ws = SharedWorkspace::new();
        ws.write("k".into(), serde_json::json!("v"), "agent".into(), None)
            .await;
        let removed = ws.remove("k").await;
        assert!(removed.is_some());
        assert!(!ws.contains("k").await);
    }

    #[tokio::test]
    async fn test_shared_workspace_keys() {
        let ws = SharedWorkspace::new();
        ws.write("a".into(), serde_json::json!(1), "agent".into(), None)
            .await;
        ws.write("b".into(), serde_json::json!(2), "agent".into(), None)
            .await;
        let keys = ws.keys().await;
        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn test_shared_workspace_snapshot() {
        let ws = SharedWorkspace::new();
        ws.write("x".into(), serde_json::json!("val"), "agent".into(), None)
            .await;
        let snap = ws.snapshot().await;
        assert_eq!(snap.len(), 1);
    }
}

// ─── 5. Verification Gates Public API ──────────────────────────────────────

mod verification_api {
    use super::*;
    use rustycode_orchestration::execution_trace::TraceEntry;

    #[test]
    fn test_verification_registry_verify_passes() {
        let registry = VerificationGateRegistry::new();
        let step = rustycode_orchestration::types::Step {
            id: "s1".into(),
            index: 0,
            description: "test".into(),
            expected_output_type: rustycode_orchestration::types::OutputType::Verification,
            suggested_tool: None,
            retry_on_failure: false,
            required_resources: rustycode_orchestration::guard::RequiredResources::new(),
        };
        let entry = TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "Bash".into(),
            serde_json::json!({}),
            "ok".into(),
            Some(0),
            0.001,
        );
        let outcome = registry.verify(&step, &entry);
        assert_eq!(outcome, VerificationOutcome::Valid);
    }
}

// ─── 6. Ensemble Strategy Public API ───────────────────────────────────────

mod ensemble_api {
    use super::*;

    #[test]
    fn test_strategy_kind_variants() {
        let kinds = [
            StrategyKind::DecomposeAndDelegate,
            StrategyKind::ParallelVote,
            StrategyKind::SequentialReview,
            StrategyKind::Adversarial,
        ];
        assert_eq!(kinds.len(), 4);
    }

    #[test]
    fn test_ensemble_strategy_constructors() {
        let dd = EnsembleStrategy::decompose_and_delegate();
        assert_eq!(dd.kind(), StrategyKind::DecomposeAndDelegate);
        assert!(!dd.participants().is_empty());

        let pv = EnsembleStrategy::parallel_vote();
        assert_eq!(pv.kind(), StrategyKind::ParallelVote);

        let sr = EnsembleStrategy::sequential_review();
        assert_eq!(sr.kind(), StrategyKind::SequentialReview);

        let adv = EnsembleStrategy::adversarial();
        assert_eq!(adv.kind(), StrategyKind::Adversarial);
    }

    #[test]
    fn test_select_for_complexity() {
        let easy = EnsembleStrategy::select_for_complexity(10);
        assert_eq!(easy.kind(), StrategyKind::SequentialReview);

        let mid = EnsembleStrategy::select_for_complexity(50);
        assert_eq!(mid.kind(), StrategyKind::ParallelVote);

        let hard = EnsembleStrategy::select_for_complexity(80);
        assert_eq!(hard.kind(), StrategyKind::DecomposeAndDelegate);
    }

    #[test]
    fn test_participant_spec_fields() {
        let spec = ParticipantSpec {
            role: "voter".into(),
            weight: 1.0,
            can_veto: false,
        };
        assert_eq!(spec.role, "voter");
        assert!((spec.weight - 1.0).abs() < f64::EPSILON);
        assert!(!spec.can_veto);
    }

    #[test]
    fn test_strategy_outcome_confidence() {
        let outcome = StrategyOutcome {
            steps: vec![],
            confidence: 0.85,
            strategy_used: StrategyKind::ParallelVote,
            participants: vec!["a".into()],
            notes: "test".into(),
        };
        assert!(outcome.is_confident());
        assert!((outcome.confidence - 0.85).abs() < f64::EPSILON);
    }
}
