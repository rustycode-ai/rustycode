//! Token compaction state.
//!
//! Groups the context monitor, compaction config, and UI flags so all
//! compaction-related concerns have a single owner.

use crate::memory::compaction::{CompactionConfig, ContextMonitor};

/// State for token compaction: monitors context usage, holds configuration,
/// and tracks whether a compaction preview is active.
#[derive(Debug)]
#[non_exhaustive]
pub struct CompactionState {
    /// Tracks token usage against the model's context window
    pub context_monitor: ContextMonitor,
    /// Compaction configuration (thresholds, strategy, model)
    pub compaction_config: CompactionConfig,
    /// Whether the compaction preview modal is visible
    pub showing_preview: bool,
    /// Whether a compaction operation is pending user confirmation
    pub pending: bool,
}

impl CompactionState {
    pub fn new(context_monitor: ContextMonitor, compaction_config: CompactionConfig) -> Self {
        Self {
            context_monitor,
            compaction_config,
            showing_preview: false,
            pending: false,
        }
    }

    /// Reset both UI flags (after dismissal or cancellation).
    pub fn clear_flags(&mut self) {
        self.showing_preview = false;
        self.pending = false;
    }
}

impl Default for CompactionState {
    fn default() -> Self {
        let config = CompactionConfig::default();
        Self::new(
            ContextMonitor::new(config.effective_max_tokens(), config.warning_threshold),
            config,
        )
    }
}
