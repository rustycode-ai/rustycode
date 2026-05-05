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
