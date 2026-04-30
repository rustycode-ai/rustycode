// ── Context Window Management Module ────────────────────────────────────────────
//
// This module provides intelligent context window management for LLM interactions.
// It handles token budgeting, compression strategies, smart assembly, and quality
// metrics to maximize the value of limited context windows.

pub mod assembler;
pub mod compression;
pub mod pruner;
pub mod quality;
pub mod window;

// Re-exports for backward compatibility
pub use assembler::{AssemblyMetrics, ContextAssembler};
pub use compression::{compress_context, CompressionResult, CompressionStrategy};
pub use pruner::ContextPruner;
pub use quality::{QualityMetrics, QualityTrend};
pub use window::{ContextWindow, WindowMetadata};
