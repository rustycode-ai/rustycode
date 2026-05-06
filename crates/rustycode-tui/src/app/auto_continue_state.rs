#[non_exhaustive]
pub(crate) struct AutoContinueState {
    pub(crate) auto_continue_enabled: bool,
    pub(crate) auto_continue_pending: bool,
    pub(crate) auto_continue_iterations: usize,
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
        assert!(!state.auto_continue_enabled);
        assert!(!state.auto_continue_pending);
        assert_eq!(state.auto_continue_iterations, 0);
    }

    #[test]
    fn default_matches_new() {
        let from_new = AutoContinueState::new();
        let from_default = AutoContinueState::default();
        assert_eq!(from_new.auto_continue_enabled, from_default.auto_continue_enabled);
        assert_eq!(from_new.auto_continue_pending, from_default.auto_continue_pending);
        assert_eq!(
            from_new.auto_continue_iterations,
            from_default.auto_continue_iterations
        );
    }

    #[test]
    fn reset_clears_all_fields() {
        let mut state = AutoContinueState::new();
        state.auto_continue_enabled = true;
        state.auto_continue_pending = true;
        state.auto_continue_iterations = 42;

        state.reset();

        assert!(!state.auto_continue_enabled);
        assert!(!state.auto_continue_pending);
        assert_eq!(state.auto_continue_iterations, 0);
    }
}
