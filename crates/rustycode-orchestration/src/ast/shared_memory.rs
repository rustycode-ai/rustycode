//! Shared memory interface for cross-agent data sharing (Gap 3).
//!
//! Provides an `AgentMemory` trait that abstracts read/write access to artifacts,
//! decisions, evidence, and open questions. Two implementations are provided:
//!
//! - `LedgerMemory`: Backed by `LedgerData` (in-memory struct).
//! - `ProgressStoreMemory`: Backed by `ProgressStore` (`SQLite`).

use std::sync::Mutex;

use super::ledger::{Decision, LedgerData};
use super::progress_store::{ArtifactRecord, ProgressStore};

/// Cross-agent data sharing via artifacts, decisions, evidence, and open questions.
pub trait AgentMemory: Send + Sync {
    fn read_artifact(&self, artifact_id: &str) -> Option<String>;
    fn write_artifact(&mut self, kind: &str, content: &str) -> String;
    fn read_decisions(&self) -> Vec<Decision>;
    fn read_evidence(&self, milestone_id: usize) -> Vec<String>;
    fn read_open_questions(&self) -> Vec<String>;
}

/// In-memory `AgentMemory` backed by `LedgerData`.
pub struct LedgerMemory {
    data: LedgerData,
}

impl LedgerMemory {
    pub const fn new(data: LedgerData) -> Self {
        Self { data }
    }

    pub const fn data(&self) -> &LedgerData {
        &self.data
    }

    pub const fn data_mut(&mut self) -> &mut LedgerData {
        &mut self.data
    }
}

impl AgentMemory for LedgerMemory {
    fn read_artifact(&self, artifact_id: &str) -> Option<String> {
        self.data
            .subagent_findings
            .iter()
            .find(|f| f.id == artifact_id)
            .map(|f| f.summary.clone())
            .or_else(|| {
                self.data
                    .decisions
                    .iter()
                    .find(|d| d.id == artifact_id)
                    .map(|d| d.decision.clone())
            })
    }

    fn write_artifact(&mut self, kind: &str, content: &str) -> String {
        let id = format!("art-{}", uuid::Uuid::new_v4());
        self.data
            .subagent_findings
            .push(super::ledger::SubagentFinding {
                id: id.clone(),
                role: kind.into(),
                summary: content.into(),
                evidence: vec![],
            });
        id
    }

    fn read_decisions(&self) -> Vec<Decision> {
        self.data.decisions.clone()
    }

    fn read_evidence(&self, milestone_id: usize) -> Vec<String> {
        self.data
            .milestones
            .get(milestone_id)
            .map(|m| m.evidence.clone())
            .unwrap_or_default()
    }

    fn read_open_questions(&self) -> Vec<String> {
        self.data
            .open_questions
            .iter()
            .filter(|q| !q.resolved)
            .map(|q| q.question.clone())
            .collect()
    }
}

/// SQLite-backed `AgentMemory` using `ProgressStore`.
pub struct ProgressStoreMemory {
    store: Mutex<ProgressStore>,
    task_id: String,
}

impl ProgressStoreMemory {
    pub fn new(store: ProgressStore, task_id: impl Into<String>) -> Self {
        Self {
            store: Mutex::new(store),
            task_id: task_id.into(),
        }
    }
}

impl AgentMemory for ProgressStoreMemory {
    fn read_artifact(&self, artifact_id: &str) -> Option<String> {
        let store = self.store.lock().unwrap_or_else(|e| {
            tracing::error!("shared_memory store lock poisoned: {e}");
            e.into_inner()
        });
        let kinds = ["decision", "finding", "artifact", "note"];
        for kind in kinds {
            if let Ok(artifacts) = store.get_artifacts_by_kind(&self.task_id, kind) {
                if let Some(found) = artifacts.iter().find(|a| a.id == artifact_id) {
                    return found.summary.clone();
                }
            }
        }
        None
    }

    fn write_artifact(&mut self, kind: &str, content: &str) -> String {
        let id = format!("art-{}", uuid::Uuid::new_v4());
        let artifact = ArtifactRecord {
            id: id.clone(),
            task_id: self.task_id.clone(),
            kind: kind.into(),
            path: None,
            content_hash: None,
            summary: Some(content.into()),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let store = self.store.lock().unwrap_or_else(|e| {
            tracing::error!("shared_memory store lock poisoned: {e}");
            e.into_inner()
        });
        if let Err(e) = store.store_artifact(&artifact) {
            tracing::warn!(error = %e, artifact_id = %id, "Failed to store shared memory artifact");
        }
        id
    }

    fn read_decisions(&self) -> Vec<Decision> {
        vec![]
    }

    fn read_evidence(&self, _milestone_id: usize) -> Vec<String> {
        vec![]
    }

    fn read_open_questions(&self) -> Vec<String> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ledger::{LedgerData, MilestoneEntry, MilestoneStatus, OpenQuestion};
    use crate::ast::types::{AstPhase, AstSnapshot};
    use std::collections::HashMap;

    fn empty_snapshot() -> AstSnapshot {
        AstSnapshot {
            current_phase: AstPhase::Classify,
            assessment: None,
            brief: None,
            skeleton: None,
            active_segments: vec![],
            completed_milestones: vec![],
            evidence: HashMap::new(),
            recovery_attempts: HashMap::new(),
            consultant_escalation: vec![],
            failed_milestones: vec![],
            report: None,
        }
    }

    fn ledger_data_with_content() -> LedgerData {
        let mut data = LedgerData::from_snapshot(empty_snapshot(), "Test Task");

        data.milestones.push(MilestoneEntry {
            id: "M1".into(),
            goal: "Implement feature".into(),
            status: MilestoneStatus::Done,
            depends_on: vec![],
            deliverable: "working code".into(),
            evidence: vec!["test_pass.txt".into(), "coverage_report.html".into()],
        });

        data.decisions.push(Decision {
            id: "D1".into(),
            decision: "Use incremental approach".into(),
            reason: "Lower risk".into(),
            alternatives: vec!["Big bang".into()],
            evidence: vec!["team_experience".into()],
        });

        data.open_questions.push(OpenQuestion {
            id: "Q1".into(),
            question: "Should we use Redis?".into(),
            resolved: false,
            resolution: None,
        });
        data.open_questions.push(OpenQuestion {
            id: "Q2".into(),
            question: "Token expiry?".into(),
            resolved: true,
            resolution: Some("15 min".into()),
        });

        data
    }

    // -- LedgerMemory tests

    #[test]
    fn ledger_memory_write_and_read_artifact() {
        let data = LedgerData::from_snapshot(empty_snapshot(), "Test");
        let mut mem = LedgerMemory::new(data);

        let id = mem.write_artifact("finding", "Found 3 relevant files");
        let content = mem.read_artifact(&id);
        assert_eq!(content, Some("Found 3 relevant files".into()));
    }

    #[test]
    fn ledger_memory_read_artifact_not_found() {
        let data = LedgerData::from_snapshot(empty_snapshot(), "Test");
        let mem = LedgerMemory::new(data);
        assert_eq!(mem.read_artifact("nonexistent"), None);
    }

    #[test]
    fn ledger_memory_read_decisions() {
        let data = ledger_data_with_content();
        let mem = LedgerMemory::new(data);

        let decisions = mem.read_decisions();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].id, "D1");
        assert_eq!(decisions[0].decision, "Use incremental approach");
    }

    #[test]
    fn ledger_memory_read_evidence() {
        let data = ledger_data_with_content();
        let mem = LedgerMemory::new(data);

        let evidence = mem.read_evidence(0);
        assert_eq!(evidence.len(), 2);
        assert!(evidence.contains(&"test_pass.txt".into()));
        assert!(evidence.contains(&"coverage_report.html".into()));
    }

    #[test]
    fn ledger_memory_read_evidence_out_of_bounds() {
        let data = ledger_data_with_content();
        let mem = LedgerMemory::new(data);

        let evidence = mem.read_evidence(99);
        assert!(evidence.is_empty());
    }

    #[test]
    fn ledger_memory_read_open_questions_unresolved_only() {
        let data = ledger_data_with_content();
        let mem = LedgerMemory::new(data);

        let questions = mem.read_open_questions();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0], "Should we use Redis?");
    }

    #[test]
    fn ledger_memory_write_multiple_artifacts() {
        let data = LedgerData::from_snapshot(empty_snapshot(), "Test");
        let mut mem = LedgerMemory::new(data);

        let id1 = mem.write_artifact("finding", "First finding");
        let id2 = mem.write_artifact("note", "Second note");
        assert_ne!(id1, id2, "Each artifact should get a unique ID");

        assert_eq!(mem.read_artifact(&id1), Some("First finding".into()));
        assert_eq!(mem.read_artifact(&id2), Some("Second note".into()));
    }

    // -- ProgressStoreMemory tests

    #[test]
    fn progress_store_memory_write_and_read_artifact() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task_id = uuid::Uuid::new_v4().to_string();

        store
            .create_task(&super::super::progress_store::TaskRecord {
                id: task_id.clone(),
                title: "Test".into(),
                complexity: "Moderate".into(),
                goal: "Test goal".into(),
                current_phase: "CLASSIFY".into(),
                status: "active".into(),
                ledger_path: "/tmp/test.md".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
            .unwrap();

        let mut mem = ProgressStoreMemory::new(store, &task_id);
        let id = mem.write_artifact("finding", "Found relevant files");

        let content = mem.read_artifact(&id);
        assert_eq!(content, Some("Found relevant files".into()));
    }

    #[test]
    fn progress_store_memory_read_artifact_not_found() {
        let store = ProgressStore::open_in_memory().unwrap();
        let mem = ProgressStoreMemory::new(store, "nonexistent-task");
        assert_eq!(mem.read_artifact("nonexistent"), None);
    }

    #[test]
    fn progress_store_memory_read_decisions_returns_empty() {
        let store = ProgressStore::open_in_memory().unwrap();
        let mem = ProgressStoreMemory::new(store, "task-1");
        assert!(mem.read_decisions().is_empty());
    }

    #[test]
    fn progress_store_memory_read_evidence_returns_empty() {
        let store = ProgressStore::open_in_memory().unwrap();
        let mem = ProgressStoreMemory::new(store, "task-1");
        assert!(mem.read_evidence(0).is_empty());
    }

    #[test]
    fn progress_store_memory_read_open_questions_returns_empty() {
        let store = ProgressStore::open_in_memory().unwrap();
        let mem = ProgressStoreMemory::new(store, "task-1");
        assert!(mem.read_open_questions().is_empty());
    }
}
