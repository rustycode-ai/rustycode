# Phase 7: Resilience & Checkpointing -- TDD Implementation Plan

**Date**: 2026-04-25
**Goal**: RustyCode survives crashes, validates checkpoints, and replays execution cleanly.
**Status**: Not Started
**See Also**: [Generative Programmer analysis](2026-04-25-generative-programmer-real-analysis.md#phase-status-map), [Architecture Review](../architecture/ARCHITECTURE-REVIEW-2026-04-20.md) § Checkpoint/rewind
**Dependencies**: Phase 2 (Explore-Plan-Act lifecycle), Phase 5 (event store + side effects), existing storage layer
**Target**: ~65 tests across 5 modules

---

## File Structure

```
New files:
  crates/rustycode-core/src/checkpoint.rs              (~400 lines, 20 tests)
  crates/rustycode-core/src/recovery.rs                (~350 lines, 18 tests)
  crates/rustycode-core/src/validation.rs              (~300 lines, 14 tests)
  crates/rustycode-storage/src/checkpoint_store.rs     (~280 lines, 13 tests)

Modified files:
  crates/rustycode-core/src/lib.rs                     (add pub mod checkpoint, recovery, validation)
  crates/rustycode-storage/src/lib.rs                  (add pub mod checkpoint_store)
  crates/rustycode-runtime/src/executor.rs             (wrap execution with checkpoint snapshots)
  crates/rustycode-core/src/session.rs                 (add checkpoint management to Session)
```

---

## Implementation Status

To be completed in this phase.

---

## Chunk 1: Checkpoint Data Model and Snapshots (rustycode-core/src/checkpoint.rs)

### 1.1 CheckpointSnapshot struct

**File**: `crates/rustycode-core/src/checkpoint.rs`

**RED -- Write failing tests first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn create_checkpoint_snapshot() {
        let checkpoint = CheckpointSnapshot {
            id: "ckpt_001".to_string(),
            session_id: "sess_001".to_string(),
            phase: ExecutionPhase::Plan,
            timestamp: SystemTime::now(),
            context_hash: "abc123def".to_string(),
            memory_state: vec!["task_1".to_string(), "task_2".to_string()],
            pending_effects: vec![],
            metadata: Default::default(),
        };
        assert_eq!(checkpoint.id, "ckpt_001");
        assert_eq!(checkpoint.phase, ExecutionPhase::Plan);
        assert_eq!(checkpoint.memory_state.len(), 2);
    }

    #[test]
    fn checkpoint_id_is_unique() {
        let cp1 = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Explore);
        let cp2 = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Explore);
        assert_ne!(cp1.id, cp2.id);
    }

    #[test]
    fn checkpoint_hash_changes_with_context() {
        let cp1 = CheckpointSnapshot::with_context(
            "sess_001",
            ExecutionPhase::Act,
            vec!["file1.txt".to_string()],
        );
        let cp2 = CheckpointSnapshot::with_context(
            "sess_001",
            ExecutionPhase::Act,
            vec!["file2.txt".to_string()],
        );
        assert_ne!(cp1.context_hash, cp2.context_hash);
    }

    #[test]
    fn checkpoint_serialization() {
        let checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        let serialized = serde_json::to_string(&checkpoint).unwrap();
        let deserialized: CheckpointSnapshot = serde_json::from_str(&serialized).unwrap();
        assert_eq!(checkpoint.id, deserialized.id);
        assert_eq!(checkpoint.session_id, deserialized.session_id);
    }

    #[test]
    fn checkpoint_validity_checking() {
        let mut checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Explore);
        assert!(checkpoint.is_valid());
        checkpoint.id = "".to_string();
        assert!(!checkpoint.is_valid());
    }

    #[test]
    fn checkpoint_with_metadata() {
        let mut checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        checkpoint.set_metadata("model", "claude-opus-4-7");
        checkpoint.set_metadata("cost_tokens", "1500");
        assert_eq!(checkpoint.get_metadata("model"), Some("claude-opus-4-7"));
        assert_eq!(checkpoint.get_metadata("cost_tokens"), Some("1500"));
    }

    #[test]
    fn checkpoint_phase_progression() {
        let explore = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Explore);
        let plan = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        let act = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Act);
        
        assert!(ExecutionPhase::Explore < ExecutionPhase::Plan);
        assert!(ExecutionPhase::Plan < ExecutionPhase::Act);
    }
}
```

### 1.2 CheckpointSnapshot struct definition

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use uuid::Uuid;

/// Execution phase enumeration with ordering for progression validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ExecutionPhase {
    Explore = 0,
    Plan = 1,
    Act = 2,
}

impl std::fmt::Display for ExecutionPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ExecutionPhase::Explore => write!(f, "explore"),
            ExecutionPhase::Plan => write!(f, "plan"),
            ExecutionPhase::Act => write!(f, "act"),
        }
    }
}

/// A point-in-time snapshot of execution state for crash recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSnapshot {
    /// Unique checkpoint ID (uuid-based)
    pub id: String,
    /// Session this checkpoint belongs to
    pub session_id: String,
    /// Which execution phase this checkpoint was taken in
    pub phase: ExecutionPhase,
    /// When the checkpoint was created
    pub timestamp: SystemTime,
    /// SHA256 hash of execution context (files, env vars, memory)
    pub context_hash: String,
    /// Memory state snapshot (list of memory file keys)
    pub memory_state: Vec<String>,
    /// Pending side effects not yet committed
    pub pending_effects: Vec<String>,
    /// Arbitrary metadata (model, cost, etc.)
    pub metadata: HashMap<String, String>,
}

impl CheckpointSnapshot {
    /// Generate a new checkpoint with a unique ID
    pub fn generate(session_id: &str, phase: ExecutionPhase) -> Self {
        Self {
            id: format!("ckpt_{}", Uuid::new_v4().to_string()[..8].to_uppercase()),
            session_id: session_id.to_string(),
            phase,
            timestamp: SystemTime::now(),
            context_hash: String::new(),
            memory_state: vec![],
            pending_effects: vec![],
            metadata: HashMap::new(),
        }
    }

    /// Create checkpoint with explicit context
    pub fn with_context(
        session_id: &str,
        phase: ExecutionPhase,
        context: Vec<String>,
    ) -> Self {
        let mut checkpoint = Self::generate(session_id, phase);
        checkpoint.memory_state = context;
        checkpoint.context_hash = Self::compute_hash(&checkpoint.memory_state);
        checkpoint
    }

    /// Compute context hash from memory state
    fn compute_hash(context: &[String]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        for item in context {
            item.hash(&mut hasher);
        }
        format!("{:x}", hasher.finish())
    }

    /// Check if checkpoint is internally valid
    pub fn is_valid(&self) -> bool {
        !self.id.is_empty() && !self.session_id.is_empty()
    }

    /// Set metadata key-value pair
    pub fn set_metadata(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}
```

---

## Chunk 2: Recovery Logic (rustycode-core/src/recovery.rs)

### 2.1 RecoveryState and validation

**File**: `crates/rustycode-core/src/recovery.rs`

**RED -- Tests first**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_from_checkpoint() {
        let checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        let recovery = Recovery::from_checkpoint(&checkpoint).unwrap();
        assert_eq!(recovery.checkpoint.id, checkpoint.id);
        assert!(recovery.is_recoverable);
    }

    #[test]
    fn recovery_detects_phase_violation() {
        let explore_checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Explore);
        let plan_checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        
        let mut recovery = Recovery::from_checkpoint(&plan_checkpoint).unwrap();
        // Attempting to recover to a phase before the latest checkpoint should fail
        assert!(!recovery.can_recover_to_phase(ExecutionPhase::Explore));
    }

    #[test]
    fn recovery_collects_pending_effects() {
        let mut checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Act);
        checkpoint.pending_effects = vec![
            "write_file:src/main.rs".to_string(),
            "git_commit:fix bug".to_string(),
        ];
        
        let recovery = Recovery::from_checkpoint(&checkpoint).unwrap();
        assert_eq!(recovery.pending_effects.len(), 2);
        assert!(recovery.pending_effects.iter().any(|e| e.contains("write_file")));
    }

    #[test]
    fn recovery_state_transitions() {
        let checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        let mut recovery = Recovery::from_checkpoint(&checkpoint).unwrap();
        
        assert_eq!(recovery.state, RecoveryState::Pending);
        recovery.mark_validated();
        assert_eq!(recovery.state, RecoveryState::Validated);
        recovery.mark_replaying();
        assert_eq!(recovery.state, RecoveryState::Replaying);
        recovery.mark_complete();
        assert_eq!(recovery.state, RecoveryState::Complete);
    }

    #[test]
    fn recovery_skips_completed_effects() {
        let mut checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Act);
        checkpoint.pending_effects = vec![
            "write:file1".to_string(),
            "write:file2".to_string(),
        ];
        
        let mut recovery = Recovery::from_checkpoint(&checkpoint).unwrap();
        recovery.mark_effect_completed("write:file1");
        
        let remaining = recovery.remaining_effects();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0], "write:file2");
    }
}
```

### 2.2 Recovery struct and methods

```rust
use super::CheckpointSnapshot;
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    Pending,    // Just loaded checkpoint, not yet validated
    Validated,  // Checkpoint is valid, ready to replay
    Replaying,  // Currently replaying effects
    Complete,   // Recovery finished successfully
}

/// Recovery handler for checkpoint-based crash recovery
pub struct Recovery {
    pub checkpoint: CheckpointSnapshot,
    pub state: RecoveryState,
    pub pending_effects: Vec<String>,
    completed_effects: std::collections::HashSet<String>,
    pub is_recoverable: bool,
}

impl Recovery {
    /// Create recovery state from a checkpoint
    pub fn from_checkpoint(checkpoint: &CheckpointSnapshot) -> Result<Self> {
        Ok(Self {
            checkpoint: checkpoint.clone(),
            state: RecoveryState::Pending,
            pending_effects: checkpoint.pending_effects.clone(),
            completed_effects: Default::default(),
            is_recoverable: checkpoint.is_valid(),
        })
    }

    /// Check if we can recover to a given phase
    pub fn can_recover_to_phase(&self, target: crate::checkpoint::ExecutionPhase) -> bool {
        target >= self.checkpoint.phase
    }

    /// Validate checkpoint integrity
    pub fn validate(&mut self) -> Result<()> {
        if !self.is_recoverable {
            anyhow::bail!("Checkpoint is not recoverable");
        }
        self.state = RecoveryState::Validated;
        Ok(())
    }

    /// Mark the recovery as in-progress
    pub fn mark_replaying(&mut self) {
        self.state = RecoveryState::Replaying;
    }

    /// Mark a specific effect as completed
    pub fn mark_effect_completed(&mut self, effect_id: &str) {
        self.completed_effects.insert(effect_id.to_string());
    }

    /// Get remaining effects to replay
    pub fn remaining_effects(&self) -> Vec<String> {
        self.pending_effects
            .iter()
            .filter(|e| !self.completed_effects.contains(e.as_str()))
            .cloned()
            .collect()
    }

    /// Mark recovery as complete
    pub fn mark_complete(&mut self) {
        self.state = RecoveryState::Complete;
    }

    /// Is recovery finished?
    pub fn is_complete(&self) -> bool {
        self.state == RecoveryState::Complete
    }
}
```

---

## Chunk 3: Checkpoint Validation (rustycode-core/src/validation.rs)

### 3.1 Checkpoint validation rules

**File**: `crates/rustycode-core/src/validation.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_checkpoint_structure() {
        let checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        let validator = CheckpointValidator::new();
        assert!(validator.validate_structure(&checkpoint).is_ok());
    }

    #[test]
    fn reject_checkpoint_with_missing_id() {
        let mut checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        checkpoint.id = "".to_string();
        let validator = CheckpointValidator::new();
        assert!(validator.validate_structure(&checkpoint).is_err());
    }

    #[test]
    fn validate_checkpoint_not_too_old() {
        let checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        let validator = CheckpointValidator::with_max_age(std::time::Duration::from_secs(3600));
        assert!(validator.validate_recency(&checkpoint).is_ok());
    }

    #[test]
    fn reject_checkpoint_too_stale() {
        let mut checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        checkpoint.timestamp = std::time::SystemTime::now()
            - std::time::Duration::from_secs(7200); // 2 hours old
        
        let validator = CheckpointValidator::with_max_age(std::time::Duration::from_secs(3600));
        assert!(validator.validate_recency(&checkpoint).is_err());
    }

    #[test]
    fn comprehensive_checkpoint_validation() {
        let checkpoint = CheckpointSnapshot::with_context(
            "sess_001",
            ExecutionPhase::Plan,
            vec!["memory_1".to_string()],
        );
        let validator = CheckpointValidator::new();
        
        let result = validator.validate_complete(&checkpoint);
        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.is_valid);
    }

    #[test]
    fn validation_report_details() {
        let checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        let validator = CheckpointValidator::new();
        let report = validator.validate_complete(&checkpoint).unwrap();
        
        assert_eq!(report.checkpoint_id, checkpoint.id);
        assert_eq!(report.checks_passed, report.total_checks);
    }
}
```

### 3.2 Validator implementation

```rust
use super::CheckpointSnapshot;
use anyhow::{bail, Result};
use std::time::{Duration, SystemTime};

/// Validates checkpoint integrity and recoverability
pub struct CheckpointValidator {
    max_age: Duration,
    require_memory_state: bool,
}

impl CheckpointValidator {
    pub fn new() -> Self {
        Self {
            max_age: Duration::from_secs(86400), // 24 hours default
            require_memory_state: false,
        }
    }

    pub fn with_max_age(max_age: Duration) -> Self {
        Self {
            max_age,
            require_memory_state: false,
        }
    }

    /// Validate checkpoint structure
    pub fn validate_structure(&self, checkpoint: &CheckpointSnapshot) -> Result<()> {
        if checkpoint.id.is_empty() {
            bail!("Checkpoint ID is empty");
        }
        if checkpoint.session_id.is_empty() {
            bail!("Session ID is empty");
        }
        Ok(())
    }

    /// Validate checkpoint recency
    pub fn validate_recency(&self, checkpoint: &CheckpointSnapshot) -> Result<()> {
        let age = SystemTime::now()
            .duration_since(checkpoint.timestamp)
            .unwrap_or(Duration::ZERO);
        
        if age > self.max_age {
            bail!("Checkpoint is too old: {:?}", age);
        }
        Ok(())
    }

    /// Comprehensive validation with report
    pub fn validate_complete(&self, checkpoint: &CheckpointSnapshot) -> Result<ValidationReport> {
        let mut report = ValidationReport {
            checkpoint_id: checkpoint.id.clone(),
            total_checks: 3,
            checks_passed: 0,
            errors: vec![],
            is_valid: true,
        };

        if self.validate_structure(checkpoint).is_ok() {
            report.checks_passed += 1;
        } else {
            report.is_valid = false;
            report.errors.push("Structure validation failed".to_string());
        }

        if self.validate_recency(checkpoint).is_ok() {
            report.checks_passed += 1;
        } else {
            report.is_valid = false;
            report.errors.push("Recency validation failed".to_string());
        }

        if !checkpoint.memory_state.is_empty() || !self.require_memory_state {
            report.checks_passed += 1;
        } else {
            report.is_valid = false;
            report.errors.push("No memory state".to_string());
        }

        Ok(report)
    }
}

impl Default for CheckpointValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Validation report with detailed results
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub checkpoint_id: String,
    pub total_checks: usize,
    pub checks_passed: usize,
    pub errors: Vec<String>,
    pub is_valid: bool,
}
```

---

## Chunk 4: Checkpoint Storage (rustycode-storage/src/checkpoint_store.rs)

### 4.1 Checkpoint persistence

**File**: `crates/rustycode-storage/src/checkpoint_store.rs`

**RED -- Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        tempfile::TempDir::new().unwrap().into_path()
    }

    #[test]
    fn save_and_load_checkpoint() {
        let dir = temp_dir();
        let store = CheckpointStore::new(&dir).unwrap();
        
        let checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        store.save(&checkpoint).unwrap();
        
        let loaded = store.load(&checkpoint.id).unwrap();
        assert_eq!(loaded.id, checkpoint.id);
        assert_eq!(loaded.session_id, checkpoint.session_id);
    }

    #[test]
    fn list_checkpoints_for_session() {
        let dir = temp_dir();
        let store = CheckpointStore::new(&dir).unwrap();
        
        let cp1 = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Explore);
        let cp2 = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        let cp3 = CheckpointSnapshot::generate("sess_002", ExecutionPhase::Plan);
        
        store.save(&cp1).unwrap();
        store.save(&cp2).unwrap();
        store.save(&cp3).unwrap();
        
        let checkpoints = store.list_for_session("sess_001").unwrap();
        assert_eq!(checkpoints.len(), 2);
        assert!(checkpoints.iter().all(|c| c.session_id == "sess_001"));
    }

    #[test]
    fn get_latest_checkpoint() {
        let dir = temp_dir();
        let store = CheckpointStore::new(&dir).unwrap();
        
        let cp1 = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Explore);
        let cp2 = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        
        store.save(&cp1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.save(&cp2).unwrap();
        
        let latest = store.get_latest("sess_001").unwrap();
        assert_eq!(latest.phase, ExecutionPhase::Plan);
    }

    #[test]
    fn delete_checkpoint() {
        let dir = temp_dir();
        let store = CheckpointStore::new(&dir).unwrap();
        
        let checkpoint = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        store.save(&checkpoint).unwrap();
        assert!(store.exists(&checkpoint.id).unwrap());
        
        store.delete(&checkpoint.id).unwrap();
        assert!(!store.exists(&checkpoint.id).unwrap());
    }

    #[test]
    fn prune_old_checkpoints() {
        let dir = temp_dir();
        let store = CheckpointStore::new(&dir).unwrap();
        
        let mut old_cp = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Plan);
        old_cp.timestamp = std::time::SystemTime::now() - std::time::Duration::from_secs(172800);
        
        let fresh_cp = CheckpointSnapshot::generate("sess_001", ExecutionPhase::Act);
        
        store.save(&old_cp).unwrap();
        store.save(&fresh_cp).unwrap();
        
        let pruned = store.prune_older_than(std::time::Duration::from_secs(86400)).unwrap();
        assert_eq!(pruned, 1);
    }
}
```

### 4.2 CheckpointStore implementation

```rust
use super::CheckpointSnapshot;
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Persistent checkpoint storage
pub struct CheckpointStore {
    base_dir: PathBuf,
}

impl CheckpointStore {
    pub fn new(base_dir: &Path) -> Result<Self> {
        fs::create_dir_all(base_dir)?;
        Ok(Self {
            base_dir: base_dir.to_path_buf(),
        })
    }

    fn checkpoint_path(&self, id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", id))
    }

    /// Save checkpoint to disk
    pub fn save(&self, checkpoint: &CheckpointSnapshot) -> Result<()> {
        let path = self.checkpoint_path(&checkpoint.id);
        let json = serde_json::to_string_pretty(checkpoint)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load checkpoint by ID
    pub fn load(&self, id: &str) -> Result<CheckpointSnapshot> {
        let path = self.checkpoint_path(id);
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Check if checkpoint exists
    pub fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.checkpoint_path(id).exists())
    }

    /// List all checkpoints for a session
    pub fn list_for_session(&self, session_id: &str) -> Result<Vec<CheckpointSnapshot>> {
        let mut checkpoints = vec![];
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(cp) = serde_json::from_str::<CheckpointSnapshot>(&content) {
                        if cp.session_id == session_id {
                            checkpoints.push(cp);
                        }
                    }
                }
            }
        }
        checkpoints.sort_by_key(|cp| cp.timestamp);
        Ok(checkpoints)
    }

    /// Get the most recent checkpoint for a session
    pub fn get_latest(&self, session_id: &str) -> Result<CheckpointSnapshot> {
        let checkpoints = self.list_for_session(session_id)?;
        checkpoints.last().cloned()
            .ok_or_else(|| anyhow::anyhow!("No checkpoints found for session {}", session_id))
    }

    /// Delete a checkpoint
    pub fn delete(&self, id: &str) -> Result<()> {
        fs::remove_file(self.checkpoint_path(id))?;
        Ok(())
    }

    /// Delete checkpoints older than given duration
    pub fn prune_older_than(&self, max_age: std::time::Duration) -> Result<usize> {
        let now = std::time::SystemTime::now();
        let mut deleted = 0;

        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(cp) = serde_json::from_str::<CheckpointSnapshot>(&content) {
                        if let Ok(age) = now.duration_since(cp.timestamp) {
                            if age > max_age {
                                fs::remove_file(path)?;
                                deleted += 1;
                            }
                        }
                    }
                }
            }
        }
        Ok(deleted)
    }
}
```

---

## Chunk 5: Executor Integration (rustycode-runtime/src/executor.rs modifications)

### 5.1 Wrap execution with checkpoint snapshots

Modify `StepExecutor::execute()` to:

```rust
pub async fn execute(&mut self, step: &Step) -> Result<StepResult> {
    // Create pre-execution checkpoint
    let pre_checkpoint = CheckpointSnapshot::with_context(
        &self.session_id,
        self.current_phase,
        self.memory_snapshot(),
    );
    self.checkpoint_store.save(&pre_checkpoint)?;

    // Execute step
    let result = self.execute_step_internal(step).await?;

    // Create post-execution checkpoint only on success
    if result.is_success() {
        let post_checkpoint = CheckpointSnapshot::with_context(
            &self.session_id,
            self.current_phase,
            self.memory_snapshot(),
        );
        post_checkpoint.set_metadata("result_tokens", &result.tokens_used.to_string());
        self.checkpoint_store.save(&post_checkpoint)?;
    }

    Ok(result)
}
```

Tests (8 tests):
- Execute with checkpoint on success
- Execute with checkpoint on failure
- Recover from checkpoint after crash
- Phase progression respects checkpoints
- Multiple checkpoints per session
- Checkpoint context includes all memory
- Failed steps don't overwrite checkpoint
- Parallel steps create independent checkpoints

---

## Chunk 6: Module Wiring (lib.rs modifications)

Update `crates/rustycode-core/src/lib.rs`:

```rust
pub mod checkpoint;
pub mod recovery;
pub mod validation;

pub use checkpoint::{CheckpointSnapshot, ExecutionPhase};
pub use recovery::Recovery;
pub use validation::CheckpointValidator;
```

Update `crates/rustycode-storage/src/lib.rs`:

```rust
pub mod checkpoint_store;
pub use checkpoint_store::CheckpointStore;
```

Update `crates/rustycode-core/src/session.rs`:

```rust
pub struct Session {
    // ... existing fields ...
    checkpoint_store: CheckpointStore,
    recovery_handler: Option<Recovery>,
}

impl Session {
    pub async fn restore_from_checkpoint(&mut self, checkpoint_id: &str) -> Result<()> {
        let checkpoint = self.checkpoint_store.load(checkpoint_id)?;
        let mut recovery = Recovery::from_checkpoint(&checkpoint)?;
        recovery.validate()?;
        self.recovery_handler = Some(recovery);
        Ok(())
    }

    pub fn pending_effects(&self) -> Vec<String> {
        self.recovery_handler
            .as_ref()
            .map(|r| r.remaining_effects())
            .unwrap_or_default()
    }
}
```

---

## Chunk 7: Full Workspace Verification

```bash
# Format
cargo fmt --check

# Clippy (must be zero warnings)
cargo clippy --workspace --all-targets -- -D warnings

# All tests
cargo test --workspace
```

### Expected test count

| Module | Tests |
|--------|-------|
| rustycode-core/src/checkpoint.rs | 8 |
| rustycode-core/src/recovery.rs | 5 |
| rustycode-core/src/validation.rs | 5 |
| rustycode-storage/src/checkpoint_store.rs | 7 |
| rustycode-runtime executor integration | 8 |
| rustycode-core session integration | 5 |
| **Total** | **38** |

---

## Integration Guide

### How the pieces connect

```
Crash/Interrupt
        |
        v
CheckpointSnapshot (pre-saved at each step)
        |
        v
Session::restore_from_checkpoint()
        |
        v
Recovery::from_checkpoint()
        |
        v
CheckpointValidator::validate_complete()
        |
        v
ExecutionReplay::replay_remaining_effects()
        |
        v
Execute pending side effects
```

### Integration points for existing code

1. **Step Execution** (`rustycode-runtime`):
   ```rust
   executor.checkpoint_store = CheckpointStore::new(&session.checkpoint_dir)?;
   executor.execute(step).await?; // Auto-saves checkpoints
   ```

2. **Session Recovery** (`rustycode-core`):
   ```rust
   if let Ok(latest) = checkpoint_store.get_latest(&session_id) {
       session.restore_from_checkpoint(&latest.id).await?;
       for effect in session.pending_effects() {
           side_effect_ledger.replay(effect).await?;
       }
   }
   ```

3. **Periodic Cleanup** (`background task`):
   ```rust
   checkpoint_store.prune_older_than(Duration::from_secs(604800))?; // 7 days
   ```

---

## Next Actions

1. **Chunk 1-2**: Implement checkpoint model and recovery logic (2-3 hours)
2. **Chunk 3-4**: Implement validation and storage (2-3 hours)
3. **Chunk 5-6**: Integrate with executor and session (2-3 hours)
4. **Chunk 7**: Workspace verification (1 hour)
5. **Follow-up**: Wire into CLI session startup (separate PR)
6. **Follow-up**: Add checkpoint inspection CLI command
7. **Follow-up**: Dashboard view of checkpoint history

---

**Status**: Ready for implementation
