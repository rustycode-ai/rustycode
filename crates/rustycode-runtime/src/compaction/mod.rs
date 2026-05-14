//! Context compaction module.
//!
//! This module houses the redesigned compaction pipeline. It provides
//! [`TokenBudget`] for tracking context window usage, [`CompactionPlan`] for
//! progressive tightening, and compaction tiers ([`tiers::SnipTier`]) that
//! progressively reduce context size.

pub mod budget;
pub mod context_block;
pub mod piggyback;
pub mod pipeline;
pub mod plan;
pub mod tiers;

pub use budget::TokenBudget;
pub use context_block::{ContextZone, SessionContextBlock, StringZone};
pub use piggyback::{
    compare_costs, emergency_compact, extract_summary, is_context_length_error, tool_definition,
    CompactSummary, CostComparison, EmergencyCompactResult, PiggybackResult, PiggybackState,
    STRONG_SYSTEM_PROMPT_SUFFIX, SYSTEM_PROMPT_SUFFIX,
};
pub use pipeline::CompactPipeline;
pub use plan::CompactionPlan;
pub use tiers::{SnipTier, SummarizeTier, TierResult, TruncateTier};
