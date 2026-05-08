//! Side-effect ledger for crash recovery.
//!
//! Tracks every state-mutating action (file writes, command executions,
//! database changes) so that after a crash, the agent can skip
//! already-completed side effects and avoid duplicate operations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Type of side effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectType {
    FileWrite,
    FileEdit,
    FileDelete,
    CommandExecution,
    DatabaseChange,
    NetworkCall,
}

impl fmt::Display for SideEffectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileWrite => write!(f, "file_write"),
            Self::FileEdit => write!(f, "file_edit"),
            Self::FileDelete => write!(f, "file_delete"),
            Self::CommandExecution => write!(f, "command_execution"),
            Self::DatabaseChange => write!(f, "database_change"),
            Self::NetworkCall => write!(f, "network_call"),
        }
    }
}

/// A single tracked side effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffect {
    /// Unique identifier for this effect.
    pub id: String,
    /// Tool that caused this effect.
    pub tool_name: String,
    /// Target of the effect (file path, command, etc.).
    pub target: String,
    /// Human-readable description.
    pub description: String,
    /// Category of side effect.
    pub side_effect_type: SideEffectType,
    /// Whether this effect can be reversed.
    pub is_reversible: bool,
    /// When this effect was recorded (UNIX timestamp millis).
    pub timestamp: u64,
    /// When this effect was completed (UNIX timestamp millis), if any.
    #[serde(default)]
    pub completed_at: Option<u64>,
}

impl SideEffect {
    /// Whether this effect has been completed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.completed_at.is_some()
    }

    /// Mark this effect as completed.
    pub fn complete(&mut self) {
        self.completed_at = Some(now_millis());
    }
}

/// Summary of ledger state for recovery checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryStatus {
    /// Total number of effects.
    pub total_count: usize,
    /// Number of completed effects.
    pub completed_count: usize,
    /// Number of pending (uncompleted) effects.
    pub pending_count: usize,
}

impl RecoveryStatus {
    /// Whether the ledger is clean (no pending effects).
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.pending_count == 0
    }
}

/// Ledger that tracks side effects for crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SideEffectLedger {
    effects: Vec<SideEffect>,
}

impl SideEffectLedger {
    /// Create a new empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the ledger is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Number of tracked effects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Record a new side effect.
    ///
    /// Returns the unique ID assigned to this effect.
    pub fn record(&mut self, mut effect: SideEffect) -> String {
        let id = format!("se-{}", self.effects.len());
        effect.id = id.clone();
        effect.timestamp = now_millis();
        self.effects.push(effect);
        id
    }

    /// Mark an effect as completed by ID.
    pub fn mark_completed(&mut self, id: &str) {
        if let Some(effect) = self.effects.iter_mut().find(|e| e.id == id) {
            effect.complete();
        }
    }

    /// Whether an effect has been completed.
    #[must_use]
    pub fn is_completed(&self, id: &str) -> bool {
        self.effects
            .iter()
            .find(|e| e.id == id)
            .is_some_and(|e| e.is_complete())
    }

    /// Get a reference to an effect by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SideEffect> {
        self.effects.iter().find(|e| e.id == id)
    }

    /// Iterator over pending (uncompleted) effects.
    pub fn pending_effects(&self) -> impl Iterator<Item = &SideEffect> {
        self.effects.iter().filter(|e| !e.is_complete())
    }

    /// Get all reversible effects.
    #[must_use]
    pub fn reversible_effects(&self) -> Vec<&SideEffect> {
        self.effects.iter().filter(|e| e.is_reversible).collect()
    }

    /// Get all irreversible effects.
    #[must_use]
    pub fn irreversible_effects(&self) -> Vec<&SideEffect> {
        self.effects.iter().filter(|e| !e.is_reversible).collect()
    }

    /// Get effects filtered by type.
    #[must_use]
    pub fn effects_by_type(&self, effect_type: SideEffectType) -> Vec<&SideEffect> {
        self.effects
            .iter()
            .filter(|e| e.side_effect_type == effect_type)
            .collect()
    }

    /// Remove all completed effects from the ledger.
    pub fn clear_completed(&mut self) {
        self.effects.retain(|e| !e.is_complete());
    }

    /// Get the recovery status summary.
    #[must_use]
    pub fn recovery_check(&self) -> RecoveryStatus {
        let completed_count = self.effects.iter().filter(|e| e.is_complete()).count();
        RecoveryStatus {
            total_count: self.effects.len(),
            completed_count,
            pending_count: self.effects.len() - completed_count,
        }
    }

    /// Save the ledger to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).with_context(|| {
            format!(
                "Failed to serialize side effect ledger to {}",
                path.display()
            )
        })?;

        // Atomic write
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json).with_context(|| {
            format!(
                "Failed to write side effect ledger to {}",
                tmp_path.display()
            )
        })?;
        if let Err(e) = std::fs::rename(&tmp_path, path) {
            if let Err(cleanup_err) = std::fs::remove_file(&tmp_path) {
                tracing::warn!("failed to clean up temp file {}: {cleanup_err}", tmp_path.display());
            }
            return Err(e).with_context(|| {
                format!("Failed to rename side effect ledger to {}", path.display())
            });
        }

        Ok(())
    }

    /// Load a ledger from a JSON file.
    ///
    /// Returns an empty ledger if the file does not exist.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(path).with_context(|| {
            format!("Failed to read side effect ledger from {}", path.display())
        })?;

        if content.trim().is_empty() {
            return Ok(Self::new());
        }

        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse side effect ledger from {}", path.display()))
    }
}

/// Get current time as milliseconds since UNIX epoch.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ledger_is_empty() {
        let ledger = SideEffectLedger::new();
        assert!(ledger.is_empty());
        assert_eq!(ledger.len(), 0);
    }

    #[test]
    fn record_side_effect() {
        let mut ledger = SideEffectLedger::new();
        let id = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "write_file".to_string(),
            target: "/tmp/test.rs".to_string(),
            description: "Created test module".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });
        assert!(!id.is_empty());
        assert_eq!(ledger.len(), 1);
    }

    #[test]
    fn check_effect_completed() {
        let mut ledger = SideEffectLedger::new();
        let id = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "write_file".to_string(),
            target: "/tmp/test.rs".to_string(),
            description: "Created test module".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });
        assert!(!ledger.is_completed(&id));

        ledger.mark_completed(&id);
        assert!(ledger.is_completed(&id));
    }

    #[test]
    fn skip_completed_side_effects() {
        let mut ledger = SideEffectLedger::new();
        let id1 = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "write_file".to_string(),
            target: "a.rs".to_string(),
            description: "Write A".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });
        let id2 = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "write_file".to_string(),
            target: "b.rs".to_string(),
            description: "Write B".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });
        let id3 = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "bash".to_string(),
            target: "cargo test".to_string(),
            description: "Run tests".to_string(),
            side_effect_type: SideEffectType::CommandExecution,
            is_reversible: false,
            timestamp: 0,
            completed_at: None,
        });

        ledger.mark_completed(&id1);
        ledger.mark_completed(&id3);

        let pending: Vec<_> = ledger.pending_effects().collect();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id2);
    }

    #[test]
    fn rollback_reversible_effects() {
        let mut ledger = SideEffectLedger::new();
        let id1 = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "write_file".to_string(),
            target: "new_file.rs".to_string(),
            description: "Created new file".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });
        let id2 = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "bash".to_string(),
            target: "cargo build".to_string(),
            description: "Built project".to_string(),
            side_effect_type: SideEffectType::CommandExecution,
            is_reversible: false,
            timestamp: 0,
            completed_at: None,
        });

        let reversible = ledger.reversible_effects();
        assert_eq!(reversible.len(), 1);
        assert_eq!(reversible[0].id, id1);

        let irreversible = ledger.irreversible_effects();
        assert_eq!(irreversible.len(), 1);
        assert_eq!(irreversible[0].id, id2);
    }

    #[test]
    fn ledger_serde_roundtrip() {
        let mut ledger = SideEffectLedger::new();
        let id = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "edit_file".to_string(),
            target: "src/main.rs".to_string(),
            description: "Fixed bug".to_string(),
            side_effect_type: SideEffectType::FileEdit,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });
        ledger.mark_completed(&id);

        let json = serde_json::to_string(&ledger).unwrap();
        let decoded: SideEffectLedger = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.len(), 1);
        assert!(decoded.is_completed(&id));
    }

    #[test]
    fn side_effect_type_display() {
        assert_eq!(SideEffectType::FileWrite.to_string(), "file_write");
        assert_eq!(SideEffectType::FileEdit.to_string(), "file_edit");
        assert_eq!(SideEffectType::FileDelete.to_string(), "file_delete");
        assert_eq!(
            SideEffectType::CommandExecution.to_string(),
            "command_execution"
        );
        assert_eq!(
            SideEffectType::DatabaseChange.to_string(),
            "database_change"
        );
        assert_eq!(SideEffectType::NetworkCall.to_string(), "network_call");
    }

    #[test]
    fn clear_completed_effects() {
        let mut ledger = SideEffectLedger::new();
        let id1 = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "write_file".to_string(),
            target: "a.rs".to_string(),
            description: "Write A".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });
        let id2 = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "write_file".to_string(),
            target: "b.rs".to_string(),
            description: "Write B".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });

        ledger.mark_completed(&id1);
        ledger.clear_completed();

        assert_eq!(ledger.len(), 1);
        assert!(!ledger.is_completed(&id2));
    }

    #[test]
    fn recovery_check_returns_uncompleted() {
        let mut ledger = SideEffectLedger::new();
        let id1 = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "write_file".to_string(),
            target: "a.rs".to_string(),
            description: "Write A".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });
        let _id2 = ledger.record(SideEffect {
            id: String::new(),
            tool_name: "write_file".to_string(),
            target: "b.rs".to_string(),
            description: "Write B".to_string(),
            side_effect_type: SideEffectType::FileWrite,
            is_reversible: true,
            timestamp: 0,
            completed_at: None,
        });
        ledger.mark_completed(&id1);

        let recovery = ledger.recovery_check();
        assert_eq!(recovery.pending_count, 1);
        assert_eq!(recovery.completed_count, 1);
        assert_eq!(recovery.total_count, 2);
    }
}
