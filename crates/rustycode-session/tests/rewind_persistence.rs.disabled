//! Tests for rewind snapshot persistence to database.
//!
//! Verifies that:
//! - Snapshots are persisted via RewindStore trait
//! - RewindState loads existing snapshots on creation
//! - Rewind/fast-forward work with DB-loaded snapshots
//! - Checkpoint references are preserved

use rustycode_session::{
    create_snapshot, create_snapshot_with_checkpoint, InteractionId, InteractionSnapshot,
    RewindMode, RewindState, RewindStore, ToolCallRecord,
};
use rustycode_storage::Storage;
use std::sync::Arc;

/// Bridge: implement RewindStore using rustycode_storage::Storage.
/// This mirrors SqlRewindStore from the TUI's storage_bridge module.
struct SqlRewindStore {
    storage: Arc<Storage>,
}

impl SqlRewindStore {
    fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
}

impl RewindStore for SqlRewindStore {
    fn save_snapshot(
        &self,
        session_id: &str,
        snapshot: &InteractionSnapshot,
    ) -> anyhow::Result<()> {
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

    fn list_snapshots(&self, session_id: &str) -> anyhow::Result<Vec<InteractionSnapshot>> {
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
                id: InteractionId(format!("int_db_{}", rec.id)),
                index: rec.interaction_number as usize,
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

fn test_db(prefix: &str) -> Arc<Storage> {
    let db_path = std::env::temp_dir().join(format!("{}-{}.db", prefix, std::process::id()));
    let _ = std::fs::remove_file(&db_path);
    let storage = Storage::open(&db_path).expect("failed to open test db");
    Arc::new(storage)
}

/// Create a session and return its valid ID string.
fn create_test_session(storage: &Storage) -> String {
    let session = rustycode_protocol::Session::builder()
        .task("test rewind")
        .build();
    let id = session.id.to_string();
    storage
        .insert_session(&session)
        .expect("failed to insert session");
    id
}

#[test]
fn test_rewind_snapshots_persist_to_database() {
    let storage = test_db("rewind-persist");
    let session_id = create_test_session(&storage);
    let store = Arc::new(SqlRewindStore::new(storage.clone()));

    let mut state = RewindState::with_store(100, store.clone(), session_id.clone());

    let snapshot = create_snapshot(
        Some("Fix the login bug".to_string()),
        Some("I'll investigate the auth module.".to_string()),
        vec![],
        None,
    );
    state.record(snapshot);

    // Verify persisted directly in storage
    let records = storage
        .list_rewind_snapshots(&session_id)
        .expect("list snapshots");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].content_preview, "Fix the login bug");
}

#[test]
fn test_load_rewind_history_from_database() {
    let storage = test_db("rewind-load");
    let session_id = create_test_session(&storage);
    let store = Arc::new(SqlRewindStore::new(storage.clone()));

    // Session 1: record snapshots
    {
        let mut state = RewindState::with_store(100, store.clone(), session_id.clone());
        state.record(create_snapshot(
            Some("First message".to_string()),
            None,
            vec![],
            None,
        ));
        state.record(create_snapshot(
            Some("Second message".to_string()),
            None,
            vec![],
            None,
        ));
        state.record(create_snapshot(
            Some("Third message".to_string()),
            None,
            vec![],
            None,
        ));
    }

    // Session 2: load from DB — should see the snapshots
    {
        let state = RewindState::with_store(100, store.clone(), session_id.clone());
        assert_eq!(state.len(), 3, "should load 3 snapshots from DB");
        assert_eq!(state.cursor_position(), 2, "cursor at last item");
    }
}

#[test]
fn test_rewind_navigation_with_persisted_snapshots() {
    let storage = test_db("rewind-nav");
    let session_id = create_test_session(&storage);
    let store = Arc::new(SqlRewindStore::new(storage.clone()));

    // Record snapshots
    let mut state = RewindState::with_store(100, store.clone(), session_id.clone());
    state.record(create_snapshot(Some("A".to_string()), None, vec![], None));
    state.record(create_snapshot(Some("B".to_string()), None, vec![], None));
    state.record(create_snapshot(Some("C".to_string()), None, vec![], None));

    // Rewind once
    let result = state
        .rewind(RewindMode::ConversationOnly)
        .expect("rewind should work");
    assert_eq!(result.new_cursor, 1);

    // Rewind again
    let result = state
        .rewind(RewindMode::ConversationOnly)
        .expect("rewind should work");
    assert_eq!(result.new_cursor, 0);

    // Fast-forward
    let result = state
        .fast_forward(RewindMode::ConversationOnly)
        .expect("ff should work");
    assert_eq!(result.new_cursor, 1);
}

#[test]
fn test_checkpoint_reference_persisted() {
    let storage = test_db("rewind-cp");
    let session_id = create_test_session(&storage);
    let store = Arc::new(SqlRewindStore::new(storage.clone()));

    // First create a checkpoint row so the FK constraint is satisfied
    let cp_id = "cp_abc123";
    let cp = rustycode_storage::CheckpointRecord {
        id: cp_id.to_string(),
        session_id: session_id.clone(),
        label: "test checkpoint".to_string(),
        commit_sha: Some("deadbeef".to_string()),
        files_json: "0".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_checkpoint(&cp).expect("save checkpoint");

    // Now save snapshot referencing that checkpoint
    let snapshot = create_snapshot_with_checkpoint(
        Some("Edit auth.rs".to_string()),
        None,
        vec![ToolCallRecord {
            tool_name: "edit_file".to_string(),
            input: serde_json::json!({"path": "auth.rs"}),
            output: Some("edited".to_string()),
            success: true,
        }],
        None,
        Some(cp_id.to_string()),
        vec![],
    );

    // Persist directly via store
    store
        .save_snapshot(&session_id, &snapshot)
        .expect("save should succeed");

    // Verify checkpoint_id persisted
    let records = storage.list_rewind_snapshots(&session_id).expect("list");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].checkpoint_id.as_deref(), Some(cp_id));

    // Verify tools_used_json persisted
    let tools: Vec<ToolCallRecord> =
        serde_json::from_str(records[0].tools_used_json.as_deref().unwrap()).unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].tool_name, "edit_file");
}
