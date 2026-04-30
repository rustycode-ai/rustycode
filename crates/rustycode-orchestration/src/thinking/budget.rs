//! Thinking budget constraints.
//!
//! Provides a structured way to limit reasoning depth and token usage.

use serde::{Deserialize, Serialize};

/// Budget constraints for a reasoning session.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThinkingBudget {
    /// Maximum number of reasoning steps allowed.
    pub max_steps: Option<usize>,
    /// Maximum number of tokens allowed for thinking.
    pub max_thinking_tokens: Option<u32>,
    /// Whether to force the model to stop thinking if budget is reached.
    pub force_stop: bool,
}

impl ThinkingBudget {
    pub const fn new(max_steps: Option<usize>, max_tokens: Option<u32>) -> Self {
        Self {
            max_steps,
            max_thinking_tokens: max_tokens,
            force_stop: true,
        }
    }
}
