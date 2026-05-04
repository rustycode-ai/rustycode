// ── Context Window Management Module ────────────────────────────────────────────
//
// This module provides intelligent context window management for LLM interactions.
// It handles token budgeting, compression strategies, smart assembly, and quality
// metrics to maximize the value of limited context windows.

pub mod assembler;
pub mod auto_compact;
pub mod compression;
pub mod pruner;
pub mod quality;
pub mod window;

// Re-exports for backward compatibility
pub use assembler::{AssemblyMetrics, ContextAssembler};
pub use auto_compact::{
    auto_compact_if_needed, auto_compact_with_threshold, should_compact, should_compact_default,
    CompactionEvent, CompactionMetrics, DEFAULT_COMPACT_THRESHOLD,
};
pub use compression::{compress_context, CompressionResult, CompressionStrategy};
pub use pruner::ContextPruner;
pub use quality::{QualityMetrics, QualityTrend};
pub use window::{ContextWindow, WindowMetadata};
