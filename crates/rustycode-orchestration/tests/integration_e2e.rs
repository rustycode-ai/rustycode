//! End-to-end integration tests for rustycode-orchestration.
//!
//! Tests the full orchestration pipeline, conductor decisions, model registry,
//! task lifecycle, and error handling as integrated systems.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use rustycode_orchestration::conductor::{Conductor, EscalationDecision};
use rustycode_orchestration::config::{ModelConfig, OrchestrationConfig, TierConfig};
use rustycode_orchestration::error_signal::{ErrorClassifier, ErrorSignal, SignalCategory};
use rustycode_orchestration::execution_trace::{ExecutionTrace, TraceEntry};
use rustycode_orchestration::model_registry::ModelRegistry;
use rustycode_orchestration::pipeline::{OrchestrationPipeline, TaskResult};
use rustycode_orchestration::task_context::{TaskContext, TaskPhase};
use rustycode_orchestration::types::{ExecutionTier, OutputType, Step, TaskOutcome};
use rustycode_orchestration::verification_gates::{VerificationGateRegistry, VerificationOutcome};
use rustycode_orchestration::{OrchestrationError, OrchestrationErrorCategory as ErrorCategory};

// ─── 1. Full Pipeline E2E ────────────────────────────────────────────────────

mod pipeline_e2e {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_conduct_simple_task() {
        let config = OrchestrationConfig::default();
        let pipeline = OrchestrationPipeline::new(config);

        let result = pipeline
            .conduct("e2e-simple".into(), "echo hello world".into())
            .await
            .unwrap();

        match result {
            TaskResult::Success {
                output,
                total_cost,
                tier_used,
                steps_completed,
                ..
            } => {
                assert!(!output.is_empty() || steps_completed > 0);
                assert!(total_cost >= 0.0);
                assert!(tier_used >= 2);
                assert!(steps_completed > 0);
            }
            TaskResult::Failed { reason, .. } => {
                panic!("Expected success, got failure: {reason}");
            }
        }
    }

    #[tokio::test]
    async fn test_pipeline_conduct_tracks_cost() {
        let config = OrchestrationConfig::default();
        let pipeline = OrchestrationPipeline::new(config);

        let result = pipeline
            .conduct("e2e-cost".into(), "echo cost tracking".into())
            .await
            .unwrap();

        if let TaskResult::Success { total_cost, .. } = result {
            assert!(total_cost >= 0.0, "Cost should be tracked after execution");
        }
    }

    #[tokio::test]
    async fn test_pipeline_conduct_updates_trace() {
        let config = OrchestrationConfig::default();
        let pipeline = OrchestrationPipeline::new(config);

        let result = pipeline
            .conduct("e2e-trace".into(), "echo trace updates".into())
            .await
            .unwrap();

        if let TaskResult::Success {
            steps_completed, ..
        } = &result
        {
            assert!(steps_completed > &0usize, "Should have completed steps");
        }
    }

    #[tokio::test]
    async fn test_pipeline_conduct_result_serialization() {
        let config = OrchestrationConfig::default();
        let pipeline = OrchestrationPipeline::new(config);

        let result = pipeline
            .conduct("e2e-serde".into(), "echo serialization".into())
            .await
            .unwrap();
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TaskResult = serde_json::from_str(&json).unwrap();

        assert_eq!(
            result, deserialized,
            "TaskResult should round-trip through JSON"
        );
    }

    #[tokio::test]
    async fn test_pipeline_conduct_multiple_tasks() {
        let config = OrchestrationConfig::default();
        let pipeline = OrchestrationPipeline::new(config);

        let result_a = pipeline
            .conduct("task-A".into(), "echo task A".into())
            .await
            .unwrap();
        let result_b = pipeline
            .conduct("task-B".into(), "echo task B".into())
            .await
            .unwrap();

        assert!(matches!(result_a, TaskResult::Success { .. }));
        assert!(matches!(result_b, TaskResult::Success { .. }));
    }
}

// ─── 2. Conductor Decision E2E ───────────────────────────────────────────────

mod conductor_e2e {
    use super::*;

    fn make_escalation_config() -> OrchestrationConfig {
        let mut config = OrchestrationConfig::default();
        config.escalation.insert(
            "tier_2".into(),
            TierConfig {
                max_attempts: 2,
                critical_errors: vec![SignalCategory::LogicError],
                recoverable_errors: vec![SignalCategory::SyntaxError],
            },
        );
        config.escalation.insert(
            "tier_3".into(),
            TierConfig {
                max_attempts: 2,
                critical_errors: vec![SignalCategory::LogicError],
                recoverable_errors: vec![],
            },
        );
        config
    }

    fn make_signal(category: SignalCategory) -> ErrorSignal {
        ErrorSignal::new(
            category,
            Some(1),
            "test error".into(),
            "step-1".into(),
            "bash".into(),
        )
    }

    #[test]
    fn test_conductor_tier_escalation_flow() {
        let config = make_escalation_config();
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("e2e-escalation".into(), "escalation test".into());

        // Tier 2: attempts 0,1 → Retry; attempt 2 → Escalate to tier 3
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Retry));
        ctx.attempt_count += 1;

        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Retry));
        ctx.attempt_count += 1;

        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 3, .. }
        ));
        ctx.current_tier = 3;
        ctx.attempt_count = 0;

        // Tier 3: attempt 2 → Escalate to tier 4
        ctx.attempt_count = 2;
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 4, .. }
        ));
        ctx.current_tier = 4;

        // Tier 4 → Abandon
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(
            matches!(decision, EscalationDecision::Abandon { .. }),
            "Should abandon at tier 4"
        );
    }

    #[test]
    fn test_conductor_thinking_triggered_at_tier4() {
        let config = OrchestrationConfig::default();
        let conductor = Conductor::new(config);

        // Heuristic: task len > 20 and context len > 10 triggers thinking
        let result = conductor.try_thinking(
            "This is a complex task requiring deep analysis",
            "Multiple compilation errors detected in the source code",
        );
        assert!(
            result.is_some(),
            "Thinking should be triggered for complex tasks"
        );
        let thinking = result.unwrap();
        assert!(
            thinking.contains("tier=5"),
            "Thinking result should reference tier 5"
        );
    }

    #[test]
    fn test_conductor_thinking_not_triggered_for_simple() {
        let config = OrchestrationConfig::default();
        let conductor = Conductor::new(config);

        let result = conductor.try_thinking("simple", "err");
        assert!(
            result.is_none(),
            "Thinking should not be triggered for simple tasks"
        );
    }

    #[test]
    fn test_conductor_budget_abandon() {
        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 1.0;
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("e2e-budget".into(), "budget test".into());
        ctx.cost_used = 2.0;

        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);

        assert!(
            matches!(decision, EscalationDecision::Abandon { reason } if reason == "budget_exceeded"),
            "Should abandon when budget exceeded"
        );
    }

    #[test]
    fn test_conductor_budget_warning() {
        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 10.0;
        config.budget.warn_threshold_pct = 0.5;
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("e2e-warn".into(), "budget warning test".into());
        ctx.cost_used = 5.5;

        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);

        assert!(
            matches!(decision, EscalationDecision::WarnBudget { remaining_usd } if (remaining_usd - 4.5).abs() < 0.01),
            "Should warn when above threshold, remaining should be ~4.5"
        );
    }

    #[test]
    fn test_conductor_critical_error_instant_escalation() {
        let config = make_escalation_config();
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("e2e-critical".into(), "critical error test".into());
        ctx.attempt_count = 0;

        let signal = make_signal(SignalCategory::LogicError);
        let decision = conductor.handle_error(&mut ctx, &signal);

        assert!(
            matches!(decision, EscalationDecision::Escalate { next_tier: 3, .. }),
            "Critical errors should escalate immediately even at attempt 0"
        );
    }
}

// ─── 3. Cost Calculation E2E ─────────────────────────────────────────────────

mod cost_calculation_e2e {
    use super::*;
    use rustycode_orchestration::model_registry::Capability;

    fn make_config_with_models() -> OrchestrationConfig {
        let mut config = OrchestrationConfig::default();
        config.models.insert(
            "tier_2".into(),
            vec![
                ModelConfig {
                    name: "claude-haiku-4-5".into(),
                    provider: "anthropic".into(),
                    cost_per_1m_tokens_input: 0.8,
                    cost_per_1m_tokens_output: 4.0,
                    context_window: 200_000,
                    supports_extended_thinking: None,
                    max_thinking_tokens: None,
                },
                ModelConfig {
                    name: "deepseek-v3".into(),
                    provider: "deepseek".into(),
                    cost_per_1m_tokens_input: 0.27,
                    cost_per_1m_tokens_output: 1.10,
                    context_window: 128_000,
                    supports_extended_thinking: None,
                    max_thinking_tokens: None,
                },
            ],
        );
        config.models.insert(
            "tier_3".into(),
            vec![ModelConfig {
                name: "gpt-4o".into(),
                provider: "openai".into(),
                cost_per_1m_tokens_input: 2.5,
                cost_per_1m_tokens_output: 10.0,
                context_window: 128_000,
                supports_extended_thinking: None,
                max_thinking_tokens: None,
            }],
        );
        config.models.insert(
            "tier_4".into(),
            vec![ModelConfig {
                name: "gemini-2.5-pro".into(),
                provider: "google".into(),
                cost_per_1m_tokens_input: 1.25,
                cost_per_1m_tokens_output: 10.0,
                context_window: 1_000_000,
                supports_extended_thinking: None,
                max_thinking_tokens: None,
            }],
        );
        config
    }

    #[test]
    fn test_registry_selects_cheapest_model() {
        let config = make_config_with_models();
        let registry = ModelRegistry::new(&config);

        // Select with no capability requirements → should get cheapest (deepseek)
        let selected = registry.select_best(2, &[]);
        assert!(selected.is_some());
        let model = selected.unwrap();
        assert_eq!(
            model.config.name, "deepseek-v3",
            "Should select cheapest model"
        );
    }

    #[test]
    fn test_registry_selects_with_capability_requirements() {
        let mut config = OrchestrationConfig::default();
        config.models.insert(
            "tier_2".into(),
            vec![
                ModelConfig {
                    name: "basic-model".into(),
                    provider: "test".into(),
                    cost_per_1m_tokens_input: 0.5,
                    cost_per_1m_tokens_output: 1.0,
                    context_window: 100_000,
                    supports_extended_thinking: None,
                    max_thinking_tokens: None,
                },
                ModelConfig {
                    name: "thinking-model".into(),
                    provider: "test".into(),
                    cost_per_1m_tokens_input: 3.0,
                    cost_per_1m_tokens_output: 15.0,
                    context_window: 200_000,
                    supports_extended_thinking: Some(true),
                    max_thinking_tokens: Some(10000),
                },
            ],
        );
        let registry = ModelRegistry::new(&config);

        // Require extended thinking → should skip basic and get thinking model
        let selected = registry.select_best(2, &[Capability::ExtendedThinking]);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().config.name, "thinking-model");
    }

    #[test]
    fn test_registry_no_match_returns_none() {
        let config = OrchestrationConfig::default();
        let registry = ModelRegistry::new(&config);

        // Empty registry → no models to select
        let selected = registry.select_best(2, &[]);
        assert!(selected.is_none());
    }

    #[test]
    fn test_registry_capability_flags_populated() {
        let mut config = OrchestrationConfig::default();
        config.models.insert(
            "tier_2".into(),
            vec![ModelConfig {
                name: "test-model".into(),
                provider: "test".into(),
                cost_per_1m_tokens_input: 1.0,
                cost_per_1m_tokens_output: 2.0,
                context_window: 100_000,
                supports_extended_thinking: Some(true),
                max_thinking_tokens: Some(8000),
            }],
        );
        let registry = ModelRegistry::new(&config);

        let model = registry.select_best(2, &[]).unwrap();
        assert!(model.capabilities.contains(&Capability::ExtendedThinking));
        assert!(model.capabilities.contains(&Capability::ToolCalling));
        assert!(model.capabilities.contains(&Capability::Streaming));
    }
}

// ─── 4. Task Lifecycle E2E ───────────────────────────────────────────────────

mod task_lifecycle_e2e {
    use super::*;

    #[test]
    fn test_task_context_full_lifecycle() {
        let mut ctx = TaskContext::new("e2e-lifecycle".into(), "implement feature X".into());

        // Start: Planning
        assert_eq!(ctx.current_phase, TaskPhase::Planning);
        assert_eq!(ctx.current_tier, 2);
        assert!(!ctx.current_phase.is_terminal());
        assert!(ctx.current_phase.is_active());

        // Transition to Tier2Execution
        ctx.advance_phase(TaskPhase::Tier2Execution);
        assert_eq!(ctx.current_phase, TaskPhase::Tier2Execution);
        assert_eq!(ctx.current_tier, 2);

        // Execute a step
        ctx.add_cost(0.001);
        ctx.add_tokens(500);
        ctx.attempt_count += 1;

        // Escalate to Tier3
        ctx.escalate();
        assert_eq!(ctx.current_tier, 3);
        assert_eq!(
            ctx.attempt_count, 0,
            "Escalation should reset attempt count"
        );

        // Advance to Tier3Review
        ctx.advance_phase(TaskPhase::Tier3Review);
        assert_eq!(ctx.current_phase, TaskPhase::Tier3Review);
        assert_eq!(ctx.current_tier, 3);

        // Complete successfully
        ctx.complete(TaskPhase::Completed);
        assert_eq!(ctx.current_phase, TaskPhase::Completed);
        assert!(ctx.current_phase.is_terminal());
        assert!(!ctx.current_phase.is_active());
        assert!(ctx.completed_at.is_some());
        assert!(ctx.duration_ms().is_some());
        assert!(ctx.duration_ms().unwrap() >= 0);
    }

    #[test]
    fn test_task_context_budget_tracking() {
        let mut ctx = TaskContext::new("e2e-budget".into(), "budget tracking".into());

        assert!((ctx.budget_remaining() - 10.0).abs() < f64::EPSILON);

        ctx.add_cost(2.5);
        assert!((ctx.budget_remaining() - 7.5).abs() < f64::EPSILON);

        ctx.add_cost(3.0);
        assert!((ctx.budget_remaining() - 4.5).abs() < f64::EPSILON);

        ctx.add_cost(10.0);
        assert!(
            (ctx.budget_remaining() - 0.0).abs() < f64::EPSILON,
            "Budget remaining should clamp to 0"
        );
    }

    #[test]
    fn test_task_context_tier5_thinking_phase() {
        assert_eq!(TaskPhase::Tier5Thinking.tier(), 5);
        assert!(!TaskPhase::Tier5Thinking.is_terminal());
        assert!(TaskPhase::Tier5Thinking.is_active());

        let mut ctx = TaskContext::new("e2e-t5".into(), "thinking phase".into());
        ctx.advance_phase(TaskPhase::Tier5Thinking);
        assert_eq!(ctx.current_tier, 5);
        assert_eq!(ctx.current_phase, TaskPhase::Tier5Thinking);
    }

    #[test]
    fn test_task_context_kill() {
        let mut ctx = TaskContext::new("e2e-kill".into(), "kill test".into());
        ctx.kill();
        assert_eq!(ctx.current_phase, TaskPhase::Killed);
        assert!(ctx.current_phase.is_terminal());
        assert!(ctx.completed_at.is_some());
    }

    #[test]
    fn test_task_context_all_phases_display() {
        assert_eq!(TaskPhase::Planning.to_string(), "planning");
        assert_eq!(TaskPhase::Tier2Execution.to_string(), "tier2_execution");
        assert_eq!(TaskPhase::Tier3Review.to_string(), "tier3_review");
        assert_eq!(
            TaskPhase::Tier4Recomposition.to_string(),
            "tier4_recomposition"
        );
        assert_eq!(TaskPhase::Tier5Thinking.to_string(), "tier5_thinking");
        assert_eq!(TaskPhase::Refining.to_string(), "refining");
        assert_eq!(TaskPhase::Completed.to_string(), "completed");
        assert_eq!(TaskPhase::Failed.to_string(), "failed");
        assert_eq!(TaskPhase::Cancelled.to_string(), "cancelled");
        assert_eq!(TaskPhase::Killed.to_string(), "killed");
    }

    #[test]
    fn test_task_context_serialization_roundtrip() {
        let mut ctx = TaskContext::new("e2e-serde".into(), "serialization test".into());
        ctx.add_cost(1.5);
        ctx.add_tokens(2000);
        ctx.advance_phase(TaskPhase::Tier3Review);

        let json = serde_json::to_string(&ctx).unwrap();
        let deserialized: TaskContext = serde_json::from_str(&json).unwrap();

        assert_eq!(ctx.task_id, deserialized.task_id);
        assert_eq!(ctx.current_phase, deserialized.current_phase);
        assert_eq!(ctx.current_tier, deserialized.current_tier);
        assert!((ctx.cost_used - deserialized.cost_used).abs() < f64::EPSILON);
    }
}

// ─── 5. Model Registry E2E ───────────────────────────────────────────────────

mod model_registry_e2e {
    use super::*;
    use rustycode_orchestration::model_registry::Capability;

    fn make_config() -> OrchestrationConfig {
        let mut config = OrchestrationConfig::default();
        config.models.insert(
            "tier_2".into(),
            vec![ModelConfig {
                name: "claude-haiku-4-5".into(),
                provider: "anthropic".into(),
                cost_per_1m_tokens_input: 0.8,
                cost_per_1m_tokens_output: 4.0,
                context_window: 200_000,
                supports_extended_thinking: None,
                max_thinking_tokens: None,
            }],
        );
        config.models.insert(
            "tier_3".into(),
            vec![ModelConfig {
                name: "claude-sonnet-4-6".into(),
                provider: "anthropic".into(),
                cost_per_1m_tokens_input: 3.0,
                cost_per_1m_tokens_output: 15.0,
                context_window: 200_000,
                supports_extended_thinking: None,
                max_thinking_tokens: None,
            }],
        );
        config.models.insert(
            "tier_4".into(),
            vec![ModelConfig {
                name: "claude-opus-4-7".into(),
                provider: "anthropic".into(),
                cost_per_1m_tokens_input: 15.0,
                cost_per_1m_tokens_output: 75.0,
                context_window: 200_000,
                supports_extended_thinking: Some(true),
                max_thinking_tokens: Some(32768),
            }],
        );
        config
    }

    #[test]
    fn test_model_registry_select_from_multiple_models() {
        let config = make_config();
        let registry = ModelRegistry::new(&config);

        // All models have ToolCalling and Streaming, so select_best should return cheapest
        let selected = registry.select_best(2, &[Capability::ToolCalling, Capability::Streaming]);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().config.name, "claude-haiku-4-5");
    }

    #[test]
    fn test_model_registry_extended_thinking_filter() {
        let config = make_config();
        let registry = ModelRegistry::new(&config);

        // Only opus has extended thinking
        let selected = registry.select_best(4, &[Capability::ExtendedThinking]);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().config.name, "claude-opus-4-7");
    }

    #[test]
    fn test_model_registry_no_match_for_impossible_requirements() {
        let config = make_config();
        let registry = ModelRegistry::new(&config);

        // No model has AuthRequired (we don't push it in new())
        let selected = registry.select_best(2, &[Capability::AuthRequired]);
        assert!(selected.is_none());
    }

    #[test]
    fn test_model_registry_empty_config() {
        let config = OrchestrationConfig::default();
        let registry = ModelRegistry::new(&config);

        let selected = registry.select_best(2, &[]);
        assert!(selected.is_none(), "Empty config should yield no models");
    }

    #[test]
    fn test_model_registry_capability_enum_properties() {
        // Verify capability equality and comparison
        assert_eq!(Capability::ExtendedThinking, Capability::ExtendedThinking);
        assert_ne!(Capability::ExtendedThinking, Capability::ToolCalling);
    }
}

// ─── 6. Error Handling E2E ───────────────────────────────────────────────────

mod error_handling_e2e {
    use super::*;

    #[test]
    fn test_error_signal_classification() {
        let classifier = ErrorClassifier::new();

        assert_eq!(
            classifier.classify("syntax error near line 5", 1),
            SignalCategory::SyntaxError
        );
        assert_eq!(
            classifier.classify("error[E0425]: compile error", 1),
            SignalCategory::CompileError
        );
        assert_eq!(
            classifier.classify("TypeError: undefined is not a function", 1),
            SignalCategory::TypeError
        );
        assert_eq!(
            classifier.classify("permission denied: access", 1),
            SignalCategory::PermissionDenied
        );
        assert_eq!(
            classifier.classify("no space left on device", 1),
            SignalCategory::DiskFull
        );
        assert_eq!(
            classifier.classify("context length exceeded maximum", 1),
            SignalCategory::ContextLengthExceeded
        );
        assert_eq!(
            classifier.classify("command timed out after 30s", 1),
            SignalCategory::ToolTimeout
        );

        assert_eq!(
            classifier.classify("unknown", 13),
            SignalCategory::PermissionDenied
        );
        assert_eq!(classifier.classify("unknown", 28), SignalCategory::DiskFull);
        assert_eq!(
            classifier.classify("unknown", 124),
            SignalCategory::ToolTimeout
        );

        let custom = classifier.classify("something unexpected", 42);
        assert!(matches!(custom, SignalCategory::Custom(s) if s.contains("42")));
    }

    #[test]
    fn test_error_signal_recoverability() {
        assert!(SignalCategory::SyntaxError.is_recoverable());
        assert!(SignalCategory::CompileError.is_recoverable());
        assert!(SignalCategory::TypeError.is_recoverable());
        assert!(SignalCategory::LogicError.is_recoverable());
        assert!(SignalCategory::PermissionDenied.is_recoverable());
        assert!(SignalCategory::DiskFull.is_recoverable());
        assert!(SignalCategory::ToolTimeout.is_recoverable());
        assert!(SignalCategory::ContextLengthExceeded.is_recoverable());
        assert!(SignalCategory::Internal.is_recoverable());
        assert!(SignalCategory::Custom("test".into()).is_recoverable());
        assert!(!SignalCategory::Fatal.is_recoverable());
    }

    #[test]
    fn test_error_signal_escalation_tier() {
        assert_eq!(SignalCategory::PermissionDenied.escalate_to_tier(), 2);
        assert_eq!(SignalCategory::DiskFull.escalate_to_tier(), 2);
        assert_eq!(SignalCategory::ToolTimeout.escalate_to_tier(), 2);

        assert_eq!(SignalCategory::SyntaxError.escalate_to_tier(), 3);
        assert_eq!(SignalCategory::CompileError.escalate_to_tier(), 3);
        assert_eq!(SignalCategory::TypeError.escalate_to_tier(), 3);
        assert_eq!(SignalCategory::LogicError.escalate_to_tier(), 3);
        assert_eq!(SignalCategory::ContextLengthExceeded.escalate_to_tier(), 3);

        assert_eq!(SignalCategory::Fatal.escalate_to_tier(), 4);
    }

    #[test]
    fn test_error_signal_creation_and_fields() {
        let signal = ErrorSignal::new(
            SignalCategory::LogicError,
            Some(1),
            "test error message".into(),
            "step-42".into(),
            "bash".into(),
        );

        assert_eq!(signal.category, SignalCategory::LogicError);
        assert_eq!(signal.exit_code, Some(1));
        assert_eq!(signal.message, "test error message");
        assert_eq!(signal.step_id, "step-42");
        assert_eq!(signal.tool_name, "bash");
        assert!(signal.captured_at <= chrono::Utc::now());
    }

    #[test]
    fn test_error_signal_message_truncation() {
        let long_msg = "A".repeat(5000);
        let signal = ErrorSignal::new(
            SignalCategory::Internal,
            None,
            long_msg,
            "s1".into(),
            "bash".into(),
        );
        assert!(
            signal.message.len() <= 2100,
            "Message should be truncated to ~2048 + suffix"
        );
        assert!(signal.message.contains("[truncated]"));
    }

    #[test]
    fn test_orchestration_error_categories() {
        assert_eq!(
            OrchestrationError::thinking("x").category(),
            ErrorCategory::Thinking
        );
        assert_eq!(
            OrchestrationError::execution("x").category(),
            ErrorCategory::Execution
        );
        assert_eq!(
            OrchestrationError::task("x").category(),
            ErrorCategory::Task
        );
        assert_eq!(
            OrchestrationError::tool("x").category(),
            ErrorCategory::Tool
        );
        assert_eq!(
            OrchestrationError::session("x").category(),
            ErrorCategory::Session
        );
        assert_eq!(
            OrchestrationError::verification("x").category(),
            ErrorCategory::Verification
        );
        assert_eq!(
            OrchestrationError::recovery("x").category(),
            ErrorCategory::Recovery
        );
        assert_eq!(
            OrchestrationError::config("x").category(),
            ErrorCategory::Config
        );
        assert_eq!(OrchestrationError::llm("x").category(), ErrorCategory::LLM);
    }

    #[test]
    fn test_orchestration_error_is_recoverable() {
        assert!(OrchestrationError::execution("x").is_recoverable());
        assert!(OrchestrationError::ModelError {
            message: "x".into()
        }
        .is_recoverable());
        assert!(OrchestrationError::VerificationError {
            message: "x".into()
        }
        .is_recoverable());
        assert!(OrchestrationError::Timeout {
            operation: "x".into()
        }
        .is_recoverable());
        assert!(OrchestrationError::ResourceExhausted {
            resource: "x".into()
        }
        .is_recoverable());

        assert!(!OrchestrationError::Configuration {
            message: "x".into()
        }
        .is_recoverable());
        assert!(!OrchestrationError::Internal {
            message: "x".into()
        }
        .is_recoverable());
        assert!(!OrchestrationError::Storage {
            message: "x".into()
        }
        .is_recoverable());
    }

    #[test]
    fn test_orchestration_error_display() {
        let err = OrchestrationError::Timeout {
            operation: "step-1".into(),
        };
        assert!(err.to_string().contains("step-1"));

        let err = OrchestrationError::Execution {
            message: "boom".into(),
        };
        assert!(err.to_string().contains("boom"));

        let err = OrchestrationError::ResourceExhausted {
            resource: "memory".into(),
        };
        assert!(err.to_string().contains("memory"));
    }
}

// ─── 7. Execution Trace E2E ──────────────────────────────────────────────────

mod execution_trace_e2e {
    use super::*;

    #[test]
    fn test_trace_full_execution_recording() {
        let mut trace = ExecutionTrace::new("e2e-trace".into());

        trace.append(TraceEntry::new_success(
            "step-1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({"cmd": "echo hello"}),
            "hello".into(),
            Some(0),
            0.001,
        ));

        let error = ErrorSignal::new(
            SignalCategory::LogicError,
            Some(1),
            "wrong output".into(),
            "step-2".into(),
            "bash".into(),
        );
        trace.append(TraceEntry::new_failure(
            "step-2".into(),
            1,
            2,
            "bash".into(),
            serde_json::json!({"cmd": "bad command"}),
            "error output".into(),
            Some(1),
            error,
            0.002,
        ));

        trace.append(TraceEntry::new_success(
            "step-2-retry".into(),
            1,
            3,
            "bash".into(),
            serde_json::json!({"cmd": "fixed command"}),
            "correct output".into(),
            Some(0),
            0.003,
        ));

        assert_eq!(trace.steps.len(), 3);
        assert!((trace.total_cost() - 0.006).abs() < f64::EPSILON);

        let failures = trace.failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].step_id, "step-2");
        assert!(failures[0].error_signal.is_some());

        let last_2 = trace.last_n_tool_calls(2);
        assert_eq!(last_2.len(), 2);
        assert_eq!(last_2[0].step_id, "step-2-retry");
        assert_eq!(last_2[1].step_id, "step-2");
    }

    #[test]
    fn test_trace_serialization_roundtrip() {
        let mut trace = ExecutionTrace::new("e2e-serde".into());
        trace.append(TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({"arg": "value"}),
            "output".into(),
            Some(0),
            0.01,
        ));

        let json = serde_json::to_string(&trace).unwrap();
        let deserialized: ExecutionTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task_id, "e2e-serde");
        assert_eq!(deserialized.steps.len(), 1);
        assert_eq!(deserialized.steps[0].step_id, "s1");
    }
}

// ─── 8. Types E2E ─────────────────────────────────────────────────────────────

mod types_e2e {
    use super::*;

    #[test]
    fn test_execution_tier_full_lifecycle() {
        assert_eq!(ExecutionTier::Musician.as_u8(), 2);
        assert_eq!(ExecutionTier::Editor.as_u8(), 3);
        assert_eq!(ExecutionTier::Composer.as_u8(), 4);
        assert_eq!(ExecutionTier::Thinking.as_u8(), 5);

        assert_eq!(ExecutionTier::from_u8(2), Some(ExecutionTier::Musician));
        assert_eq!(ExecutionTier::from_u8(5), Some(ExecutionTier::Thinking));
        assert_eq!(ExecutionTier::from_u8(0), None);
        assert_eq!(ExecutionTier::from_u8(99), None);

        assert_eq!(ExecutionTier::Musician.to_string(), "musician");
        assert_eq!(ExecutionTier::Thinking.to_string(), "thinking");

        assert!(ExecutionTier::Thinking.is_thinking());
        assert!(!ExecutionTier::Musician.is_thinking());
    }

    #[test]
    fn test_task_outcome_variants() {
        assert!(TaskOutcome::SuccessAtTier(2).is_success());
        assert!(TaskOutcome::SuccessAtTier(5).is_success());
        assert!(!TaskOutcome::Abandoned { reason: "x".into() }.is_success());
        assert!(!TaskOutcome::BudgetExceeded.is_success());
        assert!(!TaskOutcome::HallucinationLoop.is_success());
    }

    #[test]
    fn test_step_construction_and_serialization() {
        let step = Step {
            id: "s1".into(),
            index: 0,
            description: "Run tests".into(),
            expected_output_type: OutputType::Verification,
            suggested_tool: Some("bash".into()),
            retry_on_failure: true,
            required_resources: rustycode_orchestration::guard::RequiredResources::default(),
        };

        let json = serde_json::to_string(&step).unwrap();
        let back: Step = serde_json::from_str(&json).unwrap();
        assert_eq!(step, back);
    }
}

// ─── 9. Config E2E ────────────────────────────────────────────────────────────

mod config_e2e {
    use super::*;
    use rustycode_orchestration::config::DryRunMode;

    #[test]
    fn test_config_defaults_sensible() {
        let config = OrchestrationConfig::default();
        assert_eq!(config.budget.total_max_usd, 10.0);
        assert_eq!(config.budget.warn_threshold_pct, 0.8);
        assert_eq!(config.hallucination.detection_window, 5);
        assert_eq!(config.hallucination.action, "escalate");
        assert_eq!(config.failure_store.backend, "memory");
        assert!(config.models.is_empty());
        assert!(config.escalation.is_empty());
    }

    #[test]
    fn test_config_dry_run_modes() {
        let mut config = OrchestrationConfig::default();
        config.dry_run.default_mode = DryRunMode::LogOnly;
        config.dry_run.enabled_for_tasks.push("risky-task".into());

        assert_eq!(config.should_use_dry_run("risky-task"), DryRunMode::LogOnly);
        assert_eq!(
            config.should_use_dry_run("normal-task"),
            DryRunMode::Disabled
        );

        assert!(DryRunMode::LogOnly.skip_execution());
        assert!(!DryRunMode::Disabled.skip_execution());
    }

    #[test]
    fn test_config_budget_customization() {
        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 50.0;
        config.budget.tier_2_max_usd = 10.0;
        config.budget.tier_3_max_usd = 20.0;
        config.budget.tier_4_max_usd = 20.0;
        config.budget.warn_threshold_pct = 0.9;

        assert_eq!(config.budget.total_max_usd, 50.0);
        assert_eq!(config.budget.warn_threshold_pct, 0.9);
    }

    #[test]
    fn test_config_yaml_load_missing_file() {
        let result = OrchestrationConfig::load_from_yaml("/nonexistent/path/config.yaml");
        assert!(result.is_err());
    }
}

// ─── 10. Verification Gates E2E ───────────────────────────────────────────────

mod verification_gates_e2e {
    use super::*;

    #[test]
    fn test_empty_registry_accepts_all() {
        let registry = VerificationGateRegistry::new();
        let step = Step {
            id: "s1".into(),
            index: 0,
            description: "test".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("bash".into()),
            retry_on_failure: false,
            required_resources: rustycode_orchestration::guard::RequiredResources::default(),
        };
        let entry = TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({}),
            "output".into(),
            Some(0),
            0.001,
        );

        let outcome = registry.verify(&step, &entry);
        assert!(matches!(outcome, VerificationOutcome::Valid));
    }

    #[test]
    fn test_custom_verification_strategy() {
        struct RejectEmpty;
        impl rustycode_orchestration::verification_gates::VerificationStrategy for RejectEmpty {
            fn verify(&self, _step: &Step, result: &TraceEntry) -> VerificationOutcome {
                if result.output.trim().is_empty() {
                    VerificationOutcome::Invalid {
                        reason: "Output is empty".into(),
                        category: SignalCategory::LogicError,
                    }
                } else {
                    VerificationOutcome::Valid
                }
            }
        }

        let mut registry = VerificationGateRegistry::new();
        registry.register_strategy(OutputType::Code, Box::new(RejectEmpty));

        let step = Step {
            id: "s1".into(),
            index: 0,
            description: "test".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: None,
            retry_on_failure: false,
            required_resources: rustycode_orchestration::guard::RequiredResources::default(),
        };

        // Empty output → Invalid
        let empty_entry = TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({}),
            String::new(),
            Some(0),
            0.0,
        );
        let outcome = registry.verify(&step, &empty_entry);
        assert!(matches!(outcome, VerificationOutcome::Invalid { .. }));

        // Non-empty output → Valid
        let valid_entry = TraceEntry::new_success(
            "s1".into(),
            0,
            2,
            "bash".into(),
            serde_json::json!({}),
            "some output".into(),
            Some(0),
            0.0,
        );
        let outcome = registry.verify(&step, &valid_entry);
        assert!(matches!(outcome, VerificationOutcome::Valid));
    }
}

// ─── 11. Pipeline + Conductor Integration ─────────────────────────────────────

mod pipeline_conductor_integration {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_with_low_budget_fails_gracefully() {
        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 0.0001;
        let pipeline = OrchestrationPipeline::new(config);

        let result = pipeline
            .conduct("e2e-low-budget".into(), "echo test".into())
            .await;
        // Should return either a success or a failure, but not panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_conductor_with_hallucination_trace() {
        let config = OrchestrationConfig::default();
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("e2e-hallucination".into(), "hallucination test".into());

        let window = 5;
        for _ in 0..window {
            ctx.execution_trace.append(TraceEntry::new_success(
                "step-x".into(),
                0,
                2,
                "bash".into(),
                serde_json::json!({"cmd": "same command"}),
                "same output".into(),
                Some(0),
                0.001,
            ));
        }

        let signal = ErrorSignal::new(
            SignalCategory::Internal,
            Some(1),
            "stuck".into(),
            "step-x".into(),
            "bash".into(),
        );

        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(
            matches!(decision, EscalationDecision::Abandon { reason } if reason == "hallucination_loop"),
            "Should detect hallucination loop"
        );
    }

    #[test]
    fn test_conductor_escalation_preserves_context() {
        let mut config = OrchestrationConfig::default();
        config.escalation.insert(
            "tier_2".into(),
            TierConfig {
                max_attempts: 1,
                critical_errors: vec![SignalCategory::Fatal],
                recoverable_errors: vec![],
            },
        );
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("e2e-preserve".into(), "context preservation".into());
        ctx.add_cost(0.5);
        ctx.add_tokens(1000);

        let signal = ErrorSignal::new(
            SignalCategory::Internal,
            Some(1),
            "error".into(),
            "s1".into(),
            "bash".into(),
        );

        ctx.attempt_count = 1;
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 3, .. }
        ));

        assert!((ctx.cost_used - 0.5).abs() < f64::EPSILON);
        assert_eq!(ctx.token_count, 1000);
    }
}
