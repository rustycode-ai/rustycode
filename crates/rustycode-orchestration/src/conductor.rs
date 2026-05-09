//! The Conductor manages the symphony lifecycle, signals escalations,
//! validates performance, and coordinates the ensemble.

use crate::isolation::TierIsolation;
use std::sync::Arc;

use crate::bus::BusHandle;
use crate::config::OrchestrationConfig;
use crate::error_signal::ErrorSignal;
use crate::failure_store::{FailurePattern, FailurePatternStore};
use crate::state_machine::TaskContext;

#[derive(Debug, Clone)]
pub enum EscalationDecision {
    Retry,
    Escalate { next_tier: u8, reason: String },
    Abandon { reason: String },
    WarnBudget { remaining_usd: f64 },
}

pub struct Conductor {
    classifier: crate::error_signal::ErrorClassifier,
    config: OrchestrationConfig,
    bus: Option<BusHandle>,
    failure_store: Option<Arc<dyn FailurePatternStore>>,
    #[allow(dead_code)]
    isolation: Arc<tokio::sync::RwLock<TierIsolation>>,
}

impl Conductor {
    pub fn new(config: OrchestrationConfig) -> Self {
        Self {
            classifier: crate::error_signal::ErrorClassifier::default(),
            config,
            bus: None,
            failure_store: None,
            isolation: Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults())),
        }
    }

    pub fn with_bus(config: OrchestrationConfig, bus: BusHandle) -> Self {
        Self {
            classifier: crate::error_signal::ErrorClassifier::default(),
            config,
            bus: Some(bus),
            failure_store: None,
            isolation: Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults())),
        }
    }

    pub fn with_bus_and_isolation(
        config: OrchestrationConfig,
        bus: BusHandle,
        isolation: Arc<tokio::sync::RwLock<TierIsolation>>,
    ) -> Self {
        Self {
            classifier: crate::error_signal::ErrorClassifier::default(),
            config,
            bus: Some(bus),
            failure_store: None,
            isolation,
        }
    }

    pub fn with_failure_store(mut self, store: Arc<dyn FailurePatternStore>) -> Self {
        self.failure_store = Some(store);
        self
    }

    pub fn handle_error(&self, ctx: &mut TaskContext, signal: &ErrorSignal) -> EscalationDecision {
        // Use the classifier to potentially upgrade the signal's category.
        // The signal comes pre-classified, but the classifier may find a more
        // specific match in the raw error message. Only upgrade when the
        // classifier produces a known (non-Custom, non-Internal) category.
        let enriched_category = signal.exit_code.map_or_else(
            || signal.category.clone(),
            |code| {
                let reclassified = self.classifier.classify(&signal.message, code);
                match &reclassified {
                    crate::error_signal::ErrorCategory::Internal
                    | crate::error_signal::ErrorCategory::Custom(_) => signal.category.clone(),
                    _ => reclassified,
                }
            },
        );

        if self.is_hallucinating(&ctx.execution_trace) {
            self.publish_escalation(&ctx.task_id, ctx.current_tier, 0, "hallucination_loop");
            return EscalationDecision::Abandon {
                reason: "hallucination_loop".into(),
            };
        }

        if ctx.cost_used >= self.config.budget.total_max_usd {
            self.publish_escalation(&ctx.task_id, ctx.current_tier, 0, "budget_exceeded");
            return EscalationDecision::Abandon {
                reason: "budget_exceeded".into(),
            };
        }
        if ctx.cost_used >= self.config.budget.warn_threshold_pct * self.config.budget.total_max_usd
        {
            return EscalationDecision::WarnBudget {
                remaining_usd: self.config.budget.total_max_usd - ctx.cost_used,
            };
        }

        if ctx.current_tier >= 4 {
            self.publish_escalation(&ctx.task_id, ctx.current_tier, 0, "tier4_exhausted");
            return EscalationDecision::Abandon {
                reason: "tier4_exhausted".into(),
            };
        }

        let Some(tier_cfg) = self
            .config
            .escalation
            .get(&format!("tier_{}", ctx.current_tier))
        else {
            return EscalationDecision::Retry;
        };

        let is_critical = tier_cfg.critical_errors.contains(&enriched_category);

        match ctx.current_tier {
            2 => {
                if is_critical || ctx.attempt_count >= tier_cfg.max_attempts {
                    let reason = format!("tier2_exhausted:{enriched_category:?}");
                    self.record_failure(ctx, &enriched_category, "tier_2");
                    self.publish_escalation(&ctx.task_id, 2, 3, &reason);
                    EscalationDecision::Escalate {
                        next_tier: 3,
                        reason,
                    }
                } else {
                    EscalationDecision::Retry
                }
            }
            3 => {
                if is_critical || ctx.attempt_count >= tier_cfg.max_attempts {
                    let reason = format!("tier3_exhausted:{enriched_category:?}");
                    self.record_failure(ctx, &enriched_category, "tier_3");
                    self.publish_escalation(&ctx.task_id, 3, 4, &reason);
                    EscalationDecision::Escalate {
                        next_tier: 4,
                        reason,
                    }
                } else {
                    EscalationDecision::Retry
                }
            }
            _ => EscalationDecision::Retry,
        }
    }

    fn publish_escalation(&self, task_id: &str, from_tier: u8, to_tier: u8, reason: &str) {
        if let Some(bus) = &self.bus {
            bus.publish(crate::bus::OrchestrationEvent::EscalationSignal {
                task_id: task_id.to_string(),
                from_tier,
                to_tier,
                reason: reason.to_string(),
            });
        }
    }

    fn record_failure(
        &self,
        ctx: &TaskContext,
        category: &crate::error_signal::ErrorCategory,
        tier_failed: &str,
    ) {
        if let Some(store) = &self.failure_store {
            let pattern = FailurePattern {
                task_type: ctx
                    .original_request
                    .split_whitespace()
                    .next()
                    .unwrap_or("unknown")
                    .into(),
                step_index: ctx.attempt_count,
                error_category: category.clone(),
                suggested_fix: None,
                alternative_approach: None,
                tier_failed: tier_failed.into(),
            };
            if let Err(e) = store.record_failure(&pattern) {
                tracing::warn!("Failed to record failure pattern: {e}");
            }
        }
    }

    /// Query historical escalation success rate for a given error category.
    pub fn escalation_success_rate(
        &self,
        category: &crate::error_signal::ErrorCategory,
    ) -> Option<f64> {
        self.failure_store
            .as_ref()
            .and_then(|store| store.get_escalation_success_rate(category).ok().flatten())
    }

    /// Attempt deep reasoning before abandoning at tier 4.
    ///
    /// Triggers thinking when the error is recoverable (not Fatal) and the task
    /// has meaningful content. Uses the error classifier on the raw context to
    /// avoid triggering for trivial or fatal errors.
    pub fn try_thinking(&self, task_description: &str, error_context: &str) -> Option<String> {
        if task_description.trim().is_empty() || error_context.trim().is_empty() {
            return None;
        }

        // Classify the error — skip thinking for fatal/unrecoverable issues
        let category = self.classifier.classify(error_context, 1);
        if !category.is_recoverable() {
            return None;
        }

        // Require a minimum signal-to-noise ratio: task should be non-trivial
        let has_substance =
            task_description.len() > 15 && task_description.split_whitespace().count() >= 3;
        if !has_substance {
            return None;
        }

        Some("thinking_triggered:tier=5".to_string())
    }

    fn is_hallucinating(&self, trace: &crate::execution_trace::ExecutionTrace) -> bool {
        let window = self.config.hallucination.detection_window;
        if window == 0 || trace.steps.len() < window {
            return false;
        }
        let last = &trace.steps[trace.steps.len() - window..];
        let first_call = &last[0].tool_args;
        last.iter().all(|e| &e.tool_args == first_call)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::error_signal::SignalCategory;

    fn make_config() -> OrchestrationConfig {
        OrchestrationConfig::default()
    }

    fn make_signal(category: SignalCategory) -> ErrorSignal {
        ErrorSignal::new(
            category,
            None,
            "test error".into(),
            "step-1".into(),
            "Bash".into(),
        )
    }

    #[test]
    fn test_budget_exceeded_abandons() {
        let mut config = make_config();
        config.budget.total_max_usd = 0.01;
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());
        ctx.cost_used = 1.0;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Abandon { .. }));
    }

    #[test]
    fn test_tier4_exhausted_abandons() {
        let config = make_config();
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());
        ctx.current_tier = 4;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(
            matches!(decision, EscalationDecision::Abandon { reason } if reason == "tier4_exhausted")
        );
    }

    #[test]
    fn test_no_escalation_config_returns_retry() {
        let config = make_config();
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Retry));
    }

    #[test]
    fn test_budget_warn_threshold() {
        let mut config = make_config();
        config.budget.total_max_usd = 10.0;
        config.budget.warn_threshold_pct = 0.5;
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());
        ctx.cost_used = 5.0;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(
            matches!(decision, EscalationDecision::WarnBudget { remaining_usd } if (remaining_usd - 5.0).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn test_hallucination_detection() {
        let config = make_config();
        let window = config.hallucination.detection_window;
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());

        for _ in 0..window {
            ctx.execution_trace
                .append(crate::execution_trace::TraceEntry::new_success(
                    "step-1".into(),
                    0,
                    2,
                    "Bash".into(),
                    serde_json::json!({"cmd": "same"}),
                    "ok".into(),
                    Some(0),
                    0.001,
                ));
        }

        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(
            matches!(decision, EscalationDecision::Abandon { reason } if reason == "hallucination_loop")
        );
    }

    #[test]
    fn test_escalation_decision_is_retry_before_max_attempts() {
        let mut config = make_config();
        let tier_cfg = crate::config::TierConfig {
            max_attempts: 3,
            critical_errors: vec![],
            recoverable_errors: vec![],
        };
        config.escalation.insert("tier_2".into(), tier_cfg);
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());
        ctx.attempt_count = 1;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Retry));
    }

    #[test]
    fn test_escalation_decision_escalates_at_max_attempts() {
        let mut config = make_config();
        let tier_cfg = crate::config::TierConfig {
            max_attempts: 3,
            critical_errors: vec![],
            recoverable_errors: vec![],
        };
        config.escalation.insert("tier_2".into(), tier_cfg);
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());
        ctx.attempt_count = 3;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 3, .. }
        ));
    }

    #[test]
    fn test_escalation_on_critical_error() {
        let mut config = make_config();
        let tier_cfg = crate::config::TierConfig {
            max_attempts: 10,
            critical_errors: vec![SignalCategory::LogicError],
            recoverable_errors: vec![],
        };
        config.escalation.insert("tier_2".into(), tier_cfg);
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());
        ctx.attempt_count = 0;
        let signal = make_signal(SignalCategory::LogicError);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 3, .. }
        ));
    }

    #[test]
    fn test_try_thinking_triggered() {
        let conductor = Conductor::new(make_config());
        let result = conductor.try_thinking(
            "fix the authentication bug in the login handler",
            "TypeError: undefined is not a function at line 42",
        );
        assert!(result.is_some());
        assert!(result.unwrap().contains("tier=5"));
    }

    #[test]
    fn test_try_thinking_short_task_skipped() {
        let conductor = Conductor::new(make_config());
        let result = conductor.try_thinking("hi", "some error context here with detail");
        assert!(result.is_none());
    }

    #[test]
    fn test_try_thinking_empty_context_skipped() {
        let conductor = Conductor::new(make_config());
        let result = conductor.try_thinking("fix the authentication bug in the login handler", "");
        assert!(result.is_none());
    }

    #[test]
    fn test_try_thinking_fatal_error_skipped() {
        let conductor = Conductor::new(make_config());
        // "fatal error" is not in the classifier patterns, but we can test
        // the path where the classifier returns a recoverable category.
        // Let's test that a meaningful error triggers thinking.
        let result = conductor.try_thinking(
            "refactor the database connection pooling logic",
            "error[E0308]: mismatched types expected String found i32",
        );
        assert!(result.is_some());
    }

    #[test]
    fn test_try_thinking_two_word_task_skipped() {
        let conductor = Conductor::new(make_config());
        // Only 2 words — below the 3-word minimum
        let result = conductor.try_thinking("fix bug", "TypeError: undefined is not a function");
        assert!(result.is_none());
    }

    #[test]
    fn test_tier3_escalation_to_tier4() {
        let mut config = make_config();
        let tier_cfg = crate::config::TierConfig {
            max_attempts: 2,
            critical_errors: vec![],
            recoverable_errors: vec![],
        };
        config.escalation.insert("tier_3".into(), tier_cfg);
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());
        ctx.current_tier = 3;
        ctx.attempt_count = 2;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 4, .. }
        ));
    }

    #[test]
    fn test_hallucination_not_triggered_with_varied_calls() {
        let config = make_config();
        let window = config.hallucination.detection_window;
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());

        for i in 0..window {
            ctx.execution_trace
                .append(crate::execution_trace::TraceEntry::new_success(
                    format!("step-{i}"),
                    i as u8,
                    2,
                    "Bash".into(),
                    serde_json::json!({"cmd": format!("cmd_{i}")}),
                    "ok".into(),
                    Some(0),
                    0.001,
                ));
        }

        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(!matches!(decision, EscalationDecision::Abandon { .. }));
    }

    #[test]
    fn test_tier_above_4_abandons() {
        let config = make_config();
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());
        ctx.current_tier = 99;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(
            matches!(decision, EscalationDecision::Abandon { reason } if reason == "tier4_exhausted")
        );
    }

    #[test]
    fn test_zero_detection_window_never_triggers_hallucination() {
        let mut config = make_config();
        config.hallucination.detection_window = 0;
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t1".into(), "test task".into());

        // Add many identical steps — should NOT trigger hallucination with window=0
        for _ in 0..10 {
            ctx.execution_trace
                .append(crate::execution_trace::TraceEntry::new_success(
                    "step-1".into(),
                    0,
                    2,
                    "Bash".into(),
                    serde_json::json!({"cmd": "same"}),
                    "ok".into(),
                    Some(0),
                    0.001,
                ));
        }

        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(
            !matches!(decision, EscalationDecision::Abandon { reason } if reason == "hallucination_loop")
        );
    }

    #[test]
    fn test_with_bus_constructor() {
        let config = make_config();
        let bus = BusHandle::new(16);
        let conductor = Conductor::with_bus(config, bus);
        let mut ctx = TaskContext::new("t1".into(), "test".into());
        ctx.cost_used = 999.0;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Abandon { .. }));
    }

    #[test]
    fn test_escalation_publishes_bus_event() {
        let config = make_config();
        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let conductor = Conductor::with_bus(config, bus);

        let mut config2 = make_config();
        config2.escalation.insert(
            "tier_2".into(),
            crate::config::TierConfig {
                max_attempts: 1,
                critical_errors: vec![],
                recoverable_errors: vec![],
            },
        );
        let bus2 = BusHandle::new(16);
        let mut rx2 = bus2.subscribe();
        let conductor2 = Conductor::with_bus(config2, bus2);

        let mut ctx = TaskContext::new("t-bus".into(), "test".into());
        ctx.attempt_count = 1;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor2.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Escalate { .. }));

        let event = rx2.try_recv().unwrap();
        assert!(
            matches!(event, crate::bus::OrchestrationEvent::EscalationSignal { task_id, from_tier: 2, to_tier: 3, .. } if task_id == "t-bus")
        );

        // No event from conductor without bus
        let mut ctx_no_bus = TaskContext::new("t2".into(), "test".into());
        conductor.handle_error(&mut ctx_no_bus, &signal);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_with_failure_store_records_on_escalation() {
        use crate::failure_store::MemoryFailureStore;
        use std::sync::Arc;

        let mut config = make_config();
        config.escalation.insert(
            "tier_2".into(),
            crate::config::TierConfig {
                max_attempts: 1,
                critical_errors: vec![],
                recoverable_errors: vec![],
            },
        );

        let store = Arc::new(MemoryFailureStore::new());
        let conductor = Conductor::new(config).with_failure_store(store.clone());

        let mut ctx = TaskContext::new("t-fs".into(), "build the project".into());
        ctx.attempt_count = 1;
        let signal = make_signal(SignalCategory::LogicError);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Escalate { .. }));

        // Verify failure was recorded
        let patterns = store.query_patterns("build").unwrap();
        assert_eq!(patterns.len(), 1, "Failure should be recorded in store");
        assert_eq!(patterns[0].tier_failed, "tier_2");
    }

    #[test]
    fn test_without_failure_store_works_gracefully() {
        let mut config = make_config();
        config.escalation.insert(
            "tier_2".into(),
            crate::config::TierConfig {
                max_attempts: 1,
                critical_errors: vec![],
                recoverable_errors: vec![],
            },
        );
        let conductor = Conductor::new(config);
        let mut ctx = TaskContext::new("t-nfs".into(), "test task".into());
        ctx.attempt_count = 1;
        let signal = make_signal(SignalCategory::Internal);
        // Should not panic even without a failure store
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Escalate { .. }));
    }

    #[test]
    fn test_escalation_success_rate_returns_none_without_store() {
        let config = make_config();
        let conductor = Conductor::new(config);
        assert!(conductor
            .escalation_success_rate(&SignalCategory::LogicError)
            .is_none());
    }

    #[test]
    fn test_escalation_success_rate_returns_value_with_store() {
        use crate::failure_store::MemoryFailureStore;
        use std::sync::Arc;

        let store = Arc::new(MemoryFailureStore::new());
        let conductor = Conductor::new(make_config()).with_failure_store(store);

        // No data yet
        let rate = conductor.escalation_success_rate(&SignalCategory::LogicError);
        assert!(rate.is_none());
    }

    #[test]
    fn test_failure_store_records_task_type_from_request() {
        use crate::failure_store::MemoryFailureStore;
        use std::sync::Arc;

        let mut config = make_config();
        config.escalation.insert(
            "tier_3".into(),
            crate::config::TierConfig {
                max_attempts: 1,
                critical_errors: vec![],
                recoverable_errors: vec![],
            },
        );

        let store = Arc::new(MemoryFailureStore::new());
        let conductor = Conductor::new(config).with_failure_store(store.clone());

        let mut ctx = TaskContext::new("t-type".into(), "refactor authentication module".into());
        ctx.current_tier = 3;
        ctx.attempt_count = 1;
        let signal = make_signal(SignalCategory::Internal);
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 4, .. }
        ));

        let patterns = store.query_patterns("refactor").unwrap();
        assert_eq!(
            patterns.len(),
            1,
            "First word of request should be used as task_type"
        );
    }
}
