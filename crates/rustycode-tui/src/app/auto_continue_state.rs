#[non_exhaustive]
pub(crate) struct AutoContinueState {
    auto_continue_enabled: bool,
    auto_continue_pending: bool,
    auto_continue_iterations: usize,
}

impl AutoContinueState {
    pub(crate) fn new() -> Self {
        Self {
            auto_continue_enabled: false,
            auto_continue_pending: false,
            auto_continue_iterations: 0,
        }
    }

    pub(crate) fn from_env() -> Self {
        Self {
            auto_continue_enabled: std::env::var("RUSTYCODE_AUTO_CONTINUE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            ..Self::new()
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.auto_continue_enabled
    }

    pub(crate) fn toggle(&mut self) {
        self.auto_continue_enabled = !self.auto_continue_enabled;
        self.auto_continue_iterations = 0;
    }

    pub(crate) fn disable(&mut self) {
        self.auto_continue_enabled = false;
        self.auto_continue_pending = false;
        self.auto_continue_iterations = 0;
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.auto_continue_pending
    }

    pub(crate) fn mark_pending(&mut self) {
        self.auto_continue_pending = true;
    }

    pub(crate) fn clear_pending(&mut self) {
        self.auto_continue_pending = false;
    }

    pub(crate) fn iterations(&self) -> usize {
        self.auto_continue_iterations
    }

    pub(crate) fn increment_iterations(&mut self) {
        self.auto_continue_iterations += 1;
    }

    pub(crate) fn reset_iterations(&mut self) {
        self.auto_continue_iterations = 0;
    }

    pub(crate) fn reset(&mut self) {
        self.auto_continue_enabled = false;
        self.auto_continue_pending = false;
        self.auto_continue_iterations = 0;
    }
}

impl Default for AutoContinueState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_disabled() {
        let state = AutoContinueState::new();
        assert!(!state.is_enabled());
        assert!(!state.is_pending());
        assert_eq!(state.iterations(), 0);
    }

    #[test]
    fn default_matches_new() {
        let from_new = AutoContinueState::new();
        let from_default = AutoContinueState::default();
        assert_eq!(from_new.is_enabled(), from_default.is_enabled());
        assert_eq!(from_new.is_pending(), from_default.is_pending());
        assert_eq!(from_new.iterations(), from_default.iterations());
    }

    #[test]
    fn reset_clears_all_fields() {
        let mut state = AutoContinueState::new();
        state.toggle();
        state.mark_pending();
        state.increment_iterations();
        state.increment_iterations();

        state.reset();

        assert!(!state.is_enabled());
        assert!(!state.is_pending());
        assert_eq!(state.iterations(), 0);
    }

    #[test]
    fn toggle_flips_enabled_and_resets_iterations() {
        let mut state = AutoContinueState::new();
        assert!(!state.is_enabled());
        state.increment_iterations();
        assert_eq!(state.iterations(), 1);

        state.toggle();
        assert!(state.is_enabled());
        assert_eq!(state.iterations(), 0);

        state.toggle();
        assert!(!state.is_enabled());
    }

    #[test]
    fn disable_resets_everything() {
        let mut state = AutoContinueState::new();
        state.toggle();
        state.mark_pending();
        state.increment_iterations();

        state.disable();

        assert!(!state.is_enabled());
        assert!(!state.is_pending());
        assert_eq!(state.iterations(), 0);
    }

    #[test]
    fn pending_lifecycle() {
        let mut state = AutoContinueState::new();
        assert!(!state.is_pending());
        state.mark_pending();
        assert!(state.is_pending());
        state.clear_pending();
        assert!(!state.is_pending());
    }

    #[test]
    fn iterations_increment_and_reset() {
        let mut state = AutoContinueState::new();
        assert_eq!(state.iterations(), 0);
        state.increment_iterations();
        state.increment_iterations();
        state.increment_iterations();
        assert_eq!(state.iterations(), 3);
        state.reset_iterations();
        assert_eq!(state.iterations(), 0);
    }
}
