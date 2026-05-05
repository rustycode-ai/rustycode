use crate::ui::session_sidebar::McpServerStatus;
use std::time::{Duration, Instant};

pub(crate) struct McpStatus {
    pub(crate) last_mcp_refresh: Instant,
    pub(crate) last_mcp_servers: Vec<McpServerStatus>,
    pub(crate) last_mcp_connected: bool,
}

impl McpStatus {
    pub(crate) fn new() -> Self {
        Self {
            last_mcp_refresh: Instant::now(),
            last_mcp_servers: Vec::new(),
            last_mcp_connected: false,
        }
    }

    pub(crate) fn new_forced_refresh() -> Self {
        Self {
            last_mcp_refresh: Instant::now() - Duration::from_secs(60),
            ..Self::new()
        }
    }
}

impl Default for McpStatus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_defaults() {
        let status = McpStatus::new();
        assert!(status.last_mcp_servers.is_empty());
        assert!(!status.last_mcp_connected);
    }

    #[test]
    fn default_matches_new() {
        let from_new = McpStatus::new();
        let from_default = McpStatus::default();
        assert_eq!(from_new.last_mcp_servers, from_default.last_mcp_servers);
        assert_eq!(
            from_new.last_mcp_connected,
            from_default.last_mcp_connected
        );
    }

    #[test]
    fn forced_refresh_has_stale_timestamp() {
        let normal = McpStatus::new();
        let forced = McpStatus::new_forced_refresh();
        assert!(forced.last_mcp_refresh < normal.last_mcp_refresh);
    }
}
