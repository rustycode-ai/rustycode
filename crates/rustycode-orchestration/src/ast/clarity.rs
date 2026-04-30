//! Pre-pipeline clarity scoring inspired by deep-interview methodology.
//!
//! Scores task descriptions across four weighted dimensions before AST
//! classification. High-ambiguity COMPLEX tasks generate targeted questions
//! that the TUI can present to the user, producing an enriched task description
//! for better downstream routing.

use serde::{Deserialize, Serialize};
use std::fmt::Write;

/// Per-dimension clarity score (0.0 = unknown, 1.0 = crystal clear).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarityScore {
    /// What exactly needs to happen?
    pub goal: f32,
    /// What are the boundaries and limitations?
    pub constraints: f32,
    /// How do we know it's done?
    pub success_criteria: f32,
    /// What's the existing codebase state?
    pub context: f32,
}

impl ClarityScore {
    /// Weighted ambiguity score (0.0 = clear, 1.0 = completely ambiguous).
    /// Weights follow deep-interview defaults for brownfield projects.
    pub fn ambiguity(&self) -> f32 {
        let weights = [0.35, 0.25, 0.25, 0.15]; // goal, constraints, success, context
        let scores = [
            self.goal,
            self.constraints,
            self.success_criteria,
            self.context,
        ];
        // ambiguity = weighted average of (1 - clarity) per dimension
        let total = weights
            .iter()
            .zip(scores.iter())
            .map(|(w, s)| w * (1.0 - s))
            .sum::<f32>();
        total.clamp(0.0, 1.0)
    }

    /// Which dimension has the lowest clarity?
    pub fn weakest_dimension(&self) -> ClarityDimension {
        let dims = [
            (ClarityDimension::Goal, self.goal),
            (ClarityDimension::Constraints, self.constraints),
            (ClarityDimension::SuccessCriteria, self.success_criteria),
            (ClarityDimension::Context, self.context),
        ];
        dims.iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map_or(ClarityDimension::Goal, |(d, _)| *d)
    }
}

/// The four clarity dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClarityDimension {
    Goal,
    Constraints,
    SuccessCriteria,
    Context,
}

impl std::fmt::Display for ClarityDimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Goal => write!(f, "Goal"),
            Self::Constraints => write!(f, "Constraints"),
            Self::SuccessCriteria => write!(f, "Success Criteria"),
            Self::Context => write!(f, "Context"),
        }
    }
}

/// A targeted clarification question for the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationQuestion {
    /// Which dimension this question targets.
    pub dimension: ClarityDimension,
    /// The question text.
    pub question: String,
    /// Why we're asking (the gap detected).
    pub rationale: String,
}

/// Result of a clarity assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarityReport {
    /// Per-dimension clarity scores.
    pub scores: ClarityScore,
    /// Weighted ambiguity (0.0 = clear, 1.0 = ambiguous).
    pub ambiguity: f32,
    /// Questions generated for dimensions below threshold.
    pub questions: Vec<ClarificationQuestion>,
    /// Task description enriched with answers (set after user responds).
    pub enriched_task: Option<String>,
}

/// Configuration for the clarity scorer.
#[derive(Debug, Clone)]
pub struct ClarityConfig {
    /// Ambiguity threshold above which questions are generated.
    pub threshold: f32,
    /// Whether to skip scoring for non-COMPLEX tasks.
    pub skip_for_simple: bool,
}

impl Default for ClarityConfig {
    fn default() -> Self {
        Self {
            threshold: 0.35,
            skip_for_simple: true,
        }
    }
}

/// Heuristic clarity scorer for task descriptions.
///
/// Uses rule-based signal detection to score each dimension. No LLM calls
/// required for the initial version — the heuristics cover common patterns,
/// and the scoring degrades gracefully for ambiguous inputs.
pub struct ClarityScorer {
    config: ClarityConfig,
}

impl ClarityScorer {
    pub const fn new(config: ClarityConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(ClarityConfig::default())
    }

    /// Assess clarity of a task description.
    pub fn assess(&self, task: &str) -> ClarityReport {
        let scores = Self::score_dimensions(task);
        let ambiguity = scores.ambiguity();
        let questions = if ambiguity > self.config.threshold {
            Self::generate_questions(task, &scores)
        } else {
            Vec::new()
        };

        ClarityReport {
            scores,
            ambiguity,
            questions,
            enriched_task: None,
        }
    }

    /// Enrich a task description with answers from a clarity report.
    pub fn enrich_task(task: &str, report: &mut ClarityReport, answers: &[String]) -> String {
        let mut enriched = task.to_string();
        if !answers.is_empty() {
            enriched.push_str("\n\n## Clarified Requirements\n");
            for (i, answer) in answers.iter().enumerate() {
                if i < report.questions.len() {
                    let _ = writeln!(
                        enriched,
                        "- **{}**: {}",
                        report.questions[i].dimension, answer
                    );
                }
            }
        }
        report.enriched_task = Some(enriched.clone());
        enriched
    }

    fn score_dimensions(task: &str) -> ClarityScore {
        ClarityScore {
            goal: Self::score_goal(task),
            constraints: Self::score_constraints(task),
            success_criteria: Self::score_success(task),
            context: Self::score_context(task),
        }
    }

    fn score_goal(task: &str) -> f32 {
        let lower = task.to_lowercase();
        let mut score: f32 = 0.3; // baseline

        // Action verbs indicate clear intent
        let action_verbs = [
            "fix",
            "add",
            "remove",
            "rename",
            "update",
            "create",
            "implement",
            "refactor",
            "migrate",
            "delete",
            "replace",
            "move",
            "change",
        ];
        if action_verbs.iter().any(|v| lower.contains(v)) {
            score += 0.25;
        }

        // Named targets (file paths, module names, function names)
        let has_path = lower.contains('/')
            || lower.contains(".rs")
            || lower.contains(".ts")
            || lower.contains(".py")
            || lower.contains("mod ")
            || lower.contains("fn ")
            || lower.contains("struct ")
            || lower.contains("crate ");
        if has_path {
            score += 0.2;
        }

        // Specificity via word count (more detail = clearer goal, up to a point)
        let word_count = task.split_whitespace().count();
        if word_count >= 5 {
            score += 0.1;
        }
        if word_count >= 10 {
            score += 0.1;
        }

        // Vague qualifiers reduce clarity
        let vague = ["somehow", "something", "stuff", "things", "etc", "maybe"];
        if vague.iter().any(|v| lower.contains(v)) {
            score -= 0.15;
        }

        score.clamp(0.0, 1.0)
    }

    fn score_constraints(task: &str) -> f32 {
        let lower = task.to_lowercase();
        let mut score = 0.2; // baseline — constraints are rarely explicit

        // Explicit boundary signals
        let constraint_signals = [
            "only",
            "without",
            "must not",
            "should not",
            "never",
            "no more than",
            "at most",
            "at least",
            "exactly",
            "within",
            "before",
            "after",
            "keep existing",
            "backward compat",
            "don't break",
            "preserve",
            "without changing",
            "but keep",
        ];
        let constraint_count = constraint_signals
            .iter()
            .filter(|s| lower.contains(*s))
            .count();
        score += (constraint_count as f32 * 0.2).min(0.4);

        // Scope delimiters
        let scope_signals = ["in the", "in this", "only in", "scope", "limit"];
        let scope_count = scope_signals.iter().filter(|s| lower.contains(*s)).count();
        score += (scope_count as f32 * 0.1).min(0.2);

        score.clamp(0.0, 1.0)
    }

    fn score_success(task: &str) -> f32 {
        let lower = task.to_lowercase();
        let mut score = 0.2; // baseline

        // Verification signals
        let verify_signals = [
            "test",
            "tests pass",
            "build",
            "compile",
            "lint",
            "clippy",
            "verify",
            "check",
            "ensure",
            "confirm",
            "validate",
            "should",
            "expect",
        ];
        let verify_count = verify_signals.iter().filter(|s| lower.contains(*s)).count();
        score += (verify_count as f32 * 0.2).min(0.5);

        // Explicit acceptance criteria
        if lower.contains("so that") || lower.contains("in order to") || lower.contains("criteria")
        {
            score += 0.15;
        }

        // "works" or "working" is weak — indicates desired outcome but not criteria
        if lower.contains("works") || lower.contains("working") {
            score += 0.05;
        }

        score.clamp(0.0, 1.0)
    }

    fn score_context(task: &str) -> f32 {
        let lower = task.to_lowercase();
        let mut score = 0.15; // baseline — context is usually implicit

        // References to existing code/structure
        let context_signals = [
            "current", "existing", "the ", "this ", "that ", "module", "function", "method",
            "class", "file", "config", "previous",
        ];
        let context_count = context_signals
            .iter()
            .filter(|s| lower.contains(*s))
            .count();
        score += (context_count as f32 * 0.1).min(0.3);

        // References to external systems
        let system_signals = [
            "api", "database", "server", "service", "endpoint", "queue", "cache", "redis",
            "postgres", "docker",
        ];
        if system_signals.iter().any(|s| lower.contains(s)) {
            score += 0.15;
        }

        // Code-specific references (backticks, quotes, parens)
        if task.contains('`') || task.contains('(') || task.contains('"') {
            score += 0.15;
        }

        score.clamp(0.0, 1.0)
    }

    fn generate_questions(task: &str, scores: &ClarityScore) -> Vec<ClarificationQuestion> {
        let mut questions = Vec::new();
        let dim_scores = [
            (ClarityDimension::Goal, scores.goal),
            (ClarityDimension::Constraints, scores.constraints),
            (ClarityDimension::SuccessCriteria, scores.success_criteria),
            (ClarityDimension::Context, scores.context),
        ];

        for (dim, score) in dim_scores {
            if score < 0.5 {
                if let Some(q) = Self::question_for_dimension(task, dim, score) {
                    questions.push(q);
                }
            }
        }

        // Sort by score ascending (weakest first)
        questions.sort_by(|a, b| {
            let sa = Self::dim_score(scores, a.dimension);
            let sb = Self::dim_score(scores, b.dimension);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });

        questions
    }

    const fn dim_score(scores: &ClarityScore, dim: ClarityDimension) -> f32 {
        match dim {
            ClarityDimension::Goal => scores.goal,
            ClarityDimension::Constraints => scores.constraints,
            ClarityDimension::SuccessCriteria => scores.success_criteria,
            ClarityDimension::Context => scores.context,
        }
    }

    fn question_for_dimension(
        task: &str,
        dim: ClarityDimension,
        score: f32,
    ) -> Option<ClarificationQuestion> {
        let (question, rationale) = match dim {
            ClarityDimension::Goal => {
                let q = "What specific outcome should be achieved? Describe the end state, not the process.".into();
                let r = format!(
                    "Goal clarity is {:.0}% — the task intent is not fully specified.",
                    score * 100.0
                );
                (q, r)
            }
            ClarityDimension::Constraints => {
                let q = "What boundaries or constraints apply? What must NOT change, and what limitations exist?".into();
                let r = format!(
                    "Constraint clarity is {:.0}% — no explicit boundaries detected.",
                    score * 100.0
                );
                (q, r)
            }
            ClarityDimension::SuccessCriteria => {
                let q = "How will you verify this is done? What tests, checks, or observable outcomes confirm completion?".into();
                let r = format!(
                    "Success criteria clarity is {:.0}% — no verification signals found.",
                    score * 100.0
                );
                (q, r)
            }
            ClarityDimension::Context => {
                let q = "Which existing files, modules, or systems does this touch? What's the current state?".into();
                let r = format!(
                    "Context clarity is {:.0}% — no references to existing codebase structure.",
                    score * 100.0
                );
                (q, r)
            }
        };

        // Don't ask if the task is too short to have meaningful gaps
        if task.split_whitespace().count() < 3 {
            return None;
        }

        Some(ClarificationQuestion {
            dimension: dim,
            question,
            rationale,
        })
    }
}

impl Default for ClarityScorer {
    fn default() -> Self {
        Self::with_default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assess(task: &str) -> ClarityReport {
        ClarityScorer::default().assess(task)
    }

    #[test]
    fn trivial_task_has_low_ambiguity() {
        // Trivial tasks still lack explicit constraints/success criteria,
        // so ambiguity is moderate — but lower than vague tasks.
        let trivial = assess("Fix typo in README.md");
        let vague = assess("Make the system better somehow");
        assert!(
            trivial.ambiguity < vague.ambiguity,
            "trivial task should be less ambiguous than vague task ({} vs {})",
            trivial.ambiguity,
            vague.ambiguity,
        );
    }

    #[test]
    fn vague_task_has_high_ambiguity() {
        let report = assess("Make the system better somehow");
        assert!(
            report.ambiguity > 0.35,
            "vague task should be high ambiguity, got {}",
            report.ambiguity
        );
        assert!(!report.questions.is_empty(), "should generate questions");
    }

    #[test]
    fn well_specified_task_has_few_questions() {
        let task = "Add unit tests for the auth module in src/auth/mod.rs. \
                    Tests should cover login, logout, and token refresh. \
                    Run cargo test to verify. Do not change the existing API.";
        let report = assess(task);
        assert!(
            report.ambiguity < 0.5,
            "well-specified task should be moderate-low ambiguity, got {}",
            report.ambiguity
        );
    }

    #[test]
    fn questions_target_weakest_dimension() {
        let report = assess("Implement the feature");
        assert!(!report.questions.is_empty());
        // The weakest dimension should appear first
        let first_dim = report.questions[0].dimension;
        let dim_score = match first_dim {
            ClarityDimension::Goal => report.scores.goal,
            ClarityDimension::Constraints => report.scores.constraints,
            ClarityDimension::SuccessCriteria => report.scores.success_criteria,
            ClarityDimension::Context => report.scores.context,
        };
        // First question should target one of the low-scoring dimensions
        assert!(
            dim_score < 0.5,
            "first question should target a weak dimension, got {:?} with score {}",
            first_dim,
            dim_score,
        );
    }

    #[test]
    fn constraint_signals_boost_score() {
        let with_constraints =
            assess("Add caching layer but don't change the existing API and keep backward compat");
        let without = assess("Add caching layer");
        assert!(
            with_constraints.scores.constraints > without.scores.constraints,
            "constraint signals should boost constraint score"
        );
    }

    #[test]
    fn test_signals_boost_success_score() {
        let with_tests = assess("Add feature X and ensure tests pass with cargo test");
        let without = assess("Add feature X");
        assert!(
            with_tests.scores.success_criteria > without.scores.success_criteria,
            "test signals should boost success score"
        );
    }

    #[test]
    fn code_references_boost_context_score() {
        let with_ctx = assess("Update the `handle_request()` function in src/api/handler.rs to use the new Redis cache");
        let without = assess("Update the handler to use caching");
        assert!(
            with_ctx.scores.context > without.scores.context,
            "code references should boost context score"
        );
    }

    #[test]
    fn enrich_task_appends_answers() {
        let mut report = assess("Implement the feature");
        let answers: Vec<String> = report
            .questions
            .iter()
            .map(|q| format!("Answer for {}", q.dimension))
            .collect();
        let enriched = ClarityScorer::enrich_task("Implement the feature", &mut report, &answers);
        assert!(enriched.contains("Clarified Requirements"));
        assert!(report.enriched_task.is_some());
    }

    #[test]
    fn empty_task_no_questions() {
        let report = assess("hi");
        // Very short task — question generation should return None
        assert!(
            report.questions.len() <= 1,
            "very short task should generate at most 1 question"
        );
    }

    #[test]
    fn ambiguity_is_bounded() {
        let report = assess(
            "Fix typo in README.md and add tests and ensure build passes and don't break API",
        );
        assert!(report.ambiguity >= 0.0 && report.ambiguity <= 1.0);
        assert!(report.scores.goal >= 0.0 && report.scores.goal <= 1.0);
        assert!(report.scores.constraints >= 0.0 && report.scores.constraints <= 1.0);
        assert!(report.scores.success_criteria >= 0.0 && report.scores.success_criteria <= 1.0);
        assert!(report.scores.context >= 0.0 && report.scores.context <= 1.0);
    }

    #[test]
    fn weakest_dimension_identifies_lowest() {
        let scores = ClarityScore {
            goal: 0.8,
            constraints: 0.3,
            success_criteria: 0.5,
            context: 0.7,
        };
        assert_eq!(scores.weakest_dimension(), ClarityDimension::Constraints);
    }

    #[test]
    fn clarity_report_serialization_roundtrip() {
        let report = assess("Implement JWT auth with refresh tokens in src/auth/");
        let json = serde_json::to_string(&report).unwrap();
        let back: ClarityReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ambiguity, report.ambiguity);
        assert_eq!(back.questions.len(), report.questions.len());
    }

    #[test]
    fn custom_threshold_controls_questions() {
        let strict = ClarityScorer::new(ClarityConfig {
            threshold: 0.1,
            skip_for_simple: true,
        });
        let lenient = ClarityScorer::new(ClarityConfig {
            threshold: 0.8,
            skip_for_simple: true,
        });
        let strict_report = strict.assess("Add a login feature");
        let lenient_report = lenient.assess("Add a login feature");
        assert!(
            strict_report.questions.len() >= lenient_report.questions.len(),
            "strict threshold should generate more questions"
        );
    }
}
