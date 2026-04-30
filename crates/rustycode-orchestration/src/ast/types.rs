//! Core types for Adaptive Structured Thinking (AST).
//!
//! AST is a 6-phase pipeline: CLASSIFY → RESEARCH → SKELETON → EXPAND → EXECUTE → VERIFY.
//! Each phase produces a structured artifact that feeds the next phase.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Task complexity rating controlling how much structure is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplexityLevel {
    /// 1-2 files, 1-3 steps, known pattern.
    Trivial,
    /// 3-10 files, 4-10 steps, one main system.
    Moderate,
    /// 10+ files, multiple systems, uncertain dependencies.
    Complex,
}

/// A single success criterion for the task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriterion {
    pub description: String,
    pub verification_command: Option<String>,
}

/// Phase 0 output: task assessment with complexity and routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssessment {
    pub task_summary: String,
    pub complexity: ComplexityLevel,
    pub success_criteria: Vec<SuccessCriterion>,
    /// Which phases to skip based on complexity.
    pub route: PhaseRoute,
    /// Optional clarity report from pre-pipeline scoring.
    #[serde(default)]
    pub clarity: Option<super::clarity::ClarityReport>,
}

/// Routing decision based on complexity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseRoute {
    /// TRIVIAL: skip research, collapse skeleton to single step, go directly to execute.
    DirectExecute,
    /// MODERATE: quick research, skeleton covers all milestones.
    StandardSequence,
    /// COMPLEX: full research, rolling-wave expansion (2-milestone batches).
    RollingWave,
}

impl From<ComplexityLevel> for PhaseRoute {
    fn from(level: ComplexityLevel) -> Self {
        match level {
            ComplexityLevel::Trivial => Self::DirectExecute,
            ComplexityLevel::Moderate => Self::StandardSequence,
            ComplexityLevel::Complex => Self::RollingWave,
        }
    }
}

/// Phase 1 output: context brief from codebase research.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBrief {
    pub relevant_files: Vec<PathBuf>,
    pub patterns_found: Vec<String>,
    pub dependencies: Vec<String>,
    pub risks: Vec<String>,
    pub constraints: Vec<String>,
}

/// A single milestone in the skeleton.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: usize,
    pub description: String,
    pub deliverable: String,
    /// Indices of milestones this one depends on.
    pub depends_on: Vec<usize>,
}

/// Phase 2 output: milestone skeleton.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneSkeleton {
    pub milestones: Vec<Milestone>,
}

impl MilestoneSkeleton {
    pub const fn len(&self) -> usize {
        self.milestones.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.milestones.is_empty()
    }

    /// Get milestones that are ready to execute (dependencies met, not failed).
    pub fn ready_milestones(&self, completed: &[usize], failed: &[usize]) -> Vec<&Milestone> {
        let completed_set: std::collections::HashSet<usize> = completed.iter().copied().collect();
        let failed_set: std::collections::HashSet<usize> = failed.iter().copied().collect();
        self.milestones
            .iter()
            .filter(|m| {
                !completed_set.contains(&m.id)
                    && !failed_set.contains(&m.id)
                    && m.depends_on.iter().all(|d| completed_set.contains(d))
            })
            .collect()
    }
}

/// An atomic step within an expanded milestone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub action: String,
    pub file_targets: Vec<PathBuf>,
    pub expected_command: Option<String>,
    pub verification_command: Option<String>,
    pub is_risky: bool,
    pub recovery_notes: Option<String>,
}

/// Phase 3a output: expanded milestone with concrete steps.
///
/// Carries forward all requirements from the assessment so the builder
/// sees the full picture. v0.4 experimental result: crew handoffs drop
/// requirements unless the segment is self-contained.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSegment {
    pub milestone_id: usize,
    pub steps: Vec<ExecutionStep>,
    /// Success criteria that this segment must satisfy.
    /// Populated by architect from the original assessment.
    #[serde(default)]
    pub required_criteria: Vec<SuccessCriterion>,
    /// Edge cases explicitly identified during skeleton/expansion.
    /// Builder must handle all of these.
    #[serde(default)]
    pub edge_cases: Vec<String>,
}

/// Evidence collected after executing a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEvidence {
    pub step_index: usize,
    pub command_run: Option<String>,
    pub exit_code: i32,
    pub stdout_summary: String,
    pub stderr_summary: String,
    pub changed_files: Vec<PathBuf>,
    pub verification_passed: Option<bool>,
}

/// Recovery action when a step fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAction {
    pub failed_step: usize,
    pub diagnosis: String,
    pub research_needed: Vec<String>,
    pub replanned_steps: Vec<ExecutionStep>,
    pub retry_attempt: u32,
}

/// Phase 4 output: verification report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub results: Vec<CriterionResult>,
    pub overall: VerificationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionResult {
    pub criterion: SuccessCriterion,
    pub status: VerificationStatus,
    pub evidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    Pass,
    Partial,
    Fail,
}

/// Current state of the AST pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstPhase {
    Classify,
    Research,
    Skeleton,
    Expand,
    Execute,
    Verify,
    Complete,
    Failed,
}

impl std::fmt::Display for AstPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Classify => write!(f, "CLASSIFY"),
            Self::Research => write!(f, "RESEARCH"),
            Self::Skeleton => write!(f, "SKELETON"),
            Self::Expand => write!(f, "EXPAND"),
            Self::Execute => write!(f, "EXECUTE"),
            Self::Verify => write!(f, "VERIFY"),
            Self::Complete => write!(f, "COMPLETE"),
            Self::Failed => write!(f, "FAILED"),
        }
    }
}

/// Snapshot of AST pipeline state for supervisor observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstSnapshot {
    pub current_phase: AstPhase,
    pub assessment: Option<TaskAssessment>,
    pub brief: Option<ContextBrief>,
    pub skeleton: Option<MilestoneSkeleton>,
    pub active_segments: Vec<ExecutionSegment>,
    pub completed_milestones: Vec<usize>,
    pub evidence: HashMap<usize, Vec<StepEvidence>>,
    pub recovery_attempts: HashMap<usize, u32>,
    /// Milestones that triggered Consultant escalation (v0.4: 3+ consecutive retries).
    #[serde(default)]
    pub consultant_escalation: Vec<usize>,
    /// Milestones that permanently failed after exhausting recovery retries.
    #[serde(default)]
    pub failed_milestones: Vec<usize>,
    pub report: Option<VerificationReport>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complexity_to_route() {
        assert_eq!(
            PhaseRoute::from(ComplexityLevel::Trivial),
            PhaseRoute::DirectExecute
        );
        assert_eq!(
            PhaseRoute::from(ComplexityLevel::Moderate),
            PhaseRoute::StandardSequence
        );
        assert_eq!(
            PhaseRoute::from(ComplexityLevel::Complex),
            PhaseRoute::RollingWave
        );
    }

    #[test]
    fn skeleton_ready_milestones_no_deps() {
        let skeleton = MilestoneSkeleton {
            milestones: vec![
                Milestone {
                    id: 0,
                    description: "A".into(),
                    deliverable: "a".into(),
                    depends_on: vec![],
                },
                Milestone {
                    id: 1,
                    description: "B".into(),
                    deliverable: "b".into(),
                    depends_on: vec![],
                },
            ],
        };
        let ready = skeleton.ready_milestones(&[], &[]);
        assert_eq!(ready.len(), 2);
    }

    #[test]
    fn skeleton_ready_milestones_with_deps() {
        let skeleton = MilestoneSkeleton {
            milestones: vec![
                Milestone {
                    id: 0,
                    description: "A".into(),
                    deliverable: "a".into(),
                    depends_on: vec![],
                },
                Milestone {
                    id: 1,
                    description: "B".into(),
                    deliverable: "b".into(),
                    depends_on: vec![0],
                },
                Milestone {
                    id: 2,
                    description: "C".into(),
                    deliverable: "c".into(),
                    depends_on: vec![0, 1],
                },
            ],
        };
        let ready = skeleton.ready_milestones(&[], &[]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, 0);

        let ready = skeleton.ready_milestones(&[0], &[]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, 1);

        let ready = skeleton.ready_milestones(&[0, 1], &[]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, 2);
    }

    #[test]
    fn skeleton_ready_milestones_all_complete() {
        let skeleton = MilestoneSkeleton {
            milestones: vec![Milestone {
                id: 0,
                description: "A".into(),
                deliverable: "a".into(),
                depends_on: vec![],
            }],
        };
        let ready = skeleton.ready_milestones(&[0], &[]);
        assert!(ready.is_empty());
    }

    #[test]
    fn skeleton_ready_milestones_skips_failed() {
        let skeleton = MilestoneSkeleton {
            milestones: vec![
                Milestone {
                    id: 0,
                    description: "A".into(),
                    deliverable: "a".into(),
                    depends_on: vec![],
                },
                Milestone {
                    id: 1,
                    description: "B".into(),
                    deliverable: "b".into(),
                    depends_on: vec![],
                },
            ],
        };
        // Milestone 0 failed, milestone 1 should still be ready
        let ready = skeleton.ready_milestones(&[], &[0]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, 1);
    }

    #[test]
    fn ast_phase_display() {
        assert_eq!(AstPhase::Classify.to_string(), "CLASSIFY");
        assert_eq!(AstPhase::Execute.to_string(), "EXECUTE");
        assert_eq!(AstPhase::Complete.to_string(), "COMPLETE");
    }

    #[test]
    fn assessment_serialization_roundtrip() {
        let assessment = TaskAssessment {
            task_summary: "Fix typo".into(),
            complexity: ComplexityLevel::Trivial,
            success_criteria: vec![SuccessCriterion {
                description: "File changed".into(),
                verification_command: Some("grep fixed file.txt".into()),
            }],
            route: PhaseRoute::DirectExecute,

            clarity: None,
        };
        let json = serde_json::to_string(&assessment).unwrap();
        let back: TaskAssessment = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_summary, "Fix typo");
        assert_eq!(back.complexity, ComplexityLevel::Trivial);
    }

    #[test]
    fn verification_status_ordering() {
        assert!(VerificationStatus::Pass != VerificationStatus::Fail);
        assert!(VerificationStatus::Partial != VerificationStatus::Pass);
    }
}
