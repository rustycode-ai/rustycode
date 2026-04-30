//! AST Phase 3a: EXPAND.
//!
//! The `MilestoneExpander` converts near-term milestones into concrete
//! `ExecutionSegment`s containing ordered `ExecutionStep`s. The number of
//! milestones expanded at once depends on the `ComplexityLevel`:
//!
//! * **Trivial** -- single segment with 1-2 steps covering the entire task.
//! * **Moderate** -- all remaining milestones expanded immediately.
//! * **Complex** -- rolling-wave: only the next 2 milestones are expanded.

use super::types::{
    ComplexityLevel, ContextBrief, ExecutionSegment, ExecutionStep, Milestone, TaskAssessment,
};
use std::path::PathBuf;

/// Expands milestone skeletons into executable segments.
pub struct MilestoneExpander;

impl MilestoneExpander {
    pub const fn new() -> Self {
        Self
    }

    /// Expand milestones into `ExecutionSegment`s based on task complexity.
    ///
    /// * `milestones` -- the full set of milestones from the skeleton phase.
    /// * `assessment` -- the Phase 0 assessment controlling expansion depth.
    /// * `completed`  -- milestone IDs already finished (used to skip them).
    /// * `brief`      -- optional context brief; risks and constraints become edge cases.
    pub fn expand(
        &self,
        milestones: &[Milestone],
        assessment: &TaskAssessment,
        completed: &[usize],
        brief: Option<&ContextBrief>,
    ) -> Vec<ExecutionSegment> {
        let remaining: Vec<&Milestone> = milestones
            .iter()
            .filter(|m| !completed.contains(&m.id))
            .collect();

        if remaining.is_empty() {
            return Vec::new();
        }

        let to_expand = self.select_batch(&remaining, assessment.complexity);
        let edge_cases = derive_edge_cases(brief);

        to_expand
            .into_iter()
            .map(|m| self.expand_milestone(m, &assessment.success_criteria, &edge_cases))
            .collect()
    }

    /// Determine which milestones to expand based on complexity.
    #[allow(clippy::unused_self)]
    fn select_batch<'a>(
        &self,
        remaining: &[&'a Milestone],
        complexity: ComplexityLevel,
    ) -> Vec<&'a Milestone> {
        match complexity {
            ComplexityLevel::Trivial | ComplexityLevel::Moderate => remaining.to_vec(),
            ComplexityLevel::Complex => remaining.iter().take(2).copied().collect(),
        }
    }

    /// Convert a single milestone into an `ExecutionSegment`.
    fn expand_milestone(
        &self,
        milestone: &Milestone,
        success_criteria: &[super::types::SuccessCriterion],
        edge_cases: &[String],
    ) -> ExecutionSegment {
        let steps = self.generate_steps(milestone);
        ExecutionSegment {
            milestone_id: milestone.id,
            steps,
            required_criteria: success_criteria.to_vec(),
            edge_cases: edge_cases.to_vec(),
        }
    }

    /// Generate concrete execution steps for a milestone.
    ///
    /// For a TRIVIAL task this produces 1-2 steps; for others it derives
    /// steps from the milestone description and deliverable.
    fn generate_steps(&self, milestone: &Milestone) -> Vec<ExecutionStep> {
        let desc_lower = milestone.description.to_lowercase();
        let deliverable_lower = milestone.deliverable.to_lowercase();

        let is_test = desc_lower.contains("test") || deliverable_lower.contains("test");
        let is_build = desc_lower.contains("build") || deliverable_lower.contains("build");
        let is_edit = desc_lower.contains("edit")
            || desc_lower.contains("implement")
            || desc_lower.contains("fix");
        let is_file_deliverable = deliverable_lower.contains(".rs")
            || deliverable_lower.contains(".py")
            || deliverable_lower.contains(".ts")
            || deliverable_lower.contains(".js");

        let mut steps = Vec::with_capacity(2);
        if is_edit || is_file_deliverable {
            let file_targets = self.extract_file_targets(&milestone.deliverable);
            steps.push(ExecutionStep {
                action: format!("Implement: {}", milestone.description),
                file_targets,
                expected_command: None,
                verification_command: None,
                is_risky: false,
                recovery_notes: None,
            });
        } else {
            steps.push(ExecutionStep {
                action: milestone.description.clone(),
                file_targets: Vec::new(),
                expected_command: None,
                verification_command: None,
                is_risky: false,
                recovery_notes: None,
            });
        }

        if is_test {
            steps.push(ExecutionStep {
                action: format!("Run tests for: {}", milestone.deliverable),
                file_targets: Vec::new(),
                expected_command: Some("cargo test".into()),
                verification_command: Some("cargo test".into()),
                is_risky: false,
                recovery_notes: None,
            });
        } else if is_build {
            steps.push(ExecutionStep {
                action: "Verify build succeeds".into(),
                file_targets: Vec::new(),
                expected_command: Some("cargo build".into()),
                verification_command: Some("cargo build".into()),
                is_risky: false,
                recovery_notes: None,
            });
        }

        if steps.is_empty() {
            steps.push(ExecutionStep {
                action: milestone.description.clone(),
                file_targets: Vec::new(),
                expected_command: None,
                verification_command: None,
                is_risky: false,
                recovery_notes: None,
            });
        }

        steps
    }

    /// Extract file paths from a deliverable description.
    #[allow(clippy::unused_self)]
    fn extract_file_targets(&self, deliverable: &str) -> Vec<PathBuf> {
        deliverable
            .split_whitespace()
            .filter(|s| s.contains('.') || s.contains('/'))
            .map(PathBuf::from)
            .collect()
    }
}

/// Derive edge case descriptions from brief risks and constraints.
fn derive_edge_cases(brief: Option<&ContextBrief>) -> Vec<String> {
    brief.map_or_else(Vec::new, |b| {
        let mut cases = Vec::with_capacity(b.risks.len() + b.constraints.len());
        for risk in &b.risks {
            cases.push(format!("Risk: {risk}"));
        }
        for constraint in &b.constraints {
            cases.push(format!("Constraint: {constraint}"));
        }
        cases
    })
}

impl Default for MilestoneExpander {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::types::{ContextBrief, PhaseRoute, SuccessCriterion};

    fn trivial_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Fix typo".into(),
            complexity: ComplexityLevel::Trivial,
            success_criteria: vec![SuccessCriterion {
                description: "File changed".into(),
                verification_command: None,
            }],
            route: PhaseRoute::DirectExecute,

            clarity: None,
        }
    }

    fn moderate_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Add tests".into(),
            complexity: ComplexityLevel::Moderate,
            success_criteria: vec![SuccessCriterion {
                description: "Tests pass".into(),
                verification_command: Some("cargo test".into()),
            }],
            route: PhaseRoute::StandardSequence,

            clarity: None,
        }
    }

    fn complex_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Implement feature".into(),
            complexity: ComplexityLevel::Complex,
            success_criteria: vec![SuccessCriterion {
                description: "Feature works".into(),
                verification_command: None,
            }],
            route: PhaseRoute::RollingWave,

            clarity: None,
        }
    }

    fn sample_milestones() -> Vec<Milestone> {
        vec![
            Milestone {
                id: 0,
                description: "Implement auth module".into(),
                deliverable: "src/auth.rs".into(),
                depends_on: vec![],
            },
            Milestone {
                id: 1,
                description: "Add tests for auth".into(),
                deliverable: "tests/auth_test.rs".into(),
                depends_on: vec![0],
            },
            Milestone {
                id: 2,
                description: "Build and verify".into(),
                deliverable: "build output".into(),
                depends_on: vec![0, 1],
            },
            Milestone {
                id: 3,
                description: "Update documentation".into(),
                deliverable: "docs/auth.md".into(),
                depends_on: vec![0],
            },
        ]
    }

    #[test]
    fn trivial_collapses_all_milestones_into_single_batch() {
        let expander = MilestoneExpander::new();
        let assessment = trivial_assessment();
        let segments = expander.expand(&sample_milestones(), &assessment, &[], None);

        assert_eq!(segments.len(), 4);
    }

    #[test]
    fn moderate_expands_all_remaining_milestones() {
        let expander = MilestoneExpander::new();
        let assessment = moderate_assessment();
        let segments = expander.expand(&sample_milestones(), &assessment, &[], None);

        assert_eq!(segments.len(), 4);
    }

    #[test]
    fn complex_rolling_wave_expands_only_two() {
        let expander = MilestoneExpander::new();
        let assessment = complex_assessment();
        let segments = expander.expand(&sample_milestones(), &assessment, &[], None);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].milestone_id, 0);
        assert_eq!(segments[1].milestone_id, 1);
    }

    #[test]
    fn skips_completed_milestones() {
        let expander = MilestoneExpander::new();
        let assessment = moderate_assessment();
        let segments = expander.expand(&sample_milestones(), &assessment, &[0, 1], None);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].milestone_id, 2);
        assert_eq!(segments[1].milestone_id, 3);
    }

    #[test]
    fn empty_milestones_produces_no_segments() {
        let expander = MilestoneExpander::new();
        let assessment = trivial_assessment();
        let segments = expander.expand(&[], &assessment, &[], None);

        assert!(segments.is_empty());
    }

    #[test]
    fn all_completed_produces_no_segments() {
        let expander = MilestoneExpander::new();
        let assessment = moderate_assessment();
        let milestones = sample_milestones();
        let all_ids: Vec<usize> = milestones.iter().map(|m| m.id).collect();
        let segments = expander.expand(&milestones, &assessment, &all_ids, None);

        assert!(segments.is_empty());
    }

    #[test]
    fn test_milestone_produces_verification_step() {
        let expander = MilestoneExpander::new();
        let assessment = moderate_assessment();
        let milestones = vec![Milestone {
            id: 0,
            description: "Add unit tests for parser".into(),
            deliverable: "tests/parser_test.rs".into(),
            depends_on: vec![],
        }];
        let segments = expander.expand(&milestones, &assessment, &[], None);

        assert_eq!(segments.len(), 1);
        assert!(segments[0].steps.len() >= 2);
        assert!(segments[0].steps[1].verification_command.is_some());
    }

    #[test]
    fn edit_milestone_extracts_file_targets() {
        let expander = MilestoneExpander::new();
        let assessment = moderate_assessment();
        let milestones = vec![Milestone {
            id: 0,
            description: "Implement auth handler".into(),
            deliverable: "src/auth/handler.rs".into(),
            depends_on: vec![],
        }];
        let segments = expander.expand(&milestones, &assessment, &[], None);

        assert_eq!(segments[0].steps[0].file_targets.len(), 1);
        assert_eq!(
            segments[0].steps[0].file_targets[0],
            PathBuf::from("src/auth/handler.rs")
        );
    }

    #[test]
    fn complex_second_wave_after_completion() {
        let expander = MilestoneExpander::new();
        let assessment = complex_assessment();
        let milestones = sample_milestones();

        // First wave expands 0 and 1.
        let wave1 = expander.expand(&milestones, &assessment, &[], None);
        assert_eq!(wave1.len(), 2);

        // After completing 0 and 1, second wave expands 2 and 3.
        let wave2 = expander.expand(&milestones, &assessment, &[0, 1], None);
        assert_eq!(wave2.len(), 2);
        assert_eq!(wave2[0].milestone_id, 2);
        assert_eq!(wave2[1].milestone_id, 3);
    }

    #[test]
    fn edge_cases_populated_from_brief_risks_and_constraints() {
        let expander = MilestoneExpander::new();
        let assessment = complex_assessment();
        let brief = ContextBrief {
            relevant_files: vec![],
            patterns_found: vec![],
            dependencies: vec![],
            risks: vec!["token expiry edge case".into()],
            constraints: vec!["no external state".into()],
        };
        let milestones = vec![Milestone {
            id: 0,
            description: "Implement auth".into(),
            deliverable: "src/auth.rs".into(),
            depends_on: vec![],
        }];
        let segments = expander.expand(&milestones, &assessment, &[], Some(&brief));

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].edge_cases.len(), 2);
        assert!(segments[0].edge_cases[0].contains("token expiry"));
        assert!(segments[0].edge_cases[1].contains("no external state"));
    }

    #[test]
    fn edge_cases_empty_when_no_brief() {
        let expander = MilestoneExpander::new();
        let assessment = trivial_assessment();
        let milestones = vec![Milestone {
            id: 0,
            description: "Fix typo".into(),
            deliverable: "README.md".into(),
            depends_on: vec![],
        }];
        let segments = expander.expand(&milestones, &assessment, &[], None);

        assert!(segments[0].edge_cases.is_empty());
    }

    #[test]
    fn required_criteria_propagated_to_segment() {
        let expander = MilestoneExpander::new();
        let assessment = complex_assessment();
        let milestones = vec![Milestone {
            id: 0,
            description: "Implement feature".into(),
            deliverable: "src/feature.rs".into(),
            depends_on: vec![],
        }];
        let segments = expander.expand(&milestones, &assessment, &[], None);

        assert!(!segments[0].required_criteria.is_empty());
        assert_eq!(
            segments[0].required_criteria.len(),
            assessment.success_criteria.len()
        );
    }

    #[allow(clippy::no_effect_underscore_binding)]
    #[test]
    fn default_impl_matches_new() {
        let _default: MilestoneExpander = MilestoneExpander;
        let _new: MilestoneExpander = MilestoneExpander::new();
    }
}
