//! Bridge implementations connecting storage backends to tool traits.
//!
//! Implements `CheckpointStore` and `RewindStore` from `rustycode-tools`
//! and `rustycode-session` using `rustycode_storage::Storage` so the TUI
//! can persist checkpoints and rewind snapshots to SQLite.

use anyhow::Result;
use rustycode_session::{InteractionSnapshot, RewindStore, ToolCallRecord};
use rustycode_storage::Storage;
use rustycode_tools::workspace::checkpoint::{CheckpointStore, WorkspaceCheckpoint};
use std::sync::Arc;

/// Implements `CheckpointStore` backed by `Storage` (SQLite).
pub struct SqlCheckpointStore {
    storage: Arc<Storage>,
}

impl SqlCheckpointStore {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
}

impl CheckpointStore for SqlCheckpointStore {
    fn save_checkpoint(&self, session_id: &str, checkpoint: &WorkspaceCheckpoint) -> Result<()> {
        let rec = rustycode_storage::CheckpointRecord {
            id: checkpoint.id.0.clone(),
            session_id: session_id.to_string(),
            label: checkpoint.reason.clone(),
            commit_sha: if checkpoint.commit_hash.is_empty() {
                None
            } else {
                Some(checkpoint.commit_hash.clone())
            },
            files_json: checkpoint.files_changed.to_string(),
            created_at: checkpoint.created_at.to_rfc3339(),
        };
        self.storage.save_checkpoint(&rec)
    }

    fn list_checkpoints(&self, session_id: &str) -> Result<Vec<WorkspaceCheckpoint>> {
        let records = self.storage.list_checkpoints(session_id)?;
        let mut result = Vec::with_capacity(records.len());
        for rec in records {
            let created_at = chrono::DateTime::parse_from_rfc3339(&rec.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let files_changed = rec.files_json.parse::<usize>().unwrap_or(0);
            result.push(WorkspaceCheckpoint {
                id: rustycode_tools::workspace::checkpoint::CheckpointId(rec.id),
                commit_hash: rec.commit_sha.unwrap_or_default(),
                message: rec.label.clone(),
                created_at,
                files_changed,
                reason: rec.label,
            });
        }
        // Newest first is already guaranteed by the SQL query
        Ok(result)
    }

    fn delete_checkpoint(&self, id: &str) -> Result<()> {
        tracing::debug!(
            "checkpoint deletion requested for {} (not yet implemented in storage)",
            id
        );
        Ok(())
    }
}

/// Implements `RewindStore` backed by `Storage` (SQLite).
pub struct SqlRewindStore {
    storage: Arc<Storage>,
}

impl SqlRewindStore {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
}

impl RewindStore for SqlRewindStore {
    fn save_snapshot(&self, session_id: &str, snapshot: &InteractionSnapshot) -> Result<()> {
        let tools_json = if snapshot.tool_calls.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&snapshot.tool_calls)?)
        };
        let rec = rustycode_storage::RewindSnapshot {
            id: 0, // Auto-generated
            session_id: session_id.to_string(),
            interaction_number: snapshot.index as i64,
            role: snapshot
                .user_message
                .as_ref()
                .map(|_| "user".to_string())
                .unwrap_or_else(|| "assistant".to_string()),
            content_preview: snapshot.summary.clone(),
            tools_used_json: tools_json,
            checkpoint_id: snapshot.files_checkpoint_id.clone(),
            captured_at: snapshot.timestamp.to_rfc3339(),
        };
        self.storage.save_rewind_snapshot(&rec)?;
        Ok(())
    }

    fn list_snapshots(&self, session_id: &str) -> Result<Vec<InteractionSnapshot>> {
        let records = self.storage.list_rewind_snapshots(session_id)?;
        let mut result = Vec::with_capacity(records.len());
        for rec in records {
            let timestamp = chrono::DateTime::parse_from_rfc3339(&rec.captured_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            let tool_calls: Vec<ToolCallRecord> = rec
                .tools_used_json
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or_default();

            let (user_message, assistant_message) = if rec.role == "user" {
                (Some(rec.content_preview.clone()), None)
            } else {
                (None, Some(rec.content_preview.clone()))
            };

            result.push(InteractionSnapshot {
                id: rustycode_session::InteractionId(format!("int_db_{}", rec.id)),
                index: rec.interaction_number.max(0) as usize,
                timestamp,
                user_message,
                assistant_message,
                tool_calls,
                files_hash: None,
                files_checkpoint_id: rec.checkpoint_id,
                conversation_messages: vec![],
                summary: rec.content_preview,
            });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_session::InteractionId;
    use rustycode_tools::workspace::checkpoint::CheckpointId;

    fn test_storage(name: &str) -> Arc<Storage> {
        let db_path = std::env::temp_dir().join(format!(
            "rustycode-test-bridge-{}-{}.db",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_file(&db_path);
        let storage = Storage::open(&db_path).expect("failed to open test db");
        Arc::new(storage)
    }

    fn cleanup(name: &str) {
        let _ = std::fs::remove_file(std::env::temp_dir().join(format!(
            "rustycode-test-bridge-{}-{}.db",
            std::process::id(),
            name
        )));
    }

    #[test]
    fn test_sql_checkpoint_store_roundtrip() {
        let storage = test_storage("checkpoint");
        let store = SqlCheckpointStore::new(storage.clone());

        let session = rustycode_protocol::Session::builder()
            .task("test task")
            .build();
        let session_id = session.id.to_string();
        storage.insert_session(&session).expect("insert session");

        let checkpoint = WorkspaceCheckpoint {
            id: CheckpointId::new(),
            commit_hash: "abc123".to_string(),
            message: "test checkpoint".to_string(),
            created_at: chrono::Utc::now(),
            files_changed: 3,
            reason: "testing".to_string(),
        };

        store
            .save_checkpoint(&session_id, &checkpoint)
            .expect("save should succeed");

        let loaded = store
            .list_checkpoints(&session_id)
            .expect("list should succeed");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].commit_hash, "abc123");
        assert_eq!(loaded[0].files_changed, 3);
        cleanup("checkpoint");
    }

    #[test]
    fn test_sql_rewind_store_roundtrip() {
        let storage = test_storage("rewind");
        let store = SqlRewindStore::new(storage.clone());

        let session = rustycode_protocol::Session::builder()
            .task("test task")
            .build();
        let session_id = session.id.to_string();
        storage.insert_session(&session).expect("insert session");

        let snapshot = InteractionSnapshot {
            id: InteractionId::new(),
            index: 0,
            timestamp: chrono::Utc::now(),
            user_message: Some("Fix the bug".to_string()),
            assistant_message: None,
            tool_calls: vec![],
            files_hash: None,
            files_checkpoint_id: None,
            conversation_messages: vec![],
            summary: "Fix the bug".to_string(),
        };

        store
            .save_snapshot(&session_id, &snapshot)
            .expect("save should succeed");

        let loaded = store
            .list_snapshots(&session_id)
            .expect("list should succeed");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].summary, "Fix the bug");
        cleanup("rewind");
    }
}
