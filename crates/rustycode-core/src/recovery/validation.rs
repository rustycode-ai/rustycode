//! Checkpoint integrity validation for session snapshots.
//!
//! Validates that `SessionSnapshot` checkpoints are structurally sound and
//! sufficiently recent before they are used for recovery or rewind operations.
//!
//! # Example
//!
//! ```rust
//! use rustycode_core::recovery::CheckpointValidator;
//! use rustycode_protocol::session::{SessionSnapshot, SessionState};
//!
//! let validator = CheckpointValidator::new()
//!     .with_max_age(std::time::Duration::from_secs(3600));
//!
//! let snapshot = SessionSnapshot::new(
//!     "cp_123".into(),
//!     chrono::Utc::now().to_rfc3339(),
//!     3,
//!     SessionState::Active,
//!     "working".into(),
//! );
//!
//! let report = validator.validate_complete(&snapshot).expect("validation failed");
//! assert!(report.is_valid);
//! ```

use anyhow::Result;
use chrono::{DateTime, Utc};
use rustycode_protocol::session::SessionSnapshot;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Default maximum checkpoint age: 24 hours.
const DEFAULT_MAX_AGE: Duration = Duration::from_hours(24);

/// Validates checkpoint integrity for session snapshots.
///
/// Configuration controls which checks are applied and their thresholds.
/// Use the builder-style methods to customize behavior.
#[derive(Debug, Clone)]
pub struct CheckpointValidator {
    /// Maximum age a checkpoint may have before it is considered stale.
    max_age: Duration,
    /// When true, checkpoints must carry a non-empty memory/context payload.
    require_memory_state: bool,
}

impl Default for CheckpointValidator {
    fn default() -> Self {
        Self {
            max_age: DEFAULT_MAX_AGE,
            require_memory_state: false,
        }
    }
}

impl CheckpointValidator {
    /// Create a validator with default settings (24h max age, memory state not required).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum age a checkpoint may have before it is considered stale.
    pub fn with_max_age(mut self, max_age: Duration) -> Self {
        self.max_age = max_age;
        self
    }

    /// Require that checkpoints carry a non-empty context (memory state).
    pub fn with_require_memory_state(mut self, require: bool) -> Self {
        self.require_memory_state = require;
        self
    }

    /// Run all validation checks against a session snapshot.
    ///
    /// Returns a `ValidationReport` containing the results of every check
    /// and an overall pass/fail determination.
    pub fn validate_complete(&self, snapshot: &SessionSnapshot) -> Result<ValidationReport> {
        let mut errors: Vec<ValidationError> = Vec::new();

        // Structure checks
        if let Some(err) = self.validate_structure(snapshot) {
            errors.push(err);
        }

        // Recency checks
        if let Some(err) = self.validate_recency(snapshot) {
            errors.push(err);
        }

        // Optional memory-state check
        if self.require_memory_state {
            if let Some(err) = self.validate_memory_state(snapshot) {
                errors.push(err);
            }
        }

        let total_checks = if self.require_memory_state {
            3_usize
        } else {
            2
        };
        let checks_passed = total_checks.saturating_sub(errors.len());

        let mut report = ValidationReport {
            checkpoint_id: snapshot.id.clone(),
            total_checks,
            checks_passed,
            errors,
            is_valid: false,
        };
        report.finalize();
        Ok(report)
    }

    /// Validate structural integrity of a checkpoint.
    ///
    /// Returns `None` when the snapshot passes all structural checks.
    fn validate_structure(&self, snapshot: &SessionSnapshot) -> Option<ValidationError> {
        if snapshot.id.trim().is_empty() {
            return Some(ValidationError {
                kind: ValidationKind::Structure,
                message: "checkpoint ID is empty".into(),
            });
        }
        None
    }

    /// Validate that the checkpoint is sufficiently recent.
    ///
    /// Parses `created_at` as an RFC 3339 timestamp and checks the age
    /// against the configured `max_age`.
    fn validate_recency(&self, snapshot: &SessionSnapshot) -> Option<ValidationError> {
        let created_at = match DateTime::parse_from_rfc3339(&snapshot.created_at) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                return Some(ValidationError {
                    kind: ValidationKind::Recency,
                    message: format!("created_at is not a valid RFC 3339 timestamp: {e}"),
                });
            }
        };

        let age = Utc::now().signed_duration_since(created_at);
        let max_age_secs = self.max_age.as_secs() as i64;
        if age.num_seconds() > max_age_secs {
            return Some(ValidationError {
                kind: ValidationKind::Recency,
                message: format!(
                    "checkpoint is {}s old, exceeding max age of {}s",
                    age.num_seconds(),
                    max_age_secs,
                ),
            });
        }
        None
    }

    /// Validate that the snapshot carries meaningful context/memory state.
    fn validate_memory_state(&self, snapshot: &SessionSnapshot) -> Option<ValidationError> {
        if snapshot.context.trim().is_empty() {
            return Some(ValidationError {
                kind: ValidationKind::MemoryState,
                message: "checkpoint context (memory state) is empty".into(),
            });
        }
        None
    }
}

/// Overall validation result for a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// ID of the checkpoint that was validated.
    pub checkpoint_id: String,
    /// Total number of checks that were run.
    pub total_checks: usize,
    /// Number of checks that passed.
    pub checks_passed: usize,
    /// Errors from checks that failed.
    pub errors: Vec<ValidationError>,
    /// Whether the checkpoint passed all checks.
    pub is_valid: bool,
}

impl ValidationReport {
    /// Finalize the report, computing `is_valid` from the error list.
    ///
    /// Called internally by `CheckpointValidator::validate_complete`.
    fn finalize(&mut self) {
        self.is_valid = self.errors.is_empty();
    }
}

/// Category of validation check that produced an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationKind {
    /// Structural integrity (IDs, required fields).
    Structure,
    /// Timestamp recency checks.
    Recency,
    /// Memory/state payload checks.
    MemoryState,
}

/// A single validation failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Category of the failed check.
    pub kind: ValidationKind,
    /// Human-readable description of the failure.
    pub message: String,
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::session::SessionState;

    /// Helper: build a valid snapshot created right now.
    fn fresh_snapshot() -> SessionSnapshot {
        SessionSnapshot::new(
            "cp_test_001".into(),
            Utc::now().to_rfc3339(),
            5,
            SessionState::Active,
            "test context".into(),
        )
    }

    /// Helper: build a snapshot with a specific created_at timestamp.
    fn snapshot_at(created_at: &str) -> SessionSnapshot {
        SessionSnapshot::new(
            "cp_test_002".into(),
            created_at.into(),
            0,
            SessionState::Active,
            String::new(),
        )
    }

    // -- validate_structure tests ----------------------------------------

    #[test]
    fn valid_structure_passes() {
        let validator = CheckpointValidator::new();
        let snapshot = fresh_snapshot();
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(report.is_valid);
    }

    #[test]
    fn missing_id_fails_structure() {
        let mut snapshot = fresh_snapshot();
        snapshot.id = String::new();
        let validator = CheckpointValidator::new();
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(!report.is_valid);
        assert!(report.errors[0].message.contains("ID is empty"));
    }

    #[test]
    fn whitespace_only_id_fails_structure() {
        let mut snapshot = fresh_snapshot();
        snapshot.id = "   ".into();
        let validator = CheckpointValidator::new();
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(!report.is_valid);
        assert!(report.errors[0].message.contains("ID is empty"));
    }

    // -- validate_recency tests ------------------------------------------

    #[test]
    fn fresh_checkpoint_passes_recency() {
        let validator = CheckpointValidator::new().with_max_age(Duration::from_mins(1));
        let snapshot = fresh_snapshot();
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(report.is_valid);
    }

    #[test]
    fn stale_checkpoint_fails_recency() {
        // created 2 hours ago, max age 1 hour
        let two_hours_ago = Utc::now()
            .checked_sub_signed(chrono::Duration::seconds(7200))
            .unwrap()
            .to_rfc3339();
        let snapshot = snapshot_at(&two_hours_ago);
        let validator = CheckpointValidator::new().with_max_age(Duration::from_hours(1));
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(!report.is_valid);
        assert!(report
            .errors
            .iter()
            .any(|e| e.kind == ValidationKind::Recency));
    }

    #[test]
    fn invalid_timestamp_fails_recency() {
        let snapshot = snapshot_at("not-a-timestamp");
        let validator = CheckpointValidator::new();
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(!report.is_valid);
        assert!(report.errors[0].message.contains("RFC 3339"));
    }

    #[test]
    fn checkpoint_exactly_at_max_age_passes() {
        let max = Duration::from_hours(1);
        let created_at = Utc::now()
            .checked_sub_signed(chrono::Duration::seconds(3600))
            .unwrap()
            .to_rfc3339();
        let snapshot = snapshot_at(&created_at);
        let validator = CheckpointValidator::new().with_max_age(max);
        let report = validator.validate_complete(&snapshot).unwrap();
        // age == max_age should pass (not strictly greater)
        assert!(report.is_valid);
    }

    // -- require_memory_state tests --------------------------------------

    #[test]
    fn empty_context_fails_when_memory_required() {
        let mut snapshot = fresh_snapshot();
        snapshot.context = String::new();
        let validator = CheckpointValidator::new().with_require_memory_state(true);
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(!report.is_valid);
        assert!(report
            .errors
            .iter()
            .any(|e| e.kind == ValidationKind::MemoryState));
    }

    #[test]
    fn empty_context_passes_when_memory_not_required() {
        let mut snapshot = fresh_snapshot();
        snapshot.context = String::new();
        let validator = CheckpointValidator::new().with_require_memory_state(false);
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(report.is_valid);
    }

    // -- comprehensive / report detail tests -----------------------------

    #[test]
    fn comprehensive_valid_checkpoint() {
        let snapshot = fresh_snapshot();
        let validator = CheckpointValidator::new()
            .with_max_age(Duration::from_hours(24))
            .with_require_memory_state(true);
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(report.is_valid);
        assert_eq!(report.total_checks, 3);
        assert_eq!(report.checks_passed, 3);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn report_contains_checkpoint_id() {
        let mut snapshot = fresh_snapshot();
        snapshot.id = "cp_report_id_test".into();
        let validator = CheckpointValidator::new();
        let report = validator.validate_complete(&snapshot).unwrap();
        assert_eq!(report.checkpoint_id, "cp_report_id_test");
    }

    #[test]
    fn multiple_errors_reported() {
        // Empty ID + stale + no context = three errors when all checks enabled
        let two_days_ago = Utc::now()
            .checked_sub_signed(chrono::Duration::seconds(172800))
            .unwrap()
            .to_rfc3339();
        let snapshot = SessionSnapshot {
            id: String::new(),
            created_at: two_days_ago,
            last_step: 0,
            state: SessionState::Active,
            context: String::new(),
            checkpoint_git_hash: None,
            checkpoint_modified_files: Vec::new(),
            checkpoint_created_at: None,
        };
        let validator = CheckpointValidator::new()
            .with_max_age(Duration::from_hours(24))
            .with_require_memory_state(true);
        let report = validator.validate_complete(&snapshot).unwrap();
        assert!(!report.is_valid);
        assert_eq!(report.total_checks, 3);
        assert_eq!(report.errors.len(), 3);
        assert_eq!(report.checks_passed, 0);
    }
}
