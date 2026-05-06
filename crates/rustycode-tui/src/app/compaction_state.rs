use crate::memory::compaction::{CompactionConfig, ContextMonitor};

#[derive(Debug)]
#[non_exhaustive]
pub struct CompactionState {
    pub context_monitor: ContextMonitor,
    pub compaction_config: CompactionConfig,
    pub showing_preview: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_initializes_flags_false() {
        let state = CompactionState::default();
        assert!(!state.showing_preview);
        assert!(!state.pending);
    }

    #[test]
    fn clear_flags_resets_both() {
        let mut state = CompactionState::default();
        state.showing_preview = true;
        state.pending = true;
        state.clear_flags();
        assert!(!state.showing_preview);
        assert!(!state.pending);
    }

    #[test]
    fn new_uses_provided_config() {
        let config = CompactionConfig::default();
        let monitor = ContextMonitor::new(config.effective_max_tokens(), config.warning_threshold);
        let state = CompactionState::new(monitor, config);
        assert!(!state.showing_preview);
        assert!(!state.pending);
    }
}
