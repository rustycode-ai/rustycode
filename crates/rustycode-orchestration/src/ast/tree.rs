//! Explainable AST tree view.
//!
//! Converts the pipeline snapshot into a user-facing hierarchy of task,
//! milestone, slice, and step nodes with explicit status and rationale.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::types::{
    AstPhase, AstSnapshot, ExecutionSegment, ExecutionStep, Milestone, StepEvidence,
    VerificationStatus,
};

/// Node kinds used by the explainable AST tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstTreeNodeKind {
    Task,
    Milestone,
    Slice,
    Step,
}

impl std::fmt::Display for AstTreeNodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Task => f.write_str("TASK"),
            Self::Milestone => f.write_str("MILESTONE"),
            Self::Slice => f.write_str("SLICE"),
            Self::Step => f.write_str("STEP"),
        }
    }
}

/// Completion status for a tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstTreeStatus {
    Pending,
    Active,
    Blocked,
    Done,
    Partial,
}

impl std::fmt::Display for AstTreeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => f.write_str("PENDING"),
            Self::Active => f.write_str("ACTIVE"),
            Self::Blocked => f.write_str("BLOCKED"),
            Self::Done => f.write_str("DONE"),
            Self::Partial => f.write_str("PARTIAL"),
        }
    }
}

/// Short, human-readable rationale attached to a tree node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AstTreeExplanation {
    pub why: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub alternatives_considered: Vec<String>,
    pub confidence: Option<u8>,
}

/// A single explainable node in the AST tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstTreeNode {
    pub id: String,
    pub kind: AstTreeNodeKind,
    pub title: String,
    pub status: AstTreeStatus,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub ready_now: bool,
    #[serde(default)]
    pub can_run_in_parallel: bool,
    #[serde(default)]
    pub parallel_group: Option<String>,
    #[serde(default)]
    pub explanation: Option<AstTreeExplanation>,
    #[serde(default)]
    pub children: Vec<Self>,
}

impl AstTreeNode {
    pub fn new(
        id: impl Into<String>,
        kind: AstTreeNodeKind,
        title: impl Into<String>,
        status: AstTreeStatus,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            title: title.into(),
            status,
            depends_on: Vec::new(),
            blocked_by: Vec::new(),
            ready_now: false,
            can_run_in_parallel: false,
            parallel_group: None,
            explanation: None,
            children: Vec::new(),
        }
    }

    pub fn with_dependency_state(
        mut self,
        depends_on: Vec<String>,
        blocked_by: Vec<String>,
        ready_now: bool,
        can_run_in_parallel: bool,
        parallel_group: Option<String>,
    ) -> Self {
        self.depends_on = depends_on;
        self.blocked_by = blocked_by;
        self.ready_now = ready_now;
        self.can_run_in_parallel = can_run_in_parallel;
        self.parallel_group = parallel_group;
        self
    }

    pub fn with_explanation(mut self, explanation: AstTreeExplanation) -> Self {
        self.explanation = Some(explanation);
        self
    }

    pub fn with_children(mut self, children: Vec<Self>) -> Self {
        self.children = children;
        self
    }
}

/// Explainable tree rooted at the current task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstTree {
    pub root: AstTreeNode,
}

impl AstTree {
    /// Build an explainable tree from the current pipeline snapshot.
    pub fn from_snapshot(snapshot: &AstSnapshot) -> Self {
        let root = build_task_node(snapshot);
        Self { root }
    }

    /// Render the tree as markdown for the ledger/TUI.
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();
        render_node(&self.root, 0, &mut out);
        out
    }
}

fn build_task_node(snapshot: &AstSnapshot) -> AstTreeNode {
    let title = snapshot
        .assessment
        .as_ref()
        .map_or_else(|| "Untitled task".to_string(), |a| a.task_summary.clone());
    let status = task_status(snapshot);
    let explanation = snapshot
        .assessment
        .as_ref()
        .map(|assessment| AstTreeExplanation {
            why: format!(
                "AST is tracking '{}' through the {} phase pipeline.",
                assessment.task_summary, snapshot.current_phase
            ),
            evidence: assessment
                .success_criteria
                .iter()
                .map(|c| c.description.clone())
                .collect(),
            alternatives_considered: vec![
                "Direct execution without a milestone tree".into(),
                "Flat phase progress only".into(),
            ],
            confidence: Some(confidence_for_status(status)),
        });

    let ready_batch = ready_milestone_ids(snapshot);
    let parallel_group = parallel_group_for_ready_batch(&ready_batch);
    let mut children = Vec::new();
    if let Some(skeleton) = &snapshot.skeleton {
        for milestone in &skeleton.milestones {
            children.push(build_milestone_node(
                snapshot,
                milestone,
                &ready_batch,
                parallel_group.as_deref(),
            ));
        }
    } else if let Some(assessment) = &snapshot.assessment {
        children.push(
            AstTreeNode::new(
                "milestone:planning",
                AstTreeNodeKind::Milestone,
                format!("Plan work for {}", assessment.task_summary),
                milestone_status(snapshot, None),
            )
            .with_explanation(AstTreeExplanation {
                why: "The plan has not been decomposed into milestones yet.".into(),
                evidence: vec![assessment.task_summary.clone()],
                alternatives_considered: vec!["Wait for skeleton generation".into()],
                confidence: Some(40),
            }),
        );
    }

    AstTreeNode::new("task", AstTreeNodeKind::Task, title, status)
        .with_explanation(explanation.unwrap_or_default())
        .with_children(children)
}

fn build_milestone_node(
    snapshot: &AstSnapshot,
    milestone: &Milestone,
    ready_batch: &[usize],
    parallel_group: Option<&str>,
) -> AstTreeNode {
    let status = milestone_status(snapshot, Some(milestone));
    let depends_on = milestone
        .depends_on
        .iter()
        .map(|dep| format!("milestone:{dep}"))
        .collect::<Vec<_>>();
    let blocked_by = blocked_dependency_ids(snapshot, milestone);
    let ready_now = is_milestone_ready(snapshot, milestone) && status != AstTreeStatus::Done;
    let can_run_in_parallel = ready_batch.len() > 1
        && ready_batch.contains(&milestone.id)
        && matches!(status, AstTreeStatus::Pending | AstTreeStatus::Active);
    let slice = build_slice_node(
        snapshot,
        milestone,
        ready_now,
        can_run_in_parallel,
        parallel_group,
    );
    let evidence = milestone_evidence(snapshot, milestone.id);
    let explanation = AstTreeExplanation {
        why: milestone.description.clone(),
        evidence: if evidence.is_empty() {
            vec![milestone.deliverable.clone()]
        } else {
            evidence
        },
        alternatives_considered: if milestone.depends_on.is_empty() {
            vec!["Merge with neighboring milestone".into()]
        } else {
            vec![format!(
                "Defer until dependencies are complete: {:?}",
                milestone.depends_on
            )]
        },
        confidence: Some(confidence_for_status(status)),
    };

    AstTreeNode::new(
        format!("milestone:{}", milestone.id),
        AstTreeNodeKind::Milestone,
        format!("M{} - {}", milestone.id, milestone.description),
        status,
    )
    .with_dependency_state(
        depends_on,
        blocked_by,
        ready_now,
        can_run_in_parallel,
        parallel_group.map(ToOwned::to_owned),
    )
    .with_explanation(explanation)
    .with_children(vec![slice])
}

fn build_slice_node(
    snapshot: &AstSnapshot,
    milestone: &Milestone,
    ready_now: bool,
    can_run_in_parallel: bool,
    parallel_group: Option<&str>,
) -> AstTreeNode {
    let active_segment = snapshot
        .active_segments
        .iter()
        .find(|segment| segment.milestone_id == milestone.id);
    let completed = snapshot.completed_milestones.contains(&milestone.id);
    let status = if completed {
        AstTreeStatus::Done
    } else if active_segment.is_some() {
        AstTreeStatus::Active
    } else if milestone
        .depends_on
        .iter()
        .all(|dep| snapshot.completed_milestones.contains(dep))
    {
        AstTreeStatus::Pending
    } else {
        AstTreeStatus::Blocked
    };

    let depends_on = vec![format!("milestone:{}", milestone.id)];
    let blocked_by = if ready_now {
        Vec::new()
    } else {
        blocked_dependency_ids(snapshot, milestone)
    };
    let mut children = Vec::new();
    let mut explanation = AstTreeExplanation {
        why: format!("Work slice for milestone M{}.", milestone.id),
        evidence: vec![milestone.deliverable.clone()],
        alternatives_considered: vec!["Split into more slices".into()],
        confidence: Some(confidence_for_status(status)),
    };

    if let Some(segment) = active_segment {
        explanation.evidence.extend(segment.edge_cases.clone());
        children = build_step_nodes(snapshot, milestone.id, segment);
        explanation.why = format!(
            "This slice expands milestone M{} into executable steps.",
            milestone.id
        );
    } else {
        children.push(
            AstTreeNode::new(
                format!("step:{}:placeholder", milestone.id),
                AstTreeNodeKind::Step,
                "Planned task".to_string(),
                AstTreeStatus::Pending,
            )
            .with_explanation(AstTreeExplanation {
                why: "No execution segment has been assigned yet.".into(),
                evidence: vec![milestone.deliverable.clone()],
                alternatives_considered: vec!["Wait for expansion".into()],
                confidence: Some(35),
            }),
        );
    }

    AstTreeNode::new(
        format!("slice:{}", milestone.id),
        AstTreeNodeKind::Slice,
        format!("Slice for M{}", milestone.id),
        status,
    )
    .with_dependency_state(
        depends_on,
        blocked_by,
        ready_now,
        can_run_in_parallel,
        parallel_group.map(ToOwned::to_owned),
    )
    .with_explanation(explanation)
    .with_children(children)
}

fn build_step_nodes(
    snapshot: &AstSnapshot,
    milestone_id: usize,
    segment: &ExecutionSegment,
) -> Vec<AstTreeNode> {
    let evidence = snapshot.evidence.get(&milestone_id);
    segment
        .steps
        .iter()
        .enumerate()
        .map(|(idx, step)| build_step_node(snapshot, milestone_id, idx, step, evidence))
        .collect()
}

fn build_step_node(
    snapshot: &AstSnapshot,
    milestone_id: usize,
    step_index: usize,
    step: &ExecutionStep,
    evidence: Option<&Vec<StepEvidence>>,
) -> AstTreeNode {
    let status = step_status(snapshot, milestone_id, step_index, evidence);
    let mut evidence_lines = vec![step.action.clone()];
    if let Some(cmd) = step.expected_command.as_ref() {
        evidence_lines.push(format!("expected command: {cmd}"));
    }
    if let Some(cmd) = step.verification_command.as_ref() {
        evidence_lines.push(format!("verification command: {cmd}"));
    }
    if !step.file_targets.is_empty() {
        evidence_lines.push(format!(
            "file targets: {}",
            step.file_targets
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(items) = evidence {
        for item in items.iter().filter(|item| item.step_index == step_index) {
            evidence_lines.push(format!("exit code: {}", item.exit_code));
            if !item.stdout_summary.is_empty() {
                evidence_lines.push(format!("stdout: {}", item.stdout_summary));
            }
            if !item.stderr_summary.is_empty() {
                evidence_lines.push(format!("stderr: {}", item.stderr_summary));
            }
        }
    }

    AstTreeNode::new(
        format!("step:{}:{}", milestone_id, step_index),
        AstTreeNodeKind::Step,
        step.action.clone(),
        status,
    )
    .with_dependency_state(
        vec![format!("slice:{}", milestone_id)],
        if status == AstTreeStatus::Blocked {
            vec![format!("milestone:{milestone_id}")]
        } else {
            Vec::new()
        },
        matches!(status, AstTreeStatus::Active | AstTreeStatus::Done),
        false,
        None,
    )
    .with_explanation(AstTreeExplanation {
        why: step
            .recovery_notes
            .clone()
            .unwrap_or_else(|| "Atomic step in the current execution slice.".to_string()),
        evidence: evidence_lines,
        alternatives_considered: vec!["Fold into a larger step".into()],
        confidence: Some(confidence_for_status(status)),
    })
}

fn milestone_evidence(snapshot: &AstSnapshot, milestone_id: usize) -> Vec<String> {
    let mut evidence = Vec::new();
    if let Some(step_evidence) = snapshot.evidence.get(&milestone_id) {
        for item in step_evidence {
            evidence.push(format!("step {} exit {}", item.step_index, item.exit_code));
        }
    }
    if let Some(count) = snapshot.recovery_attempts.get(&milestone_id) {
        evidence.push(format!("recovery attempts: {count}"));
    }
    if snapshot.consultant_escalation.contains(&milestone_id) {
        evidence.push("consultant escalation triggered".into());
    }
    evidence
}

fn milestone_status(snapshot: &AstSnapshot, milestone: Option<&Milestone>) -> AstTreeStatus {
    let Some(milestone) = milestone else {
        return match snapshot.current_phase {
            AstPhase::Complete => AstTreeStatus::Done,
            AstPhase::Failed => AstTreeStatus::Blocked,
            _ => AstTreeStatus::Pending,
        };
    };

    if snapshot.completed_milestones.contains(&milestone.id) {
        return AstTreeStatus::Done;
    }
    if snapshot.consultant_escalation.contains(&milestone.id) {
        return AstTreeStatus::Blocked;
    }
    if snapshot
        .active_segments
        .iter()
        .any(|segment| segment.milestone_id == milestone.id)
    {
        return AstTreeStatus::Active;
    }
    if milestone
        .depends_on
        .iter()
        .all(|dep| snapshot.completed_milestones.contains(dep))
    {
        AstTreeStatus::Pending
    } else {
        AstTreeStatus::Blocked
    }
}

fn is_milestone_ready(snapshot: &AstSnapshot, milestone: &Milestone) -> bool {
    milestone
        .depends_on
        .iter()
        .all(|dep| snapshot.completed_milestones.contains(dep))
}

fn blocked_dependency_ids(snapshot: &AstSnapshot, milestone: &Milestone) -> Vec<String> {
    milestone
        .depends_on
        .iter()
        .filter(|dep| !snapshot.completed_milestones.contains(dep))
        .map(|dep| format!("milestone:{dep}"))
        .collect()
}

fn ready_milestone_ids(snapshot: &AstSnapshot) -> Vec<usize> {
    snapshot
        .skeleton
        .as_ref()
        .map(|skeleton| {
            skeleton
                .milestones
                .iter()
                .filter(|milestone| {
                    !snapshot.completed_milestones.contains(&milestone.id)
                        && is_milestone_ready(snapshot, milestone)
                })
                .map(|milestone| milestone.id)
                .collect()
        })
        .unwrap_or_default()
}

fn parallel_group_for_ready_batch(ready_batch: &[usize]) -> Option<String> {
    if ready_batch.len() > 1 {
        Some(format!(
            "ready-batch:{}",
            ready_batch
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join("-")
        ))
    } else {
        None
    }
}

fn task_status(snapshot: &AstSnapshot) -> AstTreeStatus {
    match snapshot.report.as_ref().map(|r| r.overall) {
        Some(VerificationStatus::Pass) => AstTreeStatus::Done,
        Some(VerificationStatus::Partial) => AstTreeStatus::Partial,
        Some(VerificationStatus::Fail) => AstTreeStatus::Blocked,
        None => match snapshot.current_phase {
            AstPhase::Complete => AstTreeStatus::Done,
            AstPhase::Failed => AstTreeStatus::Blocked,
            _ => AstTreeStatus::Active,
        },
    }
}

fn step_status(
    snapshot: &AstSnapshot,
    milestone_id: usize,
    step_index: usize,
    evidence: Option<&Vec<StepEvidence>>,
) -> AstTreeStatus {
    if let Some(items) = evidence {
        if let Some(item) = items.iter().find(|item| item.step_index == step_index) {
            if item.verification_passed == Some(true) || item.exit_code == 0 {
                return AstTreeStatus::Done;
            }
            return AstTreeStatus::Blocked;
        }
    }

    if snapshot
        .active_segments
        .iter()
        .any(|segment| segment.milestone_id == milestone_id)
    {
        AstTreeStatus::Active
    } else {
        AstTreeStatus::Pending
    }
}

const fn confidence_for_status(status: AstTreeStatus) -> u8 {
    match status {
        AstTreeStatus::Done => 95,
        AstTreeStatus::Active => 75,
        AstTreeStatus::Blocked => 35,
        AstTreeStatus::Partial => 60,
        AstTreeStatus::Pending => 50,
    }
}

fn render_node(node: &AstTreeNode, indent: usize, out: &mut String) {
    let padding = "  ".repeat(indent);
    let _ = writeln!(
        out,
        "{}- [{}] {}: {}",
        padding, node.status, node.kind, node.title
    );

    if let Some(explanation) = &node.explanation {
        if !explanation.why.is_empty() {
            let _ = writeln!(out, "{}  - Why: {}", padding, explanation.why);
        }
        if !explanation.evidence.is_empty() {
            let _ = writeln!(
                out,
                "{}  - Evidence: {}",
                padding,
                explanation.evidence.join(", ")
            );
        }
        if !explanation.alternatives_considered.is_empty() {
            let _ = writeln!(
                out,
                "{}  - Alternatives: {}",
                padding,
                explanation.alternatives_considered.join(", ")
            );
        }
        if let Some(confidence) = explanation.confidence {
            let _ = writeln!(out, "{}  - Confidence: {}%", padding, confidence);
        }
    }

    if !node.depends_on.is_empty() {
        let _ = writeln!(
            out,
            "{}  - Depends on: {}",
            padding,
            node.depends_on.join(", ")
        );
    }
    if !node.blocked_by.is_empty() {
        let _ = writeln!(
            out,
            "{}  - Blocked by: {}",
            padding,
            node.blocked_by.join(", ")
        );
    }
    let _ = writeln!(out, "{}  - Ready now: {}", padding, node.ready_now);
    let _ = writeln!(
        out,
        "{}  - Parallelizable: {}",
        padding, node.can_run_in_parallel
    );
    if let Some(group) = &node.parallel_group {
        let _ = writeln!(out, "{}  - Parallel group: {}", padding, group);
    }

    for child in &node.children {
        render_node(child, indent + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::types::{
        ComplexityLevel, ContextBrief, ExecutionSegment, ExecutionStep, Milestone,
        MilestoneSkeleton, PhaseRoute, StepEvidence, SuccessCriterion, TaskAssessment,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn snapshot_with_tree() -> AstSnapshot {
        let assessment = TaskAssessment {
            task_summary: "Improve AST".into(),
            complexity: ComplexityLevel::Moderate,
            success_criteria: vec![SuccessCriterion {
                description: "Tree renders".into(),
                verification_command: Some("cargo test".into()),
            }],
            route: PhaseRoute::StandardSequence,

            clarity: None,
        };
        AstSnapshot {
            current_phase: AstPhase::Execute,
            assessment: Some(assessment),
            brief: Some(ContextBrief {
                relevant_files: vec![PathBuf::from("src/lib.rs")],
                patterns_found: vec!["tree".into()],
                dependencies: vec![],
                risks: vec!["status drift".into()],
                constraints: vec!["keep backwards compatibility".into()],
            }),
            skeleton: Some(MilestoneSkeleton {
                milestones: vec![Milestone {
                    id: 0,
                    description: "Build explainable tree".into(),
                    deliverable: "src/tree.rs".into(),
                    depends_on: vec![],
                }],
            }),
            active_segments: vec![ExecutionSegment {
                milestone_id: 0,
                steps: vec![ExecutionStep {
                    action: "Add tree node model".into(),
                    file_targets: vec![PathBuf::from("src/tree.rs")],
                    expected_command: Some("cargo test".into()),
                    verification_command: Some("cargo test".into()),
                    is_risky: false,
                    recovery_notes: None,
                }],
                required_criteria: vec![],
                edge_cases: vec!["compatibility".into()],
            }],
            completed_milestones: vec![],
            evidence: HashMap::from([(
                0,
                vec![StepEvidence {
                    step_index: 0,
                    command_run: Some("cargo test".into()),
                    exit_code: 0,
                    stdout_summary: "ok".into(),
                    stderr_summary: String::new(),
                    changed_files: vec![PathBuf::from("src/tree.rs")],
                    verification_passed: Some(true),
                }],
            )]),
            recovery_attempts: HashMap::new(),
            consultant_escalation: vec![],
            failed_milestones: vec![],
            report: None,
        }
    }

    #[test]
    fn builds_tree_from_snapshot() {
        let tree = AstTree::from_snapshot(&snapshot_with_tree());
        assert_eq!(tree.root.kind, AstTreeNodeKind::Task);
        assert_eq!(tree.root.children.len(), 1);
        assert_eq!(tree.root.children[0].kind, AstTreeNodeKind::Milestone);
        assert_eq!(
            tree.root.children[0].children[0].kind,
            AstTreeNodeKind::Slice
        );
        assert_eq!(
            tree.root.children[0].children[0].children[0].kind,
            AstTreeNodeKind::Step
        );
    }

    #[test]
    fn renders_markdown_tree() {
        let tree = AstTree::from_snapshot(&snapshot_with_tree());
        let md = tree.render_markdown();
        assert!(md.contains("[ACTIVE] TASK"));
        assert!(md.contains("MILESTONE"));
        assert!(md.contains("SLICE"));
        assert!(md.contains("STEP"));
        assert!(md.contains("Why:"));
    }

    #[test]
    fn reports_dependency_and_parallel_metadata() {
        let mut snapshot = snapshot_with_tree();
        snapshot.skeleton = Some(MilestoneSkeleton {
            milestones: vec![
                Milestone {
                    id: 0,
                    description: "First".into(),
                    deliverable: "a.rs".into(),
                    depends_on: vec![],
                },
                Milestone {
                    id: 1,
                    description: "Second".into(),
                    deliverable: "b.rs".into(),
                    depends_on: vec![],
                },
            ],
        });
        let tree = AstTree::from_snapshot(&snapshot);
        let first = &tree.root.children[0];
        assert!(first.ready_now);
        assert!(first.can_run_in_parallel);
        assert!(first.parallel_group.is_some());
        assert!(first
            .depends_on
            .iter()
            .all(|dep| dep.starts_with("milestone:")));
        assert!(first.children[0].ready_now);
    }
}
