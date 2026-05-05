//! Checkpoint snapshots for session recovery and state validation.

use std::collections::HashMap;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
/// Used for recovery: a checkpoint records enough context to rewind the session
/// to a known-good state and resume from there.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointSnapshot {
    pub id: String,
    pub session_id: String,
    pub phase: ExecutionPhase,
    pub timestamp: SystemTime,
    pub context_hash: String,
    pub memory_state: Vec<String>,
    pub pending_effects: Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl CheckpointSnapshot {
    /// Create a new checkpoint with a generated UUID.
    pub fn generate(session_id: &str, phase: ExecutionPhase) -> Self {
        let id = Uuid::new_v4().to_string();
        let timestamp = SystemTime::now();
        let memory_state = vec![format!("default_memory_for_{id}")];
        let context_hash = String::new();

        let mut snapshot = Self {
            id,
            session_id: session_id.to_string(),
            phase,
            timestamp,
            context_hash,
            memory_state,
            pending_effects: Vec::new(),
            metadata: HashMap::new(),
        };
        snapshot.context_hash = snapshot.compute_hash();
        snapshot
    }

    /// Convenience constructor that accepts pre-existing memory references.
    pub fn with_context(session_id: &str, phase: ExecutionPhase, memory: Vec<String>) -> Self {
        let mut snapshot = Self::generate(session_id, phase);
        snapshot.memory_state = memory;
        snapshot.context_hash = snapshot.compute_hash();
        snapshot
    }

    /// Compute a SHA-256 hash over the checkpoint's identifying fields.
    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.id.as_bytes());
        hasher.update(self.session_id.as_bytes());
        hasher.update(format!("{:?}", self.phase).as_bytes());
        hasher.update(format!("{:?}", self.timestamp).as_bytes());
        for mem in &self.memory_state {
            hasher.update(mem.as_bytes());
        }
        for eff in &self.pending_effects {
            hasher.update(eff.as_bytes());
        }
        let result = hasher.finalize();
        let mut hex = String::with_capacity(result.len() * 2);
        for b in &result {
            use std::fmt::Write;
            let _ = write!(hex, "{b:02x}");
        }
        hex
    }

    /// Validate the checkpoint: timestamp must not be in the future and memory must be non-empty.
    pub fn is_valid(&self) -> bool {
        let timestamp_ok = self.timestamp <= SystemTime::now();
        let has_memory = !self.memory_state.is_empty();
        timestamp_ok && has_memory
    }

    /// Insert a metadata entry.
    pub fn set_metadata(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
    }

    /// Retrieve a metadata value by key.
    pub fn metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_checkpoint_snapshot() {
        let cp = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Explore);
        assert_eq!(cp.session_id, "sess-1");
        assert_eq!(cp.phase, ExecutionPhase::Explore);
        assert!(!cp.id.is_empty());
        assert!(!cp.context_hash.is_empty());
    }

    #[test]
    fn checkpoint_id_is_unique() {
        let a = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Plan);
        let b = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Plan);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn is_valid_returns_true_for_valid_checkpoint() {
        let cp = CheckpointSnapshot::with_context(
            "sess-1",
            ExecutionPhase::Plan,
            vec!["mem".to_string()],
        );
        assert!(cp.is_valid());
    }

    #[test]
    fn is_valid_returns_false_for_empty_memory() {
        let mut cp = CheckpointSnapshot::generate("sess-1", ExecutionPhase::Explore);
        cp.memory_state = Vec::new();
        assert!(!cp.is_valid());
    }
}
