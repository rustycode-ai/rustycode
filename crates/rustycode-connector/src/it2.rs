//! iTerm2 Connector Implementation using it2 CLI
//!
//! Provides iTerm2 terminal capabilities via the it2 command-line tool,
//! which uses iTerm2's Python API under the hood.
//!
//! The it2 CLI is cross-platform (macOS) and provides much better
//! performance than AppleScript-based approaches.
//!
//! Installation: pip install it2-iterm2
//! Or from: <https://github.com/mkusaka/it2>

use crate::{
    ConnectorError, ConnectorResult, PaneContent, PaneInfo, TerminalSessionId, SessionInfo, SplitDirection,
    TerminalConnector,
};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// it2 session metadata
#[derive(Debug, Clone)]
struct It2Session {
    id: TerminalSessionId,
    name: String,
    pane_count: usize,
}

/// Connector for iTerm2 terminal using it2 CLI
pub struct It2Connector {
    /// Track created sessions
    sessions: Mutex<Vec<It2Session>>,
}

impl Default for It2Connector {
    fn default() -> Self {
        Self::new()
    }
}

impl It2Connector {
    pub const fn new() -> Self {
        Self {
            sessions: Mutex::new(Vec::new()),
        }
    }

    /// Check if it2 CLI is available
    pub fn check_available() -> bool {
        Command::new("it2")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Run an it2 command without capturing output
    fn run_it2_silent(&self, args: &[&str]) -> Result<(), ConnectorError> {
        Command::new("it2")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| ConnectorError::Other(format!("Failed to execute it2: {e}")))?;
        Ok(())
    }
}

impl TerminalConnector for It2Connector {
    fn name(&self) -> &'static str {
        "it2"
    }

    fn is_available(&self) -> bool {
        Self::check_available()
    }

    fn create_session(&mut self, name: &str) -> ConnectorResult<TerminalSessionId> {
        // Create a new window with the given name
        // Create new window using it2
        self.run_it2_silent(&["window", "new"])?;

        // Set the session name
        let session_id = TerminalSessionId(format!("it2-{}-{}", name, std::process::id()));
        let _ = self.run_it2_silent(&["session", "set-name", "-s", &session_id.0, name]); // Name setting is optional

        let session = It2Session {
            id: session_id.clone(),
            name: name.to_string(),
            pane_count: 1,
        };

        self.sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?
            .push(session);

        Ok(session_id)
    }

    fn close_session(&mut self, session: &TerminalSessionId) -> ConnectorResult<()> {
        // Close the session using it2
        self.run_it2_silent(&["session", "close", "-s", &session.0])?;

        // Remove from tracked sessions
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;
        sessions.retain(|s| s.id != *session);

        Ok(())
    }

    fn session_info(&self, session: &TerminalSessionId) -> ConnectorResult<SessionInfo> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;

        let s = sessions
            .iter()
            .find(|s| s.id == *session)
            .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))?;

        // Build pane info based on tracked pane count
        let panes = (0..s.pane_count)
            .map(|i| PaneInfo {
                id: format!("{}-pane-{}", s.id.0, i),
                index: i,
                cwd: None,
                command: None,
                is_active: i == 0,
            })
            .collect();

        Ok(SessionInfo {
            id: session.clone(),
            name: s.name.clone(),
            panes,
            is_active: true,
        })
    }

    fn list_sessions(&self) -> ConnectorResult<Vec<SessionInfo>> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;

        Ok(sessions
            .iter()
            .filter_map(|s| self.session_info(&s.id).ok())
            .collect())
    }

    fn split_pane(
        &mut self,
        session: &TerminalSessionId,
        _pane_index: usize,
        direction: SplitDirection,
    ) -> ConnectorResult<usize> {
        let vertical_flag = match direction {
            SplitDirection::Horizontal => "-v", // -v splits vertically (side by side)
            SplitDirection::Vertical => "",     // no flag splits horizontally (default)
        };

        let args = if vertical_flag.is_empty() {
            vec!["session", "split", "-s", &session.0]
        } else {
            vec!["session", "split", vertical_flag, "-s", &session.0]
        };

        self.run_it2_silent(&args)?;

        // Update tracked pane count
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;

        if let Some(s) = sessions.iter_mut().find(|s| s.id == *session) {
            s.pane_count += 1;
            Ok(s.pane_count - 1)
        } else {
            Err(ConnectorError::SessionNotFound(session.0.clone()))
        }
    }

    fn send_keys(
        &mut self,
        session: &TerminalSessionId,
        _pane_index: usize,
        keys: &str,
    ) -> ConnectorResult<()> {
        // Send keys using it2 session send
        // Note: it2 send doesn't add newline, so we append Enter for command execution
        self.run_it2_silent(&["session", "send", "-s", &session.0, keys])?;

        Ok(())
    }

    fn capture_output(
        &self,
        session: &TerminalSessionId,
        _pane_index: usize,
    ) -> ConnectorResult<PaneContent> {
        // Capture screen to temp file and read it
        let temp_file = format!(
            "/tmp/iterm2_capture_{}_{}.txt",
            session.0.replace('-', "_"),
            std::process::id()
        );

        self.run_it2_silent(&["session", "capture", "-s", &session.0, "-o", &temp_file])?;

        // Check if file exists before reading
        if !std::path::Path::new(&temp_file).exists() {
            // it2 might not have created the file if session doesn't exist
            // Try capturing from active session
            self.run_it2_silent(&["session", "capture", "-o", &temp_file])?;
        }

        // Read the captured content
        let content = std::fs::read_to_string(&temp_file).map_err(|e| {
            ConnectorError::CaptureFailed(format!("Failed to read captured content: {e}"))
        })?;

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_file);

        Ok(PaneContent::new(content))
    }

    fn set_pane_title(
        &mut self,
        session: &TerminalSessionId,
        _pane_index: usize,
        title: &str,
    ) -> ConnectorResult<()> {
        // Set session name using it2
        self.run_it2_silent(&["session", "set-name", "-s", &session.0, title])?;

        Ok(())
    }

    fn select_pane(&mut self, session: &TerminalSessionId, _pane_index: usize) -> ConnectorResult<()> {
        // it2 doesn't have direct pane selection by index
        // We can use session focus which should activate the window
        self.run_it2_silent(&["session", "focus", "-s", &session.0])?;

        // Note: Pane-level selection is limited in it2
        // This is a known limitation - we focus the session but can't select specific panes

        Ok(())
    }

    fn kill_pane(&mut self, _session: &TerminalSessionId, _pane_index: usize) -> ConnectorResult<()> {
        // it2 doesn't support killing individual panes directly
        // This is a limitation of the it2 CLI
        Err(ConnectorError::Other(
            "it2 does not support killing individual panes".to_string(),
        ))
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
            if let Ok(content) = self.capture_output(session, pane_index) {
                if content.text.contains(pattern) {
                    return Ok(content);
                }
            } else {
                // Capture might fail if pane is busy, continue waiting
            }

            std::thread::sleep(Duration::from_millis(200));
        }

        Err(ConnectorError::Timeout(format!(
            "Pattern '{pattern}' not found within {timeout:?}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_available() {
        let available = It2Connector::check_available();
        println!("it2 CLI available: {available}");
    }

    #[test]
    fn test_connector_creation() {
        let connector = It2Connector::new();
        assert_eq!(connector.name(), "it2");
    }

    #[test]
    fn test_connector_default() {
        let connector = It2Connector::default();
        assert_eq!(connector.name(), "it2");
    }

    #[test]
    fn test_is_available_matches_check() {
        let connector = It2Connector::new();
        assert_eq!(connector.is_available(), It2Connector::check_available());
    }

    #[test]
    fn test_kill_pane_always_fails() {
        let mut connector = It2Connector::new();
        let session = TerminalSessionId("test".into());
        let result = connector.kill_pane(&session, 0);
        assert!(result.is_err());
        match result {
            Err(ConnectorError::Other(msg)) => {
                assert!(msg.contains("does not support killing individual panes"));
            }
            _ => panic!("Expected Other error"),
        }
    }

    #[test]
    fn test_session_info_not_found() {
        let connector = It2Connector::new();
        let session = TerminalSessionId("nonexistent".into());
        let result = connector.session_info(&session);
        assert!(result.is_err());
        match result {
            Err(ConnectorError::SessionNotFound(id)) => {
                assert_eq!(id, "nonexistent");
            }
            _ => panic!("Expected SessionNotFound error"),
        }
    }

    #[test]
    fn test_list_sessions_empty() {
        let connector = It2Connector::new();
        let sessions = connector.list_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_split_pane_session_not_found() {
        let mut connector = It2Connector::new();
        let session = TerminalSessionId("nonexistent".into());
        let result = connector.split_pane(&session, 0, SplitDirection::Horizontal);
        assert!(result.is_err());
    }
}
