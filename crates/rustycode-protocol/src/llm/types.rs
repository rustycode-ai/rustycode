//! Shared LLM operation data types

use serde::{Deserialize, Serialize};

/// Token usage for a completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCount {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
}

/// Cost breakdown for a completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cost {
    pub input_cost: f64,
    pub output_cost: f64,
    pub total_cost: f64,
}

/// Token usage information for an LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens after the last cache breakpoint (not eligible for cache)
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,

    #[serde(default)]
    pub cache_read_input_tokens: u32,

    #[serde(default)]
    pub cache_creation_input_tokens: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
}

impl Usage {
    pub fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens.saturating_add(output_tokens),
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_tokens: None,
        }
    }

    pub fn with_cache(
        input_tokens: u32,
        output_tokens: u32,
        cache_read_input_tokens: u32,
        cache_creation_input_tokens: u32,
    ) -> Self {
        let total_input = cache_read_input_tokens
            .saturating_add(cache_creation_input_tokens)
            .saturating_add(input_tokens);
        Self {
            input_tokens,
            output_tokens,
            total_tokens: total_input.saturating_add(output_tokens),
            cache_read_input_tokens,
            cache_creation_input_tokens,
            reasoning_tokens: None,
        }
    }

    pub fn total_input_tokens(&self) -> u32 {
        self.cache_read_input_tokens
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.input_tokens)
    }

    pub fn has_cache_usage(&self) -> bool {
        self.cache_read_input_tokens > 0 || self.cache_creation_input_tokens > 0
    }
}
