//! Compaction tiers — progressive, ordered passes over the conversation.
//!
//! Each tier transforms a message list and reports how many tokens it
//! approximately removed. The tiers are intentionally composable: a caller
//! can run Snip first (free), then Summarize (LLM-backed), then Truncate
//! (destructive), stopping as soon as the token budget is satisfied.

pub mod snip;
pub mod summarize;
pub mod truncate;

pub use snip::SnipTier;
pub use summarize::SummarizeTier;
pub use truncate::TruncateTier;

/// Result from a single compaction tier pass.
#[derive(Debug, Clone)]
pub struct TierResult {
    /// Messages after this tier's transformation.
    pub messages: Vec<rustycode_protocol::Message>,
    /// Estimated tokens removed by this tier (rough: chars removed / 4).
    pub tokens_removed: usize,
}
