//! Integration of session recovery with the event loop
//!
//! This module provides hooks and integration points for:
//! - Auto-saving session state periodically
//! - Detecting crashes on startup
//! - Offering recovery prompts
//! - Graceful shutdown with state flush

use crate::session_recovery::{CrashRecovery, SessionPersistence, SessionState};
use crate::ui::message::Message;
use anyhow::Result;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Configuration for session recovery
///
/// Controls how and when session state is saved. The event loop is responsible
/// for calling `mark_dirty()` and `save_state()` as needed (e.g., after messages).
#[derive(Debug, Clone)]
pub struct SessionRecoveryConfig {
    /// Whether session recovery is enabled
    pub enabled: bool,
    /// How often to auto-save state
    pub auto_save_interval: Duration,
    /// Base directory for sessions
    pub sessions_dir: PathBuf,
}

impl Default for SessionRecoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_save_interval: Duration::from_secs(30),
            sessions_dir: Self::default_sessions_dir(),
        }
    }
}

impl SessionRecoveryConfig {
    /// Get the default sessions directory (~/.rustycode/sessions)
    pub fn default_sessions_dir() -> PathBuf {
        dirs::home_dir()
            .map(|home| home.join(".rustycode").join("sessions"))
            .unwrap_or_else(|| PathBuf::from(".rustycode/sessions"))
    }
}

/// Session recovery manager integrated with event loop
pub struct SessionRecoveryManager {
    /// Session ID (unique per session)
    session_id: String,
    /// Crash detection and recovery
    crash_recovery: CrashRecovery,
    /// State persistence
    persistence: SessionPersistence,
    /// Configuration
    config: SessionRecoveryConfig,
    /// Last auto-save time
    last_save_time: Instant,
    /// Whether we need to save (dirty flag)
    dirty: bool,
}

impl SessionRecoveryManager {
    pub fn new(config: SessionRecoveryConfig) -> Result<Self> {
        // Generate or load session ID
        let session_id = generate_session_id();

        // Initialize persistence
        let persistence = SessionPersistence::new(config.sessions_dir.clone());
        let crash_recovery = CrashRecovery::new(persistence.clone());

        Ok(Self {
            session_id,
            crash_recovery,
            persistence,
            config,
            last_save_time: Instant::now(),
            dirty: false,
        })
    }

    /// Create with a specific session ID (for testing or recovery)
    pub fn with_session_id(session_id: String, config: SessionRecoveryConfig) -> Result<Self> {
        let persistence = SessionPersistence::new(config.sessions_dir.clone());
        let crash_recovery = CrashRecovery::new(persistence.clone());

        Ok(Self {
            session_id,
            crash_recovery,
            persistence,
            config,
            last_save_time: Instant::now(),
            dirty: false,
        })
    }

    /// Get the current session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Initialize the session (create lock, check for crashes)
    pub fn init_session(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Create lock file
        self.persistence.create_lock(&self.session_id)?;

        Ok(())
    }

    /// Check if there was a crash and offer recovery
    pub fn check_crash_recovery(&self) -> Result<Option<SessionState>> {
        if !self.config.enabled {
            return Ok(None);
        }

        // Check all sessions for crashes
        let sessions = self.crash_recovery.list_recoverable_sessions()?;

        for session_id in sessions {
            if self.crash_recovery.detect_crash(&session_id)? {
                // Found a crashed session, offer recovery
                if let Ok(state) = self.crash_recovery.recover_session(&session_id) {
                    return Ok(Some(state));
                }
            }
        }

        Ok(None)
    }

    /// Load a specific session by ID
    pub fn load_state(&self, session_id: &str) -> Result<SessionState> {
        self.persistence.load_state(session_id)
    }

    /// List all recoverable session IDs
    pub fn list_recoverable_sessions(&self) -> Result<Vec<String>> {
        self.crash_recovery.list_recoverable_sessions()
    }

    /// Save session state (called periodically and on major changes)
    pub fn save_state(&mut self, state: &SessionState) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.persistence.save_state(state)?;
        self.last_save_time = Instant::now();
        self.dirty = false;

        Ok(())
    }

    /// Check if auto-save is needed based on time and dirty flag
    pub fn should_auto_save(&self) -> bool {
        if !self.config.enabled || !self.dirty {
            return false;
        }

        self.last_save_time.elapsed() >= self.config.auto_save_interval
    }

    /// Mark state as dirty (needs saving)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Graceful shutdown - flush state and cleanup
    ///
    /// Each step is fault-tolerant: failures are logged but don't prevent
    /// subsequent cleanup steps from running. The lock file is always cleaned
    /// up even if backup/save fails.
    pub fn shutdown(&mut self, state: &SessionState) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Backup before final save (non-fatal if it fails)
        if let Err(e) = self.persistence.backup_state(&self.session_id) {
            tracing::warn!("Session backup failed during shutdown: {}", e);
        }

        // Save final state (non-fatal if it fails — we already tried backup)
        if let Err(e) = self.save_state(state) {
            tracing::warn!("Session save failed during shutdown: {}", e);
        }

        // Always try to delete lock file to signal clean shutdown
        if let Err(e) = self.persistence.delete_lock(&self.session_id) {
            tracing::warn!("Lock file cleanup failed during shutdown: {}", e);
        }

        Ok(())
    }

    /// Create a new session state from current app state
    pub fn create_state(
        &self,
        messages: &[Message],
        scroll_position: usize,
        execution_trace: Option<serde_json::Value>,
    ) -> SessionState {
        let mut state = SessionState::new(self.session_id.clone());
        state.messages = messages.to_vec();
        state.scroll_position = scroll_position;
        state.execution_trace = execution_trace;
        state
    }
}

/// Generate a unique session ID
fn generate_session_id() -> String {
    use chrono::Local;

    let now = Local::now();
    let timestamp = now.format("%Y%m%d_%H%M%S").to_string();
    let random = uuid::Uuid::new_v4().to_string()[0..8].to_string();

    format!("{}-{}", timestamp, random)
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config(temp_dir: &TempDir) -> SessionRecoveryConfig {
        SessionRecoveryConfig {
            enabled: true,
            auto_save_interval: Duration::from_millis(100),
            sessions_dir: temp_dir.path().to_path_buf(),
        }
    }

    #[test]
    fn test_session_recovery_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let manager = SessionRecoveryManager::new(config).expect("Failed to create manager");
        assert!(!manager.session_id().is_empty());
    }

    #[test]
    fn test_init_session() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let manager = SessionRecoveryManager::new(config).expect("Failed to create manager");
        manager.init_session().expect("Failed to init session");

        // Verify lock was created
        assert!(manager.persistence.lock_exists(manager.session_id()));
    }

    #[test]
    fn test_save_and_load_state() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let mut manager = SessionRecoveryManager::new(config).expect("Failed to create manager");
        let session_id = manager.session_id().to_string();

        let mut state = manager.create_state(&[], 42, None);
        state.messages.push(Message::new(
            crate::ui::message::MessageRole::User,
            "Test".to_string(),
        ));

        manager.save_state(&state).expect("Failed to save");

        // Load and verify
        let loaded = manager
            .persistence
            .load_state(&session_id)
            .expect("Failed to load");

        assert_eq!(loaded.scroll_position, 42);
        assert_eq!(loaded.messages.len(), 1);
    }

    #[test]
    fn test_mark_dirty_flag() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let mut manager = SessionRecoveryManager::new(config).expect("Failed to create manager");

        assert!(!manager.dirty);
        manager.mark_dirty();
        assert!(manager.dirty);
    }

    #[test]
    fn test_should_auto_save_timing() {
        let temp_dir = TempDir::new().unwrap();
        let config = SessionRecoveryConfig {
            enabled: true,
            auto_save_interval: Duration::from_millis(50),
            sessions_dir: temp_dir.path().to_path_buf(),
        };

        let mut manager = SessionRecoveryManager::new(config).expect("Failed to create manager");

        // Not dirty, should not save
        assert!(!manager.should_auto_save());

        // Mark dirty but not enough time elapsed
        manager.mark_dirty();
        assert!(!manager.should_auto_save());

        // Wait for interval to elapse
        std::thread::sleep(Duration::from_millis(60));

        // Now should auto-save
        assert!(manager.should_auto_save());
    }

    #[test]
    fn test_shutdown() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let mut manager = SessionRecoveryManager::new(config).expect("Failed to create manager");
        manager.init_session().expect("Failed to init session");

        let session_id = manager.session_id().to_string();
        let state = manager.create_state(&[], 0, None);

        assert!(manager.persistence.lock_exists(&session_id));

        manager.shutdown(&state).expect("Failed to shutdown");

        // Lock should be deleted after shutdown
        assert!(!manager.persistence.lock_exists(&session_id));
    }

    #[test]
    fn test_check_crash_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);

        let manager = SessionRecoveryManager::new(config).expect("Failed to create manager");

        // No crash initially
        let result = manager
            .check_crash_recovery()
            .expect("Failed to check crash");
        assert!(result.is_none());
    }

    #[test]
    fn test_disabled_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config(&temp_dir);
        config.enabled = false;

        let mut manager = SessionRecoveryManager::new(config).expect("Failed to create manager");

        // All operations should succeed and be no-ops
        manager.init_session().expect("Failed to init");
        manager.mark_dirty();
        assert!(!manager.should_auto_save());

        let state = manager.create_state(&[], 0, None);
        manager.save_state(&state).expect("Failed to save");
        manager.shutdown(&state).expect("Failed to shutdown");
    }
}
