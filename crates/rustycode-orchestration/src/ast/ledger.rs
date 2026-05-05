//! Markdown task ledger for AST (spec section 10.1).
//!
//! The ledger is the human-readable source of truth for task state.
//! It survives context compaction and can be shared across agents.

use std::fmt::Write as FmtWrite;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::tree::AstTree;
use super::types::{AstPhase, AstSnapshot, ContextBrief};

// Milestone status state machine

/// Lifecycle status for a single milestone.
///
/// Valid transitions:
///   Pending -> Active | Dropped
///   Active  -> Done   | Blocked | Dropped
///   Blocked -> Active | Dropped
///   Done    -> (terminal)
///   Dropped -> (terminal)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MilestoneStatus {
    Pending,
    Active,
    Blocked,
    Done,
    Dropped,
}

impl MilestoneStatus {
    /// Markdown label used in the rendered ledger.
    const fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Dropped => "dropped",
        }
    }

    /// Returns `true` when the status represents a terminal state.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Dropped)
    }
}

impl std::fmt::Display for MilestoneStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// New section types (spec 10.1)

/// A milestone entry with full status tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneEntry {
    pub id: String,
    pub goal: String,
    pub status: MilestoneStatus,
    pub depends_on: Vec<String>,
    pub deliverable: String,
    pub evidence: Vec<String>,
}

/// A recorded design or implementation decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub decision: String,
    pub reason: String,
    pub alternatives: Vec<String>,
    pub evidence: Vec<String>,
}

/// A finding from a sub-agent (scout, reviewer, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentFinding {
    pub id: String,
    pub role: String,
    pub summary: String,
    pub evidence: Vec<String>,
}

/// An open (or resolved) question about the task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenQuestion {
    pub id: String,
    pub question: String,
    pub resolved: bool,
    pub resolution: Option<String>,
}

/// A phase-transition or significant event appended to the ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEvent {
    pub id: String,
    pub phase: String,
    pub event_type: String,
    pub summary: String,
    pub timestamp: String,
}

/// An execution segment entry with segment-level metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSegmentEntry {
    pub segment_id: String,
    pub milestone_ids: Vec<String>,
    pub owner: String,
    pub steps: Vec<String>,
    pub verification_commands: Vec<String>,
}

/// A verification criterion result in the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CriterionStatus {
    Pass,
    Fail,
    Unknown,
}

impl std::fmt::Display for CriterionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS"),
            Self::Fail => write!(f, "FAIL"),
            Self::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// A verification criterion for the ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCriterion {
    pub description: String,
    pub status: CriterionStatus,
}

/// A next action item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextAction {
    pub description: String,
}

// LedgerData -- full structured data for the spec 10.1 template

/// Complete data bag for rendering a spec 10.1 task ledger.
///
/// Wraps the existing `AstSnapshot` and adds the new sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerData {
    pub snapshot: AstSnapshot,
    pub title: String,
    pub goal: String,
    pub success_criteria: Vec<String>,
    pub context_brief: Option<ContextBrief>,
    pub milestones: Vec<MilestoneEntry>,
    pub active_segments: Vec<ExecutionSegmentEntry>,
    pub decisions: Vec<Decision>,
    pub subagent_findings: Vec<SubagentFinding>,
    pub open_questions: Vec<OpenQuestion>,
    pub verification: Vec<VerificationCriterion>,
    pub events: Vec<LedgerEvent>,
    pub next_actions: Vec<NextAction>,
}

impl LedgerData {
    /// Construct a minimal `LedgerData` from just a snapshot and a title.
    ///
    /// The new sections start empty so callers can progressively populate them.
    pub fn from_snapshot(snapshot: AstSnapshot, title: impl Into<String>) -> Self {
        let title = title.into();
        let goal = snapshot
            .assessment
            .as_ref()
            .map(|a| a.task_summary.clone())
            .unwrap_or_default();
        let success_criteria = snapshot
            .assessment
            .as_ref()
            .map(|a| {
                a.success_criteria
                    .iter()
                    .map(|c| {
                        c.verification_command.as_ref().map_or_else(
                            || c.description.clone(),
                            |cmd| format!("{} -- `{}`", c.description, cmd),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let context_brief = snapshot.brief.clone();

        // Convert legacy skeleton milestones into enriched MilestoneEntry list.
        let milestones = snapshot
            .skeleton
            .as_ref()
            .map(|s| {
                s.milestones
                    .iter()
                    .map(|m| MilestoneEntry {
                        id: format!("M{}", m.id),
                        goal: m.description.clone(),
                        status: if snapshot.completed_milestones.contains(&m.id) {
                            MilestoneStatus::Done
                        } else {
                            MilestoneStatus::Pending
                        },
                        depends_on: m.depends_on.iter().map(|d| format!("M{d}")).collect(),
                        deliverable: m.deliverable.clone(),
                        evidence: vec![],
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Convert active segments to the richer entry type.
        let active_segments = snapshot
            .active_segments
            .iter()
            .enumerate()
            .map(|(i, seg)| ExecutionSegmentEntry {
                segment_id: format!("S{}", i + 1),
                milestone_ids: vec![format!("M{}", seg.milestone_id)],
                owner: "builder".to_string(),
                steps: seg.steps.iter().map(|s| s.action.clone()).collect(),
                verification_commands: seg
                    .steps
                    .iter()
                    .filter_map(|s| s.verification_command.clone())
                    .collect(),
            })
            .collect();

        // Map verification report to criterion list.
        let verification = snapshot
            .report
            .as_ref()
            .map(|r| {
                r.results
                    .iter()
                    .map(|cr| VerificationCriterion {
                        description: cr.criterion.description.clone(),
                        status: match cr.status {
                            super::types::VerificationStatus::Pass => CriterionStatus::Pass,
                            super::types::VerificationStatus::Partial
                            | super::types::VerificationStatus::Fail => CriterionStatus::Fail,
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            snapshot,
            title,
            goal,
            success_criteria,
            context_brief,
            milestones,
            active_segments,
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification,
            events: vec![],
            next_actions: vec![],
        }
    }

    /// Validate milestone state machine invariants.
    ///
    /// Returns `Ok(())` when valid, or a descriptive error listing violations.
    pub fn validate_milestones(&self) -> std::result::Result<(), String> {
        let mut errors: Vec<String> = vec![];

        // Collect active milestone ids and check dependency graph for parallelism proof.
        let active_ids: Vec<&str> = self
            .milestones
            .iter()
            .filter(|m| m.status == MilestoneStatus::Active)
            .map(|m| m.id.as_str())
            .collect();

        if active_ids.len() > 1 {
            // Check if the active milestones are all independent (no dependency between them).
            let active_set: std::collections::HashSet<&str> = active_ids.iter().copied().collect();
            let mut parallel = true;
            for m in &self.milestones {
                if active_set.contains(m.id.as_str()) {
                    for dep in &m.depends_on {
                        if active_set.contains(dep.as_str()) {
                            parallel = false;
                            errors.push(format!(
                                "Active milestone {} depends on another active milestone {}",
                                m.id, dep
                            ));
                        }
                    }
                }
            }
            if !parallel {
                errors.insert(
                    0,
                    format!(
                        "Multiple active milestones without proven parallelism: {active_ids:?}"
                    ),
                );
            }
        }

        // Check that depended-on milestones exist.
        let all_ids: std::collections::HashSet<&str> =
            self.milestones.iter().map(|m| m.id.as_str()).collect();
        for m in &self.milestones {
            for dep in &m.depends_on {
                if !all_ids.contains(dep.as_str()) {
                    errors.push(format!(
                        "Milestone {} depends on non-existent milestone {}",
                        m.id, dep
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    /// Append a phase-transition event.
    pub fn log_event(
        &mut self,
        from_phase: AstPhase,
        to_phase: AstPhase,
        summary: impl Into<String>,
    ) {
        self.events.push(LedgerEvent {
            id: uuid::Uuid::new_v4().to_string(),
            phase: to_phase.to_string(),
            event_type: "PHASE_TRANSITION".to_string(),
            summary: format!("{} -> {}: {}", from_phase, to_phase, summary.into()),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// Append a generic event.
    pub fn append_event(
        &mut self,
        phase: impl Into<String>,
        event_type: impl Into<String>,
        summary: impl Into<String>,
    ) {
        self.events.push(LedgerEvent {
            id: uuid::Uuid::new_v4().to_string(),
            phase: phase.into(),
            event_type: event_type.into(),
            summary: summary.into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }
}

// TaskLedger -- markdown renderer

/// Writes and reads the task ledger as markdown (spec 10.1 format).
pub struct TaskLedger;

impl TaskLedger {
    /// Render the full spec 10.1 ledger from `LedgerData`.
    #[allow(clippy::too_many_lines)]
    pub fn render(data: &LedgerData) -> String {
        let mut md = String::with_capacity(4096);

        // -- Title --
        let _ = writeln!(md, "# Task: {}", data.title);

        // -- Assessment --
        let _ = writeln!(md);
        let _ = writeln!(md, "## Assessment");
        let complexity = data.snapshot.assessment.as_ref().map_or_else(
            || "UNKNOWN".to_string(),
            |a| format!("{:?}", a.complexity).to_uppercase(),
        );
        let _ = writeln!(md, "- Complexity: {complexity}");
        let _ = writeln!(md, "- Goal: {}", data.goal);
        let _ = writeln!(md, "- Success criteria:");
        for c in &data.success_criteria {
            let _ = writeln!(md, "  - {c}");
        }
        let _ = writeln!(md, "- Current phase: {}", data.snapshot.current_phase);

        // -- Tree View --
        let tree = AstTree::from_snapshot(&data.snapshot);
        let _ = writeln!(md);
        let _ = writeln!(md, "## Tree View");
        let _ = write!(md, "{}", tree.render_markdown());

        // -- Context Brief --
        if let Some(brief) = &data.context_brief {
            let _ = writeln!(md);
            let _ = writeln!(md, "## Context Brief");
            let files: Vec<String> = brief
                .relevant_files
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            let _ = writeln!(md, "- Relevant files: {}", files.join(", "));
            let _ = writeln!(
                md,
                "- Patterns: {}",
                if brief.patterns_found.is_empty() {
                    "(none)".to_string()
                } else {
                    brief.patterns_found.join(", ")
                }
            );
            let _ = writeln!(
                md,
                "- Constraints: {}",
                if brief.constraints.is_empty() {
                    "(none)".to_string()
                } else {
                    brief.constraints.join(", ")
                }
            );
            let _ = writeln!(
                md,
                "- Risks: {}",
                if brief.risks.is_empty() {
                    "(none)".to_string()
                } else {
                    brief.risks.join(", ")
                }
            );
        }

        // -- Milestones --
        if !data.milestones.is_empty() {
            let _ = writeln!(md);
            let _ = writeln!(md, "## Milestones");
            for m in &data.milestones {
                let status = m.status.label();
                let deps = if m.depends_on.is_empty() {
                    "[]".to_string()
                } else {
                    format!("[{}]", m.depends_on.join(", "))
                };
                let evidence = if m.evidence.is_empty() {
                    "[]".to_string()
                } else {
                    format!(
                        "[{}]",
                        m.evidence
                            .iter()
                            .map(|e| format!("\"{e}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                let _ = writeln!(md, "- [{}] {}: {}", status, m.id, m.goal);
                let _ = writeln!(md, "  - Status: {status}");
                let _ = writeln!(md, "  - Depends on: {deps}");
                let _ = writeln!(md, "  - Deliverable: {}", m.deliverable);
                let _ = writeln!(md, "  - Evidence: {evidence}");
            }
        }

        // -- Active Execution Segment --
        if !data.active_segments.is_empty() {
            let _ = writeln!(md);
            let _ = writeln!(md, "## Active Execution Segment");
            for seg in &data.active_segments {
                let _ = writeln!(md, "- Segment id: {}", seg.segment_id);
                let _ = writeln!(md, "  - Milestones: [{}]", seg.milestone_ids.join(", "));
                let _ = writeln!(md, "  - Owner: {}", seg.owner);
                let _ = writeln!(md, "  - Steps:");
                for step in &seg.steps {
                    let _ = writeln!(md, "    - {step}");
                }
                if !seg.verification_commands.is_empty() {
                    let _ = writeln!(md, "  - Verification commands:");
                    for cmd in &seg.verification_commands {
                        let _ = writeln!(md, "    - `{cmd}`");
                    }
                }
            }
        }

        // -- Decisions --
        if !data.decisions.is_empty() {
            let _ = writeln!(md);
            let _ = writeln!(md, "## Decisions");
            for d in &data.decisions {
                let _ = writeln!(md, "- {}: {}", d.id, d.decision);
                let _ = writeln!(md, "  - Reason: {}", d.reason);
                let _ = writeln!(
                    md,
                    "  - Alternatives: {}",
                    if d.alternatives.is_empty() {
                        "(none)".to_string()
                    } else {
                        d.alternatives.join(", ")
                    }
                );
                let _ = writeln!(
                    md,
                    "  - Evidence: {}",
                    if d.evidence.is_empty() {
                        "(none)".to_string()
                    } else {
                        d.evidence.join(", ")
                    }
                );
            }
        }

        // -- Open Questions --
        if !data.open_questions.is_empty() {
            let _ = writeln!(md);
            let _ = writeln!(md, "## Open Questions");
            for q in &data.open_questions {
                if q.resolved {
                    let resolution = q.resolution.as_deref().unwrap_or("(no details)");
                    let _ = writeln!(md, "- {}: {} [RESOLVED: {}]", q.id, q.question, resolution);
                } else {
                    let _ = writeln!(md, "- {}: {} [UNRESOLVED]", q.id, q.question);
                }
            }
        }

        // -- Subagent Findings --
        if !data.subagent_findings.is_empty() {
            let _ = writeln!(md);
            let _ = writeln!(md, "## Subagent Findings");
            for f in &data.subagent_findings {
                let _ = writeln!(md, "- {}: {}", f.id, f.role);
                let _ = writeln!(md, "  - Summary: {}", f.summary);
                let _ = writeln!(
                    md,
                    "  - Evidence: {}",
                    if f.evidence.is_empty() {
                        "(none)".to_string()
                    } else {
                        format!(
                            "[{}]",
                            f.evidence
                                .iter()
                                .map(|e| format!("\"{e}\""))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                );
            }
        }

        // -- Verification --
        if !data.verification.is_empty() {
            let _ = writeln!(md);
            let _ = writeln!(md, "## Verification");
            for v in &data.verification {
                let _ = writeln!(md, "- {}: {}", v.description, v.status);
            }
        }

        // -- Events --
        if !data.events.is_empty() {
            let _ = writeln!(md);
            let _ = writeln!(md, "## Events");
            for e in &data.events {
                let _ = writeln!(md, "- [{}] {}: {}", e.timestamp, e.phase, e.summary);
            }
        }

        // -- Next Actions --
        if !data.next_actions.is_empty() {
            let _ = writeln!(md);
            let _ = writeln!(md, "## Next Actions");
            for (i, action) in data.next_actions.iter().enumerate() {
                let _ = writeln!(md, "{}. {}", i + 1, action.description);
            }
        }

        md
    }

    /// Backward-compatible render from a bare `AstSnapshot`.
    ///
    /// This wraps the snapshot in a `LedgerData` and delegates to [`render`].
    pub fn render_snapshot(snapshot: &AstSnapshot) -> String {
        let data = LedgerData::from_snapshot(snapshot.clone(), "Untitled Task");
        Self::render(&data)
    }

    /// Write the full ledger to a file.
    pub fn write_to_file(path: &Path, data: &LedgerData) -> Result<()> {
        let content = Self::render(data);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create parent dir {}", parent.display()))?;
            }
        }
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write ledger to {}", path.display()))?;
        Ok(())
    }

    /// Backward-compatible write from a bare `AstSnapshot`.
    pub fn write_snapshot_to_file(path: &Path, snapshot: &AstSnapshot) -> Result<()> {
        let data = LedgerData::from_snapshot(snapshot.clone(), "Untitled Task");
        Self::write_to_file(path, &data)
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        ComplexityLevel, CriterionResult, Milestone, MilestoneSkeleton, PhaseRoute,
        SuccessCriterion, TaskAssessment, VerificationReport, VerificationStatus,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;

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

    fn snapshot_with_assessment() -> AstSnapshot {
        let mut snap = empty_snapshot();
        snap.assessment = Some(TaskAssessment {
            task_summary: "Fix auth bug".into(),
            complexity: ComplexityLevel::Moderate,
            success_criteria: vec![SuccessCriterion {
                description: "Tests pass".into(),
                verification_command: Some("cargo test".into()),
            }],
            route: PhaseRoute::StandardSequence,

            clarity: None,
        });
        snap
    }

    // -- Backward compat tests --

    #[test]
    fn render_snapshot_empty() {
        let md = TaskLedger::render_snapshot(&empty_snapshot());
        assert!(md.contains("# Task: Untitled Task"));
        assert!(md.contains("CLASSIFY"));
    }

    #[test]
    fn render_snapshot_with_assessment() {
        let md = TaskLedger::render_snapshot(&snapshot_with_assessment());
        assert!(md.contains("Fix auth bug"));
        assert!(md.contains("MODERATE"));
        assert!(md.contains("cargo test"));
    }

    #[test]
    fn render_snapshot_with_skeleton() {
        let mut snap = empty_snapshot();
        snap.skeleton = Some(MilestoneSkeleton {
            milestones: vec![
                Milestone {
                    id: 0,
                    description: "Setup".into(),
                    deliverable: "module stubs".into(),
                    depends_on: vec![],
                },
                Milestone {
                    id: 1,
                    description: "Implement".into(),
                    deliverable: "working code".into(),
                    depends_on: vec![0],
                },
            ],
        });
        snap.completed_milestones = vec![0];
        let md = TaskLedger::render_snapshot(&snap);
        assert!(md.contains("M0: Setup"));
        assert!(md.contains("done"));
        assert!(md.contains("pending"));
    }

    #[test]
    fn render_snapshot_with_report() {
        let mut snap = empty_snapshot();
        snap.report = Some(VerificationReport {
            results: vec![CriterionResult {
                criterion: SuccessCriterion {
                    description: "Build passes".into(),
                    verification_command: None,
                },
                status: VerificationStatus::Pass,
                evidence: "exit 0".into(),
            }],
            overall: VerificationStatus::Pass,
        });
        let md = TaskLedger::render_snapshot(&snap);
        assert!(md.contains("PASS"));
        assert!(md.contains("Build passes"));
    }

    #[test]
    fn write_snapshot_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.md");
        let snap = empty_snapshot();
        TaskLedger::write_snapshot_to_file(&path, &snap).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Task: Untitled Task"));
    }

    // -- MilestoneStatus tests --

    #[test]
    fn milestone_status_labels() {
        assert_eq!(MilestoneStatus::Pending.label(), "pending");
        assert_eq!(MilestoneStatus::Active.label(), "active");
        assert_eq!(MilestoneStatus::Blocked.label(), "blocked");
        assert_eq!(MilestoneStatus::Done.label(), "done");
        assert_eq!(MilestoneStatus::Dropped.label(), "dropped");
    }

    #[test]
    fn milestone_status_terminal() {
        assert!(!MilestoneStatus::Pending.is_terminal());
        assert!(!MilestoneStatus::Active.is_terminal());
        assert!(!MilestoneStatus::Blocked.is_terminal());
        assert!(MilestoneStatus::Done.is_terminal());
        assert!(MilestoneStatus::Dropped.is_terminal());
    }

    #[test]
    fn milestone_status_display() {
        assert_eq!(MilestoneStatus::Active.to_string(), "active");
        assert_eq!(MilestoneStatus::Dropped.to_string(), "dropped");
    }

    // -- Milestone rendering tests --

    #[test]
    fn render_milestones_all_statuses() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![
                MilestoneEntry {
                    id: "M1".into(),
                    goal: "Setup test fixtures".into(),
                    status: MilestoneStatus::Pending,
                    depends_on: vec![],
                    deliverable: "test module stubs".into(),
                    evidence: vec![],
                },
                MilestoneEntry {
                    id: "M2".into(),
                    goal: "Write unit tests".into(),
                    status: MilestoneStatus::Active,
                    depends_on: vec!["M1".into()],
                    deliverable: "passing test suite".into(),
                    evidence: vec![],
                },
                MilestoneEntry {
                    id: "M3".into(),
                    goal: "Verify coverage".into(),
                    status: MilestoneStatus::Done,
                    depends_on: vec!["M2".into()],
                    deliverable: ">=80% coverage report".into(),
                    evidence: vec!["cargo test: exit=0".into()],
                },
                MilestoneEntry {
                    id: "M4".into(),
                    goal: "Blocked task".into(),
                    status: MilestoneStatus::Blocked,
                    depends_on: vec!["M1".into(), "M2".into()],
                    deliverable: "something".into(),
                    evidence: vec![],
                },
                MilestoneEntry {
                    id: "M5".into(),
                    goal: "Dropped task".into(),
                    status: MilestoneStatus::Dropped,
                    depends_on: vec![],
                    deliverable: "nothing".into(),
                    evidence: vec![],
                },
            ],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("[pending] M1: Setup test fixtures"));
        assert!(md.contains("[active] M2: Write unit tests"));
        assert!(md.contains("[done] M3: Verify coverage"));
        assert!(md.contains("[blocked] M4: Blocked task"));
        assert!(md.contains("[dropped] M5: Dropped task"));
        // Check evidence
        assert!(md.contains("\"cargo test: exit=0\""));
        // Check depends
        assert!(md.contains("M1, M2"));
    }

    // -- Decision rendering tests --

    #[test]
    fn render_decisions() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![],
            active_segments: vec![],
            decisions: vec![Decision {
                id: "D1".into(),
                decision: "Use incremental refactoring approach".into(),
                reason: "lower risk than big-bang".into(),
                alternatives: vec!["big-bang rewrite".into(), "strangler fig".into()],
                evidence: vec!["prior team experience with incremental".into()],
            }],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("## Decisions"));
        assert!(md.contains("D1: Use incremental refactoring approach"));
        assert!(md.contains("Reason: lower risk than big-bang"));
        assert!(md.contains("big-bang rewrite, strangler fig"));
        assert!(md.contains("prior team experience with incremental"));
    }

    // -- Subagent findings rendering tests --

    #[test]
    fn render_subagent_findings() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![SubagentFinding {
                id: "A1".into(),
                role: "scout".into(),
                summary: "Found 3 relevant auth files, 2 middleware files".into(),
                evidence: vec![
                    "src/auth/login.rs".into(),
                    "src/auth/token.rs".into(),
                    "src/middleware/auth.rs".into(),
                ],
            }],
            open_questions: vec![],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("## Subagent Findings"));
        assert!(md.contains("A1: scout"));
        assert!(md.contains("Summary: Found 3 relevant auth files, 2 middleware files"));
        assert!(md.contains("\"src/auth/login.rs\""));
        assert!(md.contains("\"src/auth/token.rs\""));
        assert!(md.contains("\"src/middleware/auth.rs\""));
    }

    // -- Open questions rendering tests --

    #[test]
    fn render_open_questions_unresolved() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![OpenQuestion {
                id: "Q1".into(),
                question: "Should we use Redis for session storage?".into(),
                resolved: false,
                resolution: None,
            }],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("## Open Questions"));
        assert!(md.contains("Q1: Should we use Redis for session storage? [UNRESOLVED]"));
    }

    #[test]
    fn render_open_questions_resolved() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![OpenQuestion {
                id: "Q2".into(),
                question: "Token expiry duration?".into(),
                resolved: true,
                resolution: Some("15 minutes".into()),
            }],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("Q2: Token expiry duration? [RESOLVED: 15 minutes]"));
    }

    // -- Event rendering tests --

    #[test]
    fn render_events() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![],
            events: vec![
                LedgerEvent {
                    id: "evt-1".into(),
                    phase: "RESEARCH".into(),
                    event_type: "PHASE_TRANSITION".into(),
                    summary: "CLASSIFY -> RESEARCH: Task classified as MODERATE".into(),
                    timestamp: "2026-04-26T10:00:00+00:00".into(),
                },
                LedgerEvent {
                    id: "evt-2".into(),
                    phase: "SKELETON".into(),
                    event_type: "PHASE_TRANSITION".into(),
                    summary: "RESEARCH -> SKELETON: Research complete, 5 files found".into(),
                    timestamp: "2026-04-26T10:00:05+00:00".into(),
                },
                LedgerEvent {
                    id: "evt-3".into(),
                    phase: "EXPAND".into(),
                    event_type: "PHASE_TRANSITION".into(),
                    summary: "SKELETON -> EXPAND: 3 milestones planned".into(),
                    timestamp: "2026-04-26T10:00:10+00:00".into(),
                },
            ],
            next_actions: vec![],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("## Events"));
        assert!(md.contains("[2026-04-26T10:00:00+00:00] RESEARCH: CLASSIFY -> RESEARCH: Task classified as MODERATE"));
        assert!(md.contains("[2026-04-26T10:00:05+00:00] SKELETON: RESEARCH -> SKELETON: Research complete, 5 files found"));
        assert!(md.contains(
            "[2026-04-26T10:00:10+00:00] EXPAND: SKELETON -> EXPAND: 3 milestones planned"
        ));
    }

    // -- Verification rendering tests --

    #[test]
    fn render_verification() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![
                VerificationCriterion {
                    description: "All tests pass".into(),
                    status: CriterionStatus::Pass,
                },
                VerificationCriterion {
                    description: "No clippy warnings".into(),
                    status: CriterionStatus::Fail,
                },
                VerificationCriterion {
                    description: "Coverage >= 80%".into(),
                    status: CriterionStatus::Unknown,
                },
            ],
            events: vec![],
            next_actions: vec![],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("## Verification"));
        assert!(md.contains("All tests pass: PASS"));
        assert!(md.contains("No clippy warnings: FAIL"));
        assert!(md.contains("Coverage >= 80%: UNKNOWN"));
    }

    // -- Active execution segment rendering --

    #[test]
    fn render_active_segments() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![],
            active_segments: vec![ExecutionSegmentEntry {
                segment_id: "S1".into(),
                milestone_ids: vec!["M1".into(), "M2".into()],
                owner: "builder".into(),
                steps: vec!["Create module stubs".into(), "Implement handler".into()],
                verification_commands: vec!["cargo test".into(), "cargo clippy".into()],
            }],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("## Active Execution Segment"));
        assert!(md.contains("Segment id: S1"));
        assert!(md.contains("M1, M2"));
        assert!(md.contains("Owner: builder"));
        assert!(md.contains("Create module stubs"));
        assert!(md.contains("Implement handler"));
        assert!(md.contains("`cargo test`"));
        assert!(md.contains("`cargo clippy`"));
    }

    // -- Next actions rendering --

    #[test]
    fn render_next_actions() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![],
            events: vec![],
            next_actions: vec![
                NextAction {
                    description: "Complete M2 implementation".into(),
                },
                NextAction {
                    description: "Run verification suite".into(),
                },
            ],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("## Next Actions"));
        assert!(md.contains("1. Complete M2 implementation"));
        assert!(md.contains("2. Run verification suite"));
    }

    // -- Context brief rendering --

    #[test]
    fn render_context_brief() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec![],
            context_brief: Some(ContextBrief {
                relevant_files: vec![PathBuf::from("src/auth.rs"), PathBuf::from("src/mw.rs")],
                patterns_found: vec!["repository pattern".into()],
                dependencies: vec!["serde".into()],
                risks: vec!["breaking change".into()],
                constraints: vec!["must support Rust 2021".into()],
            }),
            milestones: vec![],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        let md = TaskLedger::render(&data);
        assert!(md.contains("## Context Brief"));
        assert!(md.contains("src/auth.rs, src/mw.rs"));
        assert!(md.contains("repository pattern"));
        assert!(md.contains("must support Rust 2021"));
        assert!(md.contains("breaking change"));
    }

    // -- Full ledger template test --

    #[test]
    fn render_full_ledger_matches_spec() {
        let data = LedgerData {
            snapshot: snapshot_with_assessment(),
            title: "Fix authentication token expiry".into(),
            goal: "Fix auth bug".into(),
            success_criteria: vec!["Tests pass -- `cargo test`".into()],
            context_brief: Some(ContextBrief {
                relevant_files: vec![PathBuf::from("src/auth.rs")],
                patterns_found: vec!["token validation".into()],
                dependencies: vec!["chrono".into()],
                risks: vec!["session invalidation".into()],
                constraints: vec!["backward compatible".into()],
            }),
            milestones: vec![
                MilestoneEntry {
                    id: "M1".into(),
                    goal: "Setup test fixtures".into(),
                    status: MilestoneStatus::Done,
                    depends_on: vec![],
                    deliverable: "test module stubs".into(),
                    evidence: vec!["fixtures created".into()],
                },
                MilestoneEntry {
                    id: "M2".into(),
                    goal: "Write unit tests".into(),
                    status: MilestoneStatus::Active,
                    depends_on: vec!["M1".into()],
                    deliverable: "passing test suite".into(),
                    evidence: vec![],
                },
                MilestoneEntry {
                    id: "M3".into(),
                    goal: "Verify coverage".into(),
                    status: MilestoneStatus::Pending,
                    depends_on: vec!["M2".into()],
                    deliverable: ">=80% coverage report".into(),
                    evidence: vec![],
                },
            ],
            active_segments: vec![ExecutionSegmentEntry {
                segment_id: "S1".into(),
                milestone_ids: vec!["M2".into()],
                owner: "builder".into(),
                steps: vec!["Write token expiry test".into()],
                verification_commands: vec!["cargo test token_expiry".into()],
            }],
            decisions: vec![Decision {
                id: "D1".into(),
                decision: "Use incremental refactoring approach".into(),
                reason: "lower risk than big-bang".into(),
                alternatives: vec!["big-bang rewrite".into(), "strangler fig".into()],
                evidence: vec!["prior team experience with incremental".into()],
            }],
            subagent_findings: vec![SubagentFinding {
                id: "A1".into(),
                role: "scout".into(),
                summary: "Found 3 relevant auth files".into(),
                evidence: vec!["src/auth.rs".into()],
            }],
            open_questions: vec![
                OpenQuestion {
                    id: "Q1".into(),
                    question: "Should we use Redis for session storage?".into(),
                    resolved: false,
                    resolution: None,
                },
                OpenQuestion {
                    id: "Q2".into(),
                    question: "Token expiry duration?".into(),
                    resolved: true,
                    resolution: Some("15 minutes".into()),
                },
            ],
            verification: vec![VerificationCriterion {
                description: "Tests pass".into(),
                status: CriterionStatus::Unknown,
            }],
            events: vec![LedgerEvent {
                id: "evt-1".into(),
                phase: "RESEARCH".into(),
                event_type: "PHASE_TRANSITION".into(),
                summary: "CLASSIFY -> RESEARCH: Task classified as MODERATE".into(),
                timestamp: "2026-04-26T10:00:00+00:00".into(),
            }],
            next_actions: vec![NextAction {
                description: "Complete M2 unit tests".into(),
            }],
        };
        let md = TaskLedger::render(&data);

        // Verify all sections present
        assert!(md.contains("# Task: Fix authentication token expiry"));
        assert!(md.contains("## Assessment"));
        assert!(md.contains("MODERATE"));
        assert!(md.contains("## Context Brief"));
        assert!(md.contains("## Milestones"));
        assert!(md.contains("## Active Execution Segment"));
        assert!(md.contains("## Decisions"));
        assert!(md.contains("## Open Questions"));
        assert!(md.contains("## Subagent Findings"));
        assert!(md.contains("## Verification"));
        assert!(md.contains("## Events"));
        assert!(md.contains("## Next Actions"));

        // Verify specific content
        assert!(md.contains("[done] M1: Setup test fixtures"));
        assert!(md.contains("[active] M2: Write unit tests"));
        assert!(md.contains("[pending] M3: Verify coverage"));
        assert!(md.contains("D1: Use incremental refactoring approach"));
        assert!(md.contains("A1: scout"));
        assert!(md.contains("[UNRESOLVED]"));
        assert!(md.contains("[RESOLVED: 15 minutes]"));
        assert!(md.contains("1. Complete M2 unit tests"));
    }

    // -- LedgerData construction tests --

    #[test]
    fn ledger_data_from_snapshot() {
        let mut snap = snapshot_with_assessment();
        snap.skeleton = Some(MilestoneSkeleton {
            milestones: vec![Milestone {
                id: 0,
                description: "Setup".into(),
                deliverable: "stubs".into(),
                depends_on: vec![],
            }],
        });
        snap.completed_milestones = vec![0];

        let data = LedgerData::from_snapshot(snap, "My Task");
        assert_eq!(data.title, "My Task");
        assert_eq!(data.goal, "Fix auth bug");
        assert_eq!(data.milestones.len(), 1);
        assert_eq!(data.milestones[0].status, MilestoneStatus::Done);
        assert_eq!(data.milestones[0].id, "M0");
        assert_eq!(data.success_criteria.len(), 1);
    }

    #[test]
    fn ledger_data_validate_milestones_ok() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![
                MilestoneEntry {
                    id: "M1".into(),
                    goal: "A".into(),
                    status: MilestoneStatus::Active,
                    depends_on: vec![],
                    deliverable: "a".into(),
                    evidence: vec![],
                },
                MilestoneEntry {
                    id: "M2".into(),
                    goal: "B".into(),
                    status: MilestoneStatus::Active,
                    depends_on: vec![],
                    deliverable: "b".into(),
                    evidence: vec![],
                },
            ],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        assert!(data.validate_milestones().is_ok());
    }

    #[test]
    fn ledger_data_validate_milestones_blocked_by_active_sibling() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![
                MilestoneEntry {
                    id: "M1".into(),
                    goal: "A".into(),
                    status: MilestoneStatus::Active,
                    depends_on: vec![],
                    deliverable: "a".into(),
                    evidence: vec![],
                },
                MilestoneEntry {
                    id: "M2".into(),
                    goal: "B".into(),
                    status: MilestoneStatus::Active,
                    depends_on: vec!["M1".into()],
                    deliverable: "b".into(),
                    evidence: vec![],
                },
            ],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        let result = data.validate_milestones();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("parallelism"));
    }

    #[test]
    fn ledger_data_validate_milestones_missing_dep() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Test".into(),
            goal: "Test".into(),
            success_criteria: vec![],
            context_brief: None,
            milestones: vec![MilestoneEntry {
                id: "M1".into(),
                goal: "A".into(),
                status: MilestoneStatus::Pending,
                depends_on: vec!["M99".into()],
                deliverable: "a".into(),
                evidence: vec![],
            }],
            active_segments: vec![],
            decisions: vec![],
            subagent_findings: vec![],
            open_questions: vec![],
            verification: vec![],
            events: vec![],
            next_actions: vec![],
        };
        let result = data.validate_milestones();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("non-existent"));
    }

    // -- Event append protocol tests --

    #[test]
    fn log_phase_transition_event() {
        let mut data = LedgerData::from_snapshot(empty_snapshot(), "Test");
        assert!(data.events.is_empty());
        data.log_event(
            AstPhase::Classify,
            AstPhase::Research,
            "Task classified as MODERATE",
        );
        assert_eq!(data.events.len(), 1);
        assert_eq!(data.events[0].event_type, "PHASE_TRANSITION");
        assert!(data.events[0].summary.contains("CLASSIFY -> RESEARCH"));
        assert!(data.events[0]
            .summary
            .contains("Task classified as MODERATE"));
    }

    #[test]
    fn append_generic_event() {
        let mut data = LedgerData::from_snapshot(empty_snapshot(), "Test");
        data.append_event("EXECUTE", "STEP_COMPLETE", "Step 3 completed successfully");
        assert_eq!(data.events.len(), 1);
        assert_eq!(data.events[0].phase, "EXECUTE");
        assert_eq!(data.events[0].event_type, "STEP_COMPLETE");
        assert_eq!(data.events[0].summary, "Step 3 completed successfully");
        // Timestamp should be parseable as RFC 3339
        assert!(chrono::DateTime::parse_from_rfc3339(&data.events[0].timestamp).is_ok());
    }

    // -- Serialization roundtrip tests --

    #[test]
    fn ledger_data_serialization_roundtrip() {
        let data = LedgerData {
            snapshot: empty_snapshot(),
            title: "Serialize Test".into(),
            goal: "Test goal".into(),
            success_criteria: vec!["Tests pass".into()],
            context_brief: None,
            milestones: vec![MilestoneEntry {
                id: "M1".into(),
                goal: "Do thing".into(),
                status: MilestoneStatus::Active,
                depends_on: vec![],
                deliverable: "thing done".into(),
                evidence: vec!["proof".into()],
            }],
            active_segments: vec![],
            decisions: vec![Decision {
                id: "D1".into(),
                decision: "Choice A".into(),
                reason: "Because".into(),
                alternatives: vec!["Choice B".into()],
                evidence: vec![],
            }],
            subagent_findings: vec![SubagentFinding {
                id: "A1".into(),
                role: "scout".into(),
                summary: "Found files".into(),
                evidence: vec!["a.rs".into()],
            }],
            open_questions: vec![OpenQuestion {
                id: "Q1".into(),
                question: "What?".into(),
                resolved: false,
                resolution: None,
            }],
            verification: vec![VerificationCriterion {
                description: "Passes".into(),
                status: CriterionStatus::Pass,
            }],
            events: vec![LedgerEvent {
                id: "e1".into(),
                phase: "CLASSIFY".into(),
                event_type: "START".into(),
                summary: "Started".into(),
                timestamp: "2026-04-26T10:00:00Z".into(),
            }],
            next_actions: vec![NextAction {
                description: "Do next".into(),
            }],
        };
        let json = serde_json::to_string(&data).unwrap();
        let back: LedgerData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, "Serialize Test");
        assert_eq!(back.milestones.len(), 1);
        assert_eq!(back.milestones[0].status, MilestoneStatus::Active);
        assert_eq!(back.decisions.len(), 1);
        assert_eq!(back.subagent_findings.len(), 1);
        assert_eq!(back.open_questions.len(), 1);
        assert_eq!(back.verification.len(), 1);
        assert_eq!(back.events.len(), 1);
        assert_eq!(back.next_actions.len(), 1);
    }

    #[test]
    fn milestone_status_serialization_roundtrip() {
        for status in [
            MilestoneStatus::Pending,
            MilestoneStatus::Active,
            MilestoneStatus::Blocked,
            MilestoneStatus::Done,
            MilestoneStatus::Dropped,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: MilestoneStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    // -- write_to_file with LedgerData --

    #[test]
    fn write_ledger_data_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.md");
        let data = LedgerData::from_snapshot(empty_snapshot(), "File Test");
        TaskLedger::write_to_file(&path, &data).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Task: File Test"));
    }

    #[test]
    fn write_ledger_data_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/dir/ledger.md");
        let data = LedgerData::from_snapshot(empty_snapshot(), "Dir Test");
        TaskLedger::write_to_file(&path, &data).unwrap();
        assert!(path.exists());
    }
}
