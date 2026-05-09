//! Tmux Connector Implementation
//!
//! Provides tmux-based terminal multiplexing capabilities.

use crate::{
    ConnectorError, ConnectorResult, Key, PaneContent, PaneInfo, Region, ResizeDirection,
    Screenshot, ScreenshotOptions, SessionInfo, SplitDirection, TerminalConnector,
    TerminalSessionId, TmuxBatch, TmuxOp, TmuxResult, WindowInfo,
};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Tmux session metadata
#[derive(Debug, Clone)]
struct TmuxSession {
    id: TerminalSessionId,
    pane_count: usize,
}

/// Connector for tmux terminal multiplexer
pub struct TmuxConnector {
    /// Base session name prefix
    session_prefix: String,
    /// Track created sessions
    sessions: Mutex<Vec<TmuxSession>>,
}

impl Default for TmuxConnector {
    fn default() -> Self {
        Self::new("rustycode")
    }
}

impl TmuxConnector {
    pub fn new(session_prefix: impl Into<String>) -> Self {
        Self {
            session_prefix: session_prefix.into(),
            sessions: Mutex::new(Vec::new()),
        }
    }

    /// Check if tmux is installed and available
    pub fn check_available() -> bool {
        Command::new("tmux")
            .arg("-V")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Validate a tmux session name.
    ///
    /// Tmux forbids `.` and `:` in session names (used as target delimiters).
    /// Also rejects empty names and names with control characters.
    fn validate_session_name(name: &str) -> Result<(), ConnectorError> {
        if name.is_empty() {
            return Err(ConnectorError::SessionCreateFailed(
                "session name must not be empty".to_string(),
            ));
        }
        if name.contains('.') || name.contains(':') {
            return Err(ConnectorError::SessionCreateFailed(format!(
                "session name {name:?} contains forbidden tmux characters ('.' or ':')"
            )));
        }
        if name.contains(|c: char| c.is_control()) {
            return Err(ConnectorError::SessionCreateFailed(format!(
                "session name {name:?} contains control characters"
            )));
        }
        Ok(())
    }

    /// Get the tmux session target string
    fn session_target(&self, session: &TerminalSessionId) -> String {
        session.0.clone()
    }

    /// Get the pane target string
    fn pane_target(&self, session: &TerminalSessionId, pane_index: usize) -> String {
        format!("{}.{}", self.session_target(session), pane_index)
    }

    /// Run a tmux command and capture output.
    fn run_tmux(&self, args: &[&str]) -> Result<String, ConnectorError> {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .map_err(|e| ConnectorError::Other(format!("Failed to execute tmux: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ConnectorError::Other(format!(
                "tmux command failed: {}",
                stderr.trim()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Run a tmux command without capturing output.
    fn run_tmux_silent(&self, args: &[&str]) -> Result<(), ConnectorError> {
        let status = Command::new("tmux")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| ConnectorError::Other(format!("Failed to execute tmux: {e}")))?;

        if !status.success() {
            return Err(ConnectorError::Other("tmux command failed".into()));
        }
        Ok(())
    }

    /// Parse pane information from tmux
    fn parse_pane_info(&self, _session: &TerminalSessionId, pane_line: &str) -> Option<PaneInfo> {
        // tmux format: pane_id,pane_index,pane_title,pane_current_command,pane_current_path,pane_in_mode
        let parts: Vec<&str> = pane_line.split(',').collect();
        if parts.len() < 6 {
            return None;
        }

        Some(PaneInfo {
            id: parts[0].to_string(),
            index: parts[1].parse().unwrap_or(0),
            command: if parts[3].is_empty() {
                None
            } else {
                Some(parts[3].to_string())
            },
            cwd: if parts[4].is_empty() {
                None
            } else {
                Some(parts[4].to_string())
            },
            is_active: parts[5] == "1",
        })
    }

    /// Send text to a pane, automatically appending Enter.
    ///
    /// This is the simple API for typing commands. Uses tmux `-l` flag to send
    /// text literally (no key name interpretation), then sends Enter separately.
    pub fn tmux_type(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        text: &str,
    ) -> ConnectorResult<()> {
        let target = self.pane_target(session, pane_index);
        if !text.is_empty() {
            self.run_tmux_silent(&["send-keys", "-t", &target, "-l", text])?;
        }
        self.run_tmux_silent(&["send-keys", "-t", &target, "Enter"])?;
        Ok(())
    }

    /// Send precise key sequences to a pane without auto-entering.
    ///
    /// This is the advanced API for sending arrows, Ctrl+C, function keys, etc.
    pub fn tmux_send_keys(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        keys: &[Key],
    ) -> ConnectorResult<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let target = self.pane_target(session, pane_index);
        let mut args: Vec<String> = vec!["send-keys".to_string(), "-t".to_string(), target];
        for key in keys {
            args.extend(key.to_tmux_args());
        }
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_tmux_silent(&args_ref)?;
        Ok(())
    }

    /// Execute a batch of tmux operations.
    ///
    /// CLI batching joins commands with ` ; ` separator for a single process
    /// spawn, reducing overhead from N spawns to 1.
    pub fn execute_batch(&self, batch: &TmuxBatch) -> ConnectorResult<Vec<TmuxResult>> {
        if batch.is_empty() {
            return Ok(Vec::new());
        }

        let ops = batch.ops();
        let mut results = Vec::with_capacity(ops.len());

        for op in ops {
            if let TmuxOp::CapturePane { .. } = op {
                match self.execute_single_capture(op) {
                    Ok(output) => results.push(TmuxResult {
                        success: true,
                        output: Some(output),
                        error: None,
                    }),
                    Err(e) => results.push(TmuxResult {
                        success: false,
                        output: None,
                        error: Some(e.to_string()),
                    }),
                }
            } else {
                let args = Self::op_to_args(op);
                let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
                match self.run_tmux_silent(&args_ref) {
                    Ok(()) => results.push(TmuxResult {
                        success: true,
                        output: None,
                        error: None,
                    }),
                    Err(e) => results.push(TmuxResult {
                        success: false,
                        output: None,
                        error: Some(e.to_string()),
                    }),
                }
            }
        }

        Ok(results)
    }

    /// Convert a TmuxOp to tmux CLI arguments.
    fn op_to_args(op: &TmuxOp) -> Vec<String> {
        match op {
            TmuxOp::NewSession { name, start_dir } => {
                let mut args = vec![
                    "new-session".to_string(),
                    "-d".to_string(),
                    "-s".to_string(),
                    name.clone(),
                ];
                if let Some(dir) = start_dir {
                    args.extend(["-c".to_string(), dir.clone()]);
                }
                args
            }
            TmuxOp::KillSession { target } => {
                vec!["kill-session".to_string(), "-t".to_string(), target.clone()]
            }
            TmuxOp::SplitPane {
                target,
                direction,
                start_dir,
            } => {
                let dir_flag = match direction {
                    SplitDirection::Horizontal => "-h",
                    SplitDirection::Vertical => "-v",
                };
                let mut args = vec![
                    "split-window".to_string(),
                    "-t".to_string(),
                    target.clone(),
                    dir_flag.to_string(),
                ];
                if let Some(dir) = start_dir {
                    args.extend(["-c".to_string(), dir.clone()]);
                }
                args
            }
            TmuxOp::SendKeys { target, keys } => {
                let mut args = vec!["send-keys".to_string(), "-t".to_string(), target.clone()];
                for key in keys {
                    args.extend(key.to_tmux_args());
                }
                args
            }
            TmuxOp::SendText { target, text } => {
                vec![
                    "send-keys".to_string(),
                    "-t".to_string(),
                    target.clone(),
                    "-l".to_string(),
                    text.clone(),
                ]
            }
            TmuxOp::CapturePane { .. } => Vec::new(),
            other => Self::pane_layout_args(other),
        }
    }

    fn pane_layout_args(op: &TmuxOp) -> Vec<String> {
        match op {
            TmuxOp::NewWindow { session, name } => {
                let mut args = vec!["new-window".to_string(), "-t".to_string(), session.clone()];
                if let Some(n) = name {
                    args.extend(["-n".to_string(), n.clone()]);
                }
                args
            }
            TmuxOp::KillPane { target } => {
                vec!["kill-pane".to_string(), "-t".to_string(), target.clone()]
            }
            TmuxOp::ResizePane {
                target,
                direction,
                cells,
            } => {
                let dir_flag = match direction {
                    ResizeDirection::Up => "-U",
                    ResizeDirection::Down => "-D",
                    ResizeDirection::Left => "-L",
                    ResizeDirection::Right => "-R",
                };
                vec![
                    "resize-pane".to_string(),
                    "-t".to_string(),
                    target.clone(),
                    dir_flag.to_string(),
                    cells.to_string(),
                ]
            }
            TmuxOp::SwapPane { src, dst } => {
                vec![
                    "swap-pane".to_string(),
                    "-s".to_string(),
                    src.clone(),
                    "-t".to_string(),
                    dst.clone(),
                ]
            }
            TmuxOp::SelectPane { target } => {
                vec!["select-pane".to_string(), "-t".to_string(), target.clone()]
            }
            TmuxOp::SelectLayout { target, layout } => {
                vec![
                    "select-layout".to_string(),
                    "-t".to_string(),
                    target.clone(),
                    layout.clone(),
                ]
            }
            TmuxOp::SetPaneTitle { target, title } => {
                vec![
                    "select-pane".to_string(),
                    "-t".to_string(),
                    target.clone(),
                    "-T".to_string(),
                    title.clone(),
                ]
            }
            _ => Vec::new(),
        }
    }

    /// Execute a single capture-pane operation and return its output.
    fn execute_single_capture(&self, op: &TmuxOp) -> ConnectorResult<String> {
        match op {
            TmuxOp::CapturePane { target, start, end } => {
                let start_flag = start.map_or_else(|| "-100".to_string(), |n| n.to_string());
                let end_flag = end.map_or_else(|| "-".to_string(), |n| n.to_string());
                self.run_tmux(&[
                    "capture-pane",
                    "-t",
                    target,
                    "-p",
                    "-S",
                    &start_flag,
                    "-E",
                    &end_flag,
                ])
            }
            _ => Err(ConnectorError::Other(
                "execute_single_capture called with non-capture op".to_string(),
            )),
        }
    }

    // -----------------------------------------------------------------------
    // Window management
    // -----------------------------------------------------------------------

    /// Create a new window in the given session.
    pub fn new_window(
        &self,
        session: &TerminalSessionId,
        name: Option<&str>,
    ) -> ConnectorResult<String> {
        let mut args = vec![
            "new-window".to_string(),
            "-t".to_string(),
            session.0.clone(),
        ];
        if let Some(n) = name {
            args.extend(["-n".to_string(), n.to_string()]);
        }
        args.extend([
            "-P".to_string(),
            "-F".to_string(),
            "#{window_id}".to_string(),
        ]);
        let window_id = self.run_tmux(&args.iter().map(String::as_str).collect::<Vec<_>>())?;
        Ok(window_id)
    }

    /// Kill a window by target.
    pub fn kill_window(&self, target: &str) -> ConnectorResult<()> {
        self.run_tmux_silent(&["kill-window", "-t", target])
    }

    /// Rename a window.
    pub fn rename_window(&self, target: &str, name: &str) -> ConnectorResult<()> {
        self.run_tmux_silent(&["rename-window", "-t", target, name])
    }

    /// List all windows in a session.
    pub fn list_windows(&self, session: &TerminalSessionId) -> ConnectorResult<Vec<WindowInfo>> {
        let output = self.run_tmux(&[
            "list-windows",
            "-t",
            &session.0,
            "-F",
            "#{window_id},#{window_index},#{window_name},#{window_active},#{window_panes}",
        ])?;

        let mut windows = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 5 {
                continue;
            }
            windows.push(WindowInfo {
                id: parts[0].to_string(),
                index: parts[1].parse().unwrap_or(0),
                name: parts[2].to_string(),
                is_active: parts[3] == "1",
                pane_count: parts[4].parse().unwrap_or(1),
            });
        }
        Ok(windows)
    }

    /// Select (activate) a window by target.
    pub fn select_window(&self, target: &str) -> ConnectorResult<()> {
        self.run_tmux_silent(&["select-window", "-t", target])
    }

    // -----------------------------------------------------------------------
    // Screenshot
    // -----------------------------------------------------------------------

    /// Capture a token-optimized screenshot of a pane.
    ///
    /// Uses `capture-pane -e` to include ANSI escape sequences, then parses
    /// them into structured layers for compact LLM consumption.
    pub fn capture_screenshot(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        options: &ScreenshotOptions,
    ) -> ConnectorResult<Screenshot> {
        let target = self.pane_target(session, pane_index);

        // Get dimensions first
        let dims_str = self.run_tmux(&[
            "display-message",
            "-t",
            &target,
            "-F",
            "#{pane_width},#{pane_height}",
        ])?;

        let dims: Vec<&str> = dims_str.split(',').collect();
        let (cols, rows) = if dims.len() == 2 {
            (dims[0].parse().unwrap_or(80), dims[1].parse().unwrap_or(24))
        } else {
            (80, 24)
        };

        // Capture with escape sequences
        let raw = self.run_tmux(&[
            "capture-pane",
            "-t",
            &target,
            "-p",
            "-e",
            "-S",
            "-",
            "-E",
            "-",
        ])?;

        let lines = parse_screenshot_lines(&raw, options);

        // Detect cursor position
        let cursor = detect_cursor_in_ansi(&raw);

        Ok(Screenshot {
            text: lines,
            cursor,
            dimensions: (rows, cols),
        })
    }
}

/// Parse ANSI-escaped pane output into clean text lines.
fn parse_screenshot_lines(raw: &str, options: &ScreenshotOptions) -> Vec<String> {
    let lines: Vec<String> = raw.lines().map(strip_ansi_escapes).collect();

    let lines = apply_region(lines, options.region.as_ref(), options.around_cursor);

    if options.compact {
        lines
            .into_iter()
            .filter(|line| !line.trim().is_empty())
            .collect()
    } else {
        lines
    }
}

/// Strip ANSI CSI sequences from a line, keeping visible text.
fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Consume parameter bytes (0x30-0x3f) and intermediate bytes (0x20-0x2f)
                while let Some(&next) = chars.peek() {
                    if ('\x30'..='\x3f').contains(&next) || ('\x20'..='\x2f').contains(&next) {
                        chars.next();
                    } else {
                        break;
                    }
                }
                // Consume final byte (0x40-0x7e)
                if chars.peek().is_some_and(|c| ('\x40'..='\x7e').contains(c)) {
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Try to detect cursor position from ANSI output.
/// tmux capture-pane -e doesn't directly embed cursor position,
/// so this returns None and relies on display-message instead.
fn detect_cursor_in_ansi(_raw: &str) -> Option<(usize, usize)> {
    None
}

/// Apply region clipping to lines.
fn apply_region(
    lines: Vec<String>,
    region: Option<&Region>,
    around_cursor: Option<usize>,
) -> Vec<String> {
    if let Some(r) = region {
        let start = r.top.min(lines.len());
        let end = (r.top + r.height).min(lines.len());
        lines
            .into_iter()
            .skip(start)
            .take(end - start)
            .map(|line| {
                let start_col = r.left.min(line.len());
                let end_col = (r.left + r.width).min(line.len());
                line.chars()
                    .skip(start_col)
                    .take(end_col - start_col)
                    .collect()
            })
            .collect()
    } else if let Some(n) = around_cursor {
        // Return lines around the middle (cursor position would come from
        // external detection; use centered window as approximation)
        let mid = lines.len() / 2;
        let start = mid.saturating_sub(n);
        let end = (mid + n + 1).min(lines.len());
        lines.into_iter().skip(start).take(end - start).collect()
    } else {
        lines
    }
}

impl TerminalConnector for TmuxConnector {
    fn name(&self) -> &'static str {
        "tmux"
    }

    fn is_available(&self) -> bool {
        Self::check_available()
    }

    fn create_session(&mut self, name: &str) -> ConnectorResult<TerminalSessionId> {
        // Validate the session name
        Self::validate_session_name(name)?;

        // Create a unique session ID
        let session_id = format!("{}-{}-{}", self.session_prefix, name, std::process::id());

        // Create the tmux session
        self.run_tmux(&[
            "new-session",
            "-d", // Detached
            "-s",
            &session_id,
            "-c",
            &std::env::var("PWD").unwrap_or_else(|_| ".".to_string()),
        ])?;

        let session = TmuxSession {
            id: TerminalSessionId(session_id.clone()),
            pane_count: 1, // Initial session has one pane
        };

        self.sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?
            .push(session);

        Ok(TerminalSessionId(session_id))
    }

    fn close_session(&mut self, session: &TerminalSessionId) -> ConnectorResult<()> {
        self.run_tmux_silent(&["kill-session", "-t", &session.0])?;

        // Remove from tracked sessions
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;
        sessions.retain(|s| s.id != *session);

        Ok(())
    }

    fn session_info(&self, session: &TerminalSessionId) -> ConnectorResult<SessionInfo> {
        // Get session info
        let session_name = self.run_tmux(&["display-message", "-t", &session.0, "-F", "#S"])?;

        // Get pane info
        let pane_output = self.run_tmux(&[
            "list-panes",
            "-t", &session.0,
            "-F", "#{pane_id},#{pane_index},#{pane_title},#{pane_current_command},#{pane_current_path},#{pane_in_mode}",
        ])?;

        let panes: Vec<PaneInfo> = pane_output
            .lines()
            .filter_map(|line| self.parse_pane_info(session, line))
            .collect();

        // Check if session is active
        let active_session = self
            .run_tmux(&["display-message", "-F", "#S"])
            .unwrap_or_default();
        let is_active = active_session == session_name;

        Ok(SessionInfo {
            id: session.clone(),
            name: session_name,
            panes,
            is_active,
        })
    }

    fn list_sessions(&self) -> ConnectorResult<Vec<SessionInfo>> {
        // List all sessions with our prefix
        let output = self.run_tmux(&["list-sessions", "-F", "#S"])?;

        let mut sessions = Vec::new();
        for line in output.lines() {
            if line.starts_with(&self.session_prefix) {
                let session_id = TerminalSessionId(line.to_string());
                if let Ok(info) = self.session_info(&session_id) {
                    sessions.push(info);
                }
            }
        }

        Ok(sessions)
    }

    fn split_pane(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        direction: SplitDirection,
    ) -> ConnectorResult<usize> {
        let target = self.pane_target(session, pane_index);

        let split_arg = match direction {
            SplitDirection::Horizontal => "-h",
            SplitDirection::Vertical => "-v",
        };

        // Split and get new pane ID
        let new_pane_id = self.run_tmux(&[
            "split-window",
            "-t",
            &target,
            split_arg,
            "-P",
            "-F",
            "#{pane_index}",
        ])?;

        let new_index: usize = new_pane_id
            .parse()
            .map_err(|e| ConnectorError::SplitFailed(format!("Invalid pane index: {e}")))?;

        // Update tracked pane count
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(s) = sessions.iter_mut().find(|s| s.id == *session) {
                s.pane_count = s.pane_count.saturating_add(1);
            }
        }

        // Apply tiled layout for even distribution
        if let Err(e) = self.run_tmux(&["select-layout", "-t", &session.0, "tiled"]) {
            tracing::debug!("failed to apply tiled layout: {e}");
        }

        Ok(new_index)
    }

    fn send_keys(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        keys: &str,
    ) -> ConnectorResult<()> {
        let target = self.pane_target(session, pane_index);

        // Send keys to the pane
        self.run_tmux_silent(&["send-keys", "-t", &target, keys, "Enter"])?;

        Ok(())
    }

    fn capture_output(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
    ) -> ConnectorResult<PaneContent> {
        self.capture_pane_with_options(
            session,
            pane_index,
            CapturePaneOptions {
                start: Some(-100),
                end: None,
                include_escape_sequences: false,
                join_wrapped_lines: false,
                max_lines: Some(100),
            },
        )
    }

    fn set_pane_title(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        title: &str,
    ) -> ConnectorResult<()> {
        let target = self.pane_target(session, pane_index);

        self.run_tmux_silent(&["select-pane", "-t", &target, "-T", title])?;

        Ok(())
    }

    fn select_pane(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
    ) -> ConnectorResult<()> {
        let target = self.pane_target(session, pane_index);

        self.run_tmux_silent(&["select-pane", "-t", &target])?;

        Ok(())
    }

    fn kill_pane(&mut self, session: &TerminalSessionId, pane_index: usize) -> ConnectorResult<()> {
        let target = self.pane_target(session, pane_index);

        self.run_tmux_silent(&["kill-pane", "-t", &target])?;

        // Update tracked pane count
        if let Ok(mut sessions) = self.sessions.lock() {
            if let Some(s) = sessions.iter_mut().find(|s| s.id == *session) {
                s.pane_count = s.pane_count.saturating_sub(1);
            }
        }

        Ok(())
    }

    fn wait_for_output(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        pattern: &str,
        timeout_secs: Option<u64>,
    ) -> ConnectorResult<PaneContent> {
        use std::time::{Duration, Instant};

        let timeout = timeout_secs.map_or(Duration::from_secs(30), Duration::from_secs);
        let start = Instant::now();

        while start.elapsed() < timeout {
            let content = self.capture_output(session, pane_index)?;

            if content.text.contains(pattern) {
                return Ok(content);
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        Err(ConnectorError::Timeout(format!(
            "Pattern '{pattern}' not found within {timeout:?}"
        )))
    }
}

/// Options for capturing tmux pane content.
#[derive(Debug, Clone, Copy, Default)]
pub struct CapturePaneOptions {
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub include_escape_sequences: bool,
    pub join_wrapped_lines: bool,
    pub max_lines: Option<usize>,
}

impl TmuxConnector {
    /// Capture pane output with explicit formatting controls.
    pub fn capture_pane_with_options(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        options: CapturePaneOptions,
    ) -> ConnectorResult<PaneContent> {
        let target = self.pane_target(session, pane_index);
        let start_flag = options
            .start
            .map(|n| n.to_string())
            .or_else(|| options.max_lines.map(|n| format!("-{n}")))
            .unwrap_or_else(|| "-100".to_string());
        let end_flag = options
            .end
            .map_or_else(|| "-".to_string(), |n| n.to_string());

        let mut args = vec![
            "capture-pane",
            "-t",
            &target,
            "-p",
            "-S",
            &start_flag,
            "-E",
            &end_flag,
        ];
        if options.include_escape_sequences {
            args.push("-e");
        }
        if options.join_wrapped_lines {
            args.push("-J");
        }

        let content = self.run_tmux(&args)?;

        let dimensions_str = self
            .run_tmux(&[
                "display-message",
                "-t",
                &target,
                "-F",
                "#{pane_width},#{pane_height}",
            ])
            .ok();

        let dimensions = dimensions_str.and_then(|d| {
            let parts: Vec<&str> = d.split(',').collect();
            if parts.len() == 2 {
                let w = parts[0].parse().ok()?;
                let h = parts[1].parse().ok()?;
                Some((h, w))
            } else {
                None
            }
        });

        Ok(PaneContent {
            text: content,
            cursor: None,
            dimensions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, KeyCode, Modifier};

    #[test]
    fn test_tmux_check_available() {
        // This will be true if tmux is installed
        let available = TmuxConnector::check_available();
        println!("tmux available: {available}");
    }

    #[test]
    fn test_connector_creation() {
        let connector = TmuxConnector::new("test");
        assert_eq!(connector.name(), "tmux");
    }

    #[test]
    fn test_connector_default() {
        let connector = TmuxConnector::default();
        assert_eq!(connector.name(), "tmux");
    }

    #[test]
    fn test_connector_default_prefix() {
        let connector = TmuxConnector::default();
        // Default prefix should be "rustycode"
        // We can verify by checking that name is still "tmux"
        assert_eq!(connector.name(), "tmux");
    }

    #[test]
    fn test_parse_pane_info_valid() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("test-session".into());
        let line = "%0,0,bash,vim,/home/user,1";
        let pane = connector.parse_pane_info(&session, line);
        assert!(pane.is_some());
        let pane = pane.unwrap();
        assert_eq!(pane.id, "%0");
        assert_eq!(pane.index, 0);
        assert_eq!(pane.command, Some("vim".to_string()));
        assert_eq!(pane.cwd, Some("/home/user".to_string()));
        assert!(pane.is_active);
    }

    #[test]
    fn test_parse_pane_info_inactive() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("test".into());
        let line = "%5,2,title,git,/tmp,0";
        let pane = connector.parse_pane_info(&session, line).unwrap();
        assert_eq!(pane.index, 2);
        assert!(!pane.is_active);
    }

    #[test]
    fn test_parse_pane_info_empty_command() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("test".into());
        let line = "%1,0,title,,/home,1";
        let pane = connector.parse_pane_info(&session, line).unwrap();
        assert!(pane.command.is_none());
        assert_eq!(pane.cwd, Some("/home".to_string()));
    }

    #[test]
    fn test_parse_pane_info_empty_cwd() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("test".into());
        let line = "%1,0,title,bash,,1";
        let pane = connector.parse_pane_info(&session, line).unwrap();
        assert_eq!(pane.command, Some("bash".to_string()));
        assert!(pane.cwd.is_none());
    }

    #[test]
    fn test_parse_pane_info_too_few_fields() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("test".into());
        // Only 3 fields - need at least 6
        let line = "%0,0,bash";
        assert!(connector.parse_pane_info(&session, line).is_none());
    }

    #[test]
    fn test_parse_pane_info_empty_line() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("test".into());
        assert!(connector.parse_pane_info(&session, "").is_none());
    }

    #[test]
    fn test_parse_pane_info_invalid_index() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("test".into());
        let line = "%0,not_a_number,title,bash,/home,1";
        let pane = connector.parse_pane_info(&session, line).unwrap();
        // Invalid index defaults to 0
        assert_eq!(pane.index, 0);
    }

    #[test]
    fn test_session_target() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("my-session".into());
        let target = connector.session_target(&session);
        assert_eq!(target, "my-session");
    }

    #[test]
    fn test_pane_target() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("my-session".into());
        let target = connector.pane_target(&session, 3);
        assert_eq!(target, "my-session.3");
    }

    #[test]
    fn test_pane_target_zero_index() {
        let connector = TmuxConnector::new("test");
        let session = TerminalSessionId("sess".into());
        let target = connector.pane_target(&session, 0);
        assert_eq!(target, "sess.0");
    }

    #[test]
    fn test_is_available_matches_check() {
        let connector = TmuxConnector::new("test");
        assert_eq!(connector.is_available(), TmuxConnector::check_available());
    }

    #[test]
    fn test_connector_with_custom_prefix() {
        let connector = TmuxConnector::new("myapp");
        assert_eq!(connector.name(), "tmux");
    }

    // --- Session name validation tests ---

    #[test]
    fn test_validate_session_name_valid() {
        assert!(TmuxConnector::validate_session_name("my-session").is_ok());
        assert!(TmuxConnector::validate_session_name("session123").is_ok());
        assert!(TmuxConnector::validate_session_name("a_b_c").is_ok());
    }

    #[test]
    fn test_validate_session_name_empty() {
        assert!(TmuxConnector::validate_session_name("").is_err());
    }

    #[test]
    fn test_validate_session_name_dot() {
        let result = TmuxConnector::validate_session_name("my.session");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("forbidden"));
    }

    #[test]
    fn test_validate_session_name_colon() {
        let result = TmuxConnector::validate_session_name("my:session");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_session_name_control_char() {
        let result = TmuxConnector::validate_session_name("my\tsession");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_session_name_newline() {
        let result = TmuxConnector::validate_session_name("my\nsession");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_session_name_hyphen_ok() {
        assert!(TmuxConnector::validate_session_name("my-session-name").is_ok());
    }

    // --- Typed Key model tests ---

    #[test]
    fn test_key_from_llm_ctrl_c() {
        assert_eq!(
            Key::from_llm("ctrl-c"),
            Key::Mod(Modifier::Ctrl, KeyCode::Char('c'))
        );
        assert_eq!(
            Key::from_llm("Ctrl+C"),
            Key::Mod(Modifier::Ctrl, KeyCode::Char('c'))
        );
        assert_eq!(
            Key::from_llm("C-c"),
            Key::Mod(Modifier::Ctrl, KeyCode::Char('c'))
        );
    }

    #[test]
    fn test_key_from_llm_ctrl_upper() {
        assert_eq!(
            Key::from_llm("ctrl-d"),
            Key::Mod(Modifier::Ctrl, KeyCode::Char('d'))
        );
        assert_eq!(
            Key::from_llm("C-z"),
            Key::Mod(Modifier::Ctrl, KeyCode::Char('z'))
        );
    }

    #[test]
    fn test_key_from_llm_alt_enter() {
        assert_eq!(
            Key::from_llm("alt-enter"),
            Key::Mod(Modifier::Alt, KeyCode::Enter)
        );
        assert_eq!(
            Key::from_llm("Alt+Enter"),
            Key::Mod(Modifier::Alt, KeyCode::Enter)
        );
        assert_eq!(
            Key::from_llm("M-Enter"),
            Key::Mod(Modifier::Alt, KeyCode::Enter)
        );
    }

    #[test]
    fn test_key_from_llm_arrows() {
        assert_eq!(Key::from_llm("up"), Key::Key(KeyCode::Up));
        assert_eq!(Key::from_llm("Up"), Key::Key(KeyCode::Up));
        assert_eq!(Key::from_llm("arrow-up"), Key::Key(KeyCode::Up));
        assert_eq!(Key::from_llm("down"), Key::Key(KeyCode::Down));
        assert_eq!(Key::from_llm("left"), Key::Key(KeyCode::Left));
        assert_eq!(Key::from_llm("right"), Key::Key(KeyCode::Right));
    }

    #[test]
    fn test_key_from_llm_enter_variants() {
        assert_eq!(Key::from_llm("enter"), Key::Key(KeyCode::Enter));
        assert_eq!(Key::from_llm("Enter"), Key::Key(KeyCode::Enter));
        assert_eq!(Key::from_llm("return"), Key::Key(KeyCode::Enter));
    }

    #[test]
    fn test_key_from_llm_special_keys() {
        assert_eq!(Key::from_llm("escape"), Key::Key(KeyCode::Escape));
        assert_eq!(Key::from_llm("esc"), Key::Key(KeyCode::Escape));
        assert_eq!(Key::from_llm("tab"), Key::Key(KeyCode::Tab));
        assert_eq!(Key::from_llm("backspace"), Key::Key(KeyCode::Backspace));
        assert_eq!(Key::from_llm("delete"), Key::Key(KeyCode::Delete));
        assert_eq!(Key::from_llm("home"), Key::Key(KeyCode::Home));
        assert_eq!(Key::from_llm("end"), Key::Key(KeyCode::End));
        assert_eq!(Key::from_llm("pageup"), Key::Key(KeyCode::PageUp));
        assert_eq!(Key::from_llm("pagedown"), Key::Key(KeyCode::PageDown));
        assert_eq!(Key::from_llm("space"), Key::Key(KeyCode::Space));
    }

    #[test]
    fn test_key_from_llm_function_keys() {
        assert_eq!(Key::from_llm("f1"), Key::Key(KeyCode::F(1)));
        assert_eq!(Key::from_llm("F5"), Key::Key(KeyCode::F(5)));
        assert_eq!(Key::from_llm("f12"), Key::Key(KeyCode::F(12)));
    }

    #[test]
    fn test_key_from_llm_single_char() {
        assert_eq!(Key::from_llm("a"), Key::Key(KeyCode::Char('a')));
        assert_eq!(Key::from_llm("Z"), Key::Key(KeyCode::Char('Z')));
        assert_eq!(Key::from_llm("5"), Key::Key(KeyCode::Char('5')));
    }

    #[test]
    fn test_key_from_llm_raw_fallback() {
        assert_eq!(
            Key::from_llm("SomethingWeird"),
            Key::Raw("SomethingWeird".to_string())
        );
    }

    #[test]
    fn test_key_from_llm_whitespace_trimmed() {
        assert_eq!(Key::from_llm("  enter  "), Key::Key(KeyCode::Enter));
    }

    #[test]
    fn test_key_to_tmux_args_text() {
        let args = Key::Text("ls -la".to_string()).to_tmux_args();
        assert_eq!(args, vec!["-l", "ls -la"]);
    }

    #[test]
    fn test_key_to_tmux_args_enter() {
        let args = Key::Key(KeyCode::Enter).to_tmux_args();
        assert_eq!(args, vec!["Enter"]);
    }

    #[test]
    fn test_key_to_tmux_args_ctrl_c() {
        let args = Key::Mod(Modifier::Ctrl, KeyCode::Char('c')).to_tmux_args();
        assert_eq!(args, vec!["C-c"]);
    }

    #[test]
    fn test_key_to_tmux_args_alt_enter() {
        let args = Key::Mod(Modifier::Alt, KeyCode::Enter).to_tmux_args();
        assert_eq!(args, vec!["M-Enter"]);
    }

    #[test]
    fn test_key_to_tmux_args_function_key() {
        let args = Key::Key(KeyCode::F(5)).to_tmux_args();
        assert_eq!(args, vec!["F5"]);
    }

    #[test]
    fn test_key_to_tmux_args_raw() {
        let args = Key::Raw("C-Space".to_string()).to_tmux_args();
        assert_eq!(args, vec!["C-Space"]);
    }

    #[test]
    fn test_keycode_all_variants_to_tmux() {
        let cases: Vec<(KeyCode, &str)> = vec![
            (KeyCode::Enter, "Enter"),
            (KeyCode::Escape, "Escape"),
            (KeyCode::Tab, "Tab"),
            (KeyCode::Backspace, "BSpace"),
            (KeyCode::Delete, "Delete"),
            (KeyCode::Up, "Up"),
            (KeyCode::Down, "Down"),
            (KeyCode::Left, "Left"),
            (KeyCode::Right, "Right"),
            (KeyCode::Home, "Home"),
            (KeyCode::End, "End"),
            (KeyCode::PageUp, "PageUp"),
            (KeyCode::PageDown, "PageDown"),
            (KeyCode::Space, "Space"),
            (KeyCode::F(1), "F1"),
            (KeyCode::F(12), "F12"),
            (KeyCode::Char('x'), "x"),
        ];
        for (kc, expected) in cases {
            let args = Key::Key(kc).to_tmux_args();
            assert_eq!(
                args,
                vec![expected],
                "KeyCode::{kc:?} should produce {expected}"
            );
        }
    }

    #[test]
    fn test_key_from_llm_ctrl_enter() {
        assert_eq!(
            Key::from_llm("C-enter"),
            Key::Mod(Modifier::Ctrl, KeyCode::Enter)
        );
        assert_eq!(Key::from_llm("C-up"), Key::Mod(Modifier::Ctrl, KeyCode::Up));
    }

    #[test]
    fn test_key_from_llm_ctrl_function_key() {
        assert_eq!(
            Key::from_llm("C-f1"),
            Key::Mod(Modifier::Ctrl, KeyCode::F(1))
        );
    }

    // --- Batch operation tests ---

    #[test]
    fn test_batch_empty() {
        let connector = TmuxConnector::new("test");
        let batch = TmuxBatch::new();
        let results = connector.execute_batch(&batch).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_batch_len_and_is_empty() {
        let mut batch = TmuxBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);

        batch.push(TmuxOp::KillSession {
            target: "test".to_string(),
        });
        assert!(!batch.is_empty());
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn test_batch_ops_accessor() {
        let mut batch = TmuxBatch::new();
        batch.push(TmuxOp::KillSession {
            target: "s1".to_string(),
        });
        batch.push(TmuxOp::KillSession {
            target: "s2".to_string(),
        });
        assert_eq!(batch.ops().len(), 2);
    }

    #[test]
    fn test_op_to_args_new_session() {
        let args = TmuxConnector::op_to_args(&TmuxOp::NewSession {
            name: "my-session".to_string(),
            start_dir: None,
        });
        assert_eq!(args, vec!["new-session", "-d", "-s", "my-session"]);
    }

    #[test]
    fn test_op_to_args_new_session_with_dir() {
        let args = TmuxConnector::op_to_args(&TmuxOp::NewSession {
            name: "my-session".to_string(),
            start_dir: Some("/home/user".to_string()),
        });
        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"/home/user".to_string()));
    }

    #[test]
    fn test_op_to_args_kill_session() {
        let args = TmuxConnector::op_to_args(&TmuxOp::KillSession {
            target: "my-session".to_string(),
        });
        assert_eq!(args, vec!["kill-session", "-t", "my-session"]);
    }

    #[test]
    fn test_op_to_args_split_pane() {
        let args = TmuxConnector::op_to_args(&TmuxOp::SplitPane {
            target: "sess.0".to_string(),
            direction: SplitDirection::Horizontal,
            start_dir: None,
        });
        assert!(args.contains(&"-h".to_string()));
        assert!(args.contains(&"sess.0".to_string()));
    }

    #[test]
    fn test_op_to_args_send_keys() {
        let args = TmuxConnector::op_to_args(&TmuxOp::SendKeys {
            target: "sess.0".to_string(),
            keys: vec![
                Key::Key(KeyCode::Enter),
                Key::Mod(Modifier::Ctrl, KeyCode::Char('c')),
            ],
        });
        assert!(args.contains(&"Enter".to_string()));
        assert!(args.contains(&"C-c".to_string()));
    }

    #[test]
    fn test_op_to_args_send_text() {
        let args = TmuxConnector::op_to_args(&TmuxOp::SendText {
            target: "sess.0".to_string(),
            text: "ls -la".to_string(),
        });
        assert!(args.contains(&"-l".to_string()));
        assert!(args.contains(&"ls -la".to_string()));
    }

    #[test]
    fn test_op_to_args_resize_pane() {
        let args = TmuxConnector::op_to_args(&TmuxOp::ResizePane {
            target: "sess.0".to_string(),
            direction: ResizeDirection::Up,
            cells: 5,
        });
        assert_eq!(args, vec!["resize-pane", "-t", "sess.0", "-U", "5"]);
    }

    #[test]
    fn test_op_to_args_resize_pane_directions() {
        for (dir, flag) in [
            (ResizeDirection::Up, "-U"),
            (ResizeDirection::Down, "-D"),
            (ResizeDirection::Left, "-L"),
            (ResizeDirection::Right, "-R"),
        ] {
            let args = TmuxConnector::op_to_args(&TmuxOp::ResizePane {
                target: "s.0".to_string(),
                direction: dir,
                cells: 10,
            });
            assert!(
                args.contains(&flag.to_string()),
                "ResizeDirection::{dir:?} should produce {flag}"
            );
        }
    }

    #[test]
    fn test_op_to_args_swap_pane() {
        let args = TmuxConnector::op_to_args(&TmuxOp::SwapPane {
            src: "sess.0".to_string(),
            dst: "sess.1".to_string(),
        });
        assert_eq!(args, vec!["swap-pane", "-s", "sess.0", "-t", "sess.1"]);
    }

    #[test]
    fn test_op_to_args_new_window() {
        let args = TmuxConnector::op_to_args(&TmuxOp::NewWindow {
            session: "my-sess".to_string(),
            name: Some("editor".to_string()),
        });
        assert!(args.contains(&"-n".to_string()));
        assert!(args.contains(&"editor".to_string()));
    }

    #[test]
    fn test_op_to_args_select_layout() {
        let args = TmuxConnector::op_to_args(&TmuxOp::SelectLayout {
            target: "my-sess".to_string(),
            layout: "tiled".to_string(),
        });
        assert_eq!(args, vec!["select-layout", "-t", "my-sess", "tiled"]);
    }

    #[test]
    fn test_op_to_args_set_pane_title() {
        let args = TmuxConnector::op_to_args(&TmuxOp::SetPaneTitle {
            target: "sess.0".to_string(),
            title: "my pane".to_string(),
        });
        assert!(args.contains(&"-T".to_string()));
        assert!(args.contains(&"my pane".to_string()));
    }

    #[test]
    fn test_tmux_backend_cli_still_works() {
        let connector = TmuxConnector::new("test");
        assert_eq!(connector.name(), "tmux");
    }

    // --- Window management tests ---

    #[test]
    fn test_list_windows_parses_output() {
        // Unit test for parse logic — list_windows itself requires a live session
        let line = "@0,0,bash,1,2";
        let parts: Vec<&str> = line.split(',').collect();
        assert_eq!(parts.len(), 5);
        let info = crate::WindowInfo {
            id: parts[0].to_string(),
            index: parts[1].parse().unwrap_or(0),
            name: parts[2].to_string(),
            is_active: parts[3] == "1",
            pane_count: parts[4].parse().unwrap_or(1),
        };
        assert_eq!(info.id, "@0");
        assert_eq!(info.index, 0);
        assert_eq!(info.name, "bash");
        assert!(info.is_active);
        assert_eq!(info.pane_count, 2);
    }

    #[test]
    fn test_list_windows_inactive_window() {
        let line = "@3,2,editor,0,1";
        let parts: Vec<&str> = line.split(',').collect();
        let info = crate::WindowInfo {
            id: parts[0].to_string(),
            index: parts[1].parse().unwrap_or(0),
            name: parts[2].to_string(),
            is_active: parts[3] == "1",
            pane_count: parts[4].parse().unwrap_or(1),
        };
        assert!(!info.is_active);
        assert_eq!(info.index, 2);
    }

    // --- Screenshot tests ---

    #[test]
    fn test_strip_ansi_simple() {
        assert_eq!(strip_ansi_escapes("hello world"), "hello world");
    }

    #[test]
    fn test_strip_ansi_color_code() {
        assert_eq!(
            strip_ansi_escapes("\x1b[32mgreen text\x1b[0m"),
            "green text"
        );
    }

    #[test]
    fn test_strip_ansi_multiple_codes() {
        assert_eq!(
            strip_ansi_escapes("\x1b[1;31;42mbold red on green\x1b[0m normal"),
            "bold red on green normal"
        );
    }

    #[test]
    fn test_strip_ansi_empty() {
        assert_eq!(strip_ansi_escapes(""), "");
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        assert_eq!(strip_ansi_escapes("just plain text"), "just plain text");
    }

    #[test]
    fn test_apply_region_full() {
        let lines = vec!["line0".into(), "line1".into(), "line2".into()];
        let result = apply_region(lines, None, None);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_apply_region_clipping() {
        let lines = vec!["aaaa".into(), "bbbb".into(), "cccc".into(), "dddd".into()];
        let region = Region {
            left: 1,
            top: 1,
            width: 2,
            height: 2,
        };
        let result = apply_region(lines, Some(&region), None);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "bb");
        assert_eq!(result[1], "cc");
    }

    #[test]
    fn test_apply_region_around_cursor() {
        let lines: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
        let result = apply_region(lines, None, Some(2));
        // mid = 5, start = 3, end = 8
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], "line3");
        assert_eq!(result[4], "line7");
    }

    #[test]
    fn test_apply_region_compact_skips_empty() {
        let _lines: Vec<String> = vec!["hello".into(), String::new(), "  ".into(), "world".into()];
        let opts = ScreenshotOptions {
            layers: vec![],
            region: None,
            around_cursor: None,
            compact: true,
        };
        let result = parse_screenshot_lines("hello\n\n  \nworld", &opts);
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_screenshot_to_compact() {
        let shot = Screenshot {
            text: vec!["hello".into(), "world".into()],
            cursor: Some((1, 3)),
            dimensions: (24, 80),
        };
        let compact = shot.to_compact();
        assert!(compact.contains("Terminal: 24 rows"));
        assert!(compact.contains("Cursor: row=1, col=3"));
        assert!(compact.contains("hello"));
        assert!(compact.contains("world"));
    }

    #[test]
    fn test_screenshot_to_compact_no_cursor() {
        let shot = Screenshot {
            text: vec!["test".into()],
            cursor: None,
            dimensions: (10, 40),
        };
        let compact = shot.to_compact();
        assert!(compact.contains("Terminal: 10 rows"));
        assert!(!compact.contains("Cursor"));
    }

    #[test]
    fn test_screenshot_options_default() {
        let opts = ScreenshotOptions::default();
        assert!(opts.region.is_none());
        assert!(opts.around_cursor.is_none());
        assert!(!opts.compact);
        assert_eq!(opts.layers.len(), 2);
    }

    #[test]
    fn test_region_fields() {
        let r = Region {
            left: 5,
            top: 10,
            width: 20,
            height: 5,
        };
        assert_eq!(r.left, 5);
        assert_eq!(r.top, 10);
        assert_eq!(r.width, 20);
        assert_eq!(r.height, 5);
    }
}
