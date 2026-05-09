//! Plan Templates - Predefined plan structures for common tasks
//!
//! This module provides templates for creating plans for common development tasks,
//! reducing the time needed to create plans from scratch.

use chrono::Utc;
use rustycode_protocol::{
    Milestone, MilestoneId, MilestoneStatus, Plan, PlanDependency, PlanId, PlanStatus, PlanStep,
    SessionId,
};

/// Template types for common development tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlanTemplate {
    /// Implement a new feature
    NewFeature,
    /// Fix a bug
    BugFix,
    /// Refactor code
    Refactor,
    /// Add tests
    AddTests,
    /// Performance optimization
    Performance,
    /// Documentation
    Documentation,
    /// Security fix
    SecurityFix,
    /// Dependency update
    DependencyUpdate,
}

/// Output of a milestone template: a milestone plus its empty plan shells.
#[derive(Debug, Clone, PartialEq)]
pub struct MilestoneBlueprint {
    pub milestone: Milestone,
    pub plans: Vec<Plan>,
}

/// Template types for common milestone-level workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MilestoneTemplate {
    /// 3-5 plans: research, implementation, tests, integration
    NewFeature,
    /// 2-3 plans: reproduction, fix, regression tests
    BugInvestigation,
    /// 4-6 plans: scaffolding, migration, cleanup, tests
    MajorRefactor,
}

impl PlanTemplate {
    /// Get a human-readable description of this template
    #[allow(dead_code)] // Kept for future use
    pub const fn description(&self) -> &str {
        match self {
            Self::NewFeature => "Implement a new feature from scratch",
            Self::BugFix => "Fix a reported bug",
            Self::Refactor => "Refactor existing code for better structure",
            Self::AddTests => "Add test coverage for existing code",
            Self::Performance => "Optimize performance of existing code",
            Self::Documentation => "Add or update documentation",
            Self::SecurityFix => "Fix a security vulnerability",
            Self::DependencyUpdate => "Update project dependencies",
        }
    }

    /// Create a plan from this template
    pub fn create_plan(
        &self,
        session_id: SessionId,
        task: String,
        summary: String,
        files_to_modify: Vec<String>,
    ) -> Plan {
        Plan {
            id: PlanId::new(),
            session_id,
            task,
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary,
            approach: self.approach(),
            steps: self.steps(),
            files_to_modify,
            risks: self.risks(),
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        }
    }

    /// Helper to create a `PlanStep` with reduced boilerplate
    fn step(
        order: usize,
        title: &str,
        description: &str,
        tools: &[&str],
        expected_outcome: &str,
        rollback_hint: &str,
    ) -> PlanStep {
        PlanStep {
            order,
            title: title.to_string(),
            description: description.to_string(),
            tools: tools.iter().map(|&s| s.to_string()).collect(),
            expected_outcome: expected_outcome.to_string(),
            rollback_hint: rollback_hint.to_string(),
            execution_status: Default::default(),
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        }
    }

    /// Get the approach description for this template
    fn approach(&self) -> String {
        let steps = match self {
            Self::NewFeature => &[
                "Research existing codebase patterns",
                "Design the feature architecture",
                "Implement core functionality",
                "Add error handling",
                "Write tests",
                "Update documentation",
                "Code review and cleanup",
            ],
            Self::BugFix => &[
                "Reproduce the bug",
                "Identify root cause",
                "Write failing test",
                "Implement fix",
                "Verify test passes",
                "Check for regressions",
                "Update documentation if needed",
            ],
            Self::Refactor => &[
                "Analyze current implementation",
                "Identify refactoring opportunities",
                "Write tests for existing behavior",
                "Apply refactoring changes",
                "Verify tests still pass",
                "Update documentation",
                "Code review",
            ],
            Self::AddTests => &[
                "Identify untested code paths",
                "Design test cases",
                "Write unit tests",
                "Write integration tests",
                "Verify coverage",
                "Document test approach",
                "Review test quality",
            ],
            Self::Performance => &[
                "Profile and identify bottlenecks",
                "Set performance benchmarks",
                "Implement optimizations",
                "Measure improvements",
                "Add performance tests",
                "Document findings",
                "Monitor in production",
            ],
            Self::Documentation => &[
                "Identify documentation gaps",
                "Structure documentation",
                "Write content",
                "Add examples",
                "Review for clarity",
                "Update table of contents",
                "Publish documentation",
            ],
            Self::SecurityFix => &[
                "Understand vulnerability",
                "Identify affected code",
                "Write security test",
                "Implement fix",
                "Verify fix works",
                "Check for similar issues",
                "Update security documentation",
            ],
            Self::DependencyUpdate => &[
                "Check for breaking changes",
                "Update dependencies",
                "Fix compilation issues",
                "Run tests",
                "Update documentation",
                "Test in staging environment",
                "Monitor for issues",
            ],
        };
        steps
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {}", i + 1, s))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get the default steps for this template
    fn steps(&self) -> Vec<PlanStep> {
        match self {
            Self::NewFeature => vec![
                Self::step(
                    0,
                    "Research and Analysis",
                    "Analyze existing codebase patterns and related features",
                    &["Read", "Grep", "Glob"],
                    "Understanding of existing patterns and where to integrate new feature",
                    "No changes made in this step",
                ),
                Self::step(
                    1,
                    "Design Feature Architecture",
                    "Design the structure and interfaces for the new feature",
                    &["Write"],
                    "Design document or architecture plan",
                    "Delete design document if not needed",
                ),
                Self::step(
                    2,
                    "Implement Core Functionality",
                    "Implement the main feature logic",
                    &["Write", "edit"],
                    "Working implementation of the feature",
                    "Revert changes or delete new files",
                ),
                Self::step(
                    3,
                    "Add Error Handling",
                    "Add comprehensive error handling and validation",
                    &["edit"],
                    "Robust error handling in place",
                    "Revert error handling changes",
                ),
                Self::step(
                    4,
                    "Write Tests",
                    "Write unit and integration tests for the feature",
                    &["Write", "Bash"],
                    "Tests passing with good coverage",
                    "Delete test files if not needed",
                ),
                Self::step(
                    5,
                    "Update Documentation",
                    "Update relevant documentation and add examples",
                    &["edit", "Write"],
                    "Documentation updated with feature details",
                    "Revert documentation changes",
                ),
            ],
            Self::BugFix => vec![
                Self::step(
                    0,
                    "Reproduce the Bug",
                    "Create a minimal reproduction of the bug",
                    &["Read", "Bash"],
                    "Clear understanding of how to reproduce the bug",
                    "No changes made in this step",
                ),
                Self::step(
                    1,
                    "Identify Root Cause",
                    "Debug to find the root cause of the bug",
                    &["Read", "Grep", "LspDefinition"],
                    "Identification of the code causing the bug",
                    "No changes made in this step",
                ),
                Self::step(
                    2,
                    "Write Failing Test",
                    "Write a test that reproduces the bug",
                    &["Write"],
                    "Test that fails due to the bug",
                    "Delete the test file",
                ),
                Self::step(
                    3,
                    "Implement Fix",
                    "Fix the bug in the code",
                    &["edit"],
                    "Test now passes, bug is fixed",
                    "Revert the fix",
                ),
                Self::step(
                    4,
                    "Check for Regressions",
                    "Run all tests to ensure no regressions",
                    &["Bash"],
                    "All tests passing",
                    "No changes made in this step",
                ),
            ],
            Self::Refactor => vec![
                Self::step(
                    0,
                    "Analyze Current Implementation",
                    "Understand the current code structure",
                    &["Read", "LspDocumentSymbols"],
                    "Understanding of current implementation",
                    "No changes made in this step",
                ),
                Self::step(
                    1,
                    "Identify Refactoring Opportunities",
                    "Identify areas for improvement",
                    &["Grep"],
                    "List of refactoring opportunities",
                    "No changes made in this step",
                ),
                Self::step(
                    2,
                    "Write Tests for Existing Behavior",
                    "Ensure current behavior is captured in tests",
                    &["Write"],
                    "Tests covering current behavior",
                    "Delete test files if not needed",
                ),
                Self::step(
                    3,
                    "Apply Refactoring",
                    "Refactor the code",
                    &["edit", "MultiEdit"],
                    "Refactored code with same behavior",
                    "Revert refactoring changes",
                ),
                Self::step(
                    4,
                    "Verify Tests Still Pass",
                    "Ensure refactoring didn't break anything",
                    &["Bash"],
                    "All tests passing",
                    "No changes made in this step",
                ),
            ],
            Self::AddTests => vec![
                Self::step(
                    0,
                    "Identify Untested Code",
                    "Find code paths that need tests",
                    &["Read", "Grep"],
                    "List of untested code paths",
                    "No changes made in this step",
                ),
                Self::step(
                    1,
                    "Design Test Cases",
                    "Plan test cases for coverage",
                    &[],
                    "Test case design document",
                    "No changes made in this step",
                ),
                Self::step(
                    2,
                    "Write Unit Tests",
                    "Write unit tests for individual functions",
                    &["Write"],
                    "Unit tests covering main functionality",
                    "Delete test files if not needed",
                ),
                Self::step(
                    3,
                    "Write Integration Tests",
                    "Write integration tests for component interactions",
                    &["Write"],
                    "Integration tests covering interactions",
                    "Delete test files if not needed",
                ),
                Self::step(
                    4,
                    "Verify Coverage",
                    "Check test coverage metrics",
                    &["Bash"],
                    "Coverage report showing good coverage",
                    "No changes made in this step",
                ),
            ],
            Self::Performance => vec![
                Self::step(
                    0,
                    "Profile and Identify Bottlenecks",
                    "Profile the code to find slow spots",
                    &["Bash"],
                    "List of performance bottlenecks",
                    "No changes made in this step",
                ),
                Self::step(
                    1,
                    "Set Performance Benchmarks",
                    "Create benchmarks to measure performance",
                    &["Write"],
                    "Benchmark tests for measuring performance",
                    "Delete benchmark files if not needed",
                ),
                Self::step(
                    2,
                    "Implement Optimizations",
                    "Apply performance optimizations",
                    &["edit"],
                    "Optimized code",
                    "Revert optimization changes",
                ),
                Self::step(
                    3,
                    "Measure Improvements",
                    "Run benchmarks to verify improvements",
                    &["Bash"],
                    "Performance improvement metrics",
                    "No changes made in this step",
                ),
                Self::step(
                    4,
                    "Add Performance Tests",
                    "Add tests to ensure performance doesn't regress",
                    &["Write"],
                    "Performance tests in place",
                    "Delete test files if not needed",
                ),
            ],
            Self::Documentation => vec![
                Self::step(
                    0,
                    "Identify Documentation Gaps",
                    "Find areas that need documentation",
                    &["Read", "Glob"],
                    "List of documentation gaps",
                    "No changes made in this step",
                ),
                Self::step(
                    1,
                    "Structure Documentation",
                    "Plan documentation structure",
                    &[],
                    "Documentation structure outline",
                    "No changes made in this step",
                ),
                Self::step(
                    2,
                    "Write Content",
                    "Write the documentation content",
                    &["Write", "edit"],
                    "Complete documentation",
                    "Revert documentation changes",
                ),
                Self::step(
                    3,
                    "Add Examples",
                    "Add usage examples",
                    &["Write"],
                    "Working examples",
                    "Delete example files if not needed",
                ),
                Self::step(
                    4,
                    "Review for Clarity",
                    "Review documentation for clarity and completeness",
                    &["Read"],
                    "Reviewed and polished documentation",
                    "No changes made in this step",
                ),
            ],
            Self::SecurityFix => vec![
                Self::step(
                    0,
                    "Understand Vulnerability",
                    "Research and understand the security vulnerability",
                    &["WebFetch"],
                    "Understanding of the vulnerability",
                    "No changes made in this step",
                ),
                Self::step(
                    1,
                    "Identify Affected Code",
                    "Find all code affected by the vulnerability",
                    &["Grep", "Glob"],
                    "List of affected code locations",
                    "No changes made in this step",
                ),
                Self::step(
                    2,
                    "Write Security Test",
                    "Write a test that demonstrates the vulnerability",
                    &["Write"],
                    "Test that exposes the vulnerability",
                    "Delete test file if not needed",
                ),
                Self::step(
                    3,
                    "Implement Fix",
                    "Fix the security vulnerability",
                    &["edit"],
                    "Vulnerability is fixed",
                    "Revert the fix",
                ),
                Self::step(
                    4,
                    "Verify Fix Works",
                    "Run the security test to verify the fix",
                    &["Bash"],
                    "Security test passes",
                    "No changes made in this step",
                ),
                Self::step(
                    5,
                    "Check for Similar Issues",
                    "Search codebase for similar vulnerabilities",
                    &["Grep"],
                    "List of similar issues to fix",
                    "No changes made in this step",
                ),
            ],
            Self::DependencyUpdate => vec![
                Self::step(
                    0,
                    "Check for Breaking Changes",
                    "Review release notes for breaking changes",
                    &["WebFetch"],
                    "List of breaking changes to handle",
                    "No changes made in this step",
                ),
                Self::step(
                    1,
                    "Update Dependencies",
                    "Update the dependency versions",
                    &["edit"],
                    "Dependencies updated to new versions",
                    "Revert dependency version changes",
                ),
                Self::step(
                    2,
                    "Fix Compilation Issues",
                    "Fix any compilation errors from API changes",
                    &["edit", "Bash"],
                    "Code compiles successfully",
                    "Revert compilation fixes",
                ),
                Self::step(
                    3,
                    "Run Tests",
                    "Run all tests to ensure compatibility",
                    &["Bash"],
                    "All tests passing",
                    "No changes made in this step",
                ),
                Self::step(
                    4,
                    "Update Documentation",
                    "Update documentation if API changed",
                    &["edit"],
                    "Documentation updated",
                    "Revert documentation changes",
                ),
                Self::step(
                    5,
                    "Test in Staging",
                    "Deploy to staging and test",
                    &["Bash"],
                    "Staging tests pass",
                    "No changes made in this step",
                ),
            ],
        }
    }

    /// Get common risks for this template
    fn risks(&self) -> Vec<String> {
        let risks: &[&str] = match self {
            Self::NewFeature => &[
                "Feature may not integrate well with existing code",
                "May introduce unexpected bugs in related functionality",
                "Performance may be worse than expected",
                "User interface may need iteration",
            ],
            Self::BugFix => &[
                "Fix may break other functionality",
                "Root cause may be deeper than initially thought",
                "Fix may introduce performance regressions",
            ],
            Self::Refactor => &[
                "Refactoring may introduce subtle bugs",
                "Tests may not cover all edge cases",
                "Refactoring may take longer than estimated",
            ],
            Self::AddTests => &[
                "Tests may not cover all edge cases",
                "Tests may be slow or flaky",
                "May need to refactor code to make it testable",
            ],
            Self::Performance => &[
                "Optimization may make code harder to maintain",
                "Performance improvements may be less than expected",
                "May need to change APIs for better performance",
            ],
            Self::Documentation => &[
                "Documentation may become outdated quickly",
                "Examples may not cover all use cases",
                "Documentation may be unclear or incomplete",
            ],
            Self::SecurityFix => &[
                "Fix may break existing functionality",
                "Similar vulnerabilities may exist elsewhere",
                "Fix may introduce performance overhead",
            ],
            Self::DependencyUpdate => &[
                "New version may have breaking changes",
                "New version may have new bugs",
                "May introduce unexpected compatibility issues",
            ],
        };
        risks.iter().map(|&s| s.to_string()).collect()
    }
}

impl MilestoneTemplate {
    pub const fn description(&self) -> &str {
        match self {
            Self::NewFeature => {
                "Decompose a new feature into research, implementation, and verification plans"
            }
            Self::BugInvestigation => {
                "Break a bug investigation into reproduction, diagnosis, fix, and regression plans"
            }
            Self::MajorRefactor => {
                "Split a large refactor into scaffolding, migration, cleanup, and tests"
            }
        }
    }

    pub fn create_milestone(
        &self,
        session_id: SessionId,
        title: String,
        description: String,
    ) -> MilestoneBlueprint {
        let milestone_id = MilestoneId::new();
        let now = Utc::now();

        let plan_specs = self.plan_specs();
        let mut plans = Vec::with_capacity(plan_specs.len());
        let mut dependencies = Vec::with_capacity(plan_specs.len());
        let mut plan_ids = Vec::with_capacity(plan_specs.len());

        for spec in plan_specs {
            let plan_id = PlanId::new();
            let plan = Plan {
                id: plan_id.clone(),
                session_id: session_id.clone(),
                milestone_id: Some(milestone_id.clone()),
                task: format!("{title}: {}", spec.title),
                created_at: now,
                status: PlanStatus::Draft,
                summary: spec.summary.to_string(),
                approach: spec.approach.to_string(),
                steps: vec![],
                files_to_modify: spec.files.iter().map(|s| s.to_string()).collect(),
                risks: vec![],
                current_step_index: None,
                execution_started_at: None,
                execution_completed_at: None,
                execution_error: None,
                task_profile: None,
            };
            plan_ids.push(plan_id.clone());
            dependencies.push(PlanDependency {
                plan_id,
                depends_on: spec
                    .depends_on
                    .iter()
                    .map(|index| plan_ids[*index].clone())
                    .collect(),
            });
            plans.push(plan);
        }

        let milestone = Milestone {
            id: milestone_id,
            session_id,
            title,
            description,
            status: MilestoneStatus::Draft,
            plan_ids,
            plan_dependencies: dependencies,
            success_criteria: self.success_criteria(),
            validation_command: Some("cargo test".to_string()),
            created_at: now,
            updated_at: now,
            completed_at: None,
        };

        MilestoneBlueprint { milestone, plans }
    }

    fn plan_specs(&self) -> Vec<MilestonePlanSpec> {
        match self {
            Self::NewFeature => vec![
                MilestonePlanSpec {
                    title: "Research and shape the API",
                    summary: "Gather context, inspect existing patterns, and estimate the file scope (2-3 files)",
                    approach: "Research the current implementation and document integration points.",
                    files: &["src/auth.rs", "src/lib.rs", "tests/auth.rs"],
                    depends_on: &[],
                },
                MilestonePlanSpec {
                    title: "Implement core feature",
                    summary: "Build the main module and surface area (3-5 files)",
                    approach: "Implement the core data flow and wire the new feature into the app.",
                    files: &["src/auth.rs", "src/middleware.rs", "src/router.rs"],
                    depends_on: &[0],
                },
                MilestonePlanSpec {
                    title: "Add tests",
                    summary: "Cover the new behavior with unit and integration tests (2-4 files)",
                    approach: "Write regression tests for the new feature and important edge cases.",
                    files: &["tests/auth.rs", "tests/integration/auth.rs"],
                    depends_on: &[1],
                },
                MilestonePlanSpec {
                    title: "Integration and polish",
                    summary: "Hook the feature into the TUI/CLI surface and clean up details (1-3 files)",
                    approach: "Connect user-facing entry points and update documentation if needed.",
                    files: &["crates/rustycode-tui/src/app/mod.rs", "README.md"],
                    depends_on: &[1, 2],
                },
            ],
            Self::BugInvestigation => vec![
                MilestonePlanSpec {
                    title: "Reproduce and narrow the bug",
                    summary: "Find a minimal reproduction and log the impacted files (1-2 files)",
                    approach: "Confirm the failure mode and isolate the smallest reproducible case.",
                    files: &["tests/repro.rs", "src/lib.rs"],
                    depends_on: &[],
                },
                MilestonePlanSpec {
                    title: "Implement the fix",
                    summary: "Patch the bug at the source and keep the change small (1-3 files)",
                    approach: "Apply the fix and preserve the current behavior wherever possible.",
                    files: &["src/lib.rs", "src/handler.rs"],
                    depends_on: &[0],
                },
                MilestonePlanSpec {
                    title: "Regression tests",
                    summary: "Lock in the fix with targeted regression coverage (1-2 files)",
                    approach: "Add tests that fail before the fix and pass after it lands.",
                    files: &["tests/regression.rs"],
                    depends_on: &[1],
                },
            ],
            Self::MajorRefactor => vec![
                MilestonePlanSpec {
                    title: "Scaffold the new shape",
                    summary: "Introduce the new abstractions and file layout (2-4 files)",
                    approach: "Lay down the scaffolding so the refactor has a stable path forward.",
                    files: &["src/lib.rs", "src/module.rs"],
                    depends_on: &[],
                },
                MilestonePlanSpec {
                    title: "Migrate the core paths",
                    summary: "Move the primary logic to the new structure (3-6 files)",
                    approach: "Shift the important behavior while keeping the old behavior intact.",
                    files: &["src/module.rs", "src/adapter.rs", "src/lib.rs"],
                    depends_on: &[0],
                },
                MilestonePlanSpec {
                    title: "Cleanup and compatibility",
                    summary: "Delete dead code and preserve compatibility shims (2-4 files)",
                    approach: "Remove obsolete paths and keep temporary shims for compatibility.",
                    files: &["src/legacy.rs", "src/lib.rs"],
                    depends_on: &[1],
                },
                MilestonePlanSpec {
                    title: "Verification sweep",
                    summary: "Run the test and validation sweep (1-2 files)",
                    approach: "Add or update tests to cover the refactored system end to end.",
                    files: &["tests/refactor.rs"],
                    depends_on: &[1, 2],
                },
            ],
        }
    }

    fn success_criteria(&self) -> Vec<String> {
        match self {
            Self::NewFeature => vec![
                "Core behavior is implemented".to_string(),
                "Tests cover the new feature".to_string(),
                "Integration entry points are wired".to_string(),
            ],
            Self::BugInvestigation => vec![
                "Bug reproduction is documented".to_string(),
                "Fix eliminates the failure".to_string(),
                "Regression coverage prevents recurrence".to_string(),
            ],
            Self::MajorRefactor => vec![
                "New structure compiles cleanly".to_string(),
                "Old behavior still passes tests".to_string(),
                "Deprecated paths are removed or isolated".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone)]
struct MilestonePlanSpec {
    title: &'static str,
    summary: &'static str,
    approach: &'static str,
    files: &'static [&'static str],
    depends_on: &'static [usize],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_descriptions() {
        assert_eq!(
            PlanTemplate::NewFeature.description(),
            "Implement a new feature from scratch"
        );
        assert_eq!(PlanTemplate::BugFix.description(), "Fix a reported bug");
    }

    #[test]
    fn test_create_plan_from_template() {
        let template = PlanTemplate::BugFix;
        let session_id = SessionId::new();
        let task = "Fix login bug".to_string();
        let summary = "Fix the login authentication bug".to_string();
        let files = vec!["src/auth.rs".to_string()];

        let plan = template.create_plan(session_id, task, summary, files);

        assert_eq!(plan.task, "Fix login bug");
        assert_eq!(plan.summary, "Fix the login authentication bug");
        assert_eq!(plan.files_to_modify.len(), 1);
        assert_eq!(plan.status, PlanStatus::Draft);
        assert!(!plan.steps.is_empty());
        assert!(!plan.risks.is_empty());
    }

    #[test]
    fn test_new_feature_template_steps() {
        let template = PlanTemplate::NewFeature;
        let plan = template.create_plan(
            SessionId::new(),
            "Add feature".to_string(),
            "Summary".to_string(),
            vec![],
        );

        assert_eq!(plan.steps.len(), 6);
        assert_eq!(plan.steps[0].title, "Research and Analysis");
        assert_eq!(plan.steps[1].title, "Design Feature Architecture");
        assert_eq!(plan.steps[2].title, "Implement Core Functionality");
    }

    #[test]
    fn test_bug_fix_template_steps() {
        let template = PlanTemplate::BugFix;
        let plan = template.create_plan(
            SessionId::new(),
            "Fix bug".to_string(),
            "Summary".to_string(),
            vec![],
        );

        assert_eq!(plan.steps.len(), 5);
        assert_eq!(plan.steps[0].title, "Reproduce the Bug");
        assert_eq!(plan.steps[1].title, "Identify Root Cause");
        assert_eq!(plan.steps[2].title, "Write Failing Test");
    }

    #[test]
    fn test_template_has_risks() {
        let template = PlanTemplate::NewFeature;
        let plan = template.create_plan(
            SessionId::new(),
            "Add feature".to_string(),
            "Summary".to_string(),
            vec![],
        );

        assert!(!plan.risks.is_empty());
        assert!(plan.risks.iter().any(|r| r.contains("integrate well")));
    }

    #[test]
    fn test_template_has_approach() {
        let template = PlanTemplate::BugFix;
        let plan = template.create_plan(
            SessionId::new(),
            "Fix bug".to_string(),
            "Summary".to_string(),
            vec![],
        );

        assert!(!plan.approach.is_empty());
        assert!(plan.approach.contains("Reproduce"));
        assert!(plan.approach.contains("Identify"));
    }

    #[test]
    fn test_all_templates_have_steps() {
        let templates = [
            PlanTemplate::NewFeature,
            PlanTemplate::BugFix,
            PlanTemplate::Refactor,
            PlanTemplate::AddTests,
            PlanTemplate::Performance,
            PlanTemplate::Documentation,
            PlanTemplate::SecurityFix,
            PlanTemplate::DependencyUpdate,
        ];

        for template in templates {
            let plan = template.create_plan(
                SessionId::new(),
                "Test task".to_string(),
                "Test summary".to_string(),
                vec![],
            );
            assert!(!plan.steps.is_empty(), "{:?} should have steps", template);
            assert!(!plan.risks.is_empty(), "{:?} should have risks", template);
            assert!(
                !plan.approach.is_empty(),
                "{:?} should have approach",
                template
            );
        }
    }

    #[test]
    fn test_step_orders_are_sequential() {
        let template = PlanTemplate::NewFeature;
        let plan = template.create_plan(
            SessionId::new(),
            "Test".to_string(),
            "Test".to_string(),
            vec![],
        );

        for (i, step) in plan.steps.iter().enumerate() {
            assert_eq!(step.order, i, "Step order should be sequential");
        }
    }

    #[test]
    fn test_milestone_template_creates_blueprint() {
        let template = MilestoneTemplate::NewFeature;
        let blueprint = template.create_milestone(
            SessionId::new(),
            "Auth milestone".to_string(),
            "Group the auth work into a few ordered plans".to_string(),
        );

        assert_eq!(blueprint.milestone.status, MilestoneStatus::Draft);
        assert!(!blueprint.milestone.plan_ids.is_empty());
        assert_eq!(blueprint.milestone.plan_ids.len(), blueprint.plans.len());
        assert_eq!(blueprint.plans.len(), 4);
        assert!(blueprint
            .plans
            .iter()
            .all(|plan| plan.milestone_id == Some(blueprint.milestone.id.clone())));
        assert!(blueprint
            .milestone
            .plan_dependencies
            .iter()
            .any(|dep| !dep.depends_on.is_empty()));
    }
}
