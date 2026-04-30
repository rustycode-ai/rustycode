//! Skeleton builder for AST Phase 2: SKELETON.
//!
//! Produces a `MilestoneSkeleton` from a `TaskAssessment` and `ContextBrief`.
//! The number of milestones is proportional to task complexity:
//! - Trivial: 1 milestone
//! - Moderate: 3-5 milestones
//! - Complex: 5-7 milestones

use super::types::{ComplexityLevel, ContextBrief, Milestone, MilestoneSkeleton, TaskAssessment};

/// Builds a milestone skeleton from classification and research results.
pub struct SkeletonBuilder;

impl SkeletonBuilder {
    pub const fn new() -> Self {
        Self
    }

    /// Build a milestone skeleton for the given task assessment and context brief.
    pub fn build(&self, assessment: &TaskAssessment, brief: &ContextBrief) -> MilestoneSkeleton {
        let milestone_count = Self::target_milestone_count(assessment.complexity, brief);
        let milestones = Self::generate_milestones(milestone_count, assessment, brief);
        MilestoneSkeleton { milestones }
    }

    /// Determine the target number of milestones based on complexity and brief.
    fn target_milestone_count(complexity: ComplexityLevel, brief: &ContextBrief) -> usize {
        match complexity {
            ComplexityLevel::Trivial => 1,
            ComplexityLevel::Moderate => {
                // Scale up with number of relevant files
                let file_factor = (brief.relevant_files.len() / 5).min(2);
                3 + file_factor // 3-5
            }
            ComplexityLevel::Complex => {
                // Scale up with file count and risk count
                let file_factor = (brief.relevant_files.len() / 10).min(2);
                let risk_factor = (brief.risks.len()).min(1);
                5 + file_factor + risk_factor // 5-7, capped
            }
        }
        .min(7) // Hard cap
    }

    /// Generate milestones with descriptions, deliverables, and dependencies.
    fn generate_milestones(
        count: usize,
        assessment: &TaskAssessment,
        brief: &ContextBrief,
    ) -> Vec<Milestone> {
        match assessment.complexity {
            ComplexityLevel::Trivial => Self::trivial_skeleton(assessment),
            ComplexityLevel::Moderate => Self::moderate_skeleton(count, assessment, brief),
            ComplexityLevel::Complex => Self::complex_skeleton(count, assessment, brief),
        }
    }

    fn trivial_skeleton(assessment: &TaskAssessment) -> Vec<Milestone> {
        vec![Milestone {
            id: 0,
            description: assessment.task_summary.clone(),
            deliverable: Self::deliverable_for_summary(&assessment.task_summary),
            depends_on: vec![],
        }]
    }

    fn moderate_skeleton(
        count: usize,
        assessment: &TaskAssessment,
        brief: &ContextBrief,
    ) -> Vec<Milestone> {
        let mut milestones = Vec::with_capacity(count);
        let summary = &assessment.task_summary;
        let file_hint = Self::primary_file_hint(brief);

        // M0: Setup / scaffolding
        milestones.push(Milestone {
            id: 0,
            description: format!("Setup: prepare workspace for {summary}"),
            deliverable: format!(
                "Scaffolding and imports in place{}",
                file_hint
                    .as_ref()
                    .map(|f| format!(" ({f})"))
                    .unwrap_or_default()
            ),
            depends_on: vec![],
        });

        // M1: Core implementation
        milestones.push(Milestone {
            id: 1,
            description: format!("Implement core logic for {summary}"),
            deliverable: "Core logic implemented and compilable".into(),
            depends_on: vec![0],
        });

        // M2: Integration / wiring
        milestones.push(Milestone {
            id: 2,
            description: format!("Integrate changes with existing modules for {summary}"),
            deliverable: "Integration complete, no compilation errors".into(),
            depends_on: vec![1],
        });

        // M3: Tests (if room)
        if count >= 4 {
            milestones.push(Milestone {
                id: 3,
                description: format!("Add tests for {summary}"),
                deliverable: "Unit and/or integration tests passing".into(),
                depends_on: vec![2],
            });
        }

        // M4: Polish / verification (if room)
        if count >= 5 {
            milestones.push(Milestone {
                id: 4,
                description: format!("Verify and polish {summary}"),
                deliverable: "All criteria verified, no lint warnings".into(),
                depends_on: vec![if count >= 4 { 3 } else { 2 }],
            });
        }

        milestones
    }

    fn complex_skeleton(
        count: usize,
        assessment: &TaskAssessment,
        brief: &ContextBrief,
    ) -> Vec<Milestone> {
        let mut milestones = Vec::with_capacity(count);
        let summary = &assessment.task_summary;
        let file_hint = Self::primary_file_hint(brief);
        let has_tests = brief.patterns_found.iter().any(|p| p.contains("test"));

        // M0: Research / design
        milestones.push(Milestone {
            id: 0,
            description: format!("Design: plan architecture for {summary}"),
            deliverable: "Design document with interfaces and data flows".into(),
            depends_on: vec![],
        });

        // M1: Types and interfaces
        milestones.push(Milestone {
            id: 1,
            description: format!("Define types and interfaces for {summary}"),
            deliverable: format!(
                "Type definitions and trait signatures{}",
                file_hint
                    .as_ref()
                    .map(|f| format!(" in {f}"))
                    .unwrap_or_default()
            ),
            depends_on: vec![0],
        });

        // M2: Core implementation
        milestones.push(Milestone {
            id: 2,
            description: format!("Implement core module for {summary}"),
            deliverable: "Core module compiles and unit tests pass".into(),
            depends_on: vec![1],
        });

        // M3: Integration
        milestones.push(Milestone {
            id: 3,
            description: format!("Integrate with existing systems for {summary}"),
            deliverable: "Integration complete, workspace compiles".into(),
            depends_on: vec![2],
        });

        // M4: Comprehensive tests
        if count >= 5 {
            let depends = if has_tests { vec![3] } else { vec![2, 3] };
            milestones.push(Milestone {
                id: 4,
                description: format!("Add comprehensive tests for {summary}"),
                deliverable: "Test suite passes, coverage meets target".into(),
                depends_on: depends,
            });
        }

        // M5: Edge cases and error handling
        if count >= 6 {
            milestones.push(Milestone {
                id: 5,
                description: format!("Handle edge cases and errors for {summary}"),
                deliverable: "Error paths covered, no panics in edge cases".into(),
                depends_on: vec![4],
            });
        }

        // M6: Verification and polish
        if count >= 7 {
            milestones.push(Milestone {
                id: 6,
                description: format!("Final verification and polish for {summary}"),
                deliverable: "All success criteria verified, clean clippy".into(),
                depends_on: vec![5],
            });
        }

        milestones
    }

    /// Extract a hint about the primary file from the brief.
    fn primary_file_hint(brief: &ContextBrief) -> Option<String> {
        brief
            .relevant_files
            .first()
            .and_then(|f| f.file_name().and_then(|n| n.to_str()).map(String::from))
    }

    /// Generate a concise deliverable description from a summary.
    fn deliverable_for_summary(summary: &str) -> String {
        let lower = summary.to_lowercase();
        if lower.contains("test") {
            "Test suite passing".into()
        } else if lower.contains("fix") {
            "Bug fixed, tests passing".into()
        } else if lower.contains("add") || lower.contains("implement") {
            "Feature implemented, workspace compiles".into()
        } else if lower.contains("refactor") {
            "Refactored code compiles, tests pass".into()
        } else {
            "Change applied, workspace compiles".into()
        }
    }
}

impl Default for SkeletonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn trivial_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Fix typo in README".into(),
            complexity: ComplexityLevel::Trivial,
            success_criteria: vec![],
            route: super::super::types::PhaseRoute::DirectExecute,
            clarity: None,
        }
    }

    fn moderate_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Add unit tests for the auth module".into(),
            complexity: ComplexityLevel::Moderate,
            success_criteria: vec![],
            route: super::super::types::PhaseRoute::StandardSequence,
            clarity: None,
        }
    }

    fn complex_assessment() -> TaskAssessment {
        TaskAssessment {
            task_summary: "Implement JWT auth with refresh tokens".into(),
            complexity: ComplexityLevel::Complex,
            success_criteria: vec![],
            route: super::super::types::PhaseRoute::RollingWave,
            clarity: None,
        }
    }

    fn empty_brief() -> ContextBrief {
        ContextBrief {
            relevant_files: vec![],
            patterns_found: vec![],
            dependencies: vec![],
            risks: vec![],
            constraints: vec![],
        }
    }

    fn brief_with_files(n: usize) -> ContextBrief {
        ContextBrief {
            relevant_files: (0..n)
                .map(|i| PathBuf::from(format!("src/module_{i}.rs")))
                .collect(),
            patterns_found: vec!["integration_tests_present".into()],
            dependencies: vec!["serde".into()],
            risks: vec![],
            constraints: vec!["language: rust".into()],
        }
    }

    fn brief_with_risks() -> ContextBrief {
        ContextBrief {
            relevant_files: (0..15)
                .map(|i| PathBuf::from(format!("src/file_{i}.rs")))
                .collect(),
            patterns_found: vec![],
            dependencies: vec![],
            risks: vec!["high_file_count: 15 files".into()],
            constraints: vec![],
        }
    }

    #[test]
    fn trivial_produces_single_milestone() {
        let builder = SkeletonBuilder::new();
        let skeleton = builder.build(&trivial_assessment(), &empty_brief());

        assert_eq!(skeleton.len(), 1);
        assert!(skeleton.milestones[0].depends_on.is_empty());
    }

    #[test]
    fn trivial_milestone_has_no_dependencies() {
        let builder = SkeletonBuilder::new();
        let skeleton = builder.build(&trivial_assessment(), &empty_brief());

        let m = &skeleton.milestones[0];
        assert!(m.depends_on.is_empty());
        assert_eq!(m.id, 0);
    }

    #[test]
    fn moderate_produces_three_to_five_milestones() {
        let builder = SkeletonBuilder::new();

        let skeleton = builder.build(&moderate_assessment(), &empty_brief());
        assert!(
            (3..=5).contains(&skeleton.len()),
            "Moderate should produce 3-5 milestones, got {}",
            skeleton.len()
        );

        // With many files, should scale up
        let skeleton = builder.build(&moderate_assessment(), &brief_with_files(10));
        assert!(
            (3..=5).contains(&skeleton.len()),
            "Moderate with files should produce 3-5 milestones, got {}",
            skeleton.len()
        );
    }

    #[test]
    fn moderate_milestones_form_a_chain() {
        let builder = SkeletonBuilder::new();
        let skeleton = builder.build(&moderate_assessment(), &empty_brief());

        // Each milestone depends on the previous one
        for i in 1..skeleton.len() {
            let deps = &skeleton.milestones[i].depends_on;
            assert!(
                deps.contains(&(i - 1)),
                "Milestone {i} should depend on milestone {}",
                i - 1
            );
        }
    }

    #[test]
    fn complex_produces_five_to_seven_milestones() {
        let builder = SkeletonBuilder::new();

        let skeleton = builder.build(&complex_assessment(), &empty_brief());
        assert!(
            (5..=7).contains(&skeleton.len()),
            "Complex should produce 5-7 milestones, got {}",
            skeleton.len()
        );
    }

    #[test]
    fn complex_with_risks_scales_up() {
        let builder = SkeletonBuilder::new();

        let empty = builder.build(&complex_assessment(), &empty_brief());
        let with_risks = builder.build(&complex_assessment(), &brief_with_risks());

        assert!(
            with_risks.len() >= empty.len(),
            "Complex with risks should produce at least as many milestones"
        );
    }

    #[test]
    fn complex_milestones_start_with_design() {
        let builder = SkeletonBuilder::new();
        let skeleton = builder.build(&complex_assessment(), &brief_with_files(5));

        let first = &skeleton.milestones[0];
        let desc_lower = first.description.to_lowercase();
        assert!(
            desc_lower.contains("design") || desc_lower.contains("plan"),
            "First complex milestone should be design/planning: {}",
            first.description
        );
    }

    #[test]
    fn all_milestone_ids_are_sequential() {
        let builder = SkeletonBuilder::new();

        for (assessment, brief) in [
            (trivial_assessment(), empty_brief()),
            (moderate_assessment(), empty_brief()),
            (complex_assessment(), brief_with_files(5)),
        ] {
            let skeleton = builder.build(&assessment, &brief);
            let ids: Vec<usize> = skeleton.milestones.iter().map(|m| m.id).collect();
            let expected: Vec<usize> = (0..skeleton.len()).collect();
            assert_eq!(ids, expected, "IDs should be sequential from 0");
        }
    }

    #[test]
    fn dependencies_reference_valid_milestones() {
        let builder = SkeletonBuilder::new();
        let skeleton = builder.build(&complex_assessment(), &brief_with_files(5));

        let max_id = skeleton.len();
        for milestone in &skeleton.milestones {
            for dep in &milestone.depends_on {
                assert!(
                    *dep < max_id,
                    "Dependency {dep} references non-existent milestone (max: {})",
                    max_id - 1
                );
            }
        }
    }

    #[test]
    fn no_circular_dependencies() {
        let builder = SkeletonBuilder::new();
        let skeleton = builder.build(&complex_assessment(), &brief_with_files(5));

        // Each milestone should only depend on milestones with lower IDs
        for milestone in &skeleton.milestones {
            for dep in &milestone.depends_on {
                assert!(
                    *dep < milestone.id,
                    "Milestone {} depends on milestone {}, which creates a cycle",
                    milestone.id,
                    dep
                );
            }
        }
    }

    #[test]
    fn first_milestone_never_has_dependencies() {
        let builder = SkeletonBuilder::new();

        for (assessment, brief) in [
            (trivial_assessment(), empty_brief()),
            (moderate_assessment(), empty_brief()),
            (complex_assessment(), empty_brief()),
        ] {
            let skeleton = builder.build(&assessment, &brief);
            assert!(
                skeleton.milestones[0].depends_on.is_empty(),
                "First milestone should have no dependencies"
            );
        }
    }

    #[test]
    fn deliverable_is_non_empty() {
        let builder = SkeletonBuilder::new();
        let skeleton = builder.build(&complex_assessment(), &brief_with_files(5));

        for milestone in &skeleton.milestones {
            assert!(
                !milestone.deliverable.is_empty(),
                "Milestone {} should have a non-empty deliverable",
                milestone.id
            );
        }
    }

    #[test]
    fn milestone_count_never_exceeds_seven() {
        let builder = SkeletonBuilder::new();

        // Even with many files and risks, cap at 7
        let big_brief = ContextBrief {
            relevant_files: (0..100)
                .map(|i| PathBuf::from(format!("src/file_{i}.rs")))
                .collect(),
            patterns_found: vec![],
            dependencies: vec![],
            risks: vec!["risk1".into(), "risk2".into(), "risk3".into()],
            constraints: vec![],
        };

        let skeleton = builder.build(&complex_assessment(), &big_brief);
        assert!(
            skeleton.len() <= 7,
            "Milestone count should never exceed 7, got {}",
            skeleton.len()
        );
    }

    #[test]
    fn ready_milestones_works_with_generated_skeleton() {
        let builder = SkeletonBuilder::new();
        let skeleton = builder.build(&moderate_assessment(), &brief_with_files(5));

        // Initially, only milestone 0 should be ready
        let ready = skeleton.ready_milestones(&[], &[]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, 0);

        // After completing 0, milestone 1 should be ready
        let ready = skeleton.ready_milestones(&[0], &[]);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, 1);
    }

    // -- US-004: edge-case tests --

    #[test]
    fn circular_dependencies_detected() {
        // Milestone 0 depends on 1, milestone 1 depends on 0 — circular
        let skeleton = MilestoneSkeleton {
            milestones: vec![
                Milestone {
                    id: 0,
                    description: "A".into(),
                    deliverable: "a".into(),
                    depends_on: vec![1],
                },
                Milestone {
                    id: 1,
                    description: "B".into(),
                    deliverable: "b".into(),
                    depends_on: vec![0],
                },
            ],
        };
        // No milestones should be ready (deadlock)
        let ready = skeleton.ready_milestones(&[], &[]);
        assert!(
            ready.is_empty(),
            "circular dependencies should produce no ready milestones"
        );
    }

    #[test]
    fn self_dependent_milestone_never_ready() {
        let skeleton = MilestoneSkeleton {
            milestones: vec![Milestone {
                id: 0,
                description: "Self-referential".into(),
                deliverable: "x".into(),
                depends_on: vec![0],
            }],
        };
        let ready = skeleton.ready_milestones(&[], &[]);
        assert!(
            ready.is_empty(),
            "self-dependent milestone should never be ready"
        );
    }

    #[test]
    fn empty_skeleton_produces_no_ready() {
        let skeleton = MilestoneSkeleton { milestones: vec![] };
        let ready = skeleton.ready_milestones(&[], &[]);
        assert!(ready.is_empty());
        assert!(skeleton.is_empty());
        assert_eq!(skeleton.len(), 0);
    }

    #[test]
    fn complex_with_risks_produces_extra_milestones() {
        let builder = SkeletonBuilder::new();
        let skeleton = builder.build(&complex_assessment(), &brief_with_risks());
        // Complex with risks should produce more milestones than without
        assert!(
            skeleton.len() >= 4,
            "complex with risks should produce >= 4 milestones, got {}",
            skeleton.len()
        );
    }
}
