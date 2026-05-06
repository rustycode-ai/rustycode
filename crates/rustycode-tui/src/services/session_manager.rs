//! Session lifecycle management for the TUI
//!
//! Extracted from the TUI god object (`app::event_loop`). This service owns
//! session recovery state and provides session data to the TUI for application.
//! It does NOT own rendering fields (messages, scroll_position, etc.) — those
//! remain on the TUI struct. The TUI calls `find_most_recent_session()` to get
//! data, then applies it to its own fields.

#![allow(dead_code)]

use crate::app::session_recovery_integration::SessionRecoveryManager;
use crate::ui::message::Message;
use anyhow::Result;

/// Result of finding a recoverable session.
///
/// Contains all data the TUI needs to restore a previous session.
/// The TUI applies these fields to its own state (messages, scroll, etc.).
pub struct RecoveredSession {
    /// Session identifier (first segment before '-' used for display)
    pub session_id: String,
    /// Restored conversation messages
    pub messages: Vec<Message>,
    /// Scroll position in the message list (line offset from top)
    pub scroll_position: usize,
    /// Number of messages in the recovered session
    pub message_count: usize,
    /// How long ago the session was saved, in minutes
    pub age_minutes: i64,
}

/// Manages session lifecycle: recovery, resumption, and history persistence.
///
/// Owns the [`SessionRecoveryManager`] (crash detection + state serialization)
/// and provides a clean data-oriented API for the TUI to consume.
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct SessionManager {
    /// Session recovery manager for crash detection and session restore.
    /// `None` when session persistence is unavailable (e.g. no sessions dir).
    session_recovery: Option<SessionRecoveryManager>,
}

impl SessionManager {
    /// Create a new `SessionManager`.
    ///
    /// Pass `None` for `recovery` when session persistence is disabled or
    /// unavailable (the TUI will show "Session persistence not available").
    pub fn new(recovery: Option<SessionRecoveryManager>) -> Self {
        Self {
            session_recovery: recovery,
        }
    }

    pub fn has_recovery(&self) -> bool {
        self.session_recovery.is_some()
    }

    /// List recoverable session IDs.
    ///
    /// Delegates to [`SessionRecoveryManager::list_recoverable_sessions`].
    /// Returns an empty vec when recovery is unavailable.
    pub fn list_recoverable_sessions(&self) -> Result<Vec<String>> {
        match self.session_recovery {
            Some(ref recovery) => recovery.list_recoverable_sessions(),
            None => Ok(Vec::new()),
        }
    }

    /// Load session state for a given session ID.
    ///
    /// Delegates to [`SessionRecoveryManager::load_state`].
    pub fn load_session_state(
        &self,
        session_id: &str,
    ) -> Result<crate::services::session_recovery::SessionState> {
        match self.session_recovery {
            Some(ref recovery) => recovery.load_state(session_id),
            None => anyhow::bail!("session recovery not available"),
        }
    }

    /// Save command history on exit.
    ///
    /// Delegates to [`crate::services::session::save_command_history`].
    /// Logs a warning on failure but does not propagate the error — history
    /// saving is best-effort and should not block shutdown.
    pub fn save_history(history: &[String]) {
        if let Err(e) = crate::services::session::save_command_history(history) {
            tracing::warn!("Failed to save command history: {}", e);
        }
    }

    /// Find and load the most recent recoverable session.
    ///
    /// Iterates through recoverable sessions and returns the first one that
    /// has at least one message. Returns `Ok(None)` when no sessions are
    /// available or all are empty.
    ///
    /// This is the data-oriented extraction of the original
    /// `resume_most_recent_session` from `event_loop.rs` (L1248–1315).
    /// Instead of writing to TUI fields directly, it returns a
    /// [`RecoveredSession`] that the caller applies.
    pub fn find_most_recent_session(&self) -> Result<Option<RecoveredSession>> {
        let sessions = self.list_recoverable_sessions()?;
        if sessions.is_empty() {
            return Ok(None);
        }

        // Try sessions in order, load the first one that works
        for session_id in &sessions {
            if let Ok(state) = self.load_session_state(session_id) {
                let msg_count = state.messages.len();
                if msg_count == 0 {
                    continue;
                }

                let age = chrono::Utc::now()
                    .signed_duration_since(state.last_saved)
                    .num_minutes();

                tracing::info!(
                    "Found recoverable session {} ({} messages)",
                    session_id,
                    msg_count
                );

                return Ok(Some(RecoveredSession {
                    session_id: session_id.clone(),
                    messages: state.messages,
                    scroll_position: state.scroll_position,
                    message_count: msg_count,
                    age_minutes: age,
                }));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_recovery_false_when_none() {
        let mgr = SessionManager::new(None);
        assert!(!mgr.has_recovery());
    }

    #[test]
    fn list_recoverable_sessions_returns_empty_when_no_recovery() {
        let mgr = SessionManager::new(None);
        let sessions = mgr.list_recoverable_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn find_most_recent_returns_none_when_no_recovery() {
        let mgr = SessionManager::new(None);
        let result = mgr.find_most_recent_session().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_session_state_errors_when_no_recovery() {
        let mgr = SessionManager::new(None);
        let result = mgr.load_session_state("test-id");
        assert!(result.is_err());
    }

    #[test]
    fn recovered_session_fields() {
        let session = RecoveredSession {
            session_id: "abc-123".to_string(),
            messages: Vec::new(),
            scroll_position: 42,
            message_count: 0,
            age_minutes: 5,
        };
        assert_eq!(session.session_id, "abc-123");
        assert_eq!(session.scroll_position, 42);
        assert_eq!(session.age_minutes, 5);
    }
}
