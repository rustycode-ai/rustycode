//! Supervisor layer for orchestration strategy control.
//!
//! The supervisor watches a stream of events and converts them into strategy
//! directives. It does not execute work itself — it observes progress and
//! recommends when the task should be re-shaped (expand scope, re-plan,
//! explore alternatives, etc.).

use rustycode_protocol::ExecutionPhase;
use serde::{Deserialize, Serialize};

/// Strategy directive emitted by the supervisor.
///
/// These are recommendations with authority — the orchestration engine applies
/// them deterministically (Phase 1: advisory logging only).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SupervisionDirective {
    Continue,
    ExpandScope {
        allowed_tools: Vec<String>,
        reason: String,
    },
    ReviseScope {
        reduced_goal: String,
        reason: String,
    },
    ExploreAlternatives {
        branches: u8,
        reason: String,
    },
    Replan {
        reason: String,
    },
    EscalateTier {
        to_tier: u8,
        reason: String,
    },
    PauseForReview {
        reason: String,
    },
}

/// Normalized event consumed by the supervisor.
///
/// Intentionally small — a few strong signals rather than full runtime state.
#[derive(Debug, Clone)]
pub enum SupervisionEvent {
    ToolStarted {
        tool: String,
    },
    ToolFinished {
        tool: String,
        success: bool,
    },
    ToolFailed {
        tool: String,
        error: String,
    },
    PhaseChanged {
        from: ExecutionPhase,
        to: ExecutionPhase,
    },
    TierChanged {
        from: u8,
        to: u8,
    },
    ScopeChanged {
        active_tools: Vec<String>,
    },
    BudgetWarning {
        remaining_usd: f64,
    },
    QualitySignal {
        score: f64,
        details: String,
    },
}

/// Snapshot of task state for periodic reconciliation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub task_id: String,
    pub current_phase: String,
    pub current_tier: u8,
    pub cost_used: f64,
    pub budget_remaining: f64,
    pub attempt_count: u8,
    pub consecutive_failures: u8,
    pub steps_completed: usize,
    pub active_tools: Vec<String>,
}

/// Supervisor trait — strategy controller for task execution.
///
/// `observe()` handles immediate reactions to events.
/// `reconcile()` handles slower periodic review of task state.
pub trait Supervisor: Send + Sync {
    fn observe(&mut self, event: &SupervisionEvent) -> Option<SupervisionDirective>;
    fn reconcile(&mut self, ctx: &TaskSnapshot) -> SupervisionDirective;
    fn consecutive_failure_count(&self) -> u8;
}

/// Default thresholds for the rule-based supervisor.
const DEFAULT_MAX_CONSECUTIVE_FAILURES: u8 = 3;
const DEFAULT_BUDGET_PAUSE_PCT: f64 = 0.20;
const DEFAULT_QUALITY_WINDOW: usize = 5;
const DEFAULT_FAILURE_HISTORY_LEN: usize = 10;

/// Rule-based supervisor implementing the decision rules from the spec.
///
/// This is intentionally conservative: it intervenes only when it has a clear
/// reason to change the shape of the work.
pub struct RuleBasedSupervisor {
    consecutive_failures: u8,
    failure_history: Vec<(String, String)>,
    quality_scores: Vec<f64>,
    active_tools: Vec<String>,
    last_directive_kind: Option<String>,
    max_consecutive_failures: u8,
    budget_pause_pct: f64,
}

impl RuleBasedSupervisor {
    pub fn new() -> Self {
        Self {
            consecutive_failures: 0,
            failure_history: Vec::with_capacity(DEFAULT_FAILURE_HISTORY_LEN),
            quality_scores: Vec::with_capacity(DEFAULT_QUALITY_WINDOW),
            active_tools: Vec::new(),
            last_directive_kind: None,
            max_consecutive_failures: DEFAULT_MAX_CONSECUTIVE_FAILURES,
            budget_pause_pct: DEFAULT_BUDGET_PAUSE_PCT,
        }
    }

    /// Current consecutive failure count (for snapshot building).
    pub const fn consecutive_failure_count(&self) -> u8 {
        self.consecutive_failures
    }

    fn record_failure(&mut self, tool: String, error: String) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.failure_history.len() >= DEFAULT_FAILURE_HISTORY_LEN {
            self.failure_history.remove(0);
        }
        self.failure_history.push((tool, error));
    }

    const fn reset_failures(&mut self) {
        self.consecutive_failures = 0;
    }

    fn avg_quality(&self) -> f64 {
        if self.quality_scores.is_empty() {
            return 1.0;
        }
        #[allow(clippy::cast_precision_loss)] // quality window ≤ 5, no precision loss possible
        let avg = self.quality_scores.iter().sum::<f64>() / self.quality_scores.len() as f64;
        avg
    }

    fn is_same_directive(&self, kind: &str) -> bool {
        self.last_directive_kind.as_deref() == Some(kind)
    }

    /// Detect architectural failures — same tool failing with similar errors.
    fn has_architectural_failure_pattern(&self) -> bool {
        if self.failure_history.len() < 2 {
            return false;
        }
        let len = self.failure_history.len();
        let recent = &self.failure_history[len.saturating_sub(3)..];
        let tools: Vec<&str> = recent.iter().map(|(t, _)| t.as_str()).collect();
        tools.iter().all(|&t| t == tools[0])
    }
}

impl Default for RuleBasedSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Supervisor for RuleBasedSupervisor {
    fn observe(&mut self, event: &SupervisionEvent) -> Option<SupervisionDirective> {
        match event {
            SupervisionEvent::ToolStarted { .. } => None,

            SupervisionEvent::ToolFinished { success: true, .. } => {
                self.reset_failures();
                None
            }

            SupervisionEvent::ToolFinished {
                success: false,
                tool,
            } => {
                self.record_failure(tool.clone(), "tool returned failure".into());
                if self.consecutive_failures >= self.max_consecutive_failures
                    && !self.is_same_directive("ExploreAlternatives")
                {
                    self.last_directive_kind = Some("ExploreAlternatives".into());
                    return Some(SupervisionDirective::ExploreAlternatives {
                        branches: 2,
                        reason: format!(
                            "tool '{tool}' failed {fails} times consecutively",
                            fails = self.consecutive_failures
                        ),
                    });
                }
                None
            }

            SupervisionEvent::ToolFailed { tool, error } => {
                self.record_failure(tool.clone(), error.clone());
                if self.has_architectural_failure_pattern()
                    && !self.is_same_directive("ExploreAlternatives")
                {
                    self.last_directive_kind = Some("ExploreAlternatives".into());
                    return Some(SupervisionDirective::ExploreAlternatives {
                        branches: 2,
                        reason: format!(
                            "architectural failure pattern detected: '{tool}' keeps failing"
                        ),
                    });
                }
                None
            }

            SupervisionEvent::PhaseChanged { from, to } => {
                tracing::debug!(
                    from = ?from, to = ?to,
                    "Supervisor observed phase change"
                );
                None
            }

            SupervisionEvent::TierChanged { from, to } => {
                tracing::debug!(from = from, to = to, "Supervisor observed tier change");
                None
            }

            SupervisionEvent::ScopeChanged { active_tools } => {
                self.active_tools.clone_from(active_tools);
                None
            }

            SupervisionEvent::BudgetWarning { remaining_usd } => {
                if !self.is_same_directive("PauseForReview") {
                    self.last_directive_kind = Some("PauseForReview".into());
                    return Some(SupervisionDirective::PauseForReview {
                        reason: format!("budget running low: ${remaining_usd:.2} remaining"),
                    });
                }
                None
            }

            SupervisionEvent::QualitySignal { score, details } => {
                if self.quality_scores.len() >= DEFAULT_QUALITY_WINDOW {
                    self.quality_scores.remove(0);
                }
                self.quality_scores.push(*score);

                if *score < 2.0 && !self.is_same_directive("Replan") {
                    self.last_directive_kind = Some("Replan".into());
                    return Some(SupervisionDirective::Replan {
                        reason: format!("quality score critically low ({score:.1}): {details}"),
                    });
                }
                None
            }
        }
    }

    fn reconcile(&mut self, ctx: &TaskSnapshot) -> SupervisionDirective {
        // Budget check — pause when budget is critically low
        if ctx.budget_remaining > 0.0 && ctx.cost_used > 0.0 {
            let budget_ratio = ctx.budget_remaining / (ctx.cost_used + ctx.budget_remaining);
            if budget_ratio < self.budget_pause_pct && !self.is_same_directive("PauseForReview") {
                self.last_directive_kind = Some("PauseForReview".into());
                return SupervisionDirective::PauseForReview {
                    reason: format!(
                        "budget nearly exhausted: ${rem:.2} of ${total:.2} remaining",
                        rem = ctx.budget_remaining,
                        total = ctx.cost_used + ctx.budget_remaining
                    ),
                };
            }
        }

        // Thrashing detection — high consecutive failures with no progress
        if ctx.consecutive_failures >= self.max_consecutive_failures
            && ctx.steps_completed > 0
            && !self.is_same_directive("Replan")
        {
            self.last_directive_kind = Some("Replan".into());
            return SupervisionDirective::Replan {
                reason: format!(
                    "thrashing detected: {} consecutive failures across {} steps",
                    ctx.consecutive_failures, ctx.steps_completed
                ),
            };
        }

        // Tier escalation — if attempts at current tier are exhausted
        if ctx.attempt_count >= 4 && !self.is_same_directive("EscalateTier") {
            let next_tier = ctx.current_tier.saturating_add(1).min(5);
            if next_tier > ctx.current_tier {
                self.last_directive_kind = Some("EscalateTier".into());
                return SupervisionDirective::EscalateTier {
                    to_tier: next_tier,
                    reason: format!(
                        "tier {} exhausted after {} attempts",
                        ctx.current_tier, ctx.attempt_count
                    ),
                };
            }
        }

        // Low quality trend — multiple below-threshold quality signals
        let avg_quality = self.avg_quality();
        if self.quality_scores.len() >= 3 && avg_quality < 3.5 && !self.is_same_directive("Replan")
        {
            self.last_directive_kind = Some("Replan".into());
            return SupervisionDirective::Replan {
                reason: format!(
                    "sustained low quality (avg {avg_quality:.1}) suggests wrong approach"
                ),
            };
        }

        SupervisionDirective::Continue
    }

    fn consecutive_failure_count(&self) -> u8 {
        self.consecutive_failures
    }
}

/// Translate an `OrchestrationEvent` into a `SupervisionEvent`, if applicable.
pub const fn translate_event(event: &crate::bus::OrchestrationEvent) -> Option<SupervisionEvent> {
    match event {
        crate::bus::OrchestrationEvent::EscalationSignal {
            from_tier, to_tier, ..
        } => Some(SupervisionEvent::TierChanged {
            from: *from_tier,
            to: *to_tier,
        }),

        crate::bus::OrchestrationEvent::PhaseTransition { from, to, .. } => {
            Some(SupervisionEvent::PhaseChanged {
                from: *from,
                to: *to,
            })
        }

        crate::bus::OrchestrationEvent::ContextBudgetExceeded { tier, .. } => {
            Some(SupervisionEvent::TierChanged {
                from: *tier,
                to: tier.saturating_add(1),
            })
        }

        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_snapshot() -> TaskSnapshot {
        TaskSnapshot {
            task_id: "t1".into(),
            current_phase: "executing".into(),
            current_tier: 2,
            cost_used: 1.0,
            budget_remaining: 9.0,
            attempt_count: 0,
            consecutive_failures: 0,
            steps_completed: 5,
            active_tools: vec!["Bash".into(), "read".into()],
        }
    }

    // --- Directive serialization ---

    #[test]
    fn directive_continue_roundtrip() {
        let d = SupervisionDirective::Continue;
        let json = serde_json::to_string(&d).unwrap();
        let back: SupervisionDirective = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn directive_expand_scope_roundtrip() {
        let d = SupervisionDirective::ExpandScope {
            allowed_tools: vec!["write".into(), "Bash".into()],
            reason: "need write access".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: SupervisionDirective = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn directive_replan_roundtrip() {
        let d = SupervisionDirective::Replan {
            reason: "approach wrong".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: SupervisionDirective = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn directive_escalate_tier_roundtrip() {
        let d = SupervisionDirective::EscalateTier {
            to_tier: 4,
            reason: "tier 3 exhausted".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: SupervisionDirective = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn directive_pause_for_review_roundtrip() {
        let d = SupervisionDirective::PauseForReview {
            reason: "budget low".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: SupervisionDirective = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn directive_explore_alternatives_roundtrip() {
        let d = SupervisionDirective::ExploreAlternatives {
            branches: 3,
            reason: "stuck".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: SupervisionDirective = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn directive_revise_scope_roundtrip() {
        let d = SupervisionDirective::ReviseScope {
            reduced_goal: "focus on auth only".into(),
            reason: "scope creep".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: SupervisionDirective = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    // --- observe ---

    #[test]
    fn observe_tool_started_returns_none() {
        let mut sup = RuleBasedSupervisor::new();
        let result = sup.observe(&SupervisionEvent::ToolStarted {
            tool: "Bash".into(),
        });
        assert!(result.is_none());
    }

    #[test]
    fn observe_tool_finished_success_resets_failures() {
        let mut sup = RuleBasedSupervisor::new();
        sup.record_failure("Bash".into(), "err".into());
        assert_eq!(sup.consecutive_failures, 1);

        let result = sup.observe(&SupervisionEvent::ToolFinished {
            tool: "Bash".into(),
            success: true,
        });
        assert!(result.is_none());
        assert_eq!(sup.consecutive_failures, 0);
    }

    #[test]
    fn observe_consecutive_tool_failures_triggers_explore_alternatives() {
        let mut sup = RuleBasedSupervisor::new();
        // First 2 failures: not enough
        sup.observe(&SupervisionEvent::ToolFinished {
            tool: "Bash".into(),
            success: false,
        });
        sup.observe(&SupervisionEvent::ToolFinished {
            tool: "Bash".into(),
            success: false,
        });
        // 3rd failure crosses threshold (max_consecutive_failures = 3)
        let result = sup.observe(&SupervisionEvent::ToolFinished {
            tool: "Bash".into(),
            success: false,
        });
        assert!(matches!(
            result,
            Some(SupervisionDirective::ExploreAlternatives { .. })
        ));
    }

    #[test]
    fn observe_tool_failed_architectural_pattern() {
        let mut sup = RuleBasedSupervisor::new();
        // First failure: not enough history
        sup.observe(&SupervisionEvent::ToolFailed {
            tool: "write".into(),
            error: "permission denied".into(),
        });
        // Second failure of same tool: 2 entries, pattern detected
        let result = sup.observe(&SupervisionEvent::ToolFailed {
            tool: "write".into(),
            error: "permission denied".into(),
        });
        assert!(matches!(
            result,
            Some(SupervisionDirective::ExploreAlternatives { reason, .. })
                if reason.contains("write")
        ));
    }

    #[test]
    fn observe_budget_warning_returns_pause() {
        let mut sup = RuleBasedSupervisor::new();
        let result = sup.observe(&SupervisionEvent::BudgetWarning {
            remaining_usd: 0.50,
        });
        assert!(matches!(
            result,
            Some(SupervisionDirective::PauseForReview { reason })
                if reason.contains("0.50")
        ));
    }

    #[test]
    fn observe_budget_warning_deduplicates() {
        let mut sup = RuleBasedSupervisor::new();
        let first = sup.observe(&SupervisionEvent::BudgetWarning {
            remaining_usd: 0.50,
        });
        assert!(first.is_some());
        let second = sup.observe(&SupervisionEvent::BudgetWarning {
            remaining_usd: 0.40,
        });
        assert!(second.is_none(), "should not repeat same directive");
    }

    #[test]
    fn observe_low_quality_signal_triggers_replan() {
        let mut sup = RuleBasedSupervisor::new();
        let result = sup.observe(&SupervisionEvent::QualitySignal {
            score: 1.5,
            details: "output vague and incomplete".into(),
        });
        assert!(matches!(
            result,
            Some(SupervisionDirective::Replan { reason })
                if reason.contains("1.5")
        ));
    }

    #[test]
    fn observe_high_quality_signal_returns_none() {
        let mut sup = RuleBasedSupervisor::new();
        let result = sup.observe(&SupervisionEvent::QualitySignal {
            score: 5.0,
            details: "excellent".into(),
        });
        assert!(result.is_none());
    }

    #[test]
    fn observe_phase_change_returns_none() {
        let mut sup = RuleBasedSupervisor::new();
        let result = sup.observe(&SupervisionEvent::PhaseChanged {
            from: ExecutionPhase::Explore,
            to: ExecutionPhase::Plan,
        });
        assert!(result.is_none());
    }

    #[test]
    fn observe_tier_change_returns_none() {
        let mut sup = RuleBasedSupervisor::new();
        let result = sup.observe(&SupervisionEvent::TierChanged { from: 2, to: 3 });
        assert!(result.is_none());
    }

    #[test]
    fn observe_scope_change_updates_tools() {
        let mut sup = RuleBasedSupervisor::new();
        let result = sup.observe(&SupervisionEvent::ScopeChanged {
            active_tools: vec!["Bash".into(), "write".into(), "read".into()],
        });
        assert!(result.is_none());
        assert_eq!(sup.active_tools.len(), 3);
    }

    // --- reconcile ---

    #[test]
    fn reconcile_healthy_task_returns_continue() {
        let mut sup = RuleBasedSupervisor::new();
        let ctx = make_snapshot();
        let result = sup.reconcile(&ctx);
        assert!(matches!(result, SupervisionDirective::Continue));
    }

    #[test]
    fn reconcile_low_budget_returns_pause() {
        let mut sup = RuleBasedSupervisor::new();
        let mut ctx = make_snapshot();
        ctx.cost_used = 9.0;
        ctx.budget_remaining = 0.5; // ratio < 0.20
        let result = sup.reconcile(&ctx);
        assert!(matches!(
            result,
            SupervisionDirective::PauseForReview { reason }
                if reason.contains("0.50")
        ));
    }

    #[test]
    fn reconcile_thrashing_returns_replan() {
        let mut sup = RuleBasedSupervisor::new();
        let mut ctx = make_snapshot();
        ctx.consecutive_failures = 3;
        ctx.steps_completed = 5;
        let result = sup.reconcile(&ctx);
        assert!(matches!(
            result,
            SupervisionDirective::Replan { reason }
                if reason.contains("thrashing")
        ));
    }

    #[test]
    fn reconcile_high_attempts_returns_escalate_tier() {
        let mut sup = RuleBasedSupervisor::new();
        let mut ctx = make_snapshot();
        ctx.attempt_count = 5;
        ctx.current_tier = 2;
        let result = sup.reconcile(&ctx);
        assert!(matches!(
            result,
            SupervisionDirective::EscalateTier { to_tier: 3, .. }
        ));
    }

    #[test]
    fn reconcile_sustained_low_quality_returns_replan() {
        let mut sup = RuleBasedSupervisor::new();
        // Feed low quality scores
        for _ in 0..4 {
            sup.observe(&SupervisionEvent::QualitySignal {
                score: 2.0,
                details: "poor".into(),
            });
            // Reset last_directive so Replan isn't blocked
            sup.last_directive_kind = None;
        }
        let ctx = make_snapshot();
        let result = sup.reconcile(&ctx);
        assert!(matches!(
            result,
            SupervisionDirective::Replan { reason }
                if reason.contains("low quality")
        ));
    }

    #[test]
    fn reconcile_deduplicates_directives() {
        let mut sup = RuleBasedSupervisor::new();
        let mut ctx = make_snapshot();
        ctx.consecutive_failures = 3;
        ctx.steps_completed = 5;
        let first = sup.reconcile(&ctx);
        assert!(matches!(first, SupervisionDirective::Replan { .. }));
        let second = sup.reconcile(&ctx);
        assert!(
            matches!(second, SupervisionDirective::Continue),
            "should not repeat same directive"
        );
    }

    // --- translate_event ---

    #[test]
    fn translate_escalation_signal() {
        use crate::bus::OrchestrationEvent;
        use crate::error_signal::ErrorSignal;

        let signal = ErrorSignal::new(
            crate::error_signal::SignalCategory::LogicError,
            Some(1),
            "test".into(),
            "s1".into(),
            "Bash".into(),
        );
        let event = OrchestrationEvent::StepFailed {
            step_id: "s1".into(),
            signal,
        };
        assert!(translate_event(&event).is_none());

        let event = OrchestrationEvent::EscalationSignal {
            task_id: "t1".into(),
            from_tier: 2,
            to_tier: 3,
            reason: "exhausted".into(),
        };
        let translated = translate_event(&event);
        assert!(matches!(
            translated,
            Some(SupervisionEvent::TierChanged { from: 2, to: 3 })
        ));
    }

    #[test]
    fn translate_phase_transition() {
        use crate::bus::OrchestrationEvent;

        let event = OrchestrationEvent::PhaseTransition {
            task_id: "t1".into(),
            from: ExecutionPhase::Explore,
            to: ExecutionPhase::Plan,
            reason: "ready".into(),
        };
        let translated = translate_event(&event);
        assert!(matches!(
            translated,
            Some(SupervisionEvent::PhaseChanged {
                from: ExecutionPhase::Explore,
                to: ExecutionPhase::Plan,
            })
        ));
    }

    #[test]
    fn translate_unrelated_event_returns_none() {
        use crate::bus::OrchestrationEvent;

        let event = OrchestrationEvent::PartialResult {
            step_id: "s1".into(),
            content: "output".into(),
        };
        assert!(translate_event(&event).is_none());
    }

    // --- edge cases ---

    #[test]
    fn snapshot_serialization_roundtrip() {
        let snap = make_snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        let back: TaskSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap.task_id, back.task_id);
        assert_eq!(snap.current_tier, back.current_tier);
        assert_eq!(snap.steps_completed, back.steps_completed);
    }

    #[test]
    fn rule_based_supervisor_default() {
        let sup = RuleBasedSupervisor::default();
        assert_eq!(sup.consecutive_failures, 0);
        assert!(sup.failure_history.is_empty());
        assert!(sup.quality_scores.is_empty());
    }

    #[test]
    fn architectural_failure_pattern_mixed_tools_no_false_positive() {
        let mut sup = RuleBasedSupervisor::new();
        sup.observe(&SupervisionEvent::ToolFailed {
            tool: "Bash".into(),
            error: "err".into(),
        });
        sup.observe(&SupervisionEvent::ToolFailed {
            tool: "write".into(),
            error: "err".into(),
        });
        let result = sup.observe(&SupervisionEvent::ToolFailed {
            tool: "Bash".into(),
            error: "err".into(),
        });
        assert!(
            result.is_none(),
            "mixed tools should not trigger architectural pattern"
        );
    }

    #[test]
    fn reconcile_tier_capped_at_5() {
        let mut sup = RuleBasedSupervisor::new();
        let mut ctx = make_snapshot();
        ctx.attempt_count = 5;
        ctx.current_tier = 5;
        let result = sup.reconcile(&ctx);
        // Tier 5 is max, should not escalate further
        assert!(matches!(result, SupervisionDirective::Continue));
    }

    #[test]
    fn reconcile_zero_budget_no_pause() {
        let mut sup = RuleBasedSupervisor::new();
        let mut ctx = make_snapshot();
        ctx.cost_used = 0.0;
        ctx.budget_remaining = 0.0;
        let result = sup.reconcile(&ctx);
        assert!(matches!(result, SupervisionDirective::Continue));
    }
}
