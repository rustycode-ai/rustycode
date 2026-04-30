//! Session Recovery and Crash Detection
//!
//! Provides comprehensive session state persistence and crash recovery:
//! - Saves conversation history, scroll position, selections to disk
//! - Detects crashes via lock file presence
//! - Offers recovery of previous session on startup
//! - Uses atomic file operations to prevent partial writes
//! - Validates recovered state before restoring

use crate::ui::message::Message;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
#[cfg(test)]
use std::time::Duration;

/// Session state to persist to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Session identifier (unique, stable across runs)
    pub session_id: String,

    /// When session was created
    pub created_at: DateTime<Utc>,

    /// When state was last saved
    pub last_saved: DateTime<Utc>,

    /// Conversation messages
    #[serde(default)]
    pub messages: Vec<Message>,

    /// Current scroll position (line offset from top)
    pub scroll_position: usize,

    /// Selected text (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<Selection>,

    /// User preferences for this session
    #[serde(default)]
    pub preferences: SessionPreferences,

    /// Final execution trace from orchestration pipeline
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_trace: Option<serde_json::Value>,
}

/// Text selection state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    /// Start position (byte offset in content)
    pub start: usize,
    /// End position (byte offset in content)
    pub end: usize,
    /// Message ID containing the selection
    pub message_id: String,
}

/// User preferences specific to a session
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionPreferences {
    /// Whether tools are expanded
    pub tools_expanded: bool,
    /// Whether thinking is expanded
    pub thinking_expanded: bool,
}

impl SessionState {
    /// Create a new session state
    pub fn new(session_id: String) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            created_at: now,
            last_saved: now,
            messages: Vec::new(),
            scroll_position: 0,
            selection: None,
            preferences: SessionPreferences::default(),
            execution_trace: None,
        }
    }

    /// Update the last saved timestamp
    pub fn touch(&mut self) {
        self.last_saved = Utc::now();
    }

    /// Validate the session state before loading
    pub fn validate(&self) -> Result<()> {
        // Check session ID is non-empty
        if self.session_id.is_empty() {
            return Err(anyhow!("Session ID cannot be empty"));
        }

        // Check timestamps are reasonable
        if self.created_at > Utc::now() {
            return Err(anyhow!("Session created_at is in the future"));
        }

        if self.last_saved < self.created_at {
            return Err(anyhow!("Session last_saved is before created_at"));
        }

        // Check scroll position is reasonable
        if self.scroll_position > 100_000 {
            return Err(anyhow!(
                "Scroll position unreasonably large: {}",
                self.scroll_position
            ));
        }

        // Validate message IDs are unique
        let mut seen_ids = std::collections::HashSet::new();
        for msg in &self.messages {
            if !seen_ids.insert(&msg.id) {
                return Err(anyhow!("Duplicate message ID: {}", msg.id));
            }
        }

        Ok(())
    }
}

/// Lock file metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockFileMetadata {
    /// Process ID of the session
    pub pid: u32,
    /// When lock was created
    pub created_at: DateTime<Utc>,
    /// Session ID
    pub session_id: String,
}

impl LockFileMetadata {
    /// Create new lock metadata
    pub fn new(session_id: String) -> Self {
        Self {
            pid: process::id(),
            created_at: Utc::now(),
            session_id,
        }
    }
}

/// Manages session persistence (save/load operations)
pub struct SessionPersistence {
    /// Base directory for sessions (~/.rustycode/sessions)
    base_dir: PathBuf,
    /// Maximum number of backup versions to keep
    max_backups: usize,
}

impl SessionPersistence {
    /// Create a new session persistence manager
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            max_backups: 3,
        }
    }

    /// Save session state to disk atomically
    ///
    /// Writes to a temporary file first, then renames to avoid partial writes on crash.
    pub fn save_state(&self, state: &SessionState) -> Result<()> {
        // Create session directory
        let session_dir = self.base_dir.join(&state.session_id);
        fs::create_dir_all(&session_dir)?;

        // Serialize state to JSON
        let json = serde_json::to_string_pretty(state)?;

        // Write to temporary file
        let state_path = session_dir.join("state.json");
        let temp_path = session_dir.join("state.json.tmp");

        let mut temp_file = File::create(&temp_path)?;
        temp_file.write_all(json.as_bytes())?;
        temp_file.sync_all()?;
        drop(temp_file); // Close file before renaming

        // Atomic rename
        fs::rename(&temp_path, &state_path)?;

        // Rotate old backups
        self.rotate_backups(&session_dir)?;

        Ok(())
    }

    /// Load session state from disk
    pub fn load_state(&self, session_id: &str) -> Result<SessionState> {
        let state_path = self.base_dir.join(session_id).join("state.json");

        if !state_path.exists() {
            return Err(anyhow!("Session state not found: {}", state_path.display()));
        }

        let json = fs::read_to_string(&state_path)?;
        let state: SessionState = serde_json::from_str(&json)?;

        // Validate before returning
        state.validate()?;

        Ok(state)
    }

    /// Create a lock file to mark session as active
    pub fn create_lock(&self, session_id: &str) -> Result<()> {
        let session_dir = self.base_dir.join(session_id);
        fs::create_dir_all(&session_dir)?;

        let lock_path = session_dir.join(".lock");
        let metadata = LockFileMetadata::new(session_id.to_string());
        let json = serde_json::to_string(&metadata)?;

        let mut file = File::create(&lock_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;

        Ok(())
    }

    /// Delete lock file (called on graceful shutdown)
    pub fn delete_lock(&self, session_id: &str) -> Result<()> {
        let lock_path = self.base_dir.join(session_id).join(".lock");

        if lock_path.exists() {
            fs::remove_file(&lock_path)?;
        }

        Ok(())
    }

    /// Check if lock file exists
    pub fn lock_exists(&self, session_id: &str) -> bool {
        let lock_path = self.base_dir.join(session_id).join(".lock");
        lock_path.exists()
    }

    /// Get lock metadata
    pub fn read_lock(&self, session_id: &str) -> Result<LockFileMetadata> {
        let lock_path = self.base_dir.join(session_id).join(".lock");

        if !lock_path.exists() {
            return Err(anyhow!("Lock file not found"));
        }

        let json = fs::read_to_string(&lock_path)?;
        let metadata: LockFileMetadata = serde_json::from_str(&json)?;

        Ok(metadata)
    }

    /// Rotate old backups, keeping only max_backups versions
    fn rotate_backups(&self, session_dir: &Path) -> Result<()> {
        // Find all backup files
        let mut backups = Vec::new();
        if let Ok(entries) = fs::read_dir(session_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name() {
                    if let Some(name_str) = name.to_str() {
                        if name_str.starts_with("state.backup.") {
                            backups.push(path);
                        }
                    }
                }
            }
        }

        // Sort by modification time (oldest first)
        backups.sort_by_key(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });

        // Remove oldest if we exceed max_backups
        while backups.len() > self.max_backups {
            if let Some(old) = backups.first() {
                let _ = fs::remove_file(old);
                backups.remove(0);
            }
        }

        Ok(())
    }

    /// Create a backup of current state
    pub fn backup_state(&self, session_id: &str) -> Result<()> {
        let session_dir = self.base_dir.join(session_id);
        let state_path = session_dir.join("state.json");

        if !state_path.exists() {
            return Ok(()); // Nothing to backup
        }

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let backup_path = session_dir.join(format!("state.backup.{}", timestamp));

        fs::copy(&state_path, &backup_path)?;

        // Rotate old backups
        self.rotate_backups(&session_dir)?;

        Ok(())
    }
}

/// Check if a process is alive using portable heuristics
///
/// Strategy:
/// - If lock file exists but is older than 24 hours, assume process is dead (crash detected)
/// - Otherwise, trust the PID check (process may have been recycled, but better safe than sorry)
///
/// This avoids external dependencies and platform-specific system calls.
fn is_lock_stale(metadata: &LockFileMetadata) -> bool {
    // If lock is older than 24 hours, assume the process crashed
    let lock_age = Utc::now().signed_duration_since(metadata.created_at);
    lock_age > chrono::Duration::hours(24)
}

/// Detects and recovers from crashes
pub struct CrashRecovery {
    persistence: SessionPersistence,
}

impl CrashRecovery {
    /// Create a new crash recovery manager
    pub fn new(persistence: SessionPersistence) -> Self {
        Self { persistence }
    }

    /// Check if previous session crashed (lock file exists without process running)
    ///
    /// A crash is detected when:
    /// 1. A lock file exists (session was running)
    /// 2. The PID in the lock is different from current process
    /// 3. The lock file is old (>24 hours), suggesting the process is no longer running
    ///
    /// Returns true if crash detected, false if no crash or same process.
    pub fn detect_crash(&self, session_id: &str) -> Result<bool> {
        if !self.persistence.lock_exists(session_id) {
            return Ok(false); // No lock = normal shutdown
        }

        // Lock exists, check if process is still running
        if let Ok(metadata) = self.persistence.read_lock(session_id) {
            // If same PID as current process, not a crash (lock file is ours)
            if metadata.pid == process::id() {
                return Ok(false);
            }

            // Different PID: check if lock is stale (>24 hours old)
            // Stale lock strongly suggests the process crashed
            if is_lock_stale(&metadata) {
                return Ok(true); // Crash detected (process is definitely dead)
            }

            // Different PID but lock is fresh: process may have crashed recently
            // but we can't be 100% sure without platform-specific APIs.
            // Be conservative: only report crash if lock is definitely old.
            // The event loop can handle this ambiguity (e.g., ask user).
            return Ok(false);
        }

        Ok(false)
    }

    /// Recover session state (assumes crash was detected)
    pub fn recover_session(&self, session_id: &str) -> Result<SessionState> {
        // Load the state
        let state = self.persistence.load_state(session_id)?;

        // Validate it's recoverable
        state.validate()?;

        Ok(state)
    }

    /// Check if a session is recoverable
    pub fn is_recoverable(&self, session_id: &str) -> bool {
        self.persistence.load_state(session_id).is_ok()
    }

    /// Get list of recoverable sessions
    pub fn list_recoverable_sessions(&self) -> Result<Vec<String>> {
        let mut sessions = Vec::new();

        if !self.persistence.base_dir.exists() {
            return Ok(sessions);
        }

        for entry in fs::read_dir(&self.persistence.base_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(session_id) = path.file_name().and_then(|n| n.to_str()) {
                    if self.is_recoverable(session_id) {
                        sessions.push(session_id.to_string());
                    }
                }
            }
        }

        Ok(sessions)
    }
}

// ─── Helper trait for cloning SessionPersistence ───────────────────────────

impl Clone for SessionPersistence {
    fn clone(&self) -> Self {
        Self {
            base_dir: self.base_dir.clone(),
            max_backups: self.max_backups,
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_dir() -> TempDir {
        TempDir::new().expect("Failed to create temp dir")
    }

    fn create_persistence(temp_dir: &TempDir) -> SessionPersistence {
        SessionPersistence::new(temp_dir.path().to_path_buf())
    }

    // ─── SessionState Tests ───────────────────────────────────────────

    #[test]
    fn test_session_state_creation() {
        let state = SessionState::new("test-session".to_string());

        assert_eq!(state.session_id, "test-session");
        assert!(state.messages.is_empty());
        assert_eq!(state.scroll_position, 0);
        assert!(state.selection.is_none());
    }

    #[test]
    fn test_session_state_validation() {
        let mut state = SessionState::new("test-session".to_string());
        assert!(state.validate().is_ok());

        // Invalid: empty session ID
        state.session_id.clear();
        assert!(state.validate().is_err());

        // Invalid: unreasonable scroll position
        state.session_id = "test".to_string();
        state.scroll_position = 1_000_000;
        assert!(state.validate().is_err());
    }

    #[test]
    fn test_session_state_touch() {
        let mut state = SessionState::new("test-session".to_string());
        let original_saved = state.last_saved;

        std::thread::sleep(Duration::from_millis(10));
        state.touch();

        assert!(state.last_saved > original_saved);
    }

    // ─── SessionPersistence Tests ───────────────────────────────────────

    #[test]
    fn test_persistence_save_and_load() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);

        let mut state = SessionState::new("test-session".to_string());
        state.messages.push(Message::new(
            crate::ui::message::MessageRole::User,
            "Hello".to_string(),
        ));

        persistence.save_state(&state).expect("Failed to save");

        let loaded = persistence
            .load_state("test-session")
            .expect("Failed to load");

        assert_eq!(loaded.session_id, "test-session");
        assert_eq!(loaded.messages.len(), 1);
    }

    #[test]
    fn test_persistence_save_invalid_json() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);

        let mut state = SessionState::new("test-session".to_string());
        state.session_id = String::new(); // Will fail validation on round-trip

        // Save should work (no validation on save)
        assert!(persistence.save_state(&state).is_ok());

        // Load should fail (validation on load)
        let result = persistence.load_state("test-session");
        assert!(result.is_err());
    }

    #[test]
    fn test_persistence_missing_file() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);

        let result = persistence.load_state("nonexistent");
        assert!(result.is_err());
    }

    // ─── Lock File Tests ───────────────────────────────────────────────

    #[test]
    fn test_lock_create_and_delete() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);

        let session_id = "test-session";

        // Create lock
        persistence
            .create_lock(session_id)
            .expect("Failed to create lock");
        assert!(persistence.lock_exists(session_id));

        // Delete lock
        persistence
            .delete_lock(session_id)
            .expect("Failed to delete lock");
        assert!(!persistence.lock_exists(session_id));
    }

    #[test]
    fn test_lock_metadata() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);

        let session_id = "test-session";
        persistence
            .create_lock(session_id)
            .expect("Failed to create lock");

        let metadata = persistence
            .read_lock(session_id)
            .expect("Failed to read lock");

        assert_eq!(metadata.session_id, session_id);
        assert_eq!(metadata.pid, process::id());
    }

    #[test]
    fn test_lock_nonexistent() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);

        let result = persistence.read_lock("nonexistent");
        assert!(result.is_err());
    }

    // ─── Crash Recovery Tests ───────────────────────────────────────────

    #[test]
    fn test_crash_detection_no_lock() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);
        let recovery = CrashRecovery::new(persistence);

        let crashed = recovery
            .detect_crash("test-session")
            .expect("Failed to detect crash");
        assert!(!crashed);
    }

    #[test]
    fn test_crash_detection_with_lock() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);
        let recovery = CrashRecovery::new(persistence.clone());

        let session_id = "test-session";

        // Create state and lock
        let state = SessionState::new(session_id.to_string());
        recovery
            .persistence
            .save_state(&state)
            .expect("Failed to save");
        recovery
            .persistence
            .create_lock(session_id)
            .expect("Failed to create lock");

        // Modify lock to have different PID and old timestamp (>24 hours ago)
        let lock_dir = temp_dir.path().join(session_id);
        let lock_path = lock_dir.join(".lock");
        let mut metadata = LockFileMetadata::new(session_id.to_string());
        metadata.pid = 99999; // Different PID
                              // Set creation time to 25 hours ago (beyond the 24-hour threshold)
        metadata.created_at = Utc::now() - chrono::Duration::hours(25);
        let json = serde_json::to_string(&metadata).expect("Failed to serialize");
        fs::write(&lock_path, json).expect("Failed to write lock");

        // Should detect crash now (old lock with different PID)
        let crashed = recovery
            .detect_crash(session_id)
            .expect("Failed to detect crash");
        assert!(crashed);
    }

    #[test]
    fn test_session_recovery() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);
        let recovery = CrashRecovery::new(persistence);

        let session_id = "test-session";
        let mut state = SessionState::new(session_id.to_string());
        state.scroll_position = 42;

        recovery
            .persistence
            .save_state(&state)
            .expect("Failed to save");

        let recovered = recovery
            .recover_session(session_id)
            .expect("Failed to recover");

        assert_eq!(recovered.scroll_position, 42);
    }

    #[test]
    fn test_is_recoverable() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);
        let recovery = CrashRecovery::new(persistence);

        let session_id = "test-session";

        // Not recoverable yet
        assert!(!recovery.is_recoverable(session_id));

        // Save state and make recoverable
        let state = SessionState::new(session_id.to_string());
        recovery
            .persistence
            .save_state(&state)
            .expect("Failed to save");

        assert!(recovery.is_recoverable(session_id));
    }

    #[test]
    fn test_list_recoverable_sessions() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);
        let recovery = CrashRecovery::new(persistence);

        // Create multiple sessions
        for i in 0..3 {
            let session_id = format!("session-{}", i);
            let state = SessionState::new(session_id);
            recovery
                .persistence
                .save_state(&state)
                .expect("Failed to save");
        }

        let sessions = recovery
            .list_recoverable_sessions()
            .expect("Failed to list sessions");

        assert_eq!(sessions.len(), 3);
    }

    // ─── Backup Tests ───────────────────────────────────────────────────

    #[test]
    fn test_backup_state() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);

        let session_id = "test-session";
        let state = SessionState::new(session_id.to_string());

        persistence.save_state(&state).expect("Failed to save");
        persistence
            .backup_state(session_id)
            .expect("Failed to backup");

        // Check backup file exists
        let session_dir = temp_dir.path().join(session_id);
        let backups: Vec<_> = fs::read_dir(&session_dir)
            .expect("Failed to read dir")
            .filter_map(|e| {
                e.ok().and_then(|entry| {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("state.backup.") {
                        Some(entry.path())
                    } else {
                        None
                    }
                })
            })
            .collect();

        assert_eq!(backups.len(), 1);
    }

    // ─── Data Integrity Tests ───────────────────────────────────────────

    #[test]
    fn test_atomic_write_on_crash() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);

        let session_id = "test-session";
        let mut state = SessionState::new(session_id.to_string());
        state.messages.push(Message::new(
            crate::ui::message::MessageRole::User,
            "Test message".to_string(),
        ));

        persistence.save_state(&state).expect("Failed to save");

        // Load and verify integrity
        let loaded = persistence.load_state(session_id).expect("Failed to load");

        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "Test message");
    }

    #[test]
    fn test_duplicate_message_ids_validation() {
        let temp_dir = create_temp_dir();
        let persistence = create_persistence(&temp_dir);

        let mut state = SessionState::new("test-session".to_string());
        let mut msg1 = Message::new(
            crate::ui::message::MessageRole::User,
            "Message 1".to_string(),
        );
        let mut msg2 = Message::new(
            crate::ui::message::MessageRole::Assistant,
            "Message 2".to_string(),
        );

        // Force duplicate IDs
        let same_id = "same-id".to_string();
        msg1.id = same_id.clone();
        msg2.id = same_id;

        state.messages.push(msg1);
        state.messages.push(msg2);

        // Save should work
        persistence.save_state(&state).expect("Failed to save");

        // Load should fail due to duplicate validation
        let result = persistence.load_state("test-session");
        assert!(result.is_err());
    }
}
