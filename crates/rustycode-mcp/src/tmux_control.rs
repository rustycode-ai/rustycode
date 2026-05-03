//! tmux-backed MCP control plane for agent-driven workflows.

use crate::types::{McpContent, McpTool, McpToolResult};
use crate::{McpError, McpResult, McpServer};
use chrono::Utc;
use rustycode_connector::{
    CapturePaneOptions, ConnectorError, ITerm2NativeConnector, It2Connector, SessionId,
    SessionInfo, SplitDirection, TerminalConnector, TmuxConnector,
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

    fn create_session(&mut self, name: &str) -> Result<SessionId, ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.create_session(name),
            Self::It2(connector) => connector.create_session(name),
            Self::Iterm2Native(connector) => connector.create_session(name),
        }
    }

    fn close_session(&mut self, session: &SessionId) -> Result<(), ConnectorError> {
        match self {
            Self::Tmux(connector) => connector.close_session(session),
            Self::It2(connector) => connector.close_session(session),
            Self::Iterm2Native(connector) => connector.close_session(session),
        }
    }

    fn session_info(&self, session: &SessionId) -> Result<SessionInfo, ConnectorError> {
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
        session: &SessionId,
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
        session: &SessionId,
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
        session: &SessionId,
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
        session: &SessionId,
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
        let session_id = SessionId(required_str(&args, "session_id")?.to_string());
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
        let session_id = SessionId(required_str(&args, "session_id")?.to_string());
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
        let session_id = SessionId(required_str(&args, "session_id")?.to_string());
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
        let session_id = SessionId(required_str(&args, "session_id")?.to_string());
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
        let session_id = SessionId(required_str(&args, "session_id")?.to_string());
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
        let session_id = SessionId(required_str(&args, "session_id")?.to_string());
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
                let session_id = SessionId(session_id_str.clone());
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
        let session_id = SessionId(required_str(&args, "session_id")?.to_string());
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
                    let created = chrono::DateTime::parse_from_rfc3339(&lease.created_at).ok()?;
                    let last_used =
                        chrono::DateTime::parse_from_rfc3339(&lease.last_used_at).ok()?;
                    let expiry = last_used + chrono::Duration::seconds(lease.ttl_secs as i64);
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
            let _ = self
                .with_backend_mut(|backend| backend.close_session(&SessionId(session_id.clone())));
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
}
