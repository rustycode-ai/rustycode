pub mod cost;
pub mod token_counter;
pub mod tool_executor;

pub use cost::{
    cost_per_million_tokens_io, estimate_cost, BudgetExceeded, BudgetStatus, BudgetWarningLevel,
    CostSummary, CostTracker, ModelCost,
};
pub use rustycode_protocol::tool::{ApiCall, CostTrackerProvider};
pub use token_counter::{TokenCounter, CHARS_PER_TOKEN, MAX_TOKEN_CACHE_SIZE};
pub use tool_executor::{ToolExecutorApi, ToolInfo};

pub trait ToolCostTracker: Send + Sync {
    fn record_llm_cost(&mut self, tokens: u32, model: &str);
    fn total_cost(&self) -> f64;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeTracker {
        tokens: u32,
    }

    impl ToolCostTracker for FakeTracker {
        fn record_llm_cost(&mut self, tokens: u32, _model: &str) {
            self.tokens += tokens;
        }

        fn total_cost(&self) -> f64 {
            f64::from(self.tokens) * 0.001
        }
    }

    #[test]
    fn tracker_accumulates_cost() {
        let mut t = FakeTracker { tokens: 0 };
        t.record_llm_cost(100, "claude");
        assert!((t.total_cost() - 0.1).abs() < f64::EPSILON);
    }
}
