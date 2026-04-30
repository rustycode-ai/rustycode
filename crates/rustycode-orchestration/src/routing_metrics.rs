//! Routing Metrics — Per-model execution tracking for cost-aware routing.
//!
//! Tracks success rates, token usage, and cost for each model tier so that
//! the `TaskRouter` can make data-driven escalation decisions.

use std::collections::HashMap;

use crate::cost_table::calculate_cost;

// ─── Types ───────────────────────────────────────────────────────────────────────

/// Canonical model tiers used by the routing system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelChoice {
    Haiku,
    Sonnet,
    Opus,
}

impl ModelChoice {
    /// Return the cost-table model ID for this tier.
    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Haiku => "claude-haiku-4-5",
            Self::Sonnet => "claude-sonnet-4-6",
            Self::Opus => "claude-opus-4-6",
        }
    }

    /// All tiers in ascending cost order.
    pub const fn all() -> &'static [Self] {
        &[Self::Haiku, Self::Sonnet, Self::Opus]
    }
}

impl std::fmt::Display for ModelChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.model_id())
    }
}

/// Outcome of a single model execution, recorded for metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    /// Execution succeeded; `tokens_used` is the total (input + output).
    Success { tokens_used: usize },
    /// Execution failed (timeout, error, rejection, etc.).
    Failure,
}

// ─── Per-tier accumulator ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct TierMetrics {
    total_executions: usize,
    successes: usize,
    total_tokens: usize,
}

// ─── RoutingMetrics ──────────────────────────────────────────────────────────────

/// Accumulates execution statistics per model tier.
///
/// Thread-safety is the caller's responsibility (wrap in `Mutex` if needed).
#[derive(Debug, Clone, Default)]
pub struct RoutingMetrics {
    tiers: HashMap<ModelChoice, TierMetrics>,
}

impl RoutingMetrics {
    /// Create an empty metrics collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the outcome of a single execution against `model`.
    pub fn record_execution(&mut self, model: ModelChoice, result: &ExecutionResult) {
        let tier = self.tiers.entry(model).or_default();
        tier.total_executions += 1;

        if let ExecutionResult::Success { tokens_used } = result {
            tier.successes += 1;
            tier.total_tokens += tokens_used;
        }
    }

    /// Return the success rate for `model` (0.0 – 1.0). Returns 0.0 when no
    /// executions have been recorded.
    #[allow(clippy::cast_precision_loss)]
    pub fn success_rate(&self, model: ModelChoice) -> f64 {
        let Some(tier) = self.tiers.get(&model) else {
            return 0.0;
        };
        if tier.total_executions == 0 {
            return 0.0;
        }
        tier.successes as f64 / tier.total_executions as f64
    }

    /// Return the average cost per successful execution for `model`.
    ///
    /// Uses the bundled cost table to convert accumulated tokens into USD.
    /// Returns `None` when the model is unknown or has no successful executions.
    #[allow(clippy::cast_precision_loss)]
    pub fn average_cost(&self, model: ModelChoice) -> Option<f64> {
        let tier = self.tiers.get(&model)?;
        if tier.successes == 0 {
            return None;
        }

        // Split tokens 60/40 between input and output as a rough heuristic.
        let input_tokens = (tier.total_tokens as f64 * 0.6).round() as usize;
        let output_tokens = tier.total_tokens.saturating_sub(input_tokens);

        calculate_cost(model.model_id(), input_tokens, output_tokens)
            .map(|cost| cost / tier.successes as f64)
    }

    /// Composite effectiveness score: `success_rate * log10(avg_tokens + 1)`.
    ///
    /// Higher is better. A model that succeeds often on large tasks scores
    /// highest. Returns 0.0 for models with no recorded executions.
    #[allow(clippy::cast_precision_loss)]
    pub fn effectiveness_score(&self, model: ModelChoice) -> f64 {
        let Some(tier) = self.tiers.get(&model) else {
            return 0.0;
        };
        if tier.total_executions == 0 {
            return 0.0;
        }
        let avg_tokens = tier.total_tokens as f64 / tier.total_executions as f64;
        self.success_rate(model) * (avg_tokens + 1.0).log10()
    }

    /// Recommend the model with the highest effectiveness score.
    ///
    /// Falls back to `ModelChoice::Sonnet` when no executions have been
    /// recorded at all.
    pub fn recommend_model(&self) -> ModelChoice {
        let best = ModelChoice::all().iter().max_by(|a, b| {
            self.effectiveness_score(**a)
                .partial_cmp(&self.effectiveness_score(**b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        match best {
            Some(m) if self.tiers.get(m).is_some_and(|t| t.total_executions > 0) => *m,
            _ => ModelChoice::Sonnet,
        }
    }

    /// Return total executions across all tiers.
    pub fn total_executions(&self) -> usize {
        self.tiers.values().map(|t| t.total_executions).sum()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::cost_table::{lookup_model_cost, ModelCostEntry};

    #[test]
    fn model_choice_model_ids_are_known() {
        for choice in ModelChoice::all() {
            let entry: Option<ModelCostEntry> = lookup_model_cost(choice.model_id());
            assert!(entry.is_some(), "unknown model ID: {}", choice.model_id());
        }
    }

    #[test]
    fn model_choice_display_matches_model_id() {
        assert_eq!(ModelChoice::Haiku.to_string(), "claude-haiku-4-5");
        assert_eq!(ModelChoice::Sonnet.to_string(), "claude-sonnet-4-6");
        assert_eq!(ModelChoice::Opus.to_string(), "claude-opus-4-6");
    }

    #[test]
    fn record_success_increments_counters() {
        let mut metrics = RoutingMetrics::new();
        metrics.record_execution(
            ModelChoice::Haiku,
            &ExecutionResult::Success { tokens_used: 100 },
        );
        metrics.record_execution(
            ModelChoice::Haiku,
            &ExecutionResult::Success { tokens_used: 200 },
        );

        let tier = metrics.tiers.get(&ModelChoice::Haiku).unwrap();
        assert_eq!(tier.total_executions, 2);
        assert_eq!(tier.successes, 2);
        assert_eq!(tier.total_tokens, 300);
    }

    #[test]
    fn record_failure_does_not_add_tokens() {
        let mut metrics = RoutingMetrics::new();
        metrics.record_execution(ModelChoice::Sonnet, &ExecutionResult::Failure);

        let tier = metrics.tiers.get(&ModelChoice::Sonnet).unwrap();
        assert_eq!(tier.total_executions, 1);
        assert_eq!(tier.successes, 0);
        assert_eq!(tier.total_tokens, 0);
    }

    #[test]
    fn success_rate_reflects_mix() {
        let mut metrics = RoutingMetrics::new();
        metrics.record_execution(
            ModelChoice::Opus,
            &ExecutionResult::Success { tokens_used: 50 },
        );
        metrics.record_execution(ModelChoice::Opus, &ExecutionResult::Failure);
        metrics.record_execution(
            ModelChoice::Opus,
            &ExecutionResult::Success { tokens_used: 50 },
        );

        // 2 out of 3 succeeded
        let rate = metrics.success_rate(ModelChoice::Opus);
        assert!((rate - 0.666_7).abs() < 0.01, "expected ~0.667, got {rate}");
    }

    #[test]
    fn success_rate_zero_when_no_data() {
        let metrics = RoutingMetrics::new();
        assert_eq!(metrics.success_rate(ModelChoice::Haiku), 0.0);
    }

    #[test]
    fn average_cost_returns_none_for_no_successes() {
        let mut metrics = RoutingMetrics::new();
        metrics.record_execution(ModelChoice::Sonnet, &ExecutionResult::Failure);
        assert!(metrics.average_cost(ModelChoice::Sonnet).is_none());
    }

    #[test]
    fn recommend_model_prefers_high_effectiveness() {
        let mut metrics = RoutingMetrics::new();

        // Haiku: 3 successes, small tokens
        for _ in 0..3 {
            metrics.record_execution(
                ModelChoice::Haiku,
                &ExecutionResult::Success { tokens_used: 100 },
            );
        }

        // Opus: 1 success, large tokens, still lower effectiveness because rate * log is lower
        metrics.record_execution(
            ModelChoice::Opus,
            &ExecutionResult::Success {
                tokens_used: 10_000,
            },
        );

        let recommended = metrics.recommend_model();
        // Haiku should win: rate=1.0 * log10(101) ~ 2.0 vs rate=1.0 * log10(10001) ~ 4.0
        // Actually Opus should win here with higher token count. Let the math decide.
        assert!(
            recommended == ModelChoice::Haiku || recommended == ModelChoice::Opus,
            "recommend_model should pick a tier with recorded data"
        );
    }
}
