//! Token budget and cost tracking sub-struct for the TUI.

/// Token budget and cost tracking state for the TUI.
///
/// All fields here track cumulative session token usage and cost,
/// enabling budget enforcement and usage display.
#[non_exhaustive]
pub(crate) struct TokenBudget {
    pub(crate) session_cost_usd: f64,
    pub(crate) session_input_tokens: usize,
    pub(crate) session_output_tokens: usize,
    pub(crate) session_cache_read_tokens: usize,
    pub(crate) session_cache_creation_tokens: usize,
    /// Most recent turn input tokens for context display.
    pub(crate) last_turn_input_tokens: usize,
    /// Per-tool and per-model cost breakdowns.
    pub(crate) cost_tracker: rustycode_llm::cost_tracker::CostTracker,
}

impl TokenBudget {
    /// Create a new `TokenBudget` with all counters at zero.
    pub(crate) fn new() -> Self {
        Self {
            session_cost_usd: 0.0,
            session_input_tokens: 0,
            session_output_tokens: 0,
            session_cache_read_tokens: 0,
            session_cache_creation_tokens: 0,
            last_turn_input_tokens: 0,
            cost_tracker: rustycode_llm::cost_tracker::CostTracker::new(None),
        }
    }

    /// Reset all counters to zero (used when clearing the conversation).
    pub(crate) fn reset(&mut self) {
        self.session_cost_usd = 0.0;
        self.session_input_tokens = 0;
        self.session_output_tokens = 0;
        self.session_cache_read_tokens = 0;
        self.session_cache_creation_tokens = 0;
        self.last_turn_input_tokens = 0;
        self.cost_tracker = rustycode_llm::cost_tracker::CostTracker::new(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_zero() {
        let budget = TokenBudget::new();
        assert_eq!(budget.session_cost_usd, 0.0);
        assert_eq!(budget.session_input_tokens, 0);
        assert_eq!(budget.session_output_tokens, 0);
        assert_eq!(budget.session_cache_read_tokens, 0);
        assert_eq!(budget.session_cache_creation_tokens, 0);
        assert_eq!(budget.last_turn_input_tokens, 0);
    }

    #[test]
    fn reset_returns_to_zero() {
        let mut budget = TokenBudget::new();
        budget.session_cost_usd = 1.5;
        budget.session_input_tokens = 1000;
        budget.session_output_tokens = 500;
        budget.session_cache_read_tokens = 200;
        budget.session_cache_creation_tokens = 50;
        budget.last_turn_input_tokens = 300;

        budget.reset();

        assert_eq!(budget.session_cost_usd, 0.0);
        assert_eq!(budget.session_input_tokens, 0);
        assert_eq!(budget.session_output_tokens, 0);
        assert_eq!(budget.session_cache_read_tokens, 0);
        assert_eq!(budget.session_cache_creation_tokens, 0);
        assert_eq!(budget.last_turn_input_tokens, 0);
    }
}
