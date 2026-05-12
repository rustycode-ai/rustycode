//! Checkpoint validation for ensuring snapshot integrity before recovery.
//!
//! Provides configurable validation rules for checkpoint snapshots,
//! checking structural integrity, recency, and completeness.

use std::time::Duration;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use super::snapshot::CheckpointSnapshot;

/// Report summarizing the result of checkpoint validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// ID of the checkpoint that was validated.
    pub checkpoint_id: String,
    /// Total number of checks performed.
    pub total_checks: usize,
    /// Number of checks that passed.
    pub checks_passed: usize,
    /// Descriptions of failures.
    pub errors: Vec<String>,
    /// `true` when every check passed.
    pub is_valid: bool,
}

impl ValidationReport {
    fn new(checkpoint_id: &str) -> Self {
        Self {
            checkpoint_id: checkpoint_id.to_string(),
            total_checks: 0,
            checks_passed: 0,
            errors: Vec::new(),
            is_valid: true,
        }
    }

    fn pass(&mut self) {
        self.total_checks += 1;
        self.checks_passed += 1;
    }

    fn fail(&mut self, msg: impl Into<String>) {
        self.total_checks += 1;
        self.errors.push(msg.into());
        self.is_valid = false;
    }
}

/// Configurable validator for `CheckpointSnapshot` instances.
///
/// # Examples
///
/// ```
/// use rustycode_core::recovery::CheckpointValidator;
/// use std::time::Duration;
///
/// let validator = CheckpointValidator::new()
///     .with_max_age(Duration::from_secs(3600));
/// ```
#[derive(Debug, Clone, Default)]
pub struct CheckpointValidator {
    /// Maximum age a checkpoint may have before it is considered stale.
    /// `None` disables the recency check.
    pub max_age: Option<Duration>,
    /// Whether the checkpoint must carry at least one memory-state entry.
    pub require_memory_state: bool,
}

impl CheckpointValidator {
    /// Create a validator with relaxed defaults (no max age, memory optional).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum checkpoint age.
    pub fn with_max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age);
        self
    }

    /// Require that memory_state is non-empty.
    pub fn with_required_memory_state(mut self) -> Self {
        self.require_memory_state = true;
        self
    }

    /// Check structural integrity: id, session_id, context_hash must be non-empty.
    pub fn validate_structure(&self, checkpoint: &CheckpointSnapshot) -> Result<()> {
        if checkpoint.id.is_empty() {
            bail!("checkpoint id is empty");
        }
        if checkpoint.session_id.is_empty() {
            bail!("checkpoint session_id is empty");
        }
        if checkpoint.context_hash.is_empty() {
            bail!("checkpoint context_hash is empty");
        }
        if self.require_memory_state && checkpoint.memory_state.is_empty() {
            bail!("checkpoint memory_state is empty but required");
        }
        Ok(())
    }

    /// Check that the checkpoint is not older than `max_age`.
    ///
    /// Skipped when `max_age` is `None`.
    pub fn validate_recency(&self, checkpoint: &CheckpointSnapshot) -> Result<()> {
        if let Some(max) = self.max_age {
            let age = std::time::SystemTime::now()
                .duration_since(checkpoint.timestamp)
                .map_err(|_| anyhow::anyhow!("checkpoint timestamp is in the future"))?;

            if age > max {
                bail!("checkpoint is too old: {:?} exceeds max_age {:?}", age, max);
            }
        }
        Ok(())
    }

    /// Run all configured validations and produce a `ValidationReport`.
    pub fn validate_complete(&self, checkpoint: &CheckpointSnapshot) -> ValidationReport {
        let mut report = ValidationReport::new(&checkpoint.id);

        match self.validate_structure(checkpoint) {
            Ok(()) => report.pass(),
            Err(e) => report.fail(format!("structure: {e}")),
        }

        match self.validate_recency(checkpoint) {
            Ok(()) => report.pass(),
            Err(e) => report.fail(format!("recency: {e}")),
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::super::snapshot::ExecutionPhase;
    use super::*;

    fn valid_checkpoint() -> CheckpointSnapshot {
        CheckpointSnapshot::with_context("sess-test", ExecutionPhase::Plan, vec!["mem".to_string()])
    }

    #[test]
    fn validator_default_constructs() {
        let v = CheckpointValidator::default();
        assert!(v.max_age.is_none());
        assert!(!v.require_memory_state);
    }

    #[test]
    fn with_max_age_sets_limit() {
        let v = CheckpointValidator::new().with_max_age(Duration::from_mins(1));
        assert_eq!(v.max_age, Some(Duration::from_mins(1)));
    }

    #[test]
    fn validate_structure_succeeds_for_valid() {
        let cp = valid_checkpoint();
        let v = CheckpointValidator::new();
        assert!(v.validate_structure(&cp).is_ok());
    }

    #[test]
    fn validate_structure_fails_for_missing_id() {
        let mut cp = valid_checkpoint();
        cp.id = String::new();
        let v = CheckpointValidator::new();
        assert!(v.validate_structure(&cp).is_err());
    }

    #[test]
    fn validate_complete_all_pass() {
        let cp = valid_checkpoint();
        let v = CheckpointValidator::new();
        let report = v.validate_complete(&cp);
        assert!(report.is_valid);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn validation_report_tracks_errors() {
        let mut cp = valid_checkpoint();
        cp.id = String::new();
        let v = CheckpointValidator::new();
        let report = v.validate_complete(&cp);
        assert!(!report.is_valid);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn validation_report_is_valid_when_no_errors() {
        let cp = valid_checkpoint();
        let v = CheckpointValidator::new();
        let report = v.validate_complete(&cp);
        assert!(report.is_valid);
        assert_eq!(report.checks_passed, report.total_checks);
    }

    #[test]
    fn require_memory_state_fails_on_empty() {
        let mut cp = valid_checkpoint();
        cp.memory_state = Vec::new();
        let v = CheckpointValidator::new().with_required_memory_state();
        assert!(v.validate_structure(&cp).is_err());
    }

    #[test]
    fn validate_recency_skips_when_no_max_age() {
        let cp = valid_checkpoint();
        let v = CheckpointValidator::new(); // no max_age
        assert!(v.validate_recency(&cp).is_ok());
    }
}
