//! Token budget tracking for context compaction decisions.
//!
//! [`TokenBudget`] tracks the current token usage against a model's context window
//! and determines when compaction should be triggered. It accounts for overhead
//! that is always present (system prompt, tool definitions, context zones) and
//! reserves space for the compaction process itself (summary prompt + response).

use rustycode_llm::provider::Usage;

/// Token budget tracker for compaction decisions.
///
/// Computes capacity, trigger thresholds, and target sizes based on the
/// model's context window and reserved overhead. Callers update the current
/// token count from API `Usage` responses or local estimates, then check
/// [`should_compact`](Self::should_compact) before each turn.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    /// Current input token count, updated from API usage or estimates.
    pub current_input_tokens: usize,
    /// Model-specific context window size (e.g. 200_000 for Claude Sonnet).
    pub context_window: usize,
    /// Model max_output_tokens reservation.
    pub reserved_output: usize,
    /// Tokens always present across 4 context zones (~850 tokens).
    pub always_present_tokens: usize,
    /// Buffer for compaction summary prompt + response overhead (6000 tokens).
    pub compaction_buffer: usize,
}

impl TokenBudget {
    /// Create a new budget with the given context window and output reservation.
    ///
    /// Defaults: `always_present_tokens` = 850, `compaction_buffer` = 6000,
    /// `current_input_tokens` = 0.
    pub fn new(context_window: usize, reserved_output: usize) -> Self {
        Self {
            current_input_tokens: 0,
            context_window,
            reserved_output,
            always_present_tokens: 850,
            compaction_buffer: 6000,
        }
    }

    /// Usable capacity for conversation messages after all overhead is subtracted.
    ///
    /// Uses `saturating_sub` so a tiny context window gracefully yields 0
    /// instead of underflowing.
    pub fn conversation_capacity(&self) -> usize {
        self.context_window
            .saturating_sub(self.reserved_output)
            .saturating_sub(self.always_present_tokens)
            .saturating_sub(self.compaction_buffer)
    }

    /// Token count at which compaction should be triggered (78% of capacity).
    pub fn trigger_threshold(&self) -> usize {
        (self.conversation_capacity() as f64 * 0.78) as usize
    }

    /// Target token count after compaction completes (50% of capacity).
    pub fn target_size(&self) -> usize {
        (self.conversation_capacity() as f64 * 0.50) as usize
    }

    /// Update the current input token count from an API `Usage` response.
    ///
    /// Widens `u32` to `usize` losslessly.
    pub fn update_from_usage(&mut self, usage: &Usage) {
        self.current_input_tokens = usage.input_tokens as usize;
    }

    /// Update the current input token count from a local estimate.
    ///
    /// Use this when API usage data is unavailable (e.g. streaming providers
    /// that do not report token counts).
    pub fn update_from_estimate(&mut self, estimated_tokens: usize) {
        self.current_input_tokens = estimated_tokens;
    }

    /// Whether compaction should be triggered this turn.
    pub fn should_compact(&self) -> bool {
        self.current_input_tokens >= self.trigger_threshold()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typical_budget() -> TokenBudget {
        TokenBudget::new(200_000, 8_192)
    }

    #[test]
    fn conversation_capacity_subtracts_overhead() {
        let budget = typical_budget();
        // 200_000 - 8_192 - 850 - 6_000 = 184_958
        assert_eq!(budget.conversation_capacity(), 184_958);
    }

    #[test]
    fn trigger_threshold_is_78_percent() {
        let budget = typical_budget();
        let expected = (184_958_f64 * 0.78) as usize;
        assert_eq!(budget.trigger_threshold(), expected);
    }

    #[test]
    fn target_size_is_50_percent() {
        let budget = typical_budget();
        let expected = (184_958_f64 * 0.50) as usize;
        assert_eq!(budget.target_size(), expected);
    }

    #[test]
    fn should_compact_when_over_threshold() {
        let mut budget = typical_budget();
        let threshold = budget.trigger_threshold();
        budget.current_input_tokens = threshold + 1;
        assert!(budget.should_compact());
    }

    #[test]
    fn should_not_compact_when_under_threshold() {
        let mut budget = typical_budget();
        let threshold = budget.trigger_threshold();
        budget.current_input_tokens = threshold.saturating_sub(1);
        assert!(!budget.should_compact());
    }

    #[test]
    fn update_from_usage_widens_u32() {
        let mut budget = typical_budget();
        let usage = Usage {
            input_tokens: 50_000,
            output_tokens: 1_000,
            total_tokens: 51_000,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_tokens: None,
        };
        budget.update_from_usage(&usage);
        assert_eq!(budget.current_input_tokens, 50_000_usize);
    }

    #[test]
    fn update_from_estimate_fallback() {
        let mut budget = typical_budget();
        budget.update_from_estimate(123_456);
        assert_eq!(budget.current_input_tokens, 123_456);
    }

    #[test]
    fn small_context_window_does_not_underflow() {
        let budget = TokenBudget::new(10_000, 8_000);
        // capacity = 10_000 - 8_000 - 850 - 6_000 = 0 (saturating)
        assert_eq!(budget.conversation_capacity(), 0);
        assert_eq!(budget.trigger_threshold(), 0);
        // With 0 threshold, even 0 tokens triggers compaction — that is
        // correct: the window is too small to hold any conversation.
        assert!(budget.should_compact());
    }
}
