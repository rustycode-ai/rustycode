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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_defaults() {
        let status = LspStatus::new();
        assert!(status.last_lsp_servers.is_empty());
        assert!(!status.last_lsp_connected);
    }

    #[test]
    fn default_matches_new() {
        let from_new = LspStatus::new();
        let from_default = LspStatus::default();
        assert_eq!(from_new.last_lsp_servers, from_default.last_lsp_servers);
        assert_eq!(from_new.last_lsp_connected, from_default.last_lsp_connected);
    }

    #[test]
    fn forced_refresh_has_stale_timestamp() {
        let normal = LspStatus::new();
        let forced = LspStatus::new_forced_refresh();
        assert!(forced.last_lsp_refresh < normal.last_lsp_refresh);
    }
}
