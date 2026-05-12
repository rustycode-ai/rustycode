//! In-memory store for checkpoint snapshots, keyed by checkpoint ID.
//!
//! Provides a simple `load` / `save` API that `SessionState::restore_from_checkpoint` can use
//! without pulling in a persistent storage dependency.

use std::collections::HashMap;

use anyhow::{Context, Result};

use super::snapshot::CheckpointSnapshot;

/// In-memory checkpoint storage.
///
/// In production, this would be backed by disk or a database. For now it serves as the
/// integration surface that `SessionState` depends on, and tests can populate it directly.
#[derive(Default)]
pub struct CheckpointStore {
    checkpoints: HashMap<String, CheckpointSnapshot>,
}

impl CheckpointStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Save a checkpoint snapshot, keyed by its ID.
    pub fn save(&mut self, checkpoint: CheckpointSnapshot) {
        self.checkpoints.insert(checkpoint.id.clone(), checkpoint);
    }

    /// Load a checkpoint by ID.
    pub fn load(&self, checkpoint_id: &str) -> Result<CheckpointSnapshot> {
        self.checkpoints
            .get(checkpoint_id)
            .cloned()
            .with_context(|| format!("checkpoint not found: {checkpoint_id}"))
    }

    /// Check whether a checkpoint exists.
    pub fn contains(&self, checkpoint_id: &str) -> bool {
        self.checkpoints.contains_key(checkpoint_id)
    }

    /// Remove a checkpoint, returning it if it existed.
    pub fn remove(&mut self, checkpoint_id: &str) -> Option<CheckpointSnapshot> {
        self.checkpoints.remove(checkpoint_id)
    }

    /// Return the number of stored checkpoints.
    pub fn len(&self) -> usize {
        self.checkpoints.len()
    }

    /// Check whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.checkpoints.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::snapshot::ExecutionPhase;
    use super::*;

    #[test]
    fn save_and_load_roundtrip() {
        let mut store = CheckpointStore::new();
        let cp = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Explore);
        let id = cp.id.clone();
        store.save(cp);

        let loaded = store.load(&id).unwrap();
        assert_eq!(loaded.session_id, "sess-1");
    }

    #[test]
    fn load_missing_returns_error() {
        let store = CheckpointStore::new();
        assert!(store.load("nonexistent").is_err());
    }

    #[test]
    fn contains_checks_existence() {
        let mut store = CheckpointStore::new();
        let cp = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Act);
        let id = cp.id.clone();
        assert!(!store.contains(&id));
        store.save(cp);
        assert!(store.contains(&id));
    }

    #[test]
    fn remove_returns_checkpoint() {
        let mut store = CheckpointStore::new();
        let cp = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Plan);
        let id = cp.id.clone();
        store.save(cp);
        let removed = store.remove(&id).unwrap();
        assert_eq!(removed.session_id, "sess-1");
        assert!(store.is_empty());
    }
}
