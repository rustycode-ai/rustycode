//! Session persistence and management for Runtime.
//!
//! This module provides session save/load functionality, enabling:
//! - Session serialization to disk
//! - Session loading by ID
//! - Session forking for experimentation
//! - Session listing and deletion
//!
//! ## Usage
//!
//! ```ignore,no_run
//! use rustycode_core::session_manager::SessionManager;
//! use rustycode_protocol::Session;
//! use std::path::PathBuf;
//!
//! let storage_dir = PathBuf::from("/path/to/sessions");
//! let manager = SessionManager::new(storage_dir);
//!
//! // Save session
//! manager.save_session(&session)?;
//!
//! // Load session
//! let loaded = manager.load_session(&session_id)?;
//!
//! // Fork session
//! let new_id = manager.fork_session(&session_id)?;
//!
//! // List all sessions
//! let sessions = manager.list_sessions()?;
//! ```

use anyhow::Result;
use chrono::{DateTime, Utc};
use rustycode_protocol::{Session, SessionId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Session metadata for management and organization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Session title/summary
    pub title: String,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
    /// Project/workspace name
    pub project: Option<String>,
    /// Additional metadata
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl SessionMetadata {
    /// Create new metadata
    pub fn new(title: String) -> Self {
        Self {
            title,
            tags: Vec::new(),
            project: None,
            extra: HashMap::new(),
        }
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: String) -> Self {
        self.tags.push(tag);
        self
    }

    /// Set project
    pub fn with_project(mut self, project: String) -> Self {
        self.project = Some(project);
        self
    }
}

/// Session manager for persistence and lifecycle
pub struct SessionManager {
    storage_dir: PathBuf,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(storage_dir: PathBuf) -> Self {
        if let Err(e) = fs::create_dir_all(&storage_dir) {
            warn!("Failed to create session storage directory: {}", e);
        }

        Self { storage_dir }
    }

    /// Get file path for a session
    fn session_path(&self, session_id: &SessionId) -> PathBuf {
        self.storage_dir.join(format!("{session_id}.json"))
    }

    /// Save session to disk (atomic write via temp file + rename)
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let session_id = session.id.clone();
        let file_path = self.session_path(&session_id);

        // Convert session to JSON
        let json = serde_json::to_string_pretty(session)
            .map_err(|e| anyhow::anyhow!("Failed to serialize session {session_id}: {e}"))?;

        // Write to temp file first, then rename for atomicity
        let temp_path = file_path.with_extension("json.tmp");
        fs::write(&temp_path, &json)
            .map_err(|e| anyhow::anyhow!("Failed to write session temp file {temp_path:?}: {e}"))?;
        fs::rename(&temp_path, &file_path).map_err(|e| {
            // Clean up temp file on rename failure
            let _ = fs::remove_file(&temp_path);
            anyhow::anyhow!("Failed to rename session temp file to {file_path:?}: {e}")
        })?;

        info!("Saved session {} to {:?}", session_id, file_path);
        Ok(())
    }

    /// Load session by ID
    pub fn load_session(&self, session_id: &SessionId) -> Result<Session> {
        let file_path = self.session_path(session_id);

        // Check if file exists
        if !file_path.exists() {
            return Err(anyhow::anyhow!(
                "Session {session_id} not found at {file_path:?}"
            ));
        }

        // Read file
        let json = fs::read_to_string(&file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read session from {file_path:?}: {e}"))?;

        // Deserialize
        let session: Session = serde_json::from_str(&json)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize session {session_id}: {e}"))?;

        info!("Loaded session {} from {:?}", session_id, file_path);
        Ok(session)
    }

    /// List all sessions sorted by creation time
    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();

        // Read all JSON files in storage directory
        let entries = fs::read_dir(&self.storage_dir).map_err(|e| {
            anyhow::anyhow!(
                "Failed to read session directory {:?}: {}",
                self.storage_dir,
                e
            )
        })?;

        for entry in entries {
            let entry =
                entry.map_err(|e| anyhow::anyhow!("Failed to read directory entry: {e}"))?;
            let path = entry.path();

            // Only process JSON files
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            // Read and deserialize session
            let json = fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read session file {path:?}: {e}"))?;

            let session: Session = serde_json::from_str(&json)
                .map_err(|e| anyhow::anyhow!("Failed to deserialize session from {path:?}: {e}"))?;

            sessions.push(session);
        }

        // Sort by creation time (newest first)
        sessions.sort_by_key(|a| std::cmp::Reverse(a.created_at));

        info!(
            "Listed {} session(s) from {:?}",
            sessions.len(),
            self.storage_dir
        );
        Ok(sessions)
    }

    /// Fork an existing session
    ///
    /// Creates a new session with a copy of the original session's state.
    /// The new session has a unique ID but copies messages, mode, and task.
    pub fn fork_session(&self, session_id: &SessionId) -> Result<SessionId> {
        // Load original session
        let mut session = self.load_session(session_id)?;

        // Generate new session ID
        let new_id = SessionId::new();
        let old_id = session.id.clone();

        // Update session
        session.id = new_id.clone();
        session.created_at = Utc::now();
        session.status = rustycode_protocol::SessionStatus::Created;

        // Update title to indicate it's a fork
        let original_title = session.task.clone();
        session.task = format!("[Fork] {original_title}");

        // Save forked session
        self.save_session(&session)?;

        info!(
            "Forked session {} -> {} (from: {})",
            old_id, new_id, original_title
        );

        Ok(new_id)
    }

    /// Delete a session
    pub fn delete_session(&self, session_id: &SessionId) -> Result<()> {
        let file_path = self.session_path(session_id);

        // Check if file exists
        if !file_path.exists() {
            return Err(anyhow::anyhow!(
                "Session {session_id} not found at {file_path:?}"
            ));
        }

        // Delete file
        fs::remove_file(&file_path)
            .map_err(|e| anyhow::anyhow!("Failed to delete session {file_path:?}: {e}"))?;

        info!("Deleted session {} from {:?}", session_id, file_path);
        Ok(())
    }

    /// Clean up old sessions
    ///
    /// Removes sessions older than the specified number of days.
    /// Useful for automatic session cleanup.
    pub fn cleanup_old_sessions(&self, days_old: u64) -> Result<usize> {
        let cutoff_date = Utc::now() - chrono::Duration::days(days_old as i64);
        let mut removed_count = 0;

        let entries = fs::read_dir(&self.storage_dir)
            .map_err(|e| anyhow::anyhow!("Failed to read session directory: {e}"))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| anyhow::anyhow!("Failed to read directory entry: {e}"))?;
            let path = entry.path();

            // Only process JSON files
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            // Get file metadata
            let metadata = fs::metadata(&path)
                .map_err(|e| anyhow::anyhow!("Failed to get file metadata: {e}"))?;

            let modified = metadata
                .modified()
                .map_err(|e| anyhow::anyhow!("Failed to get modified time: {e}"))?;

            let modified_date: DateTime<Utc> = modified.into();

            // Check if file is old enough to delete
            if modified_date < cutoff_date {
                // Try to load session to get info before deleting
                let session_id_str = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Delete the file
                fs::remove_file(&path)
                    .map_err(|e| anyhow::anyhow!("Failed to delete old session: {e}"))?;

                info!(
                    "Cleaned up old session {} (modified: {:?})",
                    session_id_str, modified_date
                );
                removed_count += 1;
            }
        }

        info!(
            "Cleaned up {} old session(s) older than {} days",
            removed_count, days_old
        );
        Ok(removed_count)
    }

    /// Get storage directory path
    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    /// Get session statistics
    pub fn get_stats(&self) -> Result<SessionStats> {
        let sessions = self.list_sessions()?;

        let total_sessions = sessions.len();
        let active_sessions = sessions.iter().filter(|s| !s.status.is_terminal()).count();

        // Sessions are sorted newest-first (descending), so:
        // - first() returns newest
        // - last() returns oldest
        let newest_session = sessions.first().map(|s| s.created_at);
        let oldest_session = sessions.last().map(|s| s.created_at);

        Ok(SessionStats {
            total_sessions,
            active_sessions,
            oldest_session,
            newest_session,
        })
    }
}

/// Statistics about stored sessions
#[derive(Debug, Clone)]
pub struct SessionStats {
    /// Total number of sessions
    pub total_sessions: usize,
    /// Number of active sessions
    pub active_sessions: usize,
    /// Oldest session creation time
    pub oldest_session: Option<DateTime<Utc>>,
    /// Newest session creation time
    pub newest_session: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::{SessionMode, SessionStatus};

    fn create_test_session(task: &str) -> Session {
        Session::builder()
            .task(task.to_string())
            .with_mode(SessionMode::Planning)
            .build()
    }

    #[test]
    fn test_session_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());

        let session = create_test_session("Test task");
        let session_id = session.id.clone();

        // Save session
        assert!(manager.save_session(&session).is_ok());

        // Load session
        let loaded = manager.load_session(&session_id).unwrap();
        assert_eq!(loaded.id, session_id);
        assert_eq!(loaded.task, "Test task");
    }

    #[test]
    fn test_session_fork() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());

        let session = create_test_session("Original task");
        let original_id = session.id.clone();

        // Save original
        manager.save_session(&session).unwrap();

        // Fork session
        let new_id = manager.fork_session(&original_id).unwrap();

        // Load forked session
        let forked = manager.load_session(&new_id).unwrap();

        assert_ne!(forked.id, original_id);
        assert!(forked.task.contains("[Fork]"));
        assert_eq!(forked.status, SessionStatus::Created);

        // Original should still exist
        let original = manager.load_session(&original_id).unwrap();
        assert_eq!(original.id, original_id);
        assert_eq!(original.task, "Original task");
    }

    #[test]
    fn test_session_list() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());

        let session1 = create_test_session("Task 1");
        let session2 = create_test_session("Task 2");

        // Save sessions
        manager.save_session(&session1).unwrap();
        manager.save_session(&session2).unwrap();

        // List sessions
        let sessions = manager.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_session_delete() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());

        let session = create_test_session("Delete me");
        let session_id = session.id.clone();

        // Save session
        manager.save_session(&session).unwrap();

        // Delete session
        assert!(manager.delete_session(&session_id).is_ok());

        // Should not exist anymore
        assert!(manager.load_session(&session_id).is_err());
    }

    // --- SessionMetadata ---

    #[test]
    fn metadata_new() {
        let m = SessionMetadata::new("Test".into());
        assert_eq!(m.title, "Test");
        assert!(m.tags.is_empty());
        assert!(m.project.is_none());
        assert!(m.extra.is_empty());
    }

    #[test]
    fn metadata_with_tag() {
        let m = SessionMetadata::new("T".into()).with_tag("rust".into());
        assert_eq!(m.tags, vec!["rust"]);
    }

    #[test]
    fn metadata_with_multiple_tags() {
        let m = SessionMetadata::new("T".into())
            .with_tag("a".into())
            .with_tag("b".into());
        assert_eq!(m.tags.len(), 2);
    }

    #[test]
    fn metadata_with_project() {
        let m = SessionMetadata::new("T".into()).with_project("my-proj".into());
        assert_eq!(m.project, Some("my-proj".to_string()));
    }

    #[test]
    fn metadata_serde_roundtrip() {
        let mut m = SessionMetadata::new("Title".into());
        m.tags.push("tag1".into());
        m.project = Some("proj".into());
        m.extra.insert("key".into(), serde_json::json!("val"));
        let json = serde_json::to_string(&m).unwrap();
        let decoded: SessionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.title, "Title");
        assert_eq!(decoded.tags.len(), 1);
        assert_eq!(decoded.project, Some("proj".into()));
    }

    #[test]
    fn metadata_serde_default_tags() {
        // Tags field has #[serde(default)], so missing tags should default to empty
        let json = r#"{"title":"X"}"#;
        let decoded: SessionMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.title, "X");
        assert!(decoded.tags.is_empty());
    }

    // --- SessionManager edge cases ---

    #[test]
    fn load_nonexistent_session_errors() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let result = manager.load_session(&SessionId::new());
        assert!(result.is_err());
    }

    #[test]
    fn delete_nonexistent_session_errors() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        assert!(manager.delete_session(&SessionId::new()).is_err());
    }

    #[test]
    fn list_empty_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let sessions = manager.list_sessions().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn overwrite_session() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(temp_dir.path().to_path_buf());
        let session = create_test_session("V1");
        let id = session.id.clone();
        manager.save_session(&session).unwrap();

        let mut session2 = create_test_session("V2");
        session2.id = id.clone();
        manager.save_session(&session2).unwrap();

        let loaded = manager.load_session(&id).unwrap();
        assert_eq!(loaded.task, "V2");
    }
}
