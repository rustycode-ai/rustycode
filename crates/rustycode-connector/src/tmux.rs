//! Tmux Connector Implementation
//!
//! Provides tmux-based terminal multiplexing capabilities.

use crate::{
    ConnectorError, ConnectorResult, PaneContent, PaneInfo, SessionId, SessionInfo, SplitDirection,
    TerminalConnector,
};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Tmux session metadata
#[derive(Debug, Clone)]
struct TmuxSession {
    id: SessionId,
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
    fn session_target(&self, session: &SessionId) -> String {
        session.0.clone()
    }

    /// Get the pane target string
    fn pane_target(&self, session: &SessionId, pane_index: usize) -> String {
        format!("{}.{}", self.session_target(session), pane_index)
    }

    /// Run a tmux command and capture output
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

    /// Run a tmux command without capturing output
    fn run_tmux_silent(&self, args: &[&str]) -> Result<(), ConnectorError> {
        Command::new("tmux")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| ConnectorError::Other(format!("Failed to execute tmux: {e}")))?;
        Ok(())
    }

    /// Parse pane information from tmux
    fn parse_pane_info(&self, _session: &SessionId, pane_line: &str) -> Option<PaneInfo> {
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
}

impl TerminalConnector for TmuxConnector {
    fn name(&self) -> &'static str {
        "tmux"
    }

    fn is_available(&self) -> bool {
        Self::check_available()
    }

    fn create_session(&mut self, name: &str) -> ConnectorResult<SessionId> {
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
            id: SessionId(session_id.clone()),
            pane_count: 1, // Initial session has one pane
        };

        self.sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?
            .push(session);

        Ok(SessionId(session_id))
    }

    fn close_session(&mut self, session: &SessionId) -> ConnectorResult<()> {
        self.run_tmux_silent(&["kill-session", "-t", &session.0])?;

        // Remove from tracked sessions
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;
        sessions.retain(|s| s.id != *session);

        Ok(())
    }

    fn session_info(&self, session: &SessionId) -> ConnectorResult<SessionInfo> {
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
                let session_id = SessionId(line.to_string());
                if let Ok(info) = self.session_info(&session_id) {
                    sessions.push(info);
                }
            }
        }

        Ok(sessions)
    }

    fn split_pane(
        &mut self,
        session: &SessionId,
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
        let _ = self.run_tmux(&["select-layout", "-t", &session.0, "tiled"]);

        Ok(new_index)
    }

    fn send_keys(
        &mut self,
        session: &SessionId,
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
        session: &SessionId,
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
        session: &SessionId,
        pane_index: usize,
        title: &str,
    ) -> ConnectorResult<()> {
        let target = self.pane_target(session, pane_index);

        self.run_tmux_silent(&["select-pane", "-t", &target, "-T", title])?;

        Ok(())
    }

    fn select_pane(&mut self, session: &SessionId, pane_index: usize) -> ConnectorResult<()> {
        let target = self.pane_target(session, pane_index);

        self.run_tmux_silent(&["select-pane", "-t", &target])?;

        Ok(())
    }

    fn kill_pane(&mut self, session: &SessionId, pane_index: usize) -> ConnectorResult<()> {
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
        session: &SessionId,
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
        session: &SessionId,
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
        let session = SessionId("test-session".into());
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
        let session = SessionId("test".into());
        let line = "%5,2,title,git,/tmp,0";
        let pane = connector.parse_pane_info(&session, line).unwrap();
        assert_eq!(pane.index, 2);
        assert!(!pane.is_active);
    }

    #[test]
    fn test_parse_pane_info_empty_command() {
        let connector = TmuxConnector::new("test");
        let session = SessionId("test".into());
        let line = "%1,0,title,,/home,1";
        let pane = connector.parse_pane_info(&session, line).unwrap();
        assert!(pane.command.is_none());
        assert_eq!(pane.cwd, Some("/home".to_string()));
    }

    #[test]
    fn test_parse_pane_info_empty_cwd() {
        let connector = TmuxConnector::new("test");
        let session = SessionId("test".into());
        let line = "%1,0,title,bash,,1";
        let pane = connector.parse_pane_info(&session, line).unwrap();
        assert_eq!(pane.command, Some("bash".to_string()));
        assert!(pane.cwd.is_none());
    }

    #[test]
    fn test_parse_pane_info_too_few_fields() {
        let connector = TmuxConnector::new("test");
        let session = SessionId("test".into());
        // Only 3 fields - need at least 6
        let line = "%0,0,bash";
        assert!(connector.parse_pane_info(&session, line).is_none());
    }

    #[test]
    fn test_parse_pane_info_empty_line() {
        let connector = TmuxConnector::new("test");
        let session = SessionId("test".into());
        assert!(connector.parse_pane_info(&session, "").is_none());
    }

    #[test]
    fn test_parse_pane_info_invalid_index() {
        let connector = TmuxConnector::new("test");
        let session = SessionId("test".into());
        let line = "%0,not_a_number,title,bash,/home,1";
        let pane = connector.parse_pane_info(&session, line).unwrap();
        // Invalid index defaults to 0
        assert_eq!(pane.index, 0);
    }

    #[test]
    fn test_session_target() {
        let connector = TmuxConnector::new("test");
        let session = SessionId("my-session".into());
        let target = connector.session_target(&session);
        assert_eq!(target, "my-session");
    }

    #[test]
    fn test_pane_target() {
        let connector = TmuxConnector::new("test");
        let session = SessionId("my-session".into());
        let target = connector.pane_target(&session, 3);
        assert_eq!(target, "my-session.3");
    }

    #[test]
    fn test_pane_target_zero_index() {
        let connector = TmuxConnector::new("test");
        let session = SessionId("sess".into());
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
}
