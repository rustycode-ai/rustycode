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
