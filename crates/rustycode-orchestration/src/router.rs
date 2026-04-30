//! Task Router — Complexity-aware, cost-sensitive model selection.
//!
//! Given a task description and token budget, picks the cheapest model tier
//! (Haiku / Sonnet / Opus) that is likely to succeed. Uses
//! `strategy_selector::detect_complexity` for task analysis and `cost_table`
//! for cost estimation.

use crate::cost_table::calculate_cost;
use crate::routing_metrics::ModelChoice;
use crate::strategy_selector::StrategySelector;

// ─── Request / Response ──────────────────────────────────────────────────────────

/// Input to the routing decision.
#[derive(Debug, Clone)]
pub struct RoutingRequest {
    /// Natural-language description of the task.
    pub description: String,
    /// Estimated total tokens for the request (input + expected output).
    pub estimated_tokens: usize,
    /// Token budget ceiling. The router avoids models whose estimated cost
    /// would consume more than `budget_tokens` worth of tokens.
    pub budget_tokens: usize,
}

/// Output of the routing decision.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingDecision {
    /// The selected model tier.
    pub selected_model: ModelChoice,
    /// Human-readable explanation of why this model was chosen.
    pub rationale: String,
    /// Confidence in the decision (0.0 – 1.0).
    pub confidence: f64,
    /// Estimated cost in USD for one execution at `estimated_tokens`.
    pub estimated_cost: f64,
}

// ─── TaskRouter ──────────────────────────────────────────────────────────────────

/// Stateless router that maps tasks to model tiers.
///
/// All state is derived from the request and the bundled cost/complexity
/// helpers; no mutable state is held.
pub struct TaskRouter;

impl Default for TaskRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRouter {
    pub const fn new() -> Self {
        Self
    }

    /// Route a task to the best model tier.
    ///
    /// Decision logic:
    /// - Complexity < 2.0 AND budget generous (>= 2x estimated) -> Haiku
    /// - Complexity < 4.0 AND budget generous (>= 3x estimated) -> Sonnet
    /// - Complexity >= 4.0 OR quality-critical signals           -> Opus
    /// - Budget too tight (estimated > budget)                    -> Haiku
    pub fn route(&self, request: &RoutingRequest) -> anyhow::Result<RoutingDecision> {
        let complexity = StrategySelector::detect_complexity(&request.description);
        let budget_ratio = if request.estimated_tokens > 0 {
            request.budget_tokens as f64 / request.estimated_tokens as f64
        } else {
            f64::MAX
        };

        let budget_generous = budget_ratio >= 2.0;
        let budget_very_generous = budget_ratio >= 3.0;
        let budget_tight = request.estimated_tokens > request.budget_tokens;
        let quality_critical = is_quality_critical(&request.description);

        let (selected, rationale, confidence) = if budget_tight {
            (
                ModelChoice::Haiku,
                format!(
                    "Budget is tight ({}/{}) tokens. Using cheapest tier to stay within budget.",
                    request.estimated_tokens, request.budget_tokens
                ),
                0.9,
            )
        } else if complexity >= 4.0 || quality_critical {
            (
                ModelChoice::Opus,
                format!(
                    "High complexity ({complexity:.1}) or quality-critical task. \
                     Requires strongest model for reliable output."
                ),
                0.95,
            )
        } else if complexity < 2.0 && budget_generous {
            (
                ModelChoice::Haiku,
                format!(
                    "Low complexity ({complexity:.1}) with generous budget. \
                     Fast and cost-effective tier is appropriate."
                ),
                0.85,
            )
        } else if complexity < 4.0 && budget_very_generous {
            (
                ModelChoice::Sonnet,
                format!(
                    "Moderate complexity ({complexity:.1}) with sufficient budget. \
                     Balanced tier provides good quality at reasonable cost."
                ),
                0.85,
            )
        } else {
            // Moderate complexity, moderate budget — prefer Sonnet as the
            // safe middle ground.
            (
                ModelChoice::Sonnet,
                format!(
                    "Moderate complexity ({complexity:.1}). Defaulting to balanced tier \
                     for reliable results."
                ),
                0.7,
            )
        };

        let estimated_cost = estimate_cost(selected, request.estimated_tokens);

        Ok(RoutingDecision {
            selected_model: selected,
            rationale,
            confidence,
            estimated_cost,
        })
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────────

/// Keywords that signal quality-critical tasks where Opus is justified
/// regardless of complexity score.
fn is_quality_critical(description: &str) -> bool {
    let lower = description.to_lowercase();
    let signals = [
        "security",
        "audit",
        "production",
        "critical",
        "architecture",
        "design review",
        "data loss",
        "encryption",
        "authentication",
        "compliance",
    ];
    signals.iter().any(|s| lower.contains(s))
}

/// Estimate the USD cost for `tokens` total tokens on the given model.
///
/// Uses a 60/40 input/output split as a rough approximation. Returns 0.0
/// when the model is not in the cost table.
#[allow(clippy::cast_precision_loss)]
fn estimate_cost(model: ModelChoice, tokens: usize) -> f64 {
    let input_tokens = ((tokens as f64) * 0.6).round() as usize;
    let output_tokens = tokens.saturating_sub(input_tokens);
    calculate_cost(model.model_id(), input_tokens, output_tokens).unwrap_or(0.0)
}

// ─── Tests ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    fn make_request(desc: &str, estimated: usize, budget: usize) -> RoutingRequest {
        RoutingRequest {
            description: desc.to_string(),
            estimated_tokens: estimated,
            budget_tokens: budget,
        }
    }

    // ── Core routing tests ────────────────────────────────────────────────

    #[test]
    fn simple_task_routes_to_haiku() {
        let router = TaskRouter::new();
        let req = make_request("fix this typo", 500, 5_000);
        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, ModelChoice::Haiku);
    }

    #[test]
    fn complex_task_routes_to_opus() {
        let router = TaskRouter::new();
        let req = make_request(
            "explore and analyze the distributed systems architecture",
            4_000,
            20_000,
        );
        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, ModelChoice::Opus);
    }

    #[test]
    fn medium_task_routes_to_sonnet() {
        let router = TaskRouter::new();
        let req = make_request("implement user profile caching middleware", 2_000, 10_000);
        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, ModelChoice::Sonnet);
    }

    #[test]
    fn tight_budget_forces_haiku() {
        let router = TaskRouter::new();
        // Complex task but budget is smaller than estimated tokens.
        let req = make_request(
            "explore the compiler design for the new backend",
            10_000,
            5_000,
        );
        let decision = router.route(&req).unwrap();
        assert_eq!(
            decision.selected_model,
            ModelChoice::Haiku,
            "Tight budget should force Haiku regardless of complexity"
        );
    }

    // ── Quality-critical override ─────────────────────────────────────────

    #[test]
    fn quality_critical_overrides_to_opus() {
        let router = TaskRouter::new();
        let req = make_request("fix this security vulnerability in production", 500, 5_000);
        let decision = router.route(&req).unwrap();
        assert_eq!(
            decision.selected_model,
            ModelChoice::Opus,
            "Quality-critical keyword should override low complexity"
        );
    }

    #[test]
    fn audit_keyword_triggers_opus() {
        let router = TaskRouter::new();
        let req = make_request("audit the payment processing code", 1_000, 5_000);
        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, ModelChoice::Opus);
    }

    #[test]
    fn encryption_keyword_triggers_opus() {
        let router = TaskRouter::new();
        let req = make_request("update the encryption module", 800, 4_000);
        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, ModelChoice::Opus);
    }

    // ── Rationale and metadata ────────────────────────────────────────────

    #[test]
    fn routing_includes_rationale() {
        let router = TaskRouter::new();
        let req = make_request("fix this typo", 500, 5_000);
        let decision = router.route(&req).unwrap();
        assert!(
            !decision.rationale.is_empty(),
            "Decision should include a rationale"
        );
    }

    #[test]
    fn routing_includes_confidence() {
        let router = TaskRouter::new();
        let req = make_request("fix this typo", 500, 5_000);
        let decision = router.route(&req).unwrap();
        assert!(
            (0.0..=1.0).contains(&decision.confidence),
            "Confidence should be between 0.0 and 1.0"
        );
    }

    #[test]
    fn estimated_cost_is_positive() {
        let router = TaskRouter::new();
        let req = make_request("fix this typo", 500, 5_000);
        let decision = router.route(&req).unwrap();
        assert!(
            decision.estimated_cost > 0.0,
            "Estimated cost should be positive"
        );
    }

    // ── Budget respect ────────────────────────────────────────────────────

    #[test]
    fn budget_respects_haiku_threshold() {
        let router = TaskRouter::new();
        // complexity < 2.0, budget exactly 2x estimated → Haiku
        let req = make_request("fix the typo", 1_000, 2_000);
        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, ModelChoice::Haiku);
    }

    #[test]
    fn budget_respects_sonnet_threshold() {
        let router = TaskRouter::new();
        // complexity ~2.5, budget >= 3x → Sonnet
        let req = make_request("implement the caching layer", 1_000, 3_500);
        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, ModelChoice::Sonnet);
    }

    // ── Edge cases ────────────────────────────────────────────────────────

    #[test]
    fn zero_estimated_tokens_does_not_panic() {
        let router = TaskRouter::new();
        let req = make_request("fix this typo", 0, 5_000);
        let decision = router.route(&req);
        assert!(decision.is_ok());
    }

    #[test]
    fn default_trait_works() {
        let router = TaskRouter;
        let req = make_request("fix this typo", 500, 5_000);
        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, ModelChoice::Haiku);
    }
}
