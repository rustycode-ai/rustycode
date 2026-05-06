pub(crate) struct TokenBudget {
    pub(crate) session_cost_usd: f64,
    pub(crate) session_input_tokens: usize,
    pub(crate) session_output_tokens: usize,
    pub(crate) session_cache_read_tokens: usize,
    pub(crate) session_cache_creation_tokens: usize,
    pub(crate) last_turn_input_tokens: usize,
    /// Per-tool and per-model cost breakdowns.
    pub(crate) cost_tracker: rustycode_llm::cost_tracker::CostTracker,
}

impl TokenBudget {
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

    pub(crate) fn reset(&mut self) {
        self.session_cost_usd = 0.0;
        self.session_input_tokens = 0;
        self.session_output_tokens = 0;
        self.session_cache_read_tokens = 0;
        self.session_cache_creation_tokens = 0;
        self.last_turn_input_tokens = 0;
        self.cost_tracker = rustycode_llm::cost_tracker::CostTracker::new(None);
    }

    pub(crate) fn record_usage(
        &mut self,
        input: usize,
        output: usize,
        cache_read: usize,
        cache_creation: usize,
        cost_usd: f64,
    ) {
        self.session_input_tokens = self.session_input_tokens.saturating_add(input);
        self.session_output_tokens = self.session_output_tokens.saturating_add(output);
        self.session_cache_read_tokens = self.session_cache_read_tokens.saturating_add(cache_read);
        self.session_cache_creation_tokens = self
            .session_cache_creation_tokens
            .saturating_add(cache_creation);
        self.session_cost_usd += cost_usd;
        self.last_turn_input_tokens = input;
    }

    pub(crate) fn total_tokens(&self) -> usize {
        self.session_input_tokens
            .saturating_add(self.session_output_tokens)
            .saturating_add(self.session_cache_read_tokens)
            .saturating_add(self.session_cache_creation_tokens)
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
    fn record_usage_accumulates() {
        let mut budget = TokenBudget::new();
        budget.record_usage(100, 50, 10, 5, 0.01);
        assert_eq!(budget.session_input_tokens, 100);
        assert_eq!(budget.session_output_tokens, 50);
        assert_eq!(budget.session_cache_read_tokens, 10);
        assert_eq!(budget.session_cache_creation_tokens, 5);
        assert!((budget.session_cost_usd - 0.01).abs() < f64::EPSILON);
        assert_eq!(budget.last_turn_input_tokens, 100);
    }

    #[test]
    fn record_usage_accumulates_across_calls() {
        let mut budget = TokenBudget::new();
        budget.record_usage(100, 50, 10, 5, 0.01);
        budget.record_usage(200, 75, 20, 0, 0.02);
        assert_eq!(budget.session_input_tokens, 300);
        assert_eq!(budget.session_output_tokens, 125);
        assert_eq!(budget.last_turn_input_tokens, 200);
    }

    #[test]
    fn total_tokens_sums_all_counters() {
        let mut budget = TokenBudget::new();
        budget.record_usage(100, 50, 10, 5, 0.0);
        assert_eq!(budget.total_tokens(), 165);
    }

    #[test]
    fn total_tokens_zero_when_empty() {
        let budget = TokenBudget::new();
        assert_eq!(budget.total_tokens(), 0);
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
