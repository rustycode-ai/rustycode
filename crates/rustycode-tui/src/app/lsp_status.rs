use std::time::{Duration, Instant};

pub(crate) struct LspStatus {
    pub(crate) last_lsp_refresh: Instant,
    pub(crate) last_lsp_servers: Vec<String>,
    pub(crate) last_lsp_connected: bool,
}

impl LspStatus {
    pub(crate) fn new() -> Self {
        Self {
            last_lsp_refresh: Instant::now(),
            last_lsp_servers: Vec::new(),
            last_lsp_connected: false,
        }
    }

    pub(crate) fn new_forced_refresh() -> Self {
        Self {
            last_lsp_refresh: Instant::now() - Duration::from_secs(60),
            ..Self::new()
        }
    }
}

impl Default for LspStatus {
    fn default() -> Self {
        Self::new()
    }
}
