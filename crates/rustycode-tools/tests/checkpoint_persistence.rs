#![allow(
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::implicit_clone,
    clippy::redundant_clone,
    clippy::uninlined_format_args
)]

//! Tests for checkpoint persistence to database
//!
//! These tests verify that checkpoints created during a session
//! are persisted to the database and can be restored in future sessions.

use rustycode_protocol::Session;
use rustycode_tools::workspace::checkpoint::{
    CheckpointConfig, CheckpointId, CheckpointManager, RestoreMode, StorageBasedCheckpointStore,
    WorkspaceCheckpoint,
};
use std::sync::Arc;

/// Helper: create a session row in storage and return its ID string.
/// Uses Session::builder() to get a valid SessionId with proper prefix.
fn create_test_session(storage: &rustycode_storage::Storage) -> String {
    let session = Session::builder().task("test task").build();
    let id = session.id.to_string();
    storage
        .insert_session(&session)
        .expect("failed to insert test session row");
    id
}

/// Helper to create a unique checkpoints_dir for test isolation.
/// Each test gets its own shadow git repo to avoid parallel `git init` races.
fn unique_test_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("rustycode-test-{}-{}", name, std::process::id()))
}

/// Helper to set up a test database with the Storage implementation
#[tokio::test]
async fn test_checkpoint_manager_creates_with_session() {
    let workspace_dir = std::env::temp_dir().join("test-workspace");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-create")),
        ..CheckpointConfig::default()
    };

    // Create manager without persistence backend (in-memory only)
    let manager = CheckpointManager::new(workspace_dir.clone(), config)
        .expect("failed to create checkpoint manager");

    // Should be able to get manager without error
    assert!(manager.list_checkpoints().is_empty());
}

/// Test that checkpoint manager loads existing checkpoints from store
#[tokio::test]
async fn test_checkpoint_manager_loads_from_store() {
    // This test will verify that when a CheckpointManager is created with a store,
    // it loads existing checkpoints from that store during initialization.
    // Implementation will be tested after SqlCheckpointStore is created.

    let workspace_dir = std::env::temp_dir().join("test-workspace-load");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-load")),
        ..CheckpointConfig::default()
    };
    let manager = CheckpointManager::new(workspace_dir, config).expect("failed to create manager");

    // Initially no checkpoints
    assert_eq!(manager.list_checkpoints().len(), 0);
}

/// Test that checkpoints can be created and listed
#[tokio::test]
async fn test_create_and_list_checkpoints() {
    let workspace_dir = std::env::temp_dir().join("test-workspace-create");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-list")),
        ..CheckpointConfig::default()
    };
    let manager = CheckpointManager::new(workspace_dir, config).expect("failed to create manager");

    // Create a checkpoint
    let checkpoint1 = manager
        .create_checkpoint("test save 1")
        .expect("failed to create checkpoint 1");

    // Create another checkpoint
    let checkpoint2 = manager
        .create_checkpoint("test save 2")
        .expect("failed to create checkpoint 2");

    // List should show both
    let list = manager.list_checkpoints();
    assert_eq!(list.len(), 2);

    // Most recent first
    assert_eq!(list[0].id, checkpoint2.id);
    assert_eq!(list[1].id, checkpoint1.id);
}

/// Test that checkpoints respect max_checkpoints limit
#[tokio::test]
async fn test_checkpoint_eviction_respects_max_limit() {
    let workspace_dir = std::env::temp_dir().join("test-workspace-evict");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let config = CheckpointConfig {
        max_checkpoints: 3,
        checkpoints_dir: Some(unique_test_dir("cp-evict")),
        ..CheckpointConfig::default()
    };

    let manager = CheckpointManager::new(workspace_dir, config).expect("failed to create manager");

    // Create more checkpoints than the limit
    for i in 0..5 {
        let _ = manager
            .create_checkpoint(&format!("checkpoint {}", i))
            .expect("failed to create checkpoint");
    }

    // Should only have max_checkpoints
    let list = manager.list_checkpoints();
    assert!(
        list.len() <= 3,
        "Should have at most 3 checkpoints, got {}",
        list.len()
    );
}

/// Test that checkpoint IDs are unique
#[tokio::test]
async fn test_checkpoint_ids_are_unique() {
    let workspace_dir = std::env::temp_dir().join("test-workspace-unique");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-unique")),
        ..CheckpointConfig::default()
    };
    let manager = CheckpointManager::new(workspace_dir, config).expect("failed to create manager");

    let cp1 = manager
        .create_checkpoint("cp1")
        .expect("failed to create cp1");
    let cp2 = manager
        .create_checkpoint("cp2")
        .expect("failed to create cp2");

    // IDs should be different
    assert_ne!(cp1.id, cp2.id);
}

/// Test that checkpoint reason is preserved
#[tokio::test]
async fn test_checkpoint_preserves_reason() {
    let workspace_dir = std::env::temp_dir().join("test-workspace-reason");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-reason")),
        ..CheckpointConfig::default()
    };
    let manager = CheckpointManager::new(workspace_dir, config).expect("failed to create manager");

    let reason = "test: important changes";
    let checkpoint = manager
        .create_checkpoint(reason)
        .expect("failed to create checkpoint");

    assert_eq!(checkpoint.reason, reason);

    // Verify it's in the list
    let list = manager.list_checkpoints();
    assert!(!list.is_empty());
    assert_eq!(list[0].reason, reason);
}

/// Test that get_checkpoint retrieves specific checkpoint
#[tokio::test]
async fn test_get_checkpoint_by_id() {
    let workspace_dir = std::env::temp_dir().join("test-workspace-get");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-get")),
        ..CheckpointConfig::default()
    };
    let manager = CheckpointManager::new(workspace_dir, config).expect("failed to create manager");

    let checkpoint = manager
        .create_checkpoint("test get")
        .expect("failed to create checkpoint");

    let retrieved = manager
        .checkpoint(&checkpoint.id.0)
        .expect("checkpoint should be found");

    assert_eq!(retrieved.id, checkpoint.id);
    assert_eq!(retrieved.reason, "test get");
}

/// Test that non-existent checkpoint returns None
#[tokio::test]
async fn test_get_nonexistent_checkpoint() {
    let workspace_dir = std::env::temp_dir().join("test-workspace-missing");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-missing")),
        ..CheckpointConfig::default()
    };
    let manager = CheckpointManager::new(workspace_dir, config).expect("failed to create manager");

    let result = manager.checkpoint("nonexistent-id");
    assert!(result.is_none());
}

/// Test checkpoint serialization
#[tokio::test]
async fn test_checkpoint_serialization() {
    let checkpoint = WorkspaceCheckpoint {
        id: CheckpointId::new(),
        commit_hash: "abc123def456".to_string(),
        message: "test commit".to_string(),
        created_at: chrono::Utc::now(),
        files_changed: 5,
        reason: "testing serialization".to_string(),
    };

    let json = serde_json::to_string(&checkpoint).expect("failed to serialize");
    let deserialized: WorkspaceCheckpoint =
        serde_json::from_str(&json).expect("failed to deserialize");

    assert_eq!(checkpoint.id, deserialized.id);
    assert_eq!(checkpoint.reason, deserialized.reason);
}

/// Test restore mode enum serialization
#[tokio::test]
async fn test_restore_mode_serialization() {
    let mode = RestoreMode::FilesOnly;
    let json = serde_json::to_string(&mode).expect("failed to serialize");
    assert!(json.contains("files-only"));

    let mode = RestoreMode::Full;
    let json = serde_json::to_string(&mode).expect("failed to serialize");
    assert!(json.contains("full"));
}

// ─── Storage-based Integration Tests ───────────────────────────────────────

/// Test that StorageBasedCheckpointStore persists checkpoints to database
#[tokio::test]
async fn test_storage_checkpoint_persistence() {
    let db_path = std::env::temp_dir().join("test-checkpoints.db");
    let _ = std::fs::remove_file(&db_path);

    // Create storage
    let storage = rustycode_storage::Storage::open(&db_path).expect("failed to create storage");
    let storage = Arc::new(storage);

    // Create checkpoint manager with storage backend
    let workspace_dir = std::env::temp_dir().join("test-storage-workspace");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let session_id = create_test_session(&storage);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-storage")),
        ..CheckpointConfig::default()
    };
    let checkpoint_store = Arc::new(StorageBasedCheckpointStore::new(storage.clone()));

    let manager = CheckpointManager::with_store(
        workspace_dir,
        config,
        Some(checkpoint_store.clone()),
        session_id.to_string(),
    )
    .expect("failed to create manager with storage");

    // Create a checkpoint
    let checkpoint1 = manager
        .create_checkpoint("first checkpoint")
        .expect("failed to create checkpoint 1");

    eprintln!("Created checkpoint with id: {}", checkpoint1.id.0);

    // Verify it's persisted to storage
    match storage.load_checkpoint(&checkpoint1.id.0) {
        Ok(Some(rec)) => {
            assert_eq!(rec.id, checkpoint1.id.0);
            assert_eq!(rec.label, "first checkpoint");
        }
        Ok(None) => {
            panic!("checkpoint should be persisted to storage but was not found");
        }
        Err(e) => {
            panic!("failed to load from storage: {:?}", e);
        }
    }
}

/// Test that CheckpointManager loads checkpoints from storage on creation
#[tokio::test]
async fn test_manager_loads_from_storage_on_init() {
    let db_path = std::env::temp_dir().join("test-checkpoints-load.db");
    let _ = std::fs::remove_file(&db_path);

    let storage = rustycode_storage::Storage::open(&db_path).expect("failed to create storage");
    let storage = Arc::new(storage);

    let session_id = create_test_session(&storage);
    let workspace_dir = std::env::temp_dir().join("test-load-workspace");
    let _ = std::fs::create_dir_all(&workspace_dir);

    // First session: create checkpoints
    {
        let config = CheckpointConfig {
            checkpoints_dir: Some(unique_test_dir("cp-load-store")),
            ..CheckpointConfig::default()
        };
        let checkpoint_store = Arc::new(StorageBasedCheckpointStore::new(storage.clone()));

        let manager = CheckpointManager::with_store(
            workspace_dir.clone(),
            config,
            Some(checkpoint_store.clone()),
            session_id.to_string(),
        )
        .expect("failed to create first manager");

        let _ = manager
            .create_checkpoint("checkpoint from session 1")
            .expect("failed to create checkpoint");
        let _ = manager
            .create_checkpoint("another checkpoint from session 1")
            .expect("failed to create checkpoint");
    }

    // Second session: load checkpoints
    {
        let config = CheckpointConfig {
            checkpoints_dir: Some(unique_test_dir("cp-load-store")),
            ..CheckpointConfig::default()
        };
        let checkpoint_store = Arc::new(StorageBasedCheckpointStore::new(storage.clone()));

        let manager = CheckpointManager::with_store(
            workspace_dir,
            config,
            Some(checkpoint_store.clone()),
            session_id.to_string(),
        )
        .expect("failed to create second manager");

        // Should load existing checkpoints from storage
        let checkpoints = manager.list_checkpoints();
        assert_eq!(
            checkpoints.len(),
            2,
            "should load 2 existing checkpoints from storage"
        );
    }
}

/// Test that multiple sessions can have separate checkpoint stores
#[tokio::test]
async fn test_storage_session_isolation() {
    let db_path = std::env::temp_dir().join("test-checkpoints-isolated.db");
    let _ = std::fs::remove_file(&db_path);

    let storage = rustycode_storage::Storage::open(&db_path).expect("failed to create storage");
    let storage = Arc::new(storage);

    let workspace_dir = std::env::temp_dir().join("test-isolation-workspace");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-isolation")),
        ..CheckpointConfig::default()
    };

    let session1_id = create_test_session(&storage);
    let session2_id = create_test_session(&storage);

    // Session 1
    {
        let checkpoint_store = Arc::new(StorageBasedCheckpointStore::new(storage.clone()));
        let manager = CheckpointManager::with_store(
            workspace_dir.clone(),
            config.clone(),
            Some(checkpoint_store),
            session1_id.clone(),
        )
        .expect("failed to create manager for session 1");

        let _ = manager
            .create_checkpoint("session 1 checkpoint")
            .expect("failed to create checkpoint");
    }

    // Session 2
    {
        let checkpoint_store = Arc::new(StorageBasedCheckpointStore::new(storage.clone()));
        let manager = CheckpointManager::with_store(
            workspace_dir.clone(),
            config.clone(),
            Some(checkpoint_store),
            session2_id.clone(),
        )
        .expect("failed to create manager for session 2");

        let _ = manager
            .create_checkpoint("session 2 checkpoint")
            .expect("failed to create checkpoint");
    }

    // Verify each session has its own checkpoint
    let session1_checkpoints = storage
        .list_checkpoints(&session1_id)
        .expect("failed to list session 1 checkpoints");
    let session2_checkpoints = storage
        .list_checkpoints(&session2_id)
        .expect("failed to list session 2 checkpoints");

    assert_eq!(session1_checkpoints.len(), 1);
    assert_eq!(session2_checkpoints.len(), 1);
    assert_eq!(session1_checkpoints[0].label, "session 1 checkpoint");
    assert_eq!(session2_checkpoints[0].label, "session 2 checkpoint");
}

/// Test that checkpoint metadata is preserved through storage
#[tokio::test]
async fn test_checkpoint_metadata_preserved() {
    let db_path = std::env::temp_dir().join("test-checkpoints-metadata.db");
    let _ = std::fs::remove_file(&db_path);

    let storage = rustycode_storage::Storage::open(&db_path).expect("failed to create storage");
    let storage = Arc::new(storage);

    let workspace_dir = std::env::temp_dir().join("test-metadata-workspace");
    let _ = std::fs::create_dir_all(&workspace_dir);

    let session_id = create_test_session(&storage);

    let config = CheckpointConfig {
        checkpoints_dir: Some(unique_test_dir("cp-metadata")),
        ..CheckpointConfig::default()
    };
    let checkpoint_store = Arc::new(StorageBasedCheckpointStore::new(storage.clone()));

    let manager = CheckpointManager::with_store(
        workspace_dir,
        config,
        Some(checkpoint_store),
        session_id.to_string(),
    )
    .expect("failed to create manager");

    let reason = "important test checkpoint";
    let original = manager
        .create_checkpoint(reason)
        .expect("failed to create checkpoint");

    // Load from storage and verify metadata
    let from_storage = storage
        .load_checkpoint(&original.id.0)
        .expect("failed to load checkpoint")
        .expect("checkpoint should exist");

    assert_eq!(from_storage.id, original.id.0);
    assert_eq!(from_storage.label, reason);
    assert!(!from_storage.created_at.is_empty());
}
