// ── Context Module ────────────────────────────────────────────────────────────
//
// This module provides context window management, token budget tracking,
// prioritization, compression, and assembly for LLM interactions.
// It was consolidated from former context_management/ and context_prio/ submodules.

// ── Token budget & enforcement ──────────────────────────────────────────────
pub mod budget;
pub mod budget_enforcement;
pub mod token_counter;

// ── Ignore / LRU cache ──────────────────────────────────────────────────────
pub mod ignore;
pub mod lru_cache;

// ── Window management (formerly context_management/) ────────────────────────
pub mod assembler;
pub mod auto_compact;
pub mod compression;
pub mod pruner;
pub mod quality;
pub mod window;

// ── Prioritization & scoring (formerly context_prio/) ───────────────────────
pub mod scoring;
pub mod types;

// ── Re-exports ──────────────────────────────────────────────────────────────
pub use assembler::{AssemblyMetrics, ContextAssembler};
pub use auto_compact::{
    auto_compact_if_needed, auto_compact_with_threshold, should_compact, should_compact_default,
    CompactionEvent, CompactionMetrics, DEFAULT_COMPACT_THRESHOLD,
};
pub use budget::ContextBudget;
pub use budget_enforcement::{enforce_budget, enforce_budget_prioritized};
pub use compression::{compress_context, CompressionResult, CompressionStrategy};
pub use ignore::RustyCodeIgnore;
pub use lru_cache::LruCache;
pub use pruner::ContextPruner;
pub use quality::{QualityMetrics, QualityTrend};
pub use scoring::{
    frequency_score, keyword_relevance_score, recency_score, select_best, select_knapsack, sort_by,
    SortStrategy,
};
pub use token_counter::{CachedTokenCounter, ChatMessageInfo, TokenCounter, TokenProvider};
pub use types::{ContextItem, Metadata, Priority};
pub use window::{ContextWindow, WindowMetadata};
