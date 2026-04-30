// ── Context Prioritization Module ──────────────────────────────────────────────
//
// This module provides scoring and prioritization logic for context items.
// It helps maximize the value of limited context budget by ranking items
// by their importance and relevance.

pub mod scoring;
pub mod types;

// Re-exports for backward compatibility
pub use scoring::{
    frequency_score, keyword_relevance_score, recency_score, select_best, select_knapsack, sort_by,
    SortStrategy,
};
pub use types::{ContextItem, Metadata, Priority};
