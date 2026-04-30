// Copyright 2025 The RustyCode Authors. All rights reserved.

//! Filesystem-based checkpoint persistence for session snapshots.
//!
//! Each [`CheckpointSnapshot`] is serialized to a standalone JSON file under a
//! configurable base directory.  The file name is `{id}.json`.
//!
//! # Example
//!
//! ```rust,no_run
//! use rustycode_storage::checkpoint_store::{CheckpointStore, CheckpointSnapshot, ExecutionPhase};
//! use std::path::PathBuf;
//!
//! # fn main() -> anyhow::Result<()> {
//! let store = CheckpointStore::new(PathBuf::from("/tmp/rustycode-checkpoints"));
//!
//! let snapshot = CheckpointSnapshot::generate("sess-42", ExecutionPhase::Plan);
//!
//! store.save(&snapshot)?;
//! let loaded = store.load(&snapshot.id)?;
//! assert_eq!(loaded.id, snapshot.id);
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Execution phase at the time of checkpoint creation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum ExecutionPhase {
    Explore,
    Plan,
    Act,
}

/// Snapshot of session state captured at a specific execution phase.
///
/// Mirrors [`rustycode_core::checkpoint::CheckpointSnapshot`] to avoid a
/// cyclic dependency between `rustycode-storage` and `rustycode-core`.
/// The two types are kept wire-compatible (same serde representation).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointSnapshot {
    /// Unique checkpoint identifier.
    pub id: String,
    /// Session this checkpoint belongs to.
    pub session_id: String,
    /// Execution phase at checkpoint time.
    pub phase: ExecutionPhase,
    /// When this checkpoint was created.
    pub timestamp: SystemTime,
    /// SHA-256 hash of context for integrity validation.
    pub context_hash: String,
    /// Serialized memory references stored at checkpoint time.
    pub memory_state: Vec<String>,
    /// Effects recorded but not yet executed at checkpoint time.
    pub pending_effects: Vec<String>,
    /// Arbitrary metadata key-value pairs.
    pub metadata: HashMap<String, String>,
}

impl CheckpointSnapshot {
    /// Create a new checkpoint with a generated UUID.
    pub fn generate(session_id: &str, phase: ExecutionPhase) -> Self {
        let id = Uuid::new_v4().to_string();
        let memory_state = vec![format!("default_memory_for_{id}")];
        Self {
            id,
            session_id: session_id.to_string(),
            phase,
            timestamp: SystemTime::now(),
            context_hash: String::new(),
            memory_state,
            pending_effects: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Filesystem store for [`CheckpointSnapshot`] instances.
///
/// Checkpoints are saved as individual JSON files under a configurable base
/// directory:
///
/// ```text
/// {base_path}/
/// ├── <uuid-1>.json
/// ├── <uuid-2>.json
/// └── ...
/// ```
///
/// The directory is created lazily on first write.
pub struct CheckpointStore {
    base_path: PathBuf,
}

impl CheckpointStore {
    /// Create a new store rooted at `base_path`.
    ///
    /// The directory is created on first write if it does not already exist.
    pub const fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Persist a checkpoint to `{base_path}/{id}.json`.
    ///
    /// If a checkpoint with the same id already exists it is overwritten.
    pub fn save(&self, checkpoint: &CheckpointSnapshot) -> Result<()> {
        std::fs::create_dir_all(&self.base_path).with_context(|| {
            format!(
                "failed to create checkpoint dir {}",
                self.base_path.display()
            )
        })?;

        let path = self.checkpoint_path(&checkpoint.id);
        let json = serde_json::to_string_pretty(checkpoint)
            .with_context(|| format!("failed to serialize checkpoint {}", checkpoint.id))?;
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write checkpoint to {}", path.display()))?;
        Ok(())
    }

    /// Load a checkpoint by id.
    pub fn load(&self, id: &str) -> Result<CheckpointSnapshot> {
        let path = self.checkpoint_path(id);
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read checkpoint {}", id))?;
        serde_json::from_str(&data)
            .with_context(|| format!("failed to deserialize checkpoint {}", id))
    }

    /// Check whether a checkpoint with the given id exists on disk.
    pub fn exists(&self, id: &str) -> bool {
        self.checkpoint_path(id).is_file()
    }

    /// List all checkpoints belonging to a given session.
    ///
    /// Reads every `.json` file in `base_path`, filters by `session_id`,
    /// and returns them sorted by timestamp (oldest first).
    pub fn list_for_session(&self, session_id: &str) -> Result<Vec<CheckpointSnapshot>> {
        let mut checkpoints = Vec::new();
        if !self.base_path.is_dir() {
            return Ok(checkpoints);
        }

        for entry in std::fs::read_dir(&self.base_path)
            .with_context(|| format!("failed to read dir {}", self.base_path.display()))?
        {
            let entry = entry.context("failed to read dir entry")?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(data) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(cp): Result<CheckpointSnapshot, _> = serde_json::from_str(&data) else {
                continue;
            };
            if cp.session_id == session_id {
                checkpoints.push(cp);
            }
        }

        checkpoints.sort_by_key(|cp| cp.timestamp);
        Ok(checkpoints)
    }

    /// Return the most recent checkpoint for a session, or `None` if no
    /// checkpoints exist.
    pub fn get_latest(&self, session_id: &str) -> Result<Option<CheckpointSnapshot>> {
        let mut all = self.list_for_session(session_id)?;
        Ok(all.pop())
    }

    /// Delete a checkpoint file by id.
    ///
    /// Deleting a non-existent checkpoint is not an error.
    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.checkpoint_path(id);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to delete checkpoint {}", path.display()))?;
        }
        Ok(())
    }

    /// Delete all checkpoints older than `cutoff`.
    ///
    /// Returns the number of checkpoints removed.
    pub fn prune_older_than(&self, cutoff: SystemTime) -> Result<usize> {
        let mut removed = 0;
        if !self.base_path.is_dir() {
            return Ok(removed);
        }

        for entry in std::fs::read_dir(&self.base_path)
            .with_context(|| format!("failed to read dir {}", self.base_path.display()))?
        {
            let entry = entry.context("failed to read dir entry")?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(data) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(cp): Result<CheckpointSnapshot, _> = serde_json::from_str(&data) else {
                continue;
            };
            if cp.timestamp < cutoff {
                std::fs::remove_file(&path).ok();
                removed += 1;
            }
        }

        Ok(removed)
    }

    fn checkpoint_path(&self, id: &str) -> PathBuf {
        self.base_path.join(format!("{id}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, CheckpointStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = CheckpointStore::new(dir.path().to_path_buf());
        (dir, store)
    }

    #[test]
    fn save_and_load_roundtrip() {
        let (_dir, store) = temp_store();
        let cp = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Plan);
        store.save(&cp).expect("save");
        let loaded = store.load(&cp.id).expect("load");
        assert_eq!(loaded.id, cp.id);
        assert_eq!(loaded.session_id, cp.session_id);
        assert_eq!(loaded.phase, cp.phase);
        assert_eq!(loaded.context_hash, cp.context_hash);
    }

    #[test]
    fn exists_returns_true_after_save() {
        let (_dir, store) = temp_store();
        let cp = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Act);
        assert!(!store.exists(&cp.id));
        store.save(&cp).expect("save");
        assert!(store.exists(&cp.id));
    }

    #[test]
    fn exists_returns_false_for_nonexistent() {
        let (_dir, store) = temp_store();
        assert!(!store.exists("no-such-id"));
    }

    #[test]
    fn list_for_session_returns_all_for_session() {
        let (_dir, store) = temp_store();
        let cp1 = CheckpointSnapshot::generate("sess-A", ExecutionPhase::Explore);
        let cp2 = CheckpointSnapshot::generate("sess-A", ExecutionPhase::Plan);
        let cp3 = CheckpointSnapshot::generate("sess-B", ExecutionPhase::Act);
        store.save(&cp1).expect("save");
        store.save(&cp2).expect("save");
        store.save(&cp3).expect("save");

        let list = store.list_for_session("sess-A").expect("list");
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|cp| cp.session_id == "sess-A"));
    }

    #[test]
    fn list_for_session_returns_empty_for_nonexistent() {
        let (_dir, store) = temp_store();
        let list = store.list_for_session("no-session").expect("list");
        assert!(list.is_empty());
    }

    #[test]
    fn get_latest_returns_newest() {
        let (_dir, store) = temp_store();
        let cp_old = CheckpointSnapshot::generate("sess-X", ExecutionPhase::Explore);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let cp_new = CheckpointSnapshot::generate("sess-X", ExecutionPhase::Act);
        store.save(&cp_old).expect("save");
        store.save(&cp_new).expect("save");

        let latest = store.get_latest("sess-X").expect("latest").expect("some");
        assert_eq!(latest.id, cp_new.id);
    }

    #[test]
    fn delete_removes_checkpoint() {
        let (_dir, store) = temp_store();
        let cp = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Plan);
        store.save(&cp).expect("save");
        assert!(store.exists(&cp.id));

        store.delete(&cp.id).expect("delete");
        assert!(!store.exists(&cp.id));
    }

    #[test]
    fn prune_older_than_removes_only_old() {
        let (_dir, store) = temp_store();

        let mut cp_old = CheckpointSnapshot::generate("sess-old", ExecutionPhase::Explore);
        cp_old.timestamp = SystemTime::UNIX_EPOCH;

        let cp_new = CheckpointSnapshot::generate("sess-new", ExecutionPhase::Act);

        store.save(&cp_old).expect("save old");
        store.save(&cp_new).expect("save new");

        let cutoff = SystemTime::now() - std::time::Duration::from_mins(1);
        let removed = store.prune_older_than(cutoff).expect("prune");
        assert_eq!(removed, 1);
        assert!(!store.exists(&cp_old.id));
        assert!(store.exists(&cp_new.id));
    }
}
