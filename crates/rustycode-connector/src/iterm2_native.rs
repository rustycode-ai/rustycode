//! Native iTerm2 Connector using Unix Socket + Protobuf
//!
//! Connects directly to iTerm2 via its native API (Unix domain socket
//! with Protocol Buffers), bypassing the slow Python CLI and `AppleScript`
//! for most operations.
//!
//! # Architecture
//!
//! iTerm2 exposes a Unix domain socket at a path like:
//! `/Users/<user>/Library/Application Support/iTerm2/iterm2-socket`
//!
//! Messages are protobuf-encoded using the schema in iTerm2's api.proto.
//! The iterm2-client crate handles the low-level protocol details.
//!
//! # Authentication
//!
//! iTerm2 requires a cookie for API access, resolved in order:
//! 1. `ITERM2_COOKIE` and `ITERM2_KEY` environment variables
//! 2. `AppleScript` request to iTerm2 (prompts user on first use)
//!
//! # Window Creation Limitation
//!
//! The iTerm2 API does NOT support creating new windows programmatically.
//! Window creation uses `AppleScript` as a fallback, but all other operations
//! (`send_text`, `capture_output`, `split_pane`, etc.) use the fast native API.

use crate::{
    ConnectorError, ConnectorResult, PaneContent, PaneInfo, SessionInfo, SplitDirection,
    TerminalConnector, TerminalSessionId,
};
use std::process::Command;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

/// Session metadata
#[derive(Debug, Clone)]
struct NativeSession {
    id: TerminalSessionId,
    name: String,
    session_id: String, // iTerm2 session ID from API
    window_id: String,  // iTerm2 window ID
    pane_count: usize,
}

/// Native iTerm2 connector using Unix socket + Protobuf
pub struct ITerm2NativeConnector {
    /// iTerm2 application handle (lazily initialized)
    app: RwLock<Option<iterm2_client::App<tokio::net::UnixStream>>>,
    /// Tokio runtime for async operations (lazy, created on first use)
    rt: Mutex<Option<tokio::runtime::Runtime>>,
    /// Track created sessions
    sessions: Mutex<Vec<NativeSession>>,
}

impl Default for ITerm2NativeConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl ITerm2NativeConnector {
    pub const fn new() -> Self {
        Self {
            app: RwLock::new(None),
            rt: Mutex::new(None),
            sessions: Mutex::new(Vec::new()),
        }
    }

    /// Get or create the tokio runtime, storing it for reuse
    fn get_runtime(&self) -> ConnectorResult<tokio::runtime::Runtime> {
        // Check if we have a runtime stored
        let rt_guard = self
            .rt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if rt_guard.as_ref().is_some() {
            // Can't clone Runtime, so we just create a new one each time
            // The stored option is kept for potential future optimization
            drop(rt_guard);
        }

        // Create a new runtime
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                ConnectorError::SessionCreateFailed(format!("Failed to create runtime: {e}"))
            })
    }

    /// Connect to iTerm2 Unix socket API
    fn connect(&self) -> ConnectorResult<()> {
        if self
            .app
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
        {
            return Ok(()); // Already connected
        }

        let rt = self.get_runtime()?;

        let app = rt.block_on(async {
            let conn = iterm2_client::Connection::connect("rustycode")
                .await
                .map_err(|e| {
                    ConnectorError::SessionCreateFailed(format!("Failed to connect to iTerm2: {e}"))
                })?;
            Ok::<_, ConnectorError>(iterm2_client::App::new(conn))
        })?;

        *self
            .app
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(app);
        Ok(())
    }

    /// Create a new iTerm2 window using `AppleScript` (API doesn't support window creation)
    fn create_window_applescript(_name: &str) -> Result<(String, String, String), ConnectorError> {
        let script = r#"
            tell application "iTerm2"
                create window with default profile
                tell current window
                    set win_id to id
                    tell current tab
                        set tab_id to id
                        tell current session
                            set session_id to id
                            return {session_id, win_id, tab_id}
                        end tell
                    end tell
                end tell
            end tell
            "#
        .to_string();

        let output = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| {
                ConnectorError::SessionCreateFailed(format!("Failed to create window: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ConnectorError::SessionCreateFailed(format!(
                "AppleScript failed: {}",
                stderr.trim()
            )));
        }

        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let parts: Vec<&str> = result.split(',').collect();
        if parts.len() >= 3 {
            Ok((
                parts[0].trim().to_string(),
                parts[1].trim().to_string(),
                parts[2].trim().to_string(),
            ))
        } else {
            Err(ConnectorError::SessionCreateFailed(format!(
                "Unexpected AppleScript output: {result}"
            )))
        }
    }

    /// Check if iTerm2 native API is available
    pub fn check_available() -> bool {
        // Check if iTerm2 is running
        let output = Command::new("pgrep").arg("-x").arg("iTerm2").output();

        if let Ok(out) = output {
            if !out.status.success() {
                return false;
            }
        } else {
            return false;
        }

        // Check if the Unix socket exists (indicates API is enabled)
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let socket_path = format!("{home}/Library/Application Support/iTerm2/iterm2-socket");
        if !std::path::Path::new(&socket_path).exists() {
            // Socket might be at a different path with PID
            // Check for any iterm2-socket* file
            let socket_dir = format!("{home}/Library/Application Support/iTerm2");
            let dir = std::path::Path::new(&socket_dir);
            if dir.exists() {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("iterm2-socket")
                        {
                            return true;
                        }
                    }
                }
            }
            return false;
        }

        true
    }
}

impl TerminalConnector for ITerm2NativeConnector {
    fn name(&self) -> &'static str {
        "iTerm2-Native"
    }

    fn is_available(&self) -> bool {
        Self::check_available()
    }

    fn create_session(&mut self, name: &str) -> ConnectorResult<TerminalSessionId> {
        // Create window via AppleScript (API doesn't support this)
        let (session_id, window_id, _) = Self::create_window_applescript(name)?;

        let our_session_id = TerminalSessionId(format!("iterm2-native-{name}-{window_id}"));

        let session = NativeSession {
            id: our_session_id.clone(),
            name: name.to_string(),
            session_id,
            window_id,
            pane_count: 1,
        };

        self.sessions
            .lock()
            .map_err(|e| ConnectorError::Other(format!("Lock error: {e}")))?
            .push(session);

        Ok(our_session_id)
    }

    fn close_session(&mut self, session: &TerminalSessionId) -> ConnectorResult<()> {
        let session_data = {
            let sessions = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sessions
                .iter()
                .find(|s| s.id == *session)
                .cloned()
                .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))?
        };

        // Close via AppleScript (cleanest for window)
        let script = format!(
            r#"
            tell application "iTerm2"
                tell window id "{}"
                    close
                end tell
            end tell
            "#,
            session_data.window_id
        );

        let _ = Command::new("osascript").args(["-e", &script]).output();

        // Remove from tracked sessions
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sessions.retain(|s| s.id != *session);

        Ok(())
    }

    fn session_info(&self, session: &TerminalSessionId) -> ConnectorResult<SessionInfo> {
        let sessions = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(sessions
            .iter()
            .filter_map(|s| self.session_info(&s.id).ok())
            .collect())
    }

    // SAFETY: block_on runs on a single-threaded runtime; no other task can
    // contend the std::sync::RwLock while we await inside block_on.
    #[allow(clippy::await_holding_lock)]
    fn split_pane(
        &mut self,
        session: &TerminalSessionId,
        _pane_index: usize,
        direction: SplitDirection,
    ) -> ConnectorResult<usize> {
        self.connect()?;

        let session_data = {
            let sessions = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sessions
                .iter()
                .find(|s| s.id == *session)
                .cloned()
                .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))?
        };

        let direction_pb = match direction {
            SplitDirection::Horizontal => {
                iterm2_client::proto::split_pane_request::SplitDirection::Horizontal
            }
            SplitDirection::Vertical => {
                iterm2_client::proto::split_pane_request::SplitDirection::Vertical
            }
        };

        // Use raw RPC call since we need to find the session first
        let rt = self.get_runtime()?;
        let _split_result = rt.block_on(async {
            let app_guard = self
                .app
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let app = app_guard.as_ref().ok_or_else(|| {
                ConnectorError::NotAvailable("iTerm2 app handle not initialized".into())
            })?;

            // List sessions to find ours
            let result = app.list_sessions().await.map_err(|e| {
                ConnectorError::SplitFailed(format!("Failed to list sessions: {e}"))
            })?;

            // Find and split the session
            for window in &result.windows {
                for tab in &window.tabs {
                    for sess in &tab.sessions {
                        if sess.id == session_data.session_id {
                            return sess.split(direction_pb, false, None).await.map_err(|e| {
                                ConnectorError::SplitFailed(format!("Failed to split: {e}"))
                            });
                        }
                    }
                }
            }

            Err(ConnectorError::SessionNotFound(session_data.session_id))
        })?;

        // Update tracked pane count
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(s) = sessions.iter_mut().find(|s| s.id == *session) {
            s.pane_count += 1;
            Ok(s.pane_count - 1)
        } else {
            Err(ConnectorError::SessionNotFound(session.0.clone()))
        }
    }

    // SAFETY: see split_pane.
    #[allow(clippy::await_holding_lock)]
    fn send_keys(
        &mut self,
        session: &TerminalSessionId,
        _pane_index: usize,
        keys: &str,
    ) -> ConnectorResult<()> {
        self.connect()?;

        let session_data = {
            let sessions = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sessions
                .iter()
                .find(|s| s.id == *session)
                .cloned()
                .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))?
        };

        // Send text via native API
        let rt = self.get_runtime()?;
        rt.block_on(async {
            let app_guard = self
                .app
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let app = app_guard.as_ref().ok_or_else(|| {
                ConnectorError::NotAvailable("iTerm2 app handle not initialized".into())
            })?;
            let result = app.list_sessions().await.map_err(|e| {
                ConnectorError::SendKeysFailed(format!("Failed to list sessions: {e}"))
            })?;

            for window in &result.windows {
                for tab in &window.tabs {
                    for sess in &tab.sessions {
                        if sess.id == session_data.session_id {
                            return sess.send_text(keys).await.map_err(|e| {
                                ConnectorError::SendKeysFailed(format!("Failed to send text: {e}"))
                            });
                        }
                    }
                }
            }

            Err(ConnectorError::SessionNotFound(session_data.session_id))
        })?;

        Ok(())
    }

    // SAFETY: see split_pane.
    #[allow(clippy::await_holding_lock)]
    fn capture_output(
        &self,
        session: &TerminalSessionId,
        _pane_index: usize,
    ) -> ConnectorResult<PaneContent> {
        self.connect()?;

        let session_data = {
            let sessions = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sessions
                .iter()
                .find(|s| s.id == *session)
                .cloned()
                .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))
        }?;

        // Capture output via native API
        let rt = self.get_runtime()?;
        rt.block_on(async {
            let app_guard = self
                .app
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let app = app_guard.as_ref().ok_or_else(|| {
                ConnectorError::NotAvailable("iTerm2 app handle not initialized".into())
            })?;
            let result = app.list_sessions().await.map_err(|e| {
                ConnectorError::CaptureFailed(format!("Failed to list sessions: {e}"))
            })?;

            for window in &result.windows {
                for tab in &window.tabs {
                    for sess in &tab.sessions {
                        if sess.id == session_data.session_id {
                            // Get the screen buffer
                            let lines = sess.get_screen_contents().await.map_err(|e| {
                                ConnectorError::CaptureFailed(format!(
                                    "Failed to get screen contents: {e}"
                                ))
                            })?;

                            let text = lines.join("\n");
                            return Ok::<_, ConnectorError>(PaneContent::new(text));
                        }
                    }
                }
            }

            Err(ConnectorError::SessionNotFound(session_data.session_id))
        })
    }

    // SAFETY: see split_pane.
    #[allow(clippy::await_holding_lock)]
    fn set_pane_title(
        &mut self,
        session: &TerminalSessionId,
        _pane_index: usize,
        title: &str,
    ) -> ConnectorResult<()> {
        self.connect()?;

        let session_data = {
            let sessions = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sessions
                .iter()
                .find(|s| s.id == *session)
                .cloned()
                .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))?
        };

        // Set title via native API
        let rt = self.get_runtime()?;
        rt.block_on(async {
            let app_guard = self
                .app
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let app = app_guard.as_ref().ok_or_else(|| {
                ConnectorError::NotAvailable("iTerm2 app handle not initialized".into())
            })?;
            let result = app
                .list_sessions()
                .await
                .map_err(|e| ConnectorError::Other(format!("Failed to list sessions: {e}")))?;

            for window in &result.windows {
                for tab in &window.tabs {
                    for sess in &tab.sessions {
                        if sess.id == session_data.session_id {
                            let json_val = format!("\"{title}\"");
                            let _ = sess.set_variable("user.title", &json_val).await;
                            return Ok::<_, ConnectorError>(());
                        }
                    }
                }
            }

            Ok(())
        })?;

        Ok(())
    }

    fn select_pane(
        &mut self,
        session: &TerminalSessionId,
        pane_index: usize,
    ) -> ConnectorResult<()> {
        let session_data = {
            let sessions = self
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            sessions
                .iter()
                .find(|s| s.id == *session)
                .cloned()
                .ok_or_else(|| ConnectorError::SessionNotFound(session.0.clone()))?
        };

        // Focus via AppleScript (most reliable)
        let script = format!(
            r#"
            tell application "iTerm2"
                tell window id "{}"
                    select
                    tell current tab
                        select session id "{}"
                    end tell
                end tell
            end tell
            "#,
            session_data.window_id, session_data.session_id
        );

        Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| ConnectorError::Other(format!("Failed to select pane: {e}")))?;

        let _ = pane_index;
        Ok(())
    }

    fn kill_pane(
        &mut self,
        _session: &TerminalSessionId,
        _pane_index: usize,
    ) -> ConnectorResult<()> {
        Err(ConnectorError::Other(
            "kill_pane not implemented for iTerm2 native".to_string(),
        ))
    }

    fn wait_for_output(
        &self,
        session: &TerminalSessionId,
        pane_index: usize,
        pattern: &str,
        timeout_secs: Option<u64>,
    ) -> ConnectorResult<PaneContent> {
        let timeout = timeout_secs.map_or(Duration::from_secs(30), Duration::from_secs);
        let start = Instant::now();

        while start.elapsed() < timeout {
            if let Ok(content) = self.capture_output(session, pane_index) {
                if content.text.contains(pattern) {
                    return Ok(content);
                }
            } else {
                // Capture might fail, continue waiting
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
        let available = ITerm2NativeConnector::check_available();
        println!("iTerm2 native API available: {available}");
    }

    #[test]
    fn test_connector_creation() {
        let connector = ITerm2NativeConnector::new();
        assert_eq!(connector.name(), "iTerm2-Native");
    }

    #[test]
    fn test_connector_default() {
        let connector = ITerm2NativeConnector::default();
        assert_eq!(connector.name(), "iTerm2-Native");
    }

    #[test]
    fn test_is_available_matches_check() {
        let connector = ITerm2NativeConnector::new();
        assert_eq!(
            connector.is_available(),
            ITerm2NativeConnector::check_available()
        );
    }

    #[test]
    fn test_kill_pane_not_implemented() {
        let mut connector = ITerm2NativeConnector::new();
        let session = TerminalSessionId("test".into());
        let result = connector.kill_pane(&session, 0);
        assert!(result.is_err());
        match result {
            Err(ConnectorError::Other(msg)) => {
                assert!(msg.contains("not implemented"));
            }
            _ => panic!("Expected Other error"),
        }
    }

    #[test]
    fn test_session_info_not_found() {
        let connector = ITerm2NativeConnector::new();
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
        let connector = ITerm2NativeConnector::new();
        let sessions = connector.list_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_close_session_not_found() {
        let mut connector = ITerm2NativeConnector::new();
        let session = TerminalSessionId("nonexistent".into());
        let result = connector.close_session(&session);
        assert!(result.is_err());
        match result {
            Err(ConnectorError::SessionNotFound(id)) => {
                assert_eq!(id, "nonexistent");
            }
            _ => panic!("Expected SessionNotFound error"),
        }
    }

    #[test]
    fn test_split_pane_session_not_found() {
        let mut connector = ITerm2NativeConnector::new();
        let session = TerminalSessionId("nonexistent".into());
        let result = connector.split_pane(&session, 0, SplitDirection::Horizontal);
        assert!(result.is_err());
    }

    #[test]
    fn test_send_keys_session_not_found() {
        let mut connector = ITerm2NativeConnector::new();
        let session = TerminalSessionId("nonexistent".into());
        let result = connector.send_keys(&session, 0, "ls");
        assert!(result.is_err());
    }
}
