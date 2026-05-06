//! Thin adapter bridging TUI compaction config to the hybrid `CompactPipeline`.
//!
//! The TUI's existing `ContextMonitor` and `CompactionConfig` (in
//! `memory::compaction`) continue to work for now. This adapter provides a path
//! to migrate incrementally: the TUI can call `CompactPipelineAdapter::compact()`
//! instead of its own compaction logic, while still using its own config for
//! other purposes.

use rustycode_protocol::compaction::HybridCompactionConfig;
use rustycode_runtime::compaction::CompactPipeline;

/// Adapter bridging TUI compaction config to the hybrid pipeline.
///
/// Maps TUI-specific settings to [`HybridCompactionConfig`] defaults where
/// direct equivalents don't exist.
#[non_exhaustive]
pub struct CompactPipelineAdapter {
    pipeline: CompactPipeline,
}

impl CompactPipelineAdapter {
    /// Create adapter from the TUI's own config values.
    ///
    /// Parameters map to the corresponding fields on
    /// [`HybridCompactionConfig`]. The `compaction_buffer_tokens` field has
    /// no direct TUI equivalent and defaults to 6 000 tokens.
    #[must_use]
    pub fn new(
        trigger_threshold_pct: f64,
        target_pct: f64,
        max_passes: usize,
        tail_turns: usize,
        max_tool_output_lines: usize,
    ) -> Self {
        let config = HybridCompactionConfig {
            trigger_threshold_pct,
            target_pct,
            max_tightening_passes: max_passes,
            initial_tail_turns: tail_turns,
            max_tool_output_lines,
            compaction_buffer_tokens: 6000,
        };
        Self {
            pipeline: CompactPipeline::new(config),
        }
    }

    /// Create adapter with default config values.
    #[must_use]
    pub fn with_defaults() -> Self {
        let defaults = HybridCompactionConfig::default();
        Self::new(
            defaults.trigger_threshold_pct,
            defaults.target_pct,
            defaults.max_tightening_passes,
            defaults.initial_tail_turns,
            defaults.max_tool_output_lines,
        )
    }

    /// Access the underlying pipeline for direct use.
    #[must_use]
    pub fn pipeline(&self) -> &CompactPipeline {
        &self.pipeline
    }
}

impl Default for CompactPipelineAdapter {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_creates_with_custom_config() {
        let adapter = CompactPipelineAdapter::new(0.80, 0.45, 2, 1, 30);
        assert!(!adapter.pipeline().is_compacting());
    }

    #[test]
    fn adapter_with_defaults_works() {
        let adapter = CompactPipelineAdapter::with_defaults();
        assert!(!adapter.pipeline().is_compacting());
    }

    #[test]
    fn default_trait_matches_with_defaults() {
        let from_trait = CompactPipelineAdapter::default();
        let from_method = CompactPipelineAdapter::with_defaults();
        // Both should report the same compaction state (not compacting).
        assert_eq!(
            from_trait.pipeline().is_compacting(),
            from_method.pipeline().is_compacting()
        );
    }
}
