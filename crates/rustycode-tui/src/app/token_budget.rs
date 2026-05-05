//! Token budget and cost tracking sub-struct for the TUI.
//!
//! Groups all fields related to token usage, cost estimation, and
//! budget enforcement into one cohesive unit.

/// Token budget and cost tracking state for the TUI.
///
/// All fields here track cumulative session token usage and cost,
/// enabling budget enforcement and usage display.
pub(crate) struct TokenBudget {
    /// Cumulative session cost in USD.
    pub(crate) session_cost_usd: f64,
    /// Cumulative session input token count.
    pub(crate) session_input_tokens: usize,
    /// Cumulative session output token count.
    pub(crate) session_output_tokens: usize,
    /// Cumulative session cache read token count.
    pub(crate) session_cache_read_tokens: usize,
    /// Cumulative session cache creation token count.
    pub(crate) session_cache_creation_tokens: usize,
    /// Input tokens from the most recent turn (for context display).
    pub(crate) last_turn_input_tokens: usize,
    /// Cost tracker for per-tool and per-model cost breakdowns.
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
