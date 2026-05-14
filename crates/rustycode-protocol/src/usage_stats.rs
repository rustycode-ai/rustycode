//! Unified token and cost usage tracking for multi-agent coordination.

use serde::{Deserialize, Serialize};

/// Cumulative token and cost usage across one or more agent runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStats {
    /// Input token count.
    pub input_tokens: u64,
    /// Output token count.
    pub output_tokens: u64,
    /// Cached tokens read.
    pub cache_read_tokens: u64,
    /// Cached tokens created.
    pub cache_creation_tokens: u64,
    /// Cost in USD (if tracked).
    pub cost_usd: f64,
}

impl UsageStats {
    /// Merge another set of stats into this one.
    pub fn merge(&mut self, other: &UsageStats) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_creation_tokens += other.cache_creation_tokens;
        self.cost_usd += other.cost_usd;
    }
}
