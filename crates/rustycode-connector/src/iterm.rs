//! iTerm2 Connector Implementation
//!
//! Provides iTerm2-specific terminal capabilities via `AppleScript` and
//! iTerm2's proprietary API.
//!
//! Note: iTerm2 is macOS-only and requires the terminal to be running.

use crate::{
    ConnectorError, ConnectorResult, PaneContent, PaneInfo, SessionInfo, SplitDirection,
    TerminalConnector, TerminalSessionId,
};
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// iTerm2 session metadata
#[derive(Debug, Clone)]
struct ITermSession {
    id: TerminalSessionId,
    name: String,
    #[allow(dead_code)]
    window_id: usize,
    pane_count: usize,
}

/// Connector for iTerm2 terminal on macOS
pub struct ITermConnector {
    /// Track created sessions
    sessions: Mutex<Vec<ITermSession>>,
    /// Next window ID to use
    #[allow(dead_code)]
    next_window_id: Mutex<usize>,
}

impl Default for ITermConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl ITermConnector {
    pub const fn new() -> Self {
        Self {
            sessions: Mutex::new(Vec::new()),
            next_window_id: Mutex::new(1000), // Start from arbitrary number
        }
    }

    /// Check if running inside iTerm2
    pub fn is_inside_iterm() -> bool {
        std::env::var("TERM_PROGRAM").is_ok_and(|v| v == "iTerm.app")
    }

    /// Check if iTerm2 is running (macOS only)
    pub fn check_iterm_running() -> bool {
        #[cfg(target_os = "macos")]
        {
            Command::new("osascript")
                .args([
                    "-e",
                    r#"tell application "iTerm2" to return (count of windows) > 0"#,
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Check if this connector is available
    pub fn is_available() -> bool {
        // Must be macOS and iTerm2 must be running
        #[cfg(target_os = "macos")]
        {
            Self::check_iterm_running()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Run an `AppleScript` command (macOS only)
    #[cfg(target_os = "macos")]
    fn run_applescript(&self, script: &str) -> Result<String, ConnectorError> {
        let output = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|e| ConnectorError::Other(format!("Failed to execute osascript: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ConnectorError::Other(format!(
                "AppleScript failed: {}",
                stderr.trim()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Run an `AppleScript` command without capturing output (macOS only)
    #[cfg(target_os = "macos")]
    fn run_applescript_silent(&self, script: &str) -> Result<(), ConnectorError> {
        Command::new("osascript")
            .arg("-e")
            .arg(script)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| ConnectorError::Other(format!("Failed to execute osascript: {e}")))?;
        Ok(())
    }
}

/// Escape special characters for safe embedding in `AppleScript` string literals.
///
/// `AppleScript` uses `"..."` for strings. Backslash and double-quote must be
/// escaped to prevent breaking out of the string and injecting arbitrary
/// `AppleScript` commands.
#[cfg(target_os = "macos")]
fn escape_applescript_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[allow(unused_variables)]
impl TerminalConnector for ITermConnector {
    fn name(&self) -> &'static str {
        "iTerm2"
    }

    fn is_available(&self) -> bool {
        Self::is_available()
    }

    fn create_session(&mut self, name: &str) -> ConnectorResult<TerminalSessionId> {
        #[cfg(target_os = "macos")]
        {
            // Create a new iTerm2 window with the given name
            let window_id = {
                let mut next = self
                    .next_window_id
                    .lock()
                    .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;
                let id = *next;
                *next += 1;
                id
            };

            // Simplified AppleScript that works with iTerm2
            let script = r#"
                tell application "iTerm2"
                    activate
                    create window with default profile
                end tell
                "#
            .to_string();

            self.run_applescript(&script).map_err(|e| {
                ConnectorError::SessionCreateFailed(format!("Failed to create iTerm2 window: {e}"))
            })?;

            let session_id = TerminalSessionId(format!("iterm-{name}-{window_id}"));

            let session = ITermSession {
                id: session_id.clone(),
                name: name.to_string(),
                window_id,
                pane_count: 1,
            };

            self.sessions
                .lock()
                .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?
                .push(session);

            Ok(session_id)
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(ConnectorError::NotAvailable(
                "iTerm2 is only available on macOS".to_string(),
            ))
        }
    }

    fn close_session(&mut self, session: &TerminalSessionId) -> ConnectorResult<()> {
        #[cfg(target_os = "macos")]
        {
            let sessions = self
                .sessions
                .lock()
                .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;

            if let Some(s) = sessions.iter().find(|s| s.id == *session) {
                let window_id = s.window_id;

                // Close the window by index (iTerm2 windows are 1-indexed)
                // We use the window_id we stored which corresponds to the window index
                let script = format!(
                    r#"
                    tell application "iTerm2"
                        tell window {window_id}
                            close
                        end tell
                    end tell
                    "#
                );

                self.run_applescript_silent(&script)?;
            } else {
                // Session not found, try to close the last created window
                // This handles the case where we're reconnecting to existing windows
                let script = r#"
                tell application "iTerm2"
                    if (count of windows) > 0 then
                        close last window
                    end if
                end tell
                "#;
                if let Err(e) = self.run_applescript_silent(script) {
                    tracing::warn!("Failed to close iTerm2 fallback window: {e}");
                }
            }

            // Remove from tracked sessions
            drop(sessions);
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;
            sessions.retain(|s| s.id != *session);

            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(ConnectorError::NotAvailable(
                "iTerm2 is only available on macOS".to_string(),
            ))
        }
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

        let panes = (0..s.pane_count)
            .map(|i| PaneInfo {
                id: format!("{}-pane-{}", s.id.0, i),
                index: i,
                cwd: None,
                command: None,
                is_active: i == 0, // First pane is active by default
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
        #[cfg(target_os = "macos")]
        {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;

            let s = sessions
                .iter_mut()
                .find(|s| s.id == *session)
                .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))?;

            let split_direction = match direction {
                SplitDirection::Horizontal => "split horizontally",
                SplitDirection::Vertical => "split vertically",
            };

            // Use "current window" instead of specific window ID
            let script = format!(
                r#"
                tell application "iTerm2"
                    tell current window
                        tell current tab
                            {split_direction}
                        end tell
                    end tell
                end tell
                "#
            );

            self.run_applescript_silent(&script)
                .map_err(|e| ConnectorError::SplitFailed(format!("Failed to split pane: {e}")))?;

            s.pane_count += 1;
            Ok(s.pane_count - 1)
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(ConnectorError::NotAvailable(
                "iTerm2 is only available on macOS".to_string(),
            ))
        }
    }

    fn send_keys(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
        keys: &str,
    ) -> ConnectorResult<()> {
        #[cfg(target_os = "macos")]
        {
            let sessions = self
                .sessions
                .lock()
                .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;

            let _s = sessions
                .iter()
                .find(|s| s.id == *session)
                .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))?;

            // Use "current window" and select pane first
            let safe_keys = escape_applescript_string(keys);
            let script = format!(
                r#"
                tell application "iTerm2"
                    tell current window
                        tell current tab
                            select pane {pane_index}
                            write text "{safe_keys}"
                        end tell
                    end tell
                end tell
                "#
            );

            self.run_applescript_silent(&script)
                .map_err(|e| ConnectorError::SendKeysFailed(format!("Failed to send keys: {e}")))?;

            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(ConnectorError::NotAvailable(
                "iTerm2 is only available on macOS".to_string(),
            ))
        }
    }

    fn capture_output(
        &self,
        _session: &TerminalSessionId,
        _pane_index: usize,
    ) -> ConnectorResult<PaneContent> {
        // Note: iTerm2 doesn't have a direct way to capture pane content via AppleScript
        // This is a limitation compared to tmux
        // For now, return empty content with a note
        Err(ConnectorError::CaptureFailed(
            "iTerm2 does not support capturing pane content via AppleScript".to_string(),
        ))
    }

    fn set_pane_title(
        &mut self,
        _session: &TerminalSessionId,
        _pane_index: usize,
        title: &str,
    ) -> ConnectorResult<()> {
        #[cfg(target_os = "macos")]
        {
            // iTerm2 doesn't support setting pane titles via AppleScript directly
            // The pane title is typically set by the shell or application running in it
            // We can try to set the custom title of the window
            let safe_title = escape_applescript_string(title);
            let script = format!(
                r#"
                tell application "iTerm2"
                    tell current window
                        set custom title to "{safe_title}"
                    end tell
                end tell
                "#
            );

            self.run_applescript_silent(&script)?;
            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(ConnectorError::NotAvailable(
                "iTerm2 is only available on macOS".to_string(),
            ))
        }
    }

    fn select_pane(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
    ) -> ConnectorResult<()> {
        #[cfg(target_os = "macos")]
        {
            let sessions = self
                .sessions
                .lock()
                .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?;

            let _s = sessions
                .iter()
                .find(|s| s.id == *session)
                .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))?;

            // Use "current window" instead of specific window ID
            let script = format!(
                r#"
                tell application "iTerm2"
                    tell current window
                        tell current tab
                            select pane {pane_index}
                        end tell
                    end tell
                end tell
                "#
            );

            self.run_applescript_silent(&script)?;
            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(ConnectorError::NotAvailable(
                "iTerm2 is only available on macOS".to_string(),
            ))
        }
    }

    fn kill_pane(
        &mut self,
        _session: &TerminalSessionId,
        _pane_index: usize,
    ) -> ConnectorResult<()> {
        // Note: iTerm2 doesn't support killing individual panes via AppleScript
        // You can only close the entire window/tab
        Err(ConnectorError::Other(
            "iTerm2 does not support killing individual panes via AppleScript".to_string(),
        ))
    }

    fn wait_for_output(
        &self,
        _session: &TerminalSessionId,
        _pane_index: usize,
        _pattern: &str,
        _timeout_secs: Option<u64>,
    ) -> ConnectorResult<PaneContent> {
        Err(ConnectorError::Other(
            "wait_for_output is not supported for iTerm2 connector".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_inside_iterm() {
        let result = ITermConnector::is_inside_iterm();
        println!("Running inside iTerm2: {result}");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_check_iterm_running() {
        let result = ITermConnector::check_iterm_running();
        println!("iTerm2 running: {result}");
    }

    #[test]
    fn test_connector_creation() {
        let connector = ITermConnector::new();
        assert_eq!(connector.name(), "iTerm2");
    }

    #[test]
    fn test_connector_default() {
        let connector = ITermConnector::default();
        assert_eq!(connector.name(), "iTerm2");
    }

    #[test]
    fn test_is_available_matches_static() {
        let connector = ITermConnector::new();
        assert_eq!(connector.is_available(), ITermConnector::is_available());
    }

    #[test]
    fn test_capture_output_always_fails() {
        let connector = ITermConnector::new();
        let session = TerminalSessionId("test".into());
        let result = connector.capture_output(&session, 0);
        assert!(result.is_err());
        match result {
            Err(ConnectorError::CaptureFailed(msg)) => {
                assert!(msg.contains("does not support capturing pane content"));
            }
            _ => panic!("Expected CaptureFailed error"),
        }
    }

    #[test]
    fn test_kill_pane_always_fails() {
        let mut connector = ITermConnector::new();
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
    fn test_wait_for_output_always_fails() {
        let connector = ITermConnector::new();
        let session = TerminalSessionId("test".into());
        let result = connector.wait_for_output(&session, 0, "pattern", None);
        assert!(result.is_err());
        match result {
            Err(ConnectorError::Other(msg)) => {
                assert!(msg.contains("not supported"));
            }
            _ => panic!("Expected Other error"),
        }
    }

    #[test]
    fn test_session_info_not_found() {
        let connector = ITermConnector::new();
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
        let connector = ITermConnector::new();
        let sessions = connector.list_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_split_pane_session_not_found() {
        let mut connector = ITermConnector::new();
        let session = TerminalSessionId("nonexistent".into());
        let result = connector.split_pane(&session, 0, SplitDirection::Horizontal);
        assert!(result.is_err());
    }

    #[test]
    fn test_send_keys_session_not_found() {
        let mut connector = ITermConnector::new();
        let session = TerminalSessionId("nonexistent".into());
        let result = connector.send_keys(&session, 0, "ls");
        assert!(result.is_err());
    }

    #[test]
    fn test_select_pane_session_not_found() {
        let mut connector = ITermConnector::new();
        let session = TerminalSessionId("nonexistent".into());
        let result = connector.select_pane(&session, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_inside_iterm_check() {
        // Just verify it doesn't panic - result depends on environment
        let _ = ITermConnector::is_inside_iterm();
    }

    // --- AppleScript escaping tests ---

    #[cfg(target_os = "macos")]
    #[test]
    fn test_escape_applescript_plain_text() {
        assert_eq!(escape_applescript_string("hello world"), "hello world");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_escape_applescript_double_quote() {
        assert_eq!(
            escape_applescript_string(r#"say "hello""#),
            r#"say \"hello\""#
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_escape_applescript_backslash() {
        assert_eq!(
            escape_applescript_string(r"path\to\file"),
            r"path\\to\\file"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_escape_applescript_injection_attempt() {
        // Simulate an injection attempt that tries to break out of the string
        let malicious = r#""; do shell script "rm -rf /"; ""#;
        let escaped = escape_applescript_string(malicious);
        // Verify no unescaped quotes remain — every " must be preceded by \
        let mut chars = escaped.chars().peekable();
        let mut safe = true;
        while let Some(c) = chars.next() {
            if c == '"' {
                safe = false;
                break;
            }
            if c == '\\' {
                chars.next();
            }
        }
        assert!(safe, "Unescaped quote found in: {escaped}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_escape_applescript_mixed_special_chars() {
        let input = r#"set "x" to "y\"#;
        let escaped = escape_applescript_string(input);
        assert_eq!(escaped, r#"set \"x\" to \"y\\"#);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_escape_applescript_empty_string() {
        assert_eq!(escape_applescript_string(""), "");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_escape_applescript_no_special_chars() {
        assert_eq!(escape_applescript_string("ls -la /tmp"), "ls -la /tmp");
    }
}
