//! Prompt cache management for orchestration.
//!
//! Tracks cached system prompts and tool definitions with SHA-256-based change
//! detection, enabling token-efficient reuse of previously sent content.

pub mod cache_metrics;
pub mod prompt_cache_manager;

pub use cache_metrics::CacheMetrics;
pub use prompt_cache_manager::PromptCacheManager;
