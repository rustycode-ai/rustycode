use crate::memory::compaction::{CompactionConfig, ContextMonitor};

#[derive(Debug)]
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

    pub fn is_previewing(&self) -> bool {
        self.showing_preview
    }

    pub fn show_preview(&mut self) {
        self.showing_preview = true;
    }

    pub fn dismiss_preview(&mut self) {
        self.showing_preview = false;
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    pub fn mark_pending(&mut self) {
        self.pending = true;
    }

    pub fn clear_pending(&mut self) {
        self.pending = false;
    }

    pub fn usage_percentage(&self) -> f64 {
        self.context_monitor.usage_percentage()
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

    #[test]
    fn show_and_dismiss_preview() {
        let mut state = CompactionState::default();
        assert!(!state.is_previewing());
        state.show_preview();
        assert!(state.is_previewing());
        state.dismiss_preview();
        assert!(!state.is_previewing());
    }

    #[test]
    fn pending_lifecycle() {
        let mut state = CompactionState::default();
        assert!(!state.is_pending());
        state.mark_pending();
        assert!(state.is_pending());
        state.clear_pending();
        assert!(!state.is_pending());
    }

    #[test]
    fn clear_flags_resets_preview_and_pending() {
        let mut state = CompactionState::default();
        state.show_preview();
        state.mark_pending();
        assert!(state.is_previewing());
        assert!(state.is_pending());
        state.clear_flags();
        assert!(!state.is_previewing());
        assert!(!state.is_pending());
    }

    #[test]
    fn usage_percentage_delegates_to_monitor() {
        let state = CompactionState::default();
        let pct = state.usage_percentage();
        assert!(pct >= 0.0);
        assert!(pct <= 1.0);
    }

    /// Regression: Ctrl+C handler must be able to escape the preview by clearing flags.
    #[test]
    fn clear_flags_allows_ctrl_c_escape() {
        let mut state = CompactionState::default();
        state.show_preview();
        state.mark_pending();
        assert!(state.is_previewing());
        // Simulate what the Ctrl+C handler does — clear flags
        state.clear_flags();
        assert!(!state.is_previewing());
        assert!(!state.is_pending());
    }

    /// Regression: show_preview() must not accidentally set the pending flag.
    #[test]
    fn show_preview_does_not_set_pending() {
        let mut state = CompactionState::default();
        state.show_preview();
        assert!(state.is_previewing());
        assert!(!state.is_pending()); // pending is separate
    }

    /// Regression: dismiss_preview() must not clear the pending flag.
    #[test]
    fn dismiss_preview_does_not_clear_pending() {
        let mut state = CompactionState::default();
        state.show_preview();
        state.mark_pending();
        state.dismiss_preview();
        assert!(!state.is_previewing());
        assert!(state.is_pending()); // pending survives dismiss
        state.clear_pending();
        assert!(!state.is_pending());
    }
}
