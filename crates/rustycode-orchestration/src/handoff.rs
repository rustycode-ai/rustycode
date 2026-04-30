//! Handoff protocol for explicit context transfer between tiers.
//!
//! When execution moves from one tier to another (e.g., Musician -> Editor
//! for review, or Editor -> Composer for re-composition), a `HandoffPackage`
//! is created. This package contains the essential context the next tier needs
//! without the full conversation history of the previous tier.

use crate::isolation::{ContextBudget, TierIsolation};
use crate::state_machine::TaskContext;
use crate::types::ExecutionTier;
use serde::{Deserialize, Serialize};

/// Budget summary included in handoff to inform next tier of remaining resources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BudgetSummary {
    pub tier: u8,
    pub tokens_used: u64,
    pub tokens_limit: u64,
    pub cost_usd_used: f64,
    pub cost_usd_limit: f64,
}

impl BudgetSummary {
    /// Create a new budget summary.
    pub const fn new(
        tier: u8,
        tokens_used: u64,
        tokens_limit: u64,
        cost_usd_used: f64,
        cost_usd_limit: f64,
    ) -> Self {
        Self {
            tier,
            tokens_used,
            tokens_limit,
            cost_usd_used,
            cost_usd_limit,
        }
    }

    /// Remaining tokens.
    pub const fn tokens_remaining(&self) -> u64 {
        self.tokens_limit.saturating_sub(self.tokens_used)
    }

    /// Remaining budget in USD.
    pub fn budget_remaining(&self) -> f64 {
        (self.cost_usd_limit - self.cost_usd_used).max(0.0)
    }
}

/// Explicit context package passed between tiers.
///
/// Contains the task description, relevant code, constraints, previous tier's
/// assessment, and budget summary. No full conversation history is included --
/// each tier starts with only what it needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffPackage {
    /// What task the next tier should work on.
    pub task_description: String,
    /// The tier this package is being handed off to.
    pub target_tier: ExecutionTier,
    /// The tier that produced this package.
    pub source_tier: ExecutionTier,
    /// Relevant code snippets (not the full codebase).
    pub code_snippets: Vec<CodeSnippet>,
    /// Constraints the next tier must respect.
    pub constraints: Vec<String>,
    /// The previous tier's assessment of the task.
    pub previous_assessment: Option<String>,
    /// Budget summary from the previous tier.
    pub budget_summary: Option<BudgetSummary>,
    /// Task ID for tracing.
    pub task_id: String,
}

/// A code snippet included in the handoff.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeSnippet {
    pub file_path: String,
    pub content: String,
    pub relevance: String,
}

impl CodeSnippet {
    pub fn new(
        file_path: impl Into<String>,
        content: impl Into<String>,
        relevance: impl Into<String>,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            content: content.into(),
            relevance: relevance.into(),
        }
    }
}

/// Builder for [`HandoffPackage`].
#[derive(Debug, Clone)]
pub struct HandoffBuilder {
    task_description: String,
    target_tier: ExecutionTier,
    source_tier: ExecutionTier,
    code_snippets: Vec<CodeSnippet>,
    constraints: Vec<String>,
    previous_assessment: Option<String>,
    budget_summary: Option<BudgetSummary>,
    task_id: String,
}

impl HandoffBuilder {
    /// Start building a handoff package.
    pub fn new(
        task_id: impl Into<String>,
        task_description: impl Into<String>,
        source_tier: ExecutionTier,
        target_tier: ExecutionTier,
    ) -> Self {
        Self {
            task_description: task_description.into(),
            target_tier,
            source_tier,
            code_snippets: Vec::new(),
            constraints: Vec::new(),
            previous_assessment: None,
            budget_summary: None,
            task_id: task_id.into(),
        }
    }

    /// Add a code snippet.
    pub fn with_code_snippet(mut self, snippet: CodeSnippet) -> Self {
        self.code_snippets.push(snippet);
        self
    }

    /// Add a constraint.
    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }

    /// Set the previous tier's assessment.
    pub fn with_assessment(mut self, assessment: impl Into<String>) -> Self {
        self.previous_assessment = Some(assessment.into());
        self
    }

    /// Set the budget summary.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_budget_summary(mut self, summary: BudgetSummary) -> Self {
        self.budget_summary = Some(summary);
        self
    }

    /// Build the handoff package.
    pub fn build(self) -> HandoffPackage {
        HandoffPackage {
            task_description: self.task_description,
            target_tier: self.target_tier,
            source_tier: self.source_tier,
            code_snippets: self.code_snippets,
            constraints: self.constraints,
            previous_assessment: self.previous_assessment,
            budget_summary: self.budget_summary,
            task_id: self.task_id,
        }
    }
}

impl HandoffPackage {
    /// Create a handoff from a [`TaskContext`] for a tier transition.
    ///
    /// Extracts the essential context from the current execution state
    /// without copying the full conversation history or trace.
    pub fn from_context(
        ctx: &TaskContext,
        target_tier: ExecutionTier,
        assessment: Option<String>,
        isolation: Option<&TierIsolation>,
    ) -> Self {
        let source_tier =
            ExecutionTier::from_u8(ctx.current_tier).unwrap_or(ExecutionTier::Musician);

        // Prefer the configured per-tier limit from TierIsolation when available.
        let tokens_limit = isolation
            .and_then(|iso| iso.budget_for(ctx.current_tier).map(ContextBudget::limit))
            .unwrap_or(100_000);

        let budget_summary = Some(BudgetSummary::new(
            ctx.current_tier,
            ctx.token_count,
            tokens_limit,
            ctx.cost_used,
            ctx.budget_limit,
        ));

        Self {
            task_description: ctx.original_request.clone(),
            target_tier,
            source_tier,
            code_snippets: Vec::new(),
            constraints: vec![
                format!(
                    "complexity: {}",
                    ctx.constraints.complexity.complexity_description()
                ),
                format!("max_retries: {}", ctx.constraints.max_retries),
                format!("timeout: {}s", ctx.constraints.timeout_seconds),
            ],
            previous_assessment: assessment,
            budget_summary,
            task_id: ctx.task_id.clone(),
        }
    }

    /// Whether this package has all required fields populated.
    pub const fn is_complete(&self) -> bool {
        !self.task_description.is_empty() && !self.task_id.is_empty()
    }

    /// Estimate the token count of this package (rough approximation).
    ///
    /// Uses 4 characters per token as a rough heuristic.
    pub fn token_estimate(&self) -> u64 {
        let total_chars = self.task_description.len()
            + self.task_id.len()
            + self
                .code_snippets
                .iter()
                .map(|s| s.content.len() + s.file_path.len())
                .sum::<usize>()
            + self.constraints.iter().map(String::len).sum::<usize>()
            + self.previous_assessment.as_ref().map_or(0, String::len);

        (total_chars as u64) / 4
    }

    /// Human-readable summary for logging.
    pub fn summary(&self) -> String {
        format!(
            "HandoffPackage(task={}, {} -> {}, snippets={}, constraints={})",
            self.task_id,
            self.source_tier,
            self.target_tier,
            self.code_snippets.len(),
            self.constraints.len(),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_handoff_package_builder() {
        let package = HandoffBuilder::new(
            "task-1",
            "fix bug",
            ExecutionTier::Musician,
            ExecutionTier::Editor,
        )
        .with_code_snippet(CodeSnippet::new("file.rs", "content", "relevant"))
        .with_constraint("no-breaking")
        .with_assessment("it works")
        .build();

        assert_eq!(package.task_id, "task-1");
        assert_eq!(package.code_snippets.len(), 1);
        assert_eq!(package.constraints.len(), 1);
        assert_eq!(package.previous_assessment, Some("it works".to_string()));
    }

    #[test]
    fn test_builder_with_budget_summary() {
        let budget = BudgetSummary::new(2, 5000, 100_000, 0.5, 10.0);
        let package =
            HandoffBuilder::new("t1", "task", ExecutionTier::Musician, ExecutionTier::Editor)
                .with_budget_summary(budget.clone())
                .build();
        assert_eq!(package.budget_summary, Some(budget));
    }

    #[test]
    fn test_builder_multiple_snippets() {
        let package =
            HandoffBuilder::new("t1", "task", ExecutionTier::Editor, ExecutionTier::Composer)
                .with_code_snippet(CodeSnippet::new("a.rs", "fn a()", "main file"))
                .with_code_snippet(CodeSnippet::new("b.rs", "fn b()", "helper"))
                .build();
        assert_eq!(package.code_snippets.len(), 2);
        assert_eq!(package.code_snippets[0].file_path, "a.rs");
    }

    #[test]
    fn test_builder_multiple_constraints() {
        let package = HandoffBuilder::new(
            "t1",
            "task",
            ExecutionTier::Musician,
            ExecutionTier::Thinking,
        )
        .with_constraint("no-exec")
        .with_constraint("read-only")
        .with_constraint("max-5-turns")
        .build();
        assert_eq!(package.constraints.len(), 3);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let package = HandoffBuilder::new(
            "t1",
            "do stuff",
            ExecutionTier::Musician,
            ExecutionTier::Editor,
        )
        .with_code_snippet(CodeSnippet::new("main.rs", "fn main() {}", "entry"))
        .with_constraint("preserve-api")
        .with_assessment("looks good")
        .with_budget_summary(BudgetSummary::new(2, 1000, 100_000, 0.1, 10.0))
        .build();

        let json = serde_json::to_string(&package).unwrap();
        let deserialized: HandoffPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task_id, package.task_id);
        assert_eq!(deserialized.task_description, package.task_description);
        assert_eq!(deserialized.code_snippets.len(), 1);
        assert_eq!(deserialized.constraints.len(), 1);
        assert!(deserialized.budget_summary.is_some());
    }

    #[test]
    fn test_is_complete_with_all_fields() {
        let package =
            HandoffBuilder::new("t1", "task", ExecutionTier::Musician, ExecutionTier::Editor)
                .build();
        assert!(package.is_complete());
    }

    #[test]
    fn test_is_incomplete_without_description() {
        let package =
            HandoffBuilder::new("t1", "", ExecutionTier::Musician, ExecutionTier::Editor).build();
        assert!(!package.is_complete());
    }

    #[test]
    fn test_is_incomplete_without_task_id() {
        let package =
            HandoffBuilder::new("", "task", ExecutionTier::Musician, ExecutionTier::Editor).build();
        assert!(!package.is_complete());
    }

    #[test]
    fn test_token_estimate() {
        let package = HandoffBuilder::new(
            "t1",
            "a".repeat(100),
            ExecutionTier::Musician,
            ExecutionTier::Editor,
        )
        .with_code_snippet(CodeSnippet::new("f.rs", "b".repeat(200), "relevant"))
        .build();
        let estimate = package.token_estimate();
        assert!(estimate > 0);
        // Rough: 100 + 2 + 200 + 2 = ~304 chars / 4 = ~76 tokens
        #[expect(clippy::cast_possible_wrap)]
        let diff = estimate as i64 - 76;
        assert!(diff.unsigned_abs() < 5);
    }

    #[test]
    fn test_summary_format() {
        let package =
            HandoffBuilder::new("t1", "task", ExecutionTier::Musician, ExecutionTier::Editor)
                .with_code_snippet(CodeSnippet::new("a.rs", "x", "r"))
                .with_constraint("c1")
                .build();
        let s = package.summary();
        assert!(s.contains("t1"));
        assert!(s.contains("musician"));
        assert!(s.contains("editor"));
        assert!(s.contains("snippets=1"));
        assert!(s.contains("constraints=1"));
    }

    #[test]
    fn test_from_context_basic() {
        let ctx = TaskContext::new("t1".into(), "fix the bug".into());
        let package = HandoffPackage::from_context(&ctx, ExecutionTier::Editor, None, None);
        assert_eq!(package.task_id, "t1");
        assert_eq!(package.task_description, "fix the bug");
        assert_eq!(package.target_tier, ExecutionTier::Editor);
        assert!(package.previous_assessment.is_none());
        assert!(package.budget_summary.is_some());
    }

    #[test]
    fn test_from_context_with_assessment() {
        let ctx = TaskContext::new("t2".into(), "refactor".into());
        let package = HandoffPackage::from_context(
            &ctx,
            ExecutionTier::Composer,
            Some("needs deeper analysis".into()),
            None,
        );
        assert_eq!(
            package.previous_assessment,
            Some("needs deeper analysis".to_string())
        );
        assert_eq!(package.target_tier, ExecutionTier::Composer);
    }

    #[test]
    fn test_budget_summary_tokens_remaining() {
        let bs = BudgetSummary::new(2, 30_000, 100_000, 1.0, 10.0);
        assert_eq!(bs.tokens_remaining(), 70_000);
    }

    #[test]
    fn test_budget_summary_budget_remaining() {
        let bs = BudgetSummary::new(2, 0, 100_000, 3.0, 10.0);
        assert!((bs.budget_remaining() - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_budget_summary_budget_remaining_clamps() {
        let bs = BudgetSummary::new(2, 0, 100_000, 15.0, 10.0);
        assert!((bs.budget_remaining() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_code_snippet_new() {
        let cs = CodeSnippet::new("lib.rs", "pub fn hello()", "entry point");
        assert_eq!(cs.file_path, "lib.rs");
        assert_eq!(cs.content, "pub fn hello()");
        assert_eq!(cs.relevance, "entry point");
    }
}
