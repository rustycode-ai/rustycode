//! Budget tracking for agent execution — token and cost ceilings.
//! Budget cascades from team → agent → sub-agent.

use serde::{Deserialize, Serialize};

/// Budget allocation for an agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBudget {
    /// Maximum total tokens (input + output).
    pub max_tokens: u64,
    /// Maximum cost in USD.
    pub max_cost_usd: f64,
    /// Tokens consumed so far.
    pub tokens_used: u64,
    /// Cost consumed so far in USD.
    pub cost_used_usd: f64,
}

impl CostBudget {
    /// Create a new budget with the given limits.
    #[must_use]
    pub fn new(max_tokens: u64, max_cost_usd: f64) -> Self {
        Self {
            max_tokens,
            max_cost_usd,
            tokens_used: 0,
            cost_used_usd: 0.0,
        }
    }

    /// Unlimited budget (use for top-level orchestration).
    #[must_use]
    pub fn unlimited() -> Self {
        Self {
            max_tokens: u64::MAX,
            max_cost_usd: f64::INFINITY,
            tokens_used: 0,
            cost_used_usd: 0.0,
        }
    }

    /// Remaining tokens.
    #[must_use]
    pub fn remaining_tokens(&self) -> u64 {
        self.max_tokens.saturating_sub(self.tokens_used)
    }

    /// Remaining cost in USD.
    #[must_use]
    pub fn remaining_cost_usd(&self) -> f64 {
        (self.max_cost_usd - self.cost_used_usd).max(0.0)
    }

    /// Check if budget is exhausted.
    #[must_use]
    pub fn is_exhausted(&self) -> bool {
        self.tokens_used >= self.max_tokens || self.cost_used_usd >= self.max_cost_usd
    }

    /// Record token and cost usage. Returns false if budget would be exceeded.
    pub fn record(&mut self, tokens: u64, cost_usd: f64) -> bool {
        let new_tokens = self.tokens_used.saturating_add(tokens);
        let new_cost = self.cost_used_usd + cost_usd;
        if new_tokens > self.max_tokens || new_cost > self.max_cost_usd {
            return false;
        }
        self.tokens_used = new_tokens;
        self.cost_used_usd = new_cost;
        true
    }

    /// Subdivide this budget for a sub-agent (takes a fraction).
    /// Returns a new CostBudget with proportional limits.
    #[must_use]
    pub fn subdivide(&self, fraction: f64) -> Self {
        let fraction = fraction.clamp(0.0, 1.0);
        Self {
            max_tokens: ((self.remaining_tokens() as f64) * fraction) as u64,
            max_cost_usd: self.remaining_cost_usd() * fraction,
            tokens_used: 0,
            cost_used_usd: 0.0,
        }
    }
}

impl Default for CostBudget {
    fn default() -> Self {
        Self::new(500_000, 5.0) // Reasonable defaults: 500K tokens, $5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_budget_not_exhausted() {
        let b = CostBudget::new(1000, 1.0);
        assert!(!b.is_exhausted());
        assert_eq!(b.remaining_tokens(), 1000);
        assert_eq!(b.remaining_cost_usd(), 1.0);
    }

    #[test]
    fn record_within_budget() {
        let mut b = CostBudget::new(1000, 1.0);
        assert!(b.record(500, 0.3));
        assert_eq!(b.tokens_used, 500);
        assert!(!b.is_exhausted());
    }

    #[test]
    fn record_exceeds_tokens() {
        let mut b = CostBudget::new(1000, 1.0);
        assert!(!b.record(1001, 0.1));
        assert_eq!(b.tokens_used, 0); // Not recorded
    }

    #[test]
    fn record_exceeds_cost() {
        let mut b = CostBudget::new(1000, 1.0);
        assert!(!b.record(100, 1.5));
        assert_eq!(b.cost_used_usd, 0.0); // Not recorded
    }

    #[test]
    fn exhausted_after_full_usage() {
        let mut b = CostBudget::new(1000, 1.0);
        assert!(b.record(1000, 0.5));
        assert!(b.is_exhausted());
    }

    #[test]
    fn subdivide_proportional() {
        let b = CostBudget::new(1000, 10.0);
        let sub = b.subdivide(0.3);
        assert_eq!(sub.max_tokens, 300);
        assert!((sub.max_cost_usd - 3.0).abs() < f64::EPSILON);
        assert_eq!(sub.tokens_used, 0);
    }

    #[test]
    fn subdivide_clamps_fraction() {
        let b = CostBudget::new(1000, 10.0);
        let sub = b.subdivide(1.5); // Clamped to 1.0
        assert_eq!(sub.max_tokens, 1000);
    }

    #[test]
    fn unlimited_never_exhausted() {
        let b = CostBudget::unlimited();
        assert!(!b.is_exhausted());
    }

    #[test]
    fn serialization_round_trip() {
        let b = CostBudget::new(5000, 2.5);
        let json = serde_json::to_string(&b).unwrap();
        let back: CostBudget = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_tokens, b.max_tokens);
    }
}
