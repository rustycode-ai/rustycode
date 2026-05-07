//! Task classifier for AST Phase 0: CLASSIFY.
//!
//! Determines complexity rating and success criteria for a task.

use super::types::{ComplexityLevel, PhaseRoute, SuccessCriterion, TaskAssessment};

/// Classifies tasks into complexity levels.
///
/// Uses a hybrid approach: rule-based heuristics for known patterns,
/// with a model-based fallback for ambiguous cases.
pub struct TaskClassifier {
    #[allow(dead_code)] // Used by future rule-based classification
    rules: Vec<ClassificationRule>,
}

/// A single classification rule.
#[allow(dead_code)] // Used by future rule-based classification
struct ClassificationRule {
    pattern: fn(&str) -> bool,
    complexity: ComplexityLevel,
}

impl TaskClassifier {
    pub const fn new() -> Self {
        Self {
            rules: Self::default_rules(),
        }
    }

    /// Classify a task request into a `TaskAssessment`.
    pub fn classify(&self, request: &str) -> TaskAssessment {
        let complexity = self.determine_complexity(request);
        let success_criteria = self.extract_criteria(request);

        TaskAssessment {
            task_summary: self.summarize(request),
            complexity,
            success_criteria,
            route: PhaseRoute::from(complexity),
            clarity: None,
        }
    }

    #[allow(clippy::unused_self)]
    fn determine_complexity(&self, request: &str) -> ComplexityLevel {
        let lower = request.to_lowercase();

        // Check for trivial signals
        let trivial_signals = [
            "typo",
            "rename",
            "fix typo",
            "update comment",
            "change string",
            "bump version",
            "fix lint",
            "add import",
        ];
        if trivial_signals.iter().any(|s| lower.contains(s)) {
            let word_count = request.split_whitespace().count();
            if word_count <= 8 {
                return ComplexityLevel::Trivial;
            }
        }

        // Check for complex signals
        let complex_signals = [
            "implement",
            "refactor",
            "architect",
            "migrate",
            "redesign",
            "integrate",
            "multiple systems",
            "cross-cutting",
            "end-to-end",
            "full feature",
            "new module",
            "new crate",
        ];
        if complex_signals.iter().any(|s| lower.contains(s)) {
            return ComplexityLevel::Complex;
        }

        // Moderate signals or default
        let moderate_signals = [
            "add test",
            "tests for",
            "add unit",
            "update",
            "extend",
            "modify",
            "add support",
            "fix bug",
            "handle",
            "write test",
        ];
        if moderate_signals.iter().any(|s| lower.contains(s)) {
            return ComplexityLevel::Moderate;
        }

        // Word count heuristic as fallback
        let word_count = request.split_whitespace().count();
        if word_count <= 10 {
            ComplexityLevel::Trivial
        } else if word_count <= 25 {
            ComplexityLevel::Moderate
        } else {
            ComplexityLevel::Complex
        }
    }

    #[allow(clippy::unused_self)]
    fn extract_criteria(&self, request: &str) -> Vec<SuccessCriterion> {
        let mut criteria = Vec::new();

        // Extract explicit criteria from the request
        let lower = request.to_lowercase();

        if lower.contains("test") {
            criteria.push(SuccessCriterion {
                description: "Tests pass".into(),
                verification_command: Some("cargo test".into()),
            });
        }

        if lower.contains("build") || lower.contains("compile") {
            criteria.push(SuccessCriterion {
                description: "Build succeeds".into(),
                verification_command: Some("cargo build".into()),
            });
        }

        if lower.contains("clippy") || lower.contains("lint") {
            criteria.push(SuccessCriterion {
                description: "No clippy warnings".into(),
                verification_command: Some("cargo clippy -- -D warnings".into()),
            });
        }

        // Always add at least one criterion
        if criteria.is_empty() {
            criteria.push(SuccessCriterion {
                description: "Task objective met".into(),
                verification_command: None,
            });
        }

        criteria
    }

    #[allow(clippy::unused_self)]
    fn summarize(&self, request: &str) -> String {
        let trimmed = request.trim();
        if trimmed.len() > 200 {
            let end = trimmed.floor_char_boundary(197);
            format!("{}...", &trimmed[..end])
        } else {
            trimmed.to_string()
        }
    }

    const fn default_rules() -> Vec<ClassificationRule> {
        // Future: load rules from config or learn from history
        Vec::new()
    }
}

impl Default for TaskClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(request: &str) -> TaskAssessment {
        TaskClassifier::new().classify(request)
    }

    #[test]
    fn trivial_typo_fix() {
        let a = classify("Fix typo in README.md");
        assert_eq!(a.complexity, ComplexityLevel::Trivial);
        assert_eq!(a.route, PhaseRoute::DirectExecute);
    }

    #[test]
    fn trivial_rename() {
        let a = classify("Rename variable foo to bar");
        assert_eq!(a.complexity, ComplexityLevel::Trivial);
    }

    #[test]
    fn moderate_add_tests() {
        let a = classify("Add unit tests for the auth module");
        assert_eq!(a.complexity, ComplexityLevel::Moderate);
        assert_eq!(a.route, PhaseRoute::StandardSequence);
    }

    #[test]
    fn moderate_fix_bug() {
        let a = classify("Fix the race condition in the session handler");
        assert_eq!(a.complexity, ComplexityLevel::Moderate);
    }

    #[test]
    fn complex_implement() {
        let a = classify("Implement JWT authentication flow with refresh tokens");
        assert_eq!(a.complexity, ComplexityLevel::Complex);
        assert_eq!(a.route, PhaseRoute::RollingWave);
    }

    #[test]
    fn complex_refactor() {
        let a = classify("Refactor the entire LLM provider layer to use a unified trait");
        assert_eq!(a.complexity, ComplexityLevel::Complex);
    }

    #[test]
    fn criteria_extraction_tests() {
        let a = classify("Add tests and fix the build");
        assert!(a
            .success_criteria
            .iter()
            .any(|c| c.description.contains("Tests")));
        assert!(a
            .success_criteria
            .iter()
            .any(|c| c.description.contains("Build")));
    }

    #[test]
    fn criteria_extraction_default() {
        let a = classify("Do something");
        assert!(!a.success_criteria.is_empty());
    }

    #[test]
    fn summary_truncation() {
        let long = "x ".repeat(150);
        let a = classify(&long);
        assert!(a.task_summary.len() <= 203);
    }

    #[test]
    fn classify_bump_version() {
        let a = classify("Bump version to 2.0");
        assert_eq!(a.complexity, ComplexityLevel::Trivial);
    }

    #[test]
    fn classify_new_module() {
        let a = classify("Create a new crate for the event sourcing system");
        assert_eq!(a.complexity, ComplexityLevel::Complex);
    }

    // -- US-004: edge-case tests --

    #[test]
    fn classify_empty_string_no_panic() {
        let a = classify("");
        // Empty string may produce empty or default summary — just ensure no panic
        let _ = a.task_summary;
        assert!(
            !a.success_criteria.is_empty(),
            "should produce default criteria"
        );
    }

    #[test]
    fn classify_very_long_string() {
        let long = "Implement a feature that does ".repeat(50); // ~1500 chars
        let a = classify(&long);
        assert!(!a.task_summary.is_empty());
        assert!(a.task_summary.len() <= 203, "summary should be truncated");
    }

    #[test]
    fn classify_unicode_characters() {
        let a = classify("修正 README 中的错误并添加 日本語 のサポート");
        assert!(!a.task_summary.is_empty());
        // Unicode should be preserved in the summary
        assert!(a.task_summary.contains('修') || a.task_summary.contains("README"));
    }

    #[test]
    fn classify_special_characters() {
        let a = classify("Fix regex pattern /\\w+\\.rs$/ in the scanner");
        assert!(!a.task_summary.is_empty());
    }

    #[test]
    fn classify_contradictory_signals() {
        // "simple" suggests trivial but "refactor the entire" suggests complex
        let a = classify("Simple task: refactor the entire authentication system");
        // Should classify as at least moderate due to "refactor entire"
        assert!(
            matches!(
                a.complexity,
                ComplexityLevel::Complex | ComplexityLevel::Moderate
            ),
            "contradictory signals should lean toward higher complexity"
        );
    }
}
