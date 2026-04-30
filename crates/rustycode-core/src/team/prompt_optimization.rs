//! Prompt Optimization — Derives tuned system prompt fragments from execution traces.
//!
//! Reads mined patterns from the PatternMiner and converts them into actionable
//! prompt fragments that get injected into agent briefings. This is the "skill
//! auto-tuning" loop: every task execution produces patterns, patterns become
//! optimizations, optimizations improve future briefings.
//!
//! # Architecture
//!
//! ```text
//! ExecutionTrace → PatternMiner → DiscoveredPattern
//!                                       │
//!                     generate_optimizations()
//!                                       │
//!                               PromptOptimization
//!                                       │
//!                     select_relevant(task, top_k)
//!                                       │
//!                     format_for_briefing()
//!                                       │
//!                          ┌────────────┘
//!                          ↓
//!              build_briefing_for_role()  (in orchestrator)
//!                          │
//!                     "Hints from prior runs"
//!                          section in briefing
//! ```

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::execution_trace::{DiscoveredPattern, PatternCategory};

// ============================================================================
// Core Types
// ============================================================================

/// A prompt optimization derived from execution traces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptOptimization {
    /// What task pattern this optimization applies to (e.g., "compilation",
    /// "testing", "refactoring").
    pub task_pattern: String,
    /// The optimized prompt fragment to inject into agent briefings.
    pub prompt_fragment: String,
    /// Confidence score from the pattern miner (0.0-1.0).
    pub confidence: f32,
    /// How many traces support this optimization.
    pub evidence_count: u32,
    /// Category of optimization.
    pub category: OptimizationCategory,
}

/// Category of prompt optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OptimizationCategory {
    /// Approach suggestion — what to try first.
    ApproachHint,
    /// Error avoidance — what NOT to do.
    ErrorAvoidance,
    /// Tool sequence recommendation.
    ToolSequence,
    /// Testing strategy recommendation.
    TestingStrategy,
}

// ============================================================================
// Optimization Generation
// ============================================================================

/// Generate prompt optimizations from mined patterns.
///
/// Each `DiscoveredPattern` is classified into an `OptimizationCategory` and
/// translated into a concrete prompt fragment that an agent can act on.
/// Patterns below the minimum confidence threshold are skipped.
pub fn generate_optimizations(
    patterns: &[DiscoveredPattern],
    min_confidence: f32,
) -> Vec<PromptOptimization> {
    let mut optimizations = Vec::new();

    for pattern in patterns {
        if pattern.confidence < min_confidence {
            debug!(
                "Skipping low-confidence pattern: {} ({:.2})",
                pattern.description, pattern.confidence
            );
            continue;
        }

        let (category, task_pattern, fragment) = match pattern.category {
            PatternCategory::CodeStructure => (
                OptimizationCategory::ApproachHint,
                "structure".to_string(),
                format!(
                    "Project structure insight (supported by {} observations): {}",
                    pattern.occurrence_count, pattern.description
                ),
            ),
            PatternCategory::Testing => (
                OptimizationCategory::TestingStrategy,
                "testing".to_string(),
                format!(
                    "Testing guidance (from {} past tasks): {}",
                    pattern.occurrence_count, pattern.description
                ),
            ),
            PatternCategory::ToolUsage => (
                OptimizationCategory::ToolSequence,
                "tooling".to_string(),
                format!(
                    "Tool usage pattern (observed {} times): {}",
                    pattern.occurrence_count, pattern.description
                ),
            ),
            PatternCategory::Workflow => {
                // Classify workflow patterns as either approach hints or error
                // avoidance depending on whether they describe failure or success.
                if pattern.description.contains("Failure pattern")
                    || pattern.description.contains("failed")
                    || pattern.description.contains("exhausted")
                {
                    (
                        OptimizationCategory::ErrorAvoidance,
                        classify_workflow_task_pattern(&pattern.description),
                        format!(
                            "Avoid this approach (learned from {} failures): {}",
                            pattern.occurrence_count, pattern.description
                        ),
                    )
                } else {
                    (
                        OptimizationCategory::ApproachHint,
                        classify_workflow_task_pattern(&pattern.description),
                        format!(
                            "Recommended approach (verified {} times): {}",
                            pattern.occurrence_count, pattern.description
                        ),
                    )
                }
            }
            PatternCategory::ErrorHandling => (
                OptimizationCategory::ErrorAvoidance,
                "errors".to_string(),
                format!(
                    "Error handling insight (from {} observations): {}",
                    pattern.occurrence_count, pattern.description
                ),
            ),
            PatternCategory::Performance => (
                OptimizationCategory::ApproachHint,
                "performance".to_string(),
                format!(
                    "Performance pattern ({} observations): {}",
                    pattern.occurrence_count, pattern.description
                ),
            ),
            PatternCategory::FailureRecovery => (
                OptimizationCategory::ErrorAvoidance,
                classify_failure_task_pattern(&pattern.description),
                format!(
                    "Recovery strategy (tried {} times): {}",
                    pattern.occurrence_count, pattern.description
                ),
            ),
            PatternCategory::SuccessRecipe => (
                OptimizationCategory::ApproachHint,
                classify_success_task_pattern(&pattern.description),
                format!(
                    "Proven recipe (repeated {} times): {}",
                    pattern.occurrence_count, pattern.description
                ),
            ),
        };

        optimizations.push(PromptOptimization {
            task_pattern,
            prompt_fragment: fragment,
            confidence: pattern.confidence,
            evidence_count: pattern.occurrence_count,
            category,
        });
    }

    info!(
        "Generated {} prompt optimizations from {} patterns",
        optimizations.len(),
        patterns.len()
    );

    optimizations
}

/// Extract a coarse task-pattern tag from a workflow pattern description.
fn classify_workflow_task_pattern(description: &str) -> String {
    let lower = description.to_lowercase();
    if lower.contains("compil") {
        "compilation".to_string()
    } else if lower.contains("test") {
        "testing".to_string()
    } else if lower.contains("trust") {
        "trust".to_string()
    } else if lower.contains("turn") || lower.contains("budget") {
        "budget".to_string()
    } else {
        "general".to_string()
    }
}

/// Extract a task-pattern tag from a failure recovery description.
fn classify_failure_task_pattern(description: &str) -> String {
    let lower = description.to_lowercase();
    if lower.contains("compil") || lower.contains("error[e") {
        "compilation".to_string()
    } else if lower.contains("type") || lower.contains("mismatch") {
        "type_errors".to_string()
    } else if lower.contains("borrow") {
        "ownership".to_string()
    } else if lower.contains("import") || lower.contains("unresolved") {
        "imports".to_string()
    } else {
        "errors".to_string()
    }
}

/// Extract a task-pattern tag from a success recipe description.
fn classify_success_task_pattern(description: &str) -> String {
    let lower = description.to_lowercase();
    if lower.contains("trust") || lower.contains("high trust") {
        "high_trust".to_string()
    } else if lower.contains("test") {
        "testing".to_string()
    } else if lower.contains("refactor") {
        "refactoring".to_string()
    } else {
        "general".to_string()
    }
}

// ============================================================================
// Briefing Formatting
// ============================================================================

/// Format optimizations as an injectable prompt fragment for a given task.
///
/// Returns a markdown-formatted string suitable for injection into the
/// "Hints from prior runs" section of an agent briefing. Optimizations
/// are grouped by category and sorted by confidence (highest first).
///
/// Returns an empty string when there are no relevant optimizations.
pub fn format_for_briefing(optimizations: &[PromptOptimization], task: &str) -> String {
    let relevant = select_relevant(optimizations, task, 5);
    if relevant.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push("## Hints from Prior Runs".to_string());
    lines.push(String::new());

    // Group by category for readability
    let categories = [
        (OptimizationCategory::ApproachHint, "Recommended Approaches"),
        (OptimizationCategory::ErrorAvoidance, "Known Pitfalls"),
        (
            OptimizationCategory::ToolSequence,
            "Effective Tool Sequences",
        ),
        (OptimizationCategory::TestingStrategy, "Testing Strategies"),
    ];

    for (cat, label) in &categories {
        let items: Vec<&&PromptOptimization> =
            relevant.iter().filter(|o| o.category == *cat).collect();

        if items.is_empty() {
            continue;
        }

        lines.push(format!("### {}", label));
        for opt in items {
            lines.push(format!(
                "- {} (confidence: {:.0}%, evidence: {} tasks)",
                opt.prompt_fragment,
                opt.confidence * 100.0,
                opt.evidence_count
            ));
        }
        lines.push(String::new());
    }

    let result = lines.join("\n");
    debug!(
        "Formatted briefing with {} optimizations for task: {}",
        relevant.len(),
        task
    );
    result
}

// ============================================================================
// Relevance Selection
// ============================================================================

/// Select the most relevant optimizations for a task description.
///
/// Relevance is computed by matching the `task_pattern` field against keywords
/// in the task string. Results are sorted by confidence (descending) and
/// truncated to `top_k`.
pub fn select_relevant<'a>(
    optimizations: &'a [PromptOptimization],
    task: &str,
    top_k: usize,
) -> Vec<&'a PromptOptimization> {
    let task_lower = task.to_lowercase();

    let mut scored: Vec<(f32, &'a PromptOptimization)> = optimizations
        .iter()
        .map(|opt| {
            let relevance = compute_relevance(&opt.task_pattern, &task_lower);
            let score = relevance * opt.confidence;
            (score, opt)
        })
        .filter(|(score, _)| *score > 0.0)
        .collect();

    // Sort by composite score (relevance * confidence), descending
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    scored.into_iter().take(top_k).map(|(_, opt)| opt).collect()
}

/// Compute a relevance score between a task pattern and a task description.
///
/// Uses simple keyword matching with basic stemming to handle common English
/// variants (e.g., "testing" matches "tests"). Returns 0.0 if no keywords
/// match, 1.0 for exact pattern match, and a fractional score for partial
/// matches.
fn compute_relevance(task_pattern: &str, task_lower: &str) -> f32 {
    let pattern_lower = task_pattern.to_lowercase();

    // Exact substring match
    if task_lower.contains(&pattern_lower) {
        return 1.0;
    }

    // General patterns always match at reduced relevance
    if pattern_lower == "general" || pattern_lower == "errors" {
        return 0.3;
    }

    // Keyword-based matching: split the task pattern into tokens and check
    // how many appear in the task description (with basic stemming).
    let tokens: Vec<&str> = pattern_lower
        .split(['_', ' '])
        .filter(|t| !t.is_empty())
        .collect();

    if tokens.is_empty() {
        return 0.0;
    }

    let matched = tokens
        .iter()
        .filter(|t| token_matches(task_lower, t))
        .count();
    matched as f32 / tokens.len() as f32
}

/// Check whether a single token matches anywhere in the task text.
///
/// Tries the token as-is, then with common English suffixes stripped
/// ("ing", "tion", "s", "ed") to catch variants like "testing"/"tests",
/// "compilation"/"compile", etc.
fn token_matches(task_lower: &str, token: &str) -> bool {
    if task_lower.contains(token) {
        return true;
    }

    // Try stripping common suffixes for a fuzzy match
    let stems = [
        token.strip_suffix("ing"),
        token.strip_suffix("tion"),
        token.strip_suffix("s"),
        token.strip_suffix("ed"),
    ];

    for stem in stems.into_iter().flatten() {
        if stem.len() >= 3 && task_lower.contains(stem) {
            return true;
        }
    }

    false
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pattern(
        description: &str,
        confidence: f32,
        occurrence_count: u32,
        category: PatternCategory,
    ) -> DiscoveredPattern {
        DiscoveredPattern {
            description: description.to_string(),
            confidence,
            occurrence_count,
            source_tasks: vec!["test_task".to_string()],
            category,
        }
    }

    #[test]
    fn test_generate_optimizations_from_success_pattern() {
        let patterns = vec![
            make_pattern(
                "Task completed successfully with high trust (0.85); approach was effective",
                0.9,
                5,
                PatternCategory::SuccessRecipe,
            ),
            make_pattern(
                "Small focused changes lead to faster completion",
                0.8,
                3,
                PatternCategory::Workflow,
            ),
        ];

        let opts = generate_optimizations(&patterns, 0.5);

        assert_eq!(opts.len(), 2);

        // Success recipe should produce an ApproachHint
        let success_opt = opts
            .iter()
            .find(|o| o.category == OptimizationCategory::ApproachHint);
        assert!(
            success_opt.is_some(),
            "Should produce an ApproachHint from success recipe"
        );
        let success_opt = success_opt.unwrap();
        assert!(success_opt.prompt_fragment.contains("Proven recipe"));
        assert_eq!(success_opt.evidence_count, 5);
        assert!((success_opt.confidence - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_generate_optimizations_from_failure_pattern() {
        let patterns = vec![
            make_pattern(
                "Failure pattern: Task exceeded turn limit; may need better planning",
                0.7,
                4,
                PatternCategory::Workflow,
            ),
            make_pattern(
                "Borrow checker issues; consider cloning or restructuring ownership",
                0.6,
                2,
                PatternCategory::ErrorHandling,
            ),
        ];

        let opts = generate_optimizations(&patterns, 0.5);

        assert_eq!(opts.len(), 2);

        // The failure workflow pattern should produce an ErrorAvoidance
        let failure_opt = opts
            .iter()
            .find(|o| o.category == OptimizationCategory::ErrorAvoidance);
        assert!(
            failure_opt.is_some(),
            "Should produce ErrorAvoidance from failure pattern"
        );
        let failure_opt = failure_opt.unwrap();
        assert!(failure_opt.prompt_fragment.contains("Avoid"));
    }

    #[test]
    fn test_format_for_briefing() {
        let optimizations = vec![
            PromptOptimization {
                task_pattern: "compilation".to_string(),
                prompt_fragment: "Run cargo check early and often".to_string(),
                confidence: 0.9,
                evidence_count: 5,
                category: OptimizationCategory::ApproachHint,
            },
            PromptOptimization {
                task_pattern: "errors".to_string(),
                prompt_fragment: "Avoid assuming file paths exist".to_string(),
                confidence: 0.7,
                evidence_count: 3,
                category: OptimizationCategory::ErrorAvoidance,
            },
        ];

        let formatted = format_for_briefing(&optimizations, "fix compilation error");

        assert!(formatted.contains("## Hints from Prior Runs"));
        assert!(formatted.contains("Run cargo check early"));
        assert!(formatted.contains("Avoid assuming file paths"));
        assert!(formatted.contains("Recommended Approaches"));
        assert!(formatted.contains("Known Pitfalls"));
        assert!(formatted.contains("confidence: 90%"));
        assert!(formatted.contains("evidence: 5 tasks"));
    }

    #[test]
    fn test_select_relevant_filters_by_task() {
        let optimizations = vec![
            PromptOptimization {
                task_pattern: "compilation".to_string(),
                prompt_fragment: "Use cargo check".to_string(),
                confidence: 0.9,
                evidence_count: 5,
                category: OptimizationCategory::ApproachHint,
            },
            PromptOptimization {
                task_pattern: "testing".to_string(),
                prompt_fragment: "Write table-driven tests".to_string(),
                confidence: 0.8,
                evidence_count: 4,
                category: OptimizationCategory::TestingStrategy,
            },
            PromptOptimization {
                task_pattern: "performance".to_string(),
                prompt_fragment: "Profile before optimizing".to_string(),
                confidence: 0.7,
                evidence_count: 2,
                category: OptimizationCategory::ApproachHint,
            },
        ];

        // Task about compilation should prefer compilation optimization
        let relevant = select_relevant(&optimizations, "fix compilation errors in auth module", 3);
        assert!(!relevant.is_empty());
        assert_eq!(relevant[0].task_pattern, "compilation");

        // Task about tests should prefer testing optimization
        let relevant = select_relevant(&optimizations, "add tests for the user module", 3);
        assert!(!relevant.is_empty());
        assert_eq!(relevant[0].task_pattern, "testing");

        // Top-k limiting
        let limited = select_relevant(&optimizations, "fix compilation errors", 1);
        assert_eq!(limited.len(), 1);
    }

    #[test]
    fn test_empty_patterns_produce_no_optimizations() {
        let patterns: Vec<DiscoveredPattern> = vec![];
        let opts = generate_optimizations(&patterns, 0.5);
        assert!(opts.is_empty());

        // Also test that format_for_briefing returns empty string with no optimizations
        let formatted = format_for_briefing(&[], "any task");
        assert!(formatted.is_empty());
    }
}
