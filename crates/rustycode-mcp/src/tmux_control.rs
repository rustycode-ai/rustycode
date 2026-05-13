//! tmux-backed MCP control plane for agent-driven workflows.

use crate::types::{McpContent, McpTool, McpToolResult};
use crate::{McpError, McpResult, McpServer};
use chrono::Utc;
use rustycode_connector::{
    CapturePaneOptions, ConnectorError, ITerm2NativeConnector, It2Connector, Key, Region,
    ScreenshotLayer, ScreenshotOptions, SessionInfo, SplitDirection, TerminalConnector,
    TerminalSessionId, TmuxBatch, TmuxConnector, TmuxOp, TmuxResult, WindowInfo,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TmuxMcpConfig {
    pub workspace_root: PathBuf,
    pub session_prefix: String,
    pub preferred_backend: TerminalBackendKind,
    pub default_ttl_secs: u64,
    pub capture_lines: usize,
    pub command_timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[value(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TerminalBackendKind {
    Auto,
    Tmux,
    It2,
    Iterm2Native,
}

impl Default for TmuxMcpConfig {
    fn default() -> Self {
        Self {
            workspace_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            session_prefix: "rustycode".to_string(),
            preferred_backend: TerminalBackendKind::Auto,
            default_ttl_secs: 3600,
            capture_lines: 200,
            command_timeout_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLease {
    pub session_id: String,
    pub session_name: String,
    pub workspace_root: String,
    pub created_at: String,
    pub last_used_at: String,
    pub ttl_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandOrigin {
    Tmux,
    Workspace,
}

#[derive(Debug)]
pub struct CommandRecord {
    pub command_id: String,
    pub origin: CommandOrigin,
    pub session_id: Option<String>,
    pub pane_index: Option<usize>,
    pub command: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

#[derive(Debug, Default)]
struct TmuxState {
    leases: HashMap<String, SessionLease>,
    commands: HashMap<String, CommandRecord>,
}

enum TerminalBackend {
    Tmux(TmuxConnector),
    It2(It2Connector),
    Iterm2Native(ITerm2NativeConnector),
}

impl TerminalBackend {
    fn tmux(config: &TmuxMcpConfig) -> Self {
        Self::Tmux(TmuxConnector::new(&config.session_prefix))
    }

    const fn it2() -> Self {
        Self::It2(It2Connector::new())
    }

    const fn iterm2_native() -> Self {
        Self::Iterm2Native(ITerm2NativeConnector::new())
    }

    const fn kind(&self) -> TerminalBackendKind {
        match self {
            Self::Tmux(_) => TerminalBackendKind::Tmux,
            Self::It2(_) => TerminalBackendKind::It2,
            Self::Iterm2Native(_) => TerminalBackendKind::Iterm2Native,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Tmux(connector) => connector.name(),
            Self::It2(connector) => connector.name(),
            Self::Iterm2Native(connector) => connector.name(),
        }
    }

    fn is_available(&self) -> bool {
        match self {
            Self::Tmux(connector) => connector.is_available(),
            Self::It2(connector) => connector.is_available(),
            Self::Iterm2Native(connector) => connector.is_available(),
        }
    }

    fn create_session(&mut self, name: &str) -> Result<TerminalSessionId, ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.create_session(name),
            Self::It2(connector) => connector.create_session(name),
            Self::Iterm2Native(connector) => connector.create_session(name),
        }
    }

    fn close_session(&mut self, session: &TerminalSessionId) -> Result<(), ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.close_session(session),
            Self::It2(connector) => connector.close_session(session),
            Self::Iterm2Native(connector) => connector.close_session(session),
        }
    }

    fn session_info(&self, session: &TerminalSessionId) -> Result<SessionInfo, ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.session_info(session),
            Self::It2(connector) => connector.session_info(session),
            Self::Iterm2Native(connector) => connector.session_info(session),
        }
    }

    fn list_sessions(&self) -> Result<Vec<SessionInfo>, ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.list_sessions(),
            Self::It2(connector) => connector.list_sessions(),
            Self::Iterm2Native(connector) => connector.list_sessions(),
        }
    }

    fn split_pane(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        direction: SplitDirection,
    ) -> Result<usize, ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.split_pane(session, pane_index, direction),
            Self::It2(connector) => connector.split_pane(session, pane_index, direction),
            Self::Iterm2Native(connector) => connector.split_pane(session, pane_index, direction),
        }
    }

    fn send_keys(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        keys: &str,
    ) -> Result<(), ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.send_keys(session, pane_index, keys),
            Self::It2(connector) => connector.send_keys(session, pane_index, keys),
            Self::Iterm2Native(connector) => connector.send_keys(session, pane_index, keys),
        }
    }

    fn capture_pane(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        options: CapturePaneOptions,
    ) -> Result<rustycode_connector::PaneContent, ConnectorError> {
        match self {
            Self::Tmux(connector) => {
                connector.capture_pane_with_options(session, pane_index, options)
            }
            Self::It2(connector) => connector.capture_output(session, pane_index),
            Self::Iterm2Native(connector) => connector.capture_output(session, pane_index),
        }
    }

    fn wait_for_output(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        pattern: &str,
        timeout_secs: Option<u64>,
    ) -> Result<rustycode_connector::PaneContent, ConnectorError> {
        match self {
            Self::Tmux(connector) => {
                connector.wait_for_output(session, pane_index, pattern, timeout_secs)
            }
            Self::It2(connector) => {
                connector.wait_for_output(session, pane_index, pattern, timeout_secs)
            }
            Self::Iterm2Native(connector) => {
                connector.wait_for_output(session, pane_index, pattern, timeout_secs)
            }
        }
    }

    // -- Tmux-specific methods (return NotAvailable for other backends) --

    fn tmux_type(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        text: &str,
    ) -> Result<(), ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.tmux_type(session, pane_index, text),
            _ => Err(ConnectorError::NotAvailable(
                "typed keys require tmux backend".into(),
            )),
        }
    }

    fn tmux_send_keys(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        keys: &[Key],
    ) -> Result<(), ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.tmux_send_keys(session, pane_index, keys),
            _ => Err(ConnectorError::NotAvailable(
                "typed keys require tmux backend".into(),
            )),
        }
    }

    fn list_windows(&self, session: &TerminalSessionId) -> Result<Vec<WindowInfo>, ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.list_windows(session),
            _ => Err(ConnectorError::NotAvailable(
                "window management requires tmux backend".into(),
            )),
        }
    }

    fn new_window(
        &mut self,
        session: &TerminalSessionId,
        name: Option<&str>,
    ) -> Result<String, ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.new_window(session, name),
            _ => Err(ConnectorError::NotAvailable(
                "window management requires tmux backend".into(),
            )),
        }
    }

    fn kill_window(&self, target: &str) -> Result<(), ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.kill_window(target),
            _ => Err(ConnectorError::NotAvailable(
                "window management requires tmux backend".into(),
            )),
        }
    }

    fn rename_window(&self, target: &str, name: &str) -> Result<(), ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.rename_window(target, name),
            _ => Err(ConnectorError::NotAvailable(
                "window management requires tmux backend".into(),
            )),
        }
    }

    fn select_window(&self, target: &str) -> Result<(), ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.select_window(target),
            _ => Err(ConnectorError::NotAvailable(
                "window management requires tmux backend".into(),
            )),
        }
    }

    fn capture_screenshot(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        options: &ScreenshotOptions,
    ) -> Result<rustycode_connector::Screenshot, ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.capture_screenshot(session, pane_index, options),
            _ => Err(ConnectorError::NotAvailable(
                "screenshots require tmux backend".into(),
            )),
        }
    }

    fn execute_batch(&self, batch: &TmuxBatch) -> Result<Vec<TmuxResult>, ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.execute_batch(batch),
            _ => Err(ConnectorError::NotAvailable(
                "batch operations require tmux backend".into(),
            )),
        }
    }
}

/// tmux MCP adapter that manages tmux sessions and related execution state.
pub struct TmuxMcpServer {
    config: TmuxMcpConfig,
    backend: Arc<Mutex<TerminalBackend>>,
    state: Arc<Mutex<TmuxState>>,
}

impl TmuxMcpServer {
    fn select_backend(config: &TmuxMcpConfig, preferred: TerminalBackendKind) -> TerminalBackend {
        match preferred {
            TerminalBackendKind::Tmux => TerminalBackend::tmux(config),
            TerminalBackendKind::It2 => TerminalBackend::it2(),
            TerminalBackendKind::Iterm2Native => TerminalBackend::iterm2_native(),
            TerminalBackendKind::Auto => {
                let candidates = [
                    TerminalBackend::tmux(config),
                    TerminalBackend::iterm2_native(),
                    TerminalBackend::it2(),
                ];
                candidates
                    .into_iter()
                    .find(TerminalBackend::is_available)
                    .unwrap_or_else(|| TerminalBackend::tmux(config))
            }
        }
    }

    fn with_backend_mut<R>(
        &self,
        f: impl FnOnce(&mut TerminalBackend) -> Result<R, ConnectorError>,
    ) -> McpResult<R> {
        let mut backend = self
            .backend
            .lock()
            .map_err(|e| McpError::InternalError(e.to_string()))?;
        f(&mut backend).map_err(mux_err)
    }

    fn with_backend_ref<R>(
        &self,
        f: impl FnOnce(&TerminalBackend) -> Result<R, ConnectorError>,
    ) -> McpResult<R> {
        let backend = self
            .backend
            .lock()
            .map_err(|e| McpError::InternalError(e.to_string()))?;
        f(&backend).map_err(mux_err)
    }

    pub fn new(config: TmuxMcpConfig) -> Self {
        Self {
            backend: Arc::new(Mutex::new(Self::select_backend(
                &config,
                TerminalBackendKind::Tmux,
            ))),
            state: Arc::new(Mutex::new(TmuxState::default())),
            config,
        }
    }

    pub fn auto(config: TmuxMcpConfig) -> Self {
        Self {
            backend: Arc::new(Mutex::new(Self::select_backend(
                &config,
                config.preferred_backend,
            ))),
            state: Arc::new(Mutex::new(TmuxState::default())),
            config,
        }
    }

    pub fn with_backend_kind(
        config: TmuxMcpConfig,
        preferred_backend: TerminalBackendKind,
    ) -> Self {
        Self {
            backend: Arc::new(Mutex::new(Self::select_backend(&config, preferred_backend))),
            state: Arc::new(Mutex::new(TmuxState::default())),
            config,
        }
    }

    pub fn with_it2(config: TmuxMcpConfig) -> Self {
        Self {
            backend: Arc::new(Mutex::new(Self::select_backend(
                &config,
                TerminalBackendKind::It2,
            ))),
            state: Arc::new(Mutex::new(TmuxState::default())),
            config,
        }
    }

    pub fn with_iterm2_native(config: TmuxMcpConfig) -> Self {
        Self {
            backend: Arc::new(Mutex::new(Self::select_backend(
                &config,
                TerminalBackendKind::Iterm2Native,
            ))),
            state: Arc::new(Mutex::new(TmuxState::default())),
            config,
        }
    }

    pub fn tool_definitions() -> Vec<McpTool> {
        Self::tool_definitions_for("terminal", "terminal")
    }

    pub fn legacy_tool_definitions() -> Vec<McpTool> {
        Self::tool_definitions_for("tmux", "tmux")
    }

    fn tool_definitions_for(namespace: &str, backend_label: &str) -> Vec<McpTool> {
        vec![
            tool(
                &format!("{namespace}.create_session"),
                &format!("Create a leased {backend_label} session for agent-driven work."),
                json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "ttl_secs": {"type": "integer"},
                    },
                    "required": ["name"]
                }),
            ),
            tool(
                &format!("{namespace}.list_sessions"),
                &format!("List {backend_label} sessions created by this MCP server."),
                json!({"type": "object", "properties": {}}),
            ),
            tool(
                &format!("{namespace}.session_info"),
                &format!("Inspect a {backend_label} session and its panes."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                    },
                    "required": ["session_id"]
                }),
            ),
            tool(
                &format!("{namespace}.close_session"),
                &format!("Close a {backend_label} session and clean up its lease."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                    },
                    "required": ["session_id"]
                }),
            ),
            tool(
                &format!("{namespace}.split_pane"),
                &format!("Split a pane in a {backend_label} session."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "pane_index": {"type": "integer"},
                        "direction": {"type": "string", "enum": ["horizontal", "vertical"]},
                    },
                    "required": ["session_id", "pane_index", "direction"]
                }),
            ),
            tool(
                &format!("{namespace}.send_keys"),
                &format!("Send keys or a command to a {backend_label} pane."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "pane_index": {"type": "integer"},
                        "keys": {"type": "string"},
                    },
                    "required": ["session_id", "pane_index", "keys"]
                }),
            ),
            tool(
                &format!("{namespace}.capture_pane"),
                &format!("Capture pane output from a {backend_label} pane, optionally preserving escape sequences."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "pane_index": {"type": "integer"},
                        "start": {"type": "integer"},
                        "end": {"type": "integer"},
                        "include_escape_sequences": {"type": "boolean"},
                        "join_wrapped_lines": {"type": "boolean"},
                    },
                    "required": ["session_id", "pane_index"]
                }),
            ),
            tool(
                &format!("{namespace}.execute_command"),
                &format!("Send a command to a {backend_label} pane and track it as an execution handle."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "pane_index": {"type": "integer"},
                        "command": {"type": "string"},
                        "track_result": {"type": "boolean"},
                    },
                    "required": ["session_id", "pane_index", "command"]
                }),
            ),
            tool(
                &format!("{namespace}.get_command_result"),
                "Read the current state of a tracked command.",
                json!({
                    "type": "object",
                    "properties": {
                        "command_id": {"type": "string"},
                    },
                    "required": ["command_id"]
                }),
            ),
            tool(
                &format!("{namespace}.wait_for_output"),
                &format!("Wait for output to appear in a {backend_label} pane."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "pane_index": {"type": "integer"},
                        "pattern": {"type": "string"},
                        "timeout_secs": {"type": "integer"},
                    },
                    "required": ["session_id", "pane_index", "pattern"]
                }),
            ),
            tool(
                &format!("{namespace}.reap_leases"),
                &format!("Remove expired {backend_label} session leases and clean up their sessions."),
                json!({
                    "type": "object",
                    "properties": {}
                }),
            ),
            tool(
                &format!("{namespace}.type_keys"),
                &format!("Send typed keys or key combinations to a {backend_label} pane (tmux only). Supports control keys like ctrl-c, arrows, etc."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "pane_index": {"type": "integer"},
                        "keys": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Array of key strings. Use 'ctrl-c', 'alt-enter', 'Up', 'Enter', or plain text."
                        },
                        "auto_enter": {
                            "type": "boolean",
                            "description": "If true and keys is a single text string, append Enter automatically."
                        },
                    },
                    "required": ["session_id", "pane_index", "keys"]
                }),
            ),
            tool(
                &format!("{namespace}.screenshot"),
                &format!("Take a token-optimized screenshot of a {backend_label} pane (tmux only). Returns layered text + cursor data."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "pane_index": {"type": "integer"},
                        "layers": {
                            "type": "array",
                            "items": {"type": "string", "enum": ["text", "cursor", "fg_colors", "bg_colors", "styles"]},
                            "description": "Data layers to include. Default: [\"text\", \"cursor\"]"
                        },
                        "region": {
                            "type": "object",
                            "properties": {
                                "left": {"type": "integer"},
                                "top": {"type": "integer"},
                                "width": {"type": "integer"},
                                "height": {"type": "integer"}
                            },
                            "description": "Rectangular region to capture (full screen if omitted)."
                        },
                        "around_cursor": {
                            "type": "integer",
                            "description": "Number of lines around cursor to include (mutually exclusive with region)."
                        },
                        "compact": {
                            "type": "boolean",
                            "description": "Skip empty lines to reduce token usage."
                        },
                    },
                    "required": ["session_id", "pane_index"]
                }),
            ),
            tool(
                &format!("{namespace}.list_windows"),
                &format!("List windows in a {backend_label} session (tmux only)."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                    },
                    "required": ["session_id"]
                }),
            ),
            tool(
                &format!("{namespace}.new_window"),
                &format!("Create a new window in a {backend_label} session (tmux only)."),
                json!({
                    "type": "object",
                    "properties": {
                        "session_id": {"type": "string"},
                        "name": {"type": "string", "description": "Optional window name."},
                    },
                    "required": ["session_id"]
                }),
            ),
            tool(
                &format!("{namespace}.kill_window"),
                &format!("Kill a {backend_label} window (tmux only)."),
                json!({
                    "type": "object",
                    "properties": {
                        "target": {"type": "string", "description": "Window target (e.g., 'session_name:window_index')."},
                    },
                    "required": ["target"]
                }),
            ),
            tool(
                &format!("{namespace}.rename_window"),
                &format!("Rename a {backend_label} window (tmux only)."),
                json!({
                    "type": "object",
                    "properties": {
                        "target": {"type": "string", "description": "Window target."},
                        "name": {"type": "string"},
                    },
                    "required": ["target", "name"]
                }),
            ),
            tool(
                &format!("{namespace}.select_window"),
                &format!("Select/activate a {backend_label} window (tmux only)."),
                json!({
                    "type": "object",
                    "properties": {
                        "target": {"type": "string", "description": "Window target."},
                    },
                    "required": ["target"]
                }),
            ),
            tool(
                &format!("{namespace}.execute_batch"),
                "Execute a batch of tmux operations atomically (tmux only).",
                json!({
                    "type": "object",
                    "properties": {
                        "operations": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "kind": {
                                        "type": "string",
                                        "enum": [
                                            "new_session", "kill_session", "split_pane",
                                            "send_keys", "send_text", "capture_pane",
                                            "new_window", "kill_pane", "resize_pane",
                                            "swap_pane", "select_pane", "select_layout",
                                            "set_pane_title"
                                        ]
                                    },
                                    "target": {"type": "string"},
                                    "text": {"type": "string"},
                                    "keys": {"type": "array", "items": {"type": "string"}},
                                    "direction": {"type": "string", "enum": ["horizontal", "vertical"]},
                                    "layout": {"type": "string"},
                                    "title": {"type": "string"},
                                    "cells": {"type": "integer"},
                                    "src": {"type": "string"},
                                    "dst": {"type": "string"},
                                    "name": {"type": "string"},
                                    "start_dir": {"type": "string"},
                                    "start": {"type": "integer"},
                                    "end": {"type": "integer"},
                                },
                                "required": ["kind"]
                            },
                        },
                    },
                    "required": ["operations"]
                }),
            ),
            tool(
                "workspace.exec",
                "Run a command in the workspace root and return captured output.",
                json!({
                    "type": "object",
                    "properties": {
                        "command": {"type": "string"},
                        "timeout_secs": {"type": "integer"},
                    },
                    "required": ["command"]
                }),
            ),
            tool(
                "workspace.run_tests",
                "Run cargo test in the workspace root, optionally with a filter.",
                json!({
                    "type": "object",
                    "properties": {
                        "filter": {"type": "string"},
                        "timeout_secs": {"type": "integer"},
                    },
                    "required": []
                }),
            ),
        ]
    }

    pub async fn register_into(&self, server: &McpServer) -> McpResult<()> {
        let state = self.state.clone();
        let config = self.config.clone();
        let backend = self.backend.clone();
        server
            .register_resource(
                "terminal://server/info",
                "terminal server info",
                "Snapshot of terminal MCP server state",
                "application/json",
                move || {
                    let snapshot = {
                        let backend = backend
                            .lock()
                            .map_err(|e| McpError::InternalError(e.to_string()))?;
                        snapshot_state(
                            &state,
                            &config,
                            backend.name(),
                            backend.is_available(),
                            backend.kind(),
                        )?
                    };
                    Ok(vec![McpContent::Text {
                        text: serde_json::to_string_pretty(&snapshot).unwrap_or_else(|e| {
                            format!("{{\"error\":\"failed to serialize snapshot: {e}\"}}")
                        }),
                    }])
                },
            )
            .await;

        let state = self.state.clone();
        let config = self.config.clone();
        let backend = self.backend.clone();
        server
            .register_resource(
                "tmux://server/info",
                "tmux server info",
                "Snapshot of tmux MCP server state",
                "application/json",
                move || {
                    let snapshot = {
                        let backend = backend
                            .lock()
                            .map_err(|e| McpError::InternalError(e.to_string()))?;
                        snapshot_state(
                            &state,
                            &config,
                            backend.name(),
                            backend.is_available(),
                            backend.kind(),
                        )?
                    };
                    Ok(vec![McpContent::Text {
                        text: serde_json::to_string_pretty(&snapshot).unwrap_or_else(|e| {
                            format!("{{\"error\":\"failed to serialize snapshot: {e}\"}}")
                        }),
                    }])
                },
            )
            .await;

        for tool in Self::tool_definitions() {
            let name = tool.name.clone();
            let registered = self.clone();
            server
                .register_tool(tool, move |args| registered.dispatch(&name, args))
                .await;
        }

        for tool in Self::legacy_tool_definitions() {
            let name = tool.name.clone();
            let registered = self.clone();
            server
                .register_tool(tool, move |args| registered.dispatch(&name, args))
                .await;
        }

        Ok(())
    }

    pub fn backend_kind(&self) -> TerminalBackendKind {
        self.backend
            .lock()
            .map_or(TerminalBackendKind::Tmux, |backend| backend.kind())
    }

    fn dispatch(&self, tool_name: &str, args: serde_json::Value) -> McpResult<McpToolResult> {
        self.maintenance_tick()?;
        match tool_name {
            "terminal.create_session" | "tmux.create_session" => self.create_session(args),
            "terminal.list_sessions" | "tmux.list_sessions" => self.list_sessions(),
            "terminal.session_info" | "tmux.session_info" => self.session_info(args),
            "terminal.close_session" | "tmux.close_session" => self.close_session(args),
            "terminal.split_pane" | "tmux.split_pane" => self.split_pane(args),
            "terminal.send_keys" | "tmux.send_keys" => self.send_keys(args),
            "terminal.capture_pane" | "tmux.capture_pane" => self.capture_pane(args),
            "terminal.execute_command" | "tmux.execute_command" => self.execute_command(args),
            "terminal.get_command_result" | "tmux.get_command_result" => {
                self.get_command_result(args)
            }
            "terminal.wait_for_output" | "tmux.wait_for_output" => self.wait_for_output(args),
            "terminal.reap_leases" | "tmux.reap_leases" => self.reap_leases(args),
            "terminal.type_keys" | "tmux.type_keys" => self.type_keys(args),
            "terminal.screenshot" | "tmux.screenshot" => self.screenshot(args),
            "terminal.list_windows" | "tmux.list_windows" => self.list_windows(args),
            "terminal.new_window" | "tmux.new_window" => self.new_window(args),
            "terminal.kill_window" | "tmux.kill_window" => self.kill_window(args),
            "terminal.rename_window" | "tmux.rename_window" => self.rename_window(args),
            "terminal.select_window" | "tmux.select_window" => self.select_window(args),
            "terminal.execute_batch" | "tmux.execute_batch" => self.execute_batch(args),
            "workspace.exec" => self.workspace_exec(args),
            "workspace.run_tests" => self.workspace_run_tests(args),
            other => Err(McpError::ToolNotFound(other.to_string())),
        }
    }

    fn create_session(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let name = required_str(&args, "name")?;
        let ttl_secs = args
            .get("ttl_secs")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(self.config.default_ttl_secs);
        let session_id = self.with_backend_mut(|backend| backend.create_session(name))?;

        let now = Utc::now().to_rfc3339();
        let lease = SessionLease {
            session_name: session_id.0.clone(),
            session_id: session_id.0.clone(),
            workspace_root: self.config.workspace_root.display().to_string(),
            created_at: now.clone(),
            last_used_at: now.clone(),
            ttl_secs,
        };

        let mut state = self
            .state
            .lock()
            .map_err(|e| McpError::InternalError(e.to_string()))?;
        state.leases.insert(session_id.0.clone(), lease.clone());

        Ok(ok_result(json!({
            "session_id": lease.session_id,
            "session_name": lease.session_name,
            "pane_id": 0,
            "lease_expires_in_secs": lease.ttl_secs,
        })))
    }

    fn list_sessions(&self) -> McpResult<McpToolResult> {
        let sessions = self.with_backend_ref(TerminalBackend::list_sessions)?;
        Ok(ok_result(json!({
            "sessions": sessions.into_iter().map(session_to_json).collect::<Vec<_>>()
        })))
    }

    fn session_info(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let info = self.with_backend_ref(|backend| backend.session_info(&session_id))?;
        let mut state = self
            .state
            .lock()
            .map_err(|e| McpError::InternalError(e.to_string()))?;
        if let Some(lease) = state.leases.get_mut(&session_id.0) {
            lease.last_used_at = Utc::now().to_rfc3339();
        }
        Ok(ok_result(session_to_json(info)))
    }

    fn close_session(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        self.with_backend_mut(|backend| backend.close_session(&session_id))?;
        let mut state = self
            .state
            .lock()
            .map_err(|e| McpError::InternalError(e.to_string()))?;
        state.leases.remove(&session_id.0);
        Ok(ok_result(json!({
            "session_id": session_id.0,
            "closed": true
        })))
    }

    fn split_pane(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let pane_index = required_usize(&args, "pane_index")?;
        let direction = match required_str(&args, "direction")? {
            "horizontal" => SplitDirection::Horizontal,
            "vertical" => SplitDirection::Vertical,
            other => {
                return Err(McpError::InvalidRequest(format!(
                    "unknown direction '{other}'"
                )))
            }
        };
        let new_pane = self
            .with_backend_mut(|backend| backend.split_pane(&session_id, pane_index, direction))?;
        self.touch_session(&session_id.0)?;
        Ok(ok_result(json!({
            "session_id": session_id.0,
            "pane_index": new_pane,
        })))
    }

    fn send_keys(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let pane_index = required_usize(&args, "pane_index")?;
        let keys = required_str(&args, "keys")?;
        self.with_backend_mut(|backend| backend.send_keys(&session_id, pane_index, keys))?;
        self.touch_session(&session_id.0)?;
        Ok(ok_result(json!({
            "session_id": session_id.0,
            "pane_index": pane_index,
            "sent": true
        })))
    }

    fn capture_pane(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let pane_index = required_usize(&args, "pane_index")?;
        let start = args.get("start").and_then(serde_json::Value::as_i64);
        let end = args.get("end").and_then(serde_json::Value::as_i64);
        let include_escape_sequences = args
            .get("include_escape_sequences")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let join_wrapped_lines = args
            .get("join_wrapped_lines")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let content = self.with_backend_ref(|backend| {
            backend.capture_pane(
                &session_id,
                pane_index,
                CapturePaneOptions {
                    start,
                    end,
                    include_escape_sequences,
                    join_wrapped_lines,
                    max_lines: Some(self.config.capture_lines),
                },
            )
        })?;
        self.touch_session(&session_id.0)?;
        Ok(ok_result(json!({
            "session_id": session_id.0,
            "pane_index": pane_index,
            "captured_at": Utc::now().to_rfc3339(),
            "content": content.text,
            "dimensions": content.dimensions,
        })))
    }

    fn execute_command(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let pane_index = required_usize(&args, "pane_index")?;
        let command = required_str(&args, "command")?;
        let track_result = args
            .get("track_result")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let command_id = format!("cmd_{}", uuid::Uuid::new_v4().simple());

        self.with_backend_mut(|backend| backend.send_keys(&session_id, pane_index, command))?;

        let started_at = Utc::now().to_rfc3339();
        if track_result {
            let mut state = self
                .state
                .lock()
                .map_err(|e| McpError::InternalError(e.to_string()))?;
            state.commands.insert(
                command_id.clone(),
                CommandRecord {
                    command_id: command_id.clone(),
                    origin: CommandOrigin::Tmux,
                    session_id: Some(session_id.0.clone()),
                    pane_index: Some(pane_index),
                    command: command.to_string(),
                    status: "running".to_string(),
                    started_at,
                    finished_at: None,
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                },
            );
        }
        self.touch_session(&session_id.0)?;
        Ok(ok_result(json!({
            "command_id": command_id,
            "session_id": session_id.0,
            "pane_index": pane_index,
            "status": if track_result { "running" } else { "sent" },
        })))
    }

    fn get_command_result(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let command_id = required_str(&args, "command_id")?;
        let mut state = self
            .state
            .lock()
            .map_err(|e| McpError::InternalError(e.to_string()))?;
        let record = state
            .commands
            .get_mut(command_id)
            .ok_or_else(|| McpError::ResourceNotFound(command_id.to_string()))?;

        let mut payload = json!({
            "command_id": record.command_id,
            "origin": match record.origin { CommandOrigin::Tmux => "tmux", CommandOrigin::Workspace => "workspace" },
            "session_id": record.session_id,
            "pane_index": record.pane_index,
            "command": record.command,
            "status": record.status,
            "started_at": record.started_at,
            "finished_at": record.finished_at,
            "exit_code": record.exit_code,
        });

        if matches!(record.origin, CommandOrigin::Tmux) && record.status == "running" {
            if let (Some(session_id_str), Some(pane_index)) =
                (&record.session_id, record.pane_index)
            {
                let session_id = TerminalSessionId(session_id_str.clone());
                let capture = self.with_backend_ref(|backend| {
                    backend.capture_pane(
                        &session_id,
                        pane_index,
                        CapturePaneOptions {
                            start: Some(-(self.config.capture_lines as i64)),
                            end: None,
                            include_escape_sequences: false,
                            join_wrapped_lines: true,
                            max_lines: Some(self.config.capture_lines),
                        },
                    )
                })?;

                payload["stdout"] = json!(capture.text);

                if capture.text.contains("$ ") || capture.text.ends_with('\n') {
                    record.finished_at = Some(Utc::now().to_rfc3339());
                }
            }
        } else {
            payload["stdout"] = json!(record.stdout.clone().unwrap_or_default());
            payload["stderr"] = json!(record.stderr.clone().unwrap_or_default());
        }

        Ok(ok_result(payload))
    }

    fn wait_for_output(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let pane_index = required_usize(&args, "pane_index")?;
        let pattern = required_str(&args, "pattern")?.to_string();
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(30);
        let content = self.with_backend_ref(|backend| {
            backend.wait_for_output(&session_id, pane_index, &pattern, Some(timeout_secs))
        })?;
        self.touch_session(&session_id.0)?;
        Ok(ok_result(json!({
            "session_id": session_id.0,
            "pane_index": pane_index,
            "pattern": pattern,
            "matched": content.text.contains(&pattern),
            "content": content.text,
        })))
    }

    fn reap_leases(&self, _args: serde_json::Value) -> McpResult<McpToolResult> {
        let removed = self.reap_expired_leases()?;
        Ok(ok_result(json!({
            "removed": removed,
        })))
    }

    fn type_keys(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let pane_index = required_usize(&args, "pane_index")?;
        let keys_raw = args
            .get("keys")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::InvalidRequest("'keys' array is required".into()))?;
        let auto_enter = args
            .get("auto_enter")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        let keys: Vec<String> = keys_raw
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        if keys.is_empty() {
            return Err(McpError::InvalidRequest("'keys' must not be empty".into()));
        }

        if auto_enter && keys.len() == 1 {
            // Simple path: type text with auto-enter
            self.with_backend_mut(|backend| backend.tmux_type(&session_id, pane_index, &keys[0]))?;
        } else {
            // Precise path: parse each key string into typed Key
            let parsed: Vec<Key> = keys.iter().map(|k| Key::from_llm(k)).collect();
            self.with_backend_mut(|backend| {
                backend.tmux_send_keys(&session_id, pane_index, &parsed)
            })?;
        }

        self.touch_session(&session_id.0)?;
        Ok(ok_result(json!({
            "session_id": session_id.0,
            "pane_index": pane_index,
            "sent": true,
        })))
    }

    fn screenshot(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let pane_index = required_usize(&args, "pane_index")?;
        let layers = args.get("layers").and_then(|v| v.as_array()).map_or_else(
            || vec![ScreenshotLayer::Text, ScreenshotLayer::Cursor],
            |arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().and_then(parse_screenshot_layer))
                    .collect::<Vec<_>>()
            },
        );
        let region = args.get("region").and_then(|v| {
            Some(Region {
                left: v.get("left")?.as_u64()? as usize,
                top: v.get("top")?.as_u64()? as usize,
                width: v.get("width")?.as_u64()? as usize,
                height: v.get("height")?.as_u64()? as usize,
            })
        });
        let around_cursor = args
            .get("around_cursor")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as usize);
        let compact = args
            .get("compact")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let options = ScreenshotOptions {
            layers,
            region,
            around_cursor,
            compact,
        };

        let screenshot = self.with_backend_ref(|backend| {
            backend.capture_screenshot(&session_id, pane_index, &options)
        })?;
        self.touch_session(&session_id.0)?;

        Ok(ok_result(json!({
            "session_id": session_id.0,
            "pane_index": pane_index,
            "dimensions": [screenshot.dimensions.0, screenshot.dimensions.1],
            "cursor": screenshot.cursor.map(|(r, c)| json!([r, c])),
            "compact_output": screenshot.to_compact(),
        })))
    }

    fn list_windows(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let windows = self.with_backend_ref(|backend| backend.list_windows(&session_id))?;
        self.touch_session(&session_id.0)?;
        Ok(ok_result(json!({
            "session_id": session_id.0,
            "windows": windows.into_iter().map(|w| json!({
                "id": w.id,
                "index": w.index,
                "name": w.name,
                "is_active": w.is_active,
                "pane_count": w.pane_count,
            })).collect::<Vec<_>>(),
        })))
    }

    fn new_window(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let session_id = TerminalSessionId(required_str(&args, "session_id")?.to_string());
        let name = args.get("name").and_then(serde_json::Value::as_str);
        let window_id = self.with_backend_mut(|backend| backend.new_window(&session_id, name))?;
        self.touch_session(&session_id.0)?;
        Ok(ok_result(json!({
            "session_id": session_id.0,
            "window_id": window_id,
        })))
    }

    fn kill_window(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let target = required_str(&args, "target")?;
        self.with_backend_ref(|backend| backend.kill_window(target))?;
        Ok(ok_result(json!({
            "target": target,
            "killed": true,
        })))
    }

    fn rename_window(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let target = required_str(&args, "target")?;
        let name = required_str(&args, "name")?;
        self.with_backend_ref(|backend| backend.rename_window(target, name))?;
        Ok(ok_result(json!({
            "target": target,
            "renamed_to": name,
        })))
    }

    fn select_window(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let target = required_str(&args, "target")?;
        self.with_backend_ref(|backend| backend.select_window(target))?;
        Ok(ok_result(json!({
            "target": target,
            "selected": true,
        })))
    }

    fn execute_batch(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let ops_raw = args
            .get("operations")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::InvalidRequest("'operations' array is required".into()))?;

        let mut batch = TmuxBatch::new();
        for op_val in ops_raw {
            let op = parse_batch_op(op_val)?;
            batch.push(op);
        }

        if batch.is_empty() {
            return Err(McpError::InvalidRequest(
                "'operations' must not be empty".into(),
            ));
        }

        let results = self.with_backend_ref(|backend| backend.execute_batch(&batch))?;
        let results_json: Vec<serde_json::Value> = results
            .into_iter()
            .map(|r| {
                json!({
                    "success": r.success,
                    "output": r.output,
                    "error": r.error,
                })
            })
            .collect();

        Ok(ok_result(json!({
            "results": results_json,
        })))
    }

    fn workspace_exec(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let command = required_str(&args, "command")?;
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(self.config.command_timeout_secs);
        let command_id = format!("cmd_{}", uuid::Uuid::new_v4().simple());
        let started_at = Utc::now().to_rfc3339();

        let result = run_workspace_command(command, &self.config.workspace_root, timeout_secs)?;
        let finished_at = Utc::now().to_rfc3339();
        {
            let mut state = self
                .state
                .lock()
                .map_err(|e| McpError::InternalError(e.to_string()))?;
            state.commands.insert(
                command_id.clone(),
                CommandRecord {
                    command_id: command_id.clone(),
                    origin: CommandOrigin::Workspace,
                    session_id: None,
                    pane_index: None,
                    command: command.to_string(),
                    status: "finished".to_string(),
                    started_at: started_at.clone(),
                    finished_at: Some(finished_at.clone()),
                    exit_code: Some(result.exit_code),
                    stdout: Some(result.stdout.clone()),
                    stderr: Some(result.stderr.clone()),
                },
            );
        }
        Ok(ok_result(json!({
            "command_id": command_id,
            "status": "finished",
            "exit_code": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "started_at": started_at,
            "finished_at": finished_at,
        })))
    }

    fn workspace_run_tests(&self, args: serde_json::Value) -> McpResult<McpToolResult> {
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(self.config.command_timeout_secs);
        let filter = args
            .get("filter")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let mut command = String::from("cargo test");
        if !filter.is_empty() {
            command.push(' ');
            command.push_str(filter);
        }
        self.workspace_exec(json!({
            "command": command,
            "timeout_secs": timeout_secs
        }))
    }

    fn touch_session(&self, session_id: &str) -> McpResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| McpError::InternalError(e.to_string()))?;
        if let Some(lease) = state.leases.get_mut(session_id) {
            lease.last_used_at = Utc::now().to_rfc3339();
        }
        Ok(())
    }

    pub fn reap_expired_leases(&self) -> McpResult<usize> {
        let now = Utc::now();
        let expired_ids = {
            let state = self
                .state
                .lock()
                .map_err(|e| McpError::InternalError(e.to_string()))?;
            state
                .leases
                .values()
                .filter_map(|lease| {
                    let created = chrono::DateTime::parse_from_rfc3339(&lease.created_at)
                        .inspect_err(|e| {
                            tracing::debug!(
                                "Invalid created_at timestamp for lease {}: {e}",
                                lease.session_id
                            );
                        })
                        .ok()?;
                    let last_used = chrono::DateTime::parse_from_rfc3339(&lease.last_used_at)
                        .inspect_err(|e| {
                            tracing::debug!(
                                "Invalid last_used_at timestamp for lease {}: {e}",
                                lease.session_id
                            );
                        })
                        .ok()?;
                    let ttl = lease.ttl_secs.try_into().unwrap_or(i64::MAX);
                    let expiry = last_used
                        .checked_add_signed(chrono::Duration::seconds(ttl))
                        .unwrap_or_else(|| {
                            tracing::warn!(
                                "TTL overflow for lease {}, capping expiry",
                                lease.session_id
                            );
                            last_used + chrono::Duration::seconds(ttl)
                        });
                    if expiry < now.with_timezone(&created.timezone()) {
                        Some(lease.session_id.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };

        let mut removed = 0usize;
        for session_id in expired_ids {
            let _ = self.with_backend_mut(|backend| {
                backend.close_session(&TerminalSessionId(session_id.clone()))
            });
            let mut state = self
                .state
                .lock()
                .map_err(|e| McpError::InternalError(e.to_string()))?;
            if state.leases.remove(&session_id).is_some() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn maintenance_tick(&self) -> McpResult<()> {
        let _ = self.reap_expired_leases()?;
        Ok(())
    }
}

impl Clone for TmuxMcpServer {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            backend: self.backend.clone(),
            state: self.state.clone(),
        }
    }
}

fn tool(name: &str, description: &str, input_schema: serde_json::Value) -> McpTool {
    McpTool {
        name: name.to_string(),
        title: None,
        description: description.to_string(),
        input_schema,
        output_schema: None,
        annotations: None,
        category: Some("terminal".to_string()),
    }
}

fn ok_result(value: serde_json::Value) -> McpToolResult {
    McpToolResult {
        content: vec![McpContent::Text {
            text: serde_json::to_string_pretty(&value)
                .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")),
        }],
        is_error: Some(false),
        structured_content: None,
        meta: None,
    }
}

fn required_str<'a>(args: &'a serde_json::Value, key: &str) -> McpResult<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::InvalidRequest(format!("'{key}' is required")))
}

fn required_usize(args: &serde_json::Value, key: &str) -> McpResult<usize> {
    args.get(key)
        .and_then(serde_json::Value::as_u64)
        .map(|n| n as usize)
        .ok_or_else(|| McpError::InvalidRequest(format!("'{key}' is required")))
}

fn mux_err(err: ConnectorError) -> McpError {
    McpError::CallFailed(err.to_string())
}

#[derive(Debug)]
struct WorkspaceCommandOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

fn run_workspace_command(
    command: &str,
    cwd: &Path,
    timeout_secs: u64,
) -> McpResult<WorkspaceCommandOutput> {
    let mut child = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| McpError::CallFailed(format!("failed to spawn command: {e}")))?;

    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| McpError::CallFailed(format!("wait failed: {e}")))?
        {
            let output = child
                .wait_with_output()
                .map_err(|e| McpError::CallFailed(format!("failed to collect output: {e}")))?;
            return Ok(WorkspaceCommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: status.code().unwrap_or(-1),
            });
        }
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            let _ = child.kill();
            let output = child.wait_with_output().map_err(|e| {
                McpError::CallFailed(format!("failed to collect timed out output: {e}"))
            })?;
            return Ok(WorkspaceCommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: format!(
                    "{}\nTimed out after {} seconds",
                    String::from_utf8_lossy(&output.stderr),
                    timeout_secs
                ),
                exit_code: 124,
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn session_to_json(session: SessionInfo) -> serde_json::Value {
    json!({
        "session_id": session.id.0,
        "name": session.name,
        "is_active": session.is_active,
        "panes": session.panes.into_iter().map(|pane| json!({
            "id": pane.id,
            "index": pane.index,
            "cwd": pane.cwd,
            "command": pane.command,
            "is_active": pane.is_active,
        })).collect::<Vec<_>>(),
    })
}

fn snapshot_state(
    state: &Arc<Mutex<TmuxState>>,
    config: &TmuxMcpConfig,
    backend_name: &str,
    backend_available: bool,
    backend_kind: TerminalBackendKind,
) -> McpResult<serde_json::Value> {
    let state = state
        .lock()
        .map_err(|e| McpError::InternalError(e.to_string()))?;
    Ok(json!({
        "workspace_root": config.workspace_root,
        "session_prefix": config.session_prefix,
        "preferred_backend": config.preferred_backend,
        "backend": {
            "name": backend_name,
            "available": backend_available,
            "kind": backend_kind,
        },
        "leases": state.leases.values().cloned().collect::<Vec<_>>(),
        "commands": state.commands.keys().cloned().collect::<Vec<_>>(),
    }))
}

fn parse_screenshot_layer(s: &str) -> Option<ScreenshotLayer> {
    match s {
        "text" => Some(ScreenshotLayer::Text),
        "cursor" => Some(ScreenshotLayer::Cursor),
        "fg_colors" => Some(ScreenshotLayer::FgColors),
        "bg_colors" => Some(ScreenshotLayer::BgColors),
        "styles" => Some(ScreenshotLayer::Styles),
        "bold" => Some(ScreenshotLayer::Bold),
        "italic" => Some(ScreenshotLayer::Italic),
        "underline" => Some(ScreenshotLayer::Underline),
        _ => None,
    }
}

fn parse_batch_op(val: &serde_json::Value) -> McpResult<TmuxOp> {
    let kind = val
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| McpError::InvalidRequest("each operation needs a 'kind'".into()))?;
    let target = val.get("target").and_then(serde_json::Value::as_str);
    let str_field = |v: &serde_json::Value, field: &str| {
        v.get(field)
            .and_then(serde_json::Value::as_str)
            .map(String::from)
    };

    match kind {
        "new_session" => Ok(TmuxOp::NewSession {
            name: target.unwrap_or("session").to_string(),
            start_dir: str_field(val, "start_dir"),
        }),
        "kill_session" => Ok(TmuxOp::KillSession {
            target: target
                .ok_or_else(|| McpError::InvalidRequest("kill_session needs 'target'".into()))?
                .to_string(),
        }),
        "split_pane" => {
            let direction = match val
                .get("direction")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("horizontal")
            {
                "vertical" => SplitDirection::Vertical,
                _ => SplitDirection::Horizontal,
            };
            Ok(TmuxOp::SplitPane {
                target: target
                    .ok_or_else(|| McpError::InvalidRequest("split_pane needs 'target'".into()))?
                    .to_string(),
                direction,
                start_dir: str_field(val, "start_dir"),
            })
        }
        "send_keys" => {
            let keys: Vec<Key> = val.get("keys").and_then(|v| v.as_array()).map_or_else(
                || {
                    tracing::debug!("send_keys missing 'keys' field, defaulting to empty");
                    Vec::new()
                },
                |arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(Key::from_llm))
                        .collect()
                },
            );
            Ok(TmuxOp::SendKeys {
                target: target
                    .ok_or_else(|| McpError::InvalidRequest("send_keys needs 'target'".into()))?
                    .to_string(),
                keys,
            })
        }
        "send_text" => Ok(TmuxOp::SendText {
            target: target
                .ok_or_else(|| McpError::InvalidRequest("send_text needs 'target'".into()))?
                .to_string(),
            text: str_field(val, "text").unwrap_or_else(|| {
                tracing::warn!("send_text missing 'text' field, defaulting to empty");
                String::new()
            }),
        }),
        "capture_pane" => Ok(TmuxOp::CapturePane {
            target: target
                .ok_or_else(|| McpError::InvalidRequest("capture_pane needs 'target'".into()))?
                .to_string(),
            start: val.get("start").and_then(serde_json::Value::as_i64),
            end: val.get("end").and_then(serde_json::Value::as_i64),
        }),
        "new_window" => Ok(TmuxOp::NewWindow {
            session: target
                .ok_or_else(|| {
                    McpError::InvalidRequest("new_window needs 'target' (session)".into())
                })?
                .to_string(),
            name: str_field(val, "name"),
        }),
        "kill_pane" => Ok(TmuxOp::KillPane {
            target: target
                .ok_or_else(|| McpError::InvalidRequest("kill_pane needs 'target'".into()))?
                .to_string(),
        }),
        "resize_pane" => {
            let direction = match val
                .get("direction")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("up")
            {
                "down" => rustycode_connector::ResizeDirection::Down,
                "left" => rustycode_connector::ResizeDirection::Left,
                "right" => rustycode_connector::ResizeDirection::Right,
                _ => rustycode_connector::ResizeDirection::Up,
            };
            Ok(TmuxOp::ResizePane {
                target: target
                    .ok_or_else(|| McpError::InvalidRequest("resize_pane needs 'target'".into()))?
                    .to_string(),
                direction,
                cells: val
                    .get("cells")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(5) as usize,
            })
        }
        "swap_pane" => Ok(TmuxOp::SwapPane {
            src: target
                .ok_or_else(|| McpError::InvalidRequest("swap_pane needs 'target' (src)".into()))?
                .to_string(),
            dst: str_field(val, "dst")
                .ok_or_else(|| McpError::InvalidRequest("swap_pane needs 'dst'".into()))?,
        }),
        "select_pane" => Ok(TmuxOp::SelectPane {
            target: target
                .ok_or_else(|| McpError::InvalidRequest("select_pane needs 'target'".into()))?
                .to_string(),
        }),
        "select_layout" => Ok(TmuxOp::SelectLayout {
            target: target
                .ok_or_else(|| McpError::InvalidRequest("select_layout needs 'target'".into()))?
                .to_string(),
            layout: str_field(val, "layout").unwrap_or_else(|| "even-horizontal".into()),
        }),
        "set_pane_title" => Ok(TmuxOp::SetPaneTitle {
            target: target
                .ok_or_else(|| McpError::InvalidRequest("set_pane_title needs 'target'".into()))?
                .to_string(),
            title: str_field(val, "title")
                .ok_or_else(|| McpError::InvalidRequest("set_pane_title needs 'title'".into()))?,
        }),
        other => Err(McpError::InvalidRequest(format!(
            "unknown operation kind '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_include_lifecycle_and_capture_tools() {
        let generic_tools = TmuxMcpServer::tool_definitions();
        let generic_names: Vec<_> = generic_tools.iter().map(|t| t.name.as_str()).collect();
        assert!(generic_names.contains(&"terminal.create_session"));
        assert!(generic_names.contains(&"terminal.capture_pane"));
        assert!(generic_names.contains(&"terminal.execute_command"));
        assert!(generic_names.contains(&"terminal.get_command_result"));
        assert!(generic_names.contains(&"terminal.wait_for_output"));
        assert!(generic_names.contains(&"workspace.exec"));
        assert!(generic_names.contains(&"workspace.run_tests"));

        let legacy_tools = TmuxMcpServer::legacy_tool_definitions();
        let legacy_names: Vec<_> = legacy_tools.iter().map(|t| t.name.as_str()).collect();
        assert!(legacy_names.contains(&"tmux.create_session"));
        assert!(legacy_names.contains(&"tmux.capture_pane"));
        assert!(legacy_names.contains(&"tmux.execute_command"));
        assert!(legacy_names.contains(&"tmux.get_command_result"));
        assert!(legacy_names.contains(&"tmux.wait_for_output"));
    }

    #[test]
    fn test_reap_expired_leases_with_no_state_is_zero() {
        let server = TmuxMcpServer::new(TmuxMcpConfig::default());
        let removed = server.reap_expired_leases().unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn test_reap_expired_leases_removes_old_entry() {
        let server = TmuxMcpServer::new(TmuxMcpConfig::default());
        let now = Utc::now();
        let expired = now - chrono::Duration::seconds(10);

        {
            let mut state = server.state.lock().unwrap();
            state.leases.insert(
                "rustycode-test-1".to_string(),
                SessionLease {
                    session_id: "rustycode-test-1".to_string(),
                    session_name: "rustycode-test-1".to_string(),
                    workspace_root: "/tmp".to_string(),
                    created_at: expired.to_rfc3339(),
                    last_used_at: expired.to_rfc3339(),
                    ttl_secs: 1,
                },
            );
        }

        let removed = server.reap_expired_leases().unwrap();
        assert_eq!(removed, 1);
        let state = server.state.lock().unwrap();
        assert!(state.leases.is_empty());
    }

    #[test]
    fn test_backend_selection_can_be_forced() {
        let config = TmuxMcpConfig::default();
        let tmux_server =
            TmuxMcpServer::with_backend_kind(config.clone(), TerminalBackendKind::Tmux);
        assert_eq!(tmux_server.backend_kind(), TerminalBackendKind::Tmux);

        let it2_server = TmuxMcpServer::with_backend_kind(config, TerminalBackendKind::It2);
        assert_eq!(it2_server.backend_kind(), TerminalBackendKind::It2);
    }

    // -- New tool tests --

    #[test]
    fn test_tool_definitions_include_new_tools() {
        let tools = TmuxMcpServer::tool_definitions();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"terminal.type_keys"));
        assert!(names.contains(&"terminal.screenshot"));
        assert!(names.contains(&"terminal.list_windows"));
        assert!(names.contains(&"terminal.new_window"));
        assert!(names.contains(&"terminal.kill_window"));
        assert!(names.contains(&"terminal.rename_window"));
        assert!(names.contains(&"terminal.select_window"));
        assert!(names.contains(&"terminal.execute_batch"));

        let legacy = TmuxMcpServer::legacy_tool_definitions();
        let legacy_names: Vec<_> = legacy.iter().map(|t| t.name.as_str()).collect();
        assert!(legacy_names.contains(&"tmux.type_keys"));
        assert!(legacy_names.contains(&"tmux.screenshot"));
        assert!(legacy_names.contains(&"tmux.list_windows"));
        assert!(legacy_names.contains(&"tmux.execute_batch"));
    }

    #[test]
    fn test_parse_screenshot_layer_all_valid() {
        assert_eq!(parse_screenshot_layer("text"), Some(ScreenshotLayer::Text));
        assert_eq!(
            parse_screenshot_layer("cursor"),
            Some(ScreenshotLayer::Cursor)
        );
        assert_eq!(
            parse_screenshot_layer("fg_colors"),
            Some(ScreenshotLayer::FgColors)
        );
        assert_eq!(
            parse_screenshot_layer("bg_colors"),
            Some(ScreenshotLayer::BgColors)
        );
        assert_eq!(
            parse_screenshot_layer("styles"),
            Some(ScreenshotLayer::Styles)
        );
        assert_eq!(parse_screenshot_layer("bold"), Some(ScreenshotLayer::Bold));
        assert_eq!(
            parse_screenshot_layer("italic"),
            Some(ScreenshotLayer::Italic)
        );
        assert_eq!(
            parse_screenshot_layer("underline"),
            Some(ScreenshotLayer::Underline)
        );
    }

    #[test]
    fn test_parse_screenshot_layer_unknown_returns_none() {
        assert_eq!(parse_screenshot_layer("unknown"), None);
        assert_eq!(parse_screenshot_layer(""), None);
    }

    #[test]
    fn test_parse_batch_op_new_session() {
        let op = parse_batch_op(&json!({"kind": "new_session", "target": "my-session"})).unwrap();
        if let TmuxOp::NewSession { name, start_dir } = op {
            assert_eq!(name, "my-session");
            assert!(start_dir.is_none());
        } else {
            panic!("expected NewSession");
        }
    }

    #[test]
    fn test_parse_batch_op_send_keys_parses_ctrl() {
        let op = parse_batch_op(&json!({
            "kind": "send_keys",
            "target": "sess:0.0",
            "keys": ["ctrl-c", "Enter"]
        }))
        .unwrap();
        if let TmuxOp::SendKeys { target, keys } = op {
            assert_eq!(target, "sess:0.0");
            assert_eq!(keys.len(), 2);
        } else {
            panic!("expected SendKeys");
        }
    }

    #[test]
    fn test_parse_batch_op_resize_pane_defaults() {
        let op = parse_batch_op(&json!({"kind": "resize_pane", "target": "sess:0.0"})).unwrap();
        if let TmuxOp::ResizePane {
            target,
            direction,
            cells,
        } = op
        {
            assert_eq!(target, "sess:0.0");
            assert_eq!(cells, 5);
            assert!(matches!(
                direction,
                rustycode_connector::ResizeDirection::Up
            ));
        } else {
            panic!("expected ResizePane");
        }
    }

    #[test]
    fn test_parse_batch_op_unknown_kind_errors() {
        let result = parse_batch_op(&json!({"kind": "nonexistent"}));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_batch_op_missing_target_errors() {
        let result = parse_batch_op(&json!({"kind": "kill_session"}));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_batch_op_swap_pane_needs_dst() {
        let result = parse_batch_op(&json!({"kind": "swap_pane", "target": "a"}));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_batch_op_split_pane_vertical() {
        let op = parse_batch_op(&json!({
            "kind": "split_pane",
            "target": "sess:0",
            "direction": "vertical"
        }))
        .unwrap();
        if let TmuxOp::SplitPane { direction, .. } = op {
            assert_eq!(direction, SplitDirection::Vertical);
        } else {
            panic!("expected SplitPane");
        }
    }

    #[test]
    fn test_type_keys_requires_keys_array() {
        let server = TmuxMcpServer::new(TmuxMcpConfig::default());
        let result = server.dispatch(
            "terminal.type_keys",
            json!({"session_id": "s1", "pane_index": 0}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_batch_empty_ops_errors() {
        let server = TmuxMcpServer::new(TmuxMcpConfig::default());
        let result = server.dispatch("terminal.execute_batch", json!({"operations": []}));
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_batch_missing_ops_errors() {
        let server = TmuxMcpServer::new(TmuxMcpConfig::default());
        let result = server.dispatch("terminal.execute_batch", json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn test_kill_window_requires_target() {
        let server = TmuxMcpServer::new(TmuxMcpConfig::default());
        let result = server.dispatch("terminal.kill_window", json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn test_rename_window_requires_both_fields() {
        let server = TmuxMcpServer::new(TmuxMcpConfig::default());
        let result = server.dispatch("terminal.rename_window", json!({"target": "s:0"}));
        assert!(result.is_err());
    }
}
