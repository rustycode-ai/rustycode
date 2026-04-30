//! Checkpoint recovery state machine for replaying effects after restoring a checkpoint.
//!
//! Tracks which pending effects have been replayed and provides a state machine
//! (`RecoveryState`) that gates transitions: Pending → Validated → Replaying → Complete.

use std::collections::HashSet;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::checkpoint::{CheckpointSnapshot, ExecutionPhase};

/// State of the recovery process.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum RecoveryState {
    Pending,
    Validated,
    Replaying,
    Complete,
}

/// Manages recovery from a checkpoint snapshot, tracking which effects have been replayed.
pub struct Recovery {
    checkpoint: CheckpointSnapshot,
    state: RecoveryState,
    completed_effects: HashSet<String>,
}

impl Recovery {
    /// Create a new recovery from a checkpoint. State starts as `Pending`.
    pub fn from_checkpoint(checkpoint: CheckpointSnapshot) -> Self {
        Self {
            checkpoint,
            state: RecoveryState::Pending,
            completed_effects: HashSet::new(),
        }
    }

    /// Check whether the checkpoint's phase is at or before the given target phase.
    pub fn can_recover_to_phase(&self, target: ExecutionPhase) -> bool {
        self.checkpoint.phase <= target
    }

    /// Validate the checkpoint and transition to `Validated` if valid.
    pub fn validate(&mut self) -> Result<()> {
        if self.checkpoint.is_valid() {
            self.state = RecoveryState::Validated;
            Ok(())
        } else {
            bail!("checkpoint is not valid: timestamp or memory state invalid");
        }
    }

    /// Skip validation and mark as `Validated`.
    pub fn mark_validated(&mut self) {
        self.state = RecoveryState::Validated;
    }

    /// Transition to `Replaying` state.
    pub fn mark_replaying(&mut self) {
        self.state = RecoveryState::Replaying;
    }

    /// Record that an effect has been completed during replay.
    pub fn mark_effect_completed(&mut self, effect_id: &str) {
        self.completed_effects.insert(effect_id.to_string());
    }

    /// Return effects from the checkpoint that have not yet been completed.
    pub fn remaining_effects(&self) -> Vec<String> {
        self.checkpoint
            .pending_effects
            .iter()
            .filter(|e| !self.completed_effects.contains(*e))
            .cloned()
            .collect()
    }

    /// Transition to `Complete` state.
    pub fn mark_complete(&mut self) {
        self.state = RecoveryState::Complete;
    }

    /// Check whether recovery is complete.
    pub fn is_complete(&self) -> bool {
        self.state == RecoveryState::Complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::ExecutionPhase::*;

    fn valid_checkpoint(phase: ExecutionPhase, effects: Vec<String>) -> CheckpointSnapshot {
        let mut cp = CheckpointSnapshot::generate("test-session", phase);
        cp.pending_effects = effects;
        cp
    }

    #[test]
    fn from_checkpoint_sets_pending_state() {
        let cp = valid_checkpoint(Explore, vec![]);
        let recovery = Recovery::from_checkpoint(cp);
        assert_eq!(recovery.state, RecoveryState::Pending);
    }

    #[test]
    fn validate_succeeds_for_valid_checkpoint() {
        let cp = valid_checkpoint(Plan, vec!["e1".into()]);
        let mut recovery = Recovery::from_checkpoint(cp);
        assert!(recovery.validate().is_ok());
        assert_eq!(recovery.state, RecoveryState::Validated);
    }

    #[test]
    fn validate_fails_for_invalid_checkpoint() {
        let mut cp = CheckpointSnapshot::generate("test-session", Act);
        cp.memory_state = Vec::new();
        let mut recovery = Recovery::from_checkpoint(cp);
        assert!(recovery.validate().is_err());
        assert_eq!(recovery.state, RecoveryState::Pending);
    }

    #[test]
    fn remaining_effects_returns_pending() {
        let cp = valid_checkpoint(Act, vec!["a".into(), "b".into(), "c".into()]);
        let mut recovery = Recovery::from_checkpoint(cp);
        recovery.mark_effect_completed("b");
        let mut remaining = recovery.remaining_effects();
        remaining.sort();
        assert_eq!(remaining, vec!["a", "c"]);
    }
}
