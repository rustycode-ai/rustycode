//! Exclusion clauses for skill activation accuracy.
//!
//! Skills declare what they should NOT be used for via `excludes` in YAML
//! frontmatter. These clauses reduce false-positive activations when multiple
//! skills have overlapping trigger keywords.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Condition types for exclusion clauses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExclusionCondition {
    /// Exclude based on target platform (e.g., "windows", "linux").
    Platform { value: String },
    /// Exclude based on programming language (e.g., "python", "rust").
    Language { value: String },
    /// Exclude based on tool version constraint (e.g., "rustc < 1.70").
    ToolVersion {
        tool: String,
        #[serde(default)]
        min_version: Option<String>,
        #[serde(default)]
        max_version: Option<String>,
    },
    /// Exclude based on environment variable presence/value.
    EnvVar {
        name: String,
        #[serde(default)]
        value: Option<String>,
    },
    /// Exclude based on substring match in the task context.
    ContextMatch { pattern: String },
}

/// An exclusion clause that can prevent a skill from activating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExclusionClause {
    /// Human-readable reason for the exclusion.
    pub reason: String,
    /// The condition that triggers this exclusion.
    pub condition: ExclusionCondition,
}

impl ExclusionClause {
    /// Create a context-match exclusion clause from a simple string.
    pub fn from_pattern(pattern: impl Into<String>) -> Self {
        Self {
            reason: "excluded by pattern match".to_string(),
            condition: ExclusionCondition::ContextMatch {
                pattern: pattern.into(),
            },
        }
    }

    /// Create a platform exclusion clause.
    pub fn platform(platform: impl Into<String>) -> Self {
        Self {
            reason: "not applicable on this platform".to_string(),
            condition: ExclusionCondition::Platform {
                value: platform.into(),
            },
        }
    }

    /// Create a language exclusion clause.
    pub fn language(language: impl Into<String>) -> Self {
        Self {
            reason: "not applicable for this language".to_string(),
            condition: ExclusionCondition::Language {
                value: language.into(),
            },
        }
    }

    /// Create an environment variable exclusion clause.
    pub fn env_var(name: impl Into<String>, value: Option<String>) -> Self {
        Self {
            reason: "excluded by environment variable".to_string(),
            condition: ExclusionCondition::EnvVar {
                name: name.into(),
                value,
            },
        }
    }
}

/// Result of evaluating exclusion clauses against a context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExclusionResult {
    /// No exclusion clauses matched; the skill should proceed.
    Proceed,
    /// An exclusion clause matched; skip this skill.
    ShouldSkip { reason: String },
}

impl ExclusionResult {
    /// Whether the skill should be skipped.
    pub const fn should_skip(&self) -> bool {
        matches!(self, Self::ShouldSkip { .. })
    }
}

/// A set of exclusion clauses for a skill.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExclusionClauseSet {
    /// The exclusion clauses.
    clauses: Vec<ExclusionClause>,
}

impl ExclusionClauseSet {
    /// Create an empty exclusion set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an exclusion set from a list of simple string patterns.
    /// Each string is treated as a context-match pattern.
    /// Empty strings are ignored. All strings are trimmed.
    pub fn from_patterns(patterns: &[String]) -> Self {
        let clauses = patterns
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| ExclusionClause::from_pattern(s.to_string()))
            .collect();
        Self { clauses }
    }

    /// Create an exclusion set from structured clauses.
    #[allow(clippy::missing_const_for_fn)]
    pub fn from_clauses(clauses: Vec<ExclusionClause>) -> Self {
        Self { clauses }
    }

    /// Add a clause to the set.
    pub fn add(&mut self, clause: ExclusionClause) {
        self.clauses.push(clause);
    }

    /// Number of exclusion clauses.
    pub const fn len(&self) -> usize {
        self.clauses.len()
    }

    /// Whether there are no exclusion clauses.
    pub const fn is_empty(&self) -> bool {
        self.clauses.is_empty()
    }

    /// Get all clauses.
    pub fn clauses(&self) -> &[ExclusionClause] {
        &self.clauses
    }
}

/// Runtime context against which exclusion clauses are evaluated.
#[derive(Debug, Clone, Default)]
pub struct ExclusionContext {
    /// Current platform (e.g., "linux", "macos", "windows").
    pub platform: Option<String>,
    /// Target language (e.g., "rust", "python").
    pub language: Option<String>,
    /// Available tool versions.
    pub tool_versions: HashMap<String, String>,
    pub task_context: Option<String>,
}

impl ExclusionContext {
    /// Create a context from a task description alone.
    pub fn from_task(task: impl Into<String>) -> Self {
        Self {
            task_context: Some(task.into()),
            ..Self::default()
        }
    }

    /// Detect the current platform.
    pub fn detect_platform() -> String {
        if cfg!(target_os = "linux") {
            "linux".to_string()
        } else if cfg!(target_os = "macos") {
            "macos".to_string()
        } else if cfg!(target_os = "windows") {
            "windows".to_string()
        } else {
            "unknown".to_string()
        }
    }
}

/// Evaluates exclusion clauses against a runtime context.
pub struct ConditionEvaluator;

impl ConditionEvaluator {
    /// Evaluate a single condition against a context.
    pub fn evaluate_condition(condition: &ExclusionCondition, ctx: &ExclusionContext) -> bool {
        match condition {
            ExclusionCondition::Platform { value } => ctx
                .platform
                .as_ref()
                .is_some_and(|p| p.to_lowercase() == value.to_lowercase()),

            ExclusionCondition::Language { value } => ctx
                .language
                .as_ref()
                .is_some_and(|l| l.to_lowercase() == value.to_lowercase()),

            ExclusionCondition::ToolVersion {
                tool,
                min_version,
                max_version,
            } => {
                if let Some(installed) = ctx.tool_versions.get(tool) {
                    if let Some(min) = min_version {
                        if !Self::version_gte(installed, min) {
                            return false;
                        }
                    }
                    if let Some(max) = max_version {
                        if Self::version_gte(installed, max) {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            }

            ExclusionCondition::EnvVar { name, value } => std::env::var(name)
                .is_ok_and(|env_val| value.as_ref().is_none_or(|expected| env_val == *expected)),

            ExclusionCondition::ContextMatch { pattern } => {
                ctx.task_context.as_ref().is_some_and(|c| {
                    let ctx_l = c.to_lowercase();
                    let pat_l = pattern.to_lowercase();
                    // Direct substring match or any individual word in the pattern appearing in context
                    ctx_l.contains(&pat_l)
                        || pat_l
                            .split_whitespace()
                            .any(|w| !w.is_empty() && ctx_l.contains(w))
                })
            }
        }
    }

    /// Simple version comparison: returns true if `version >= min_version`.
    /// Compares dot-separated numeric components.
    fn version_gte(version: &str, min_version: &str) -> bool {
        let parse_parts =
            |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
        let v_parts = parse_parts(version);
        let m_parts = parse_parts(min_version);
        let max_len = v_parts.len().max(m_parts.len());
        for i in 0..max_len {
            let v = v_parts.get(i).copied().unwrap_or(0);
            let m = m_parts.get(i).copied().unwrap_or(0);
            if v > m {
                return true;
            }
            if v < m {
                return false;
            }
        }
        true
    }

    /// Evaluate all clauses in an exclusion set against a context.
    pub fn evaluate(set: &ExclusionClauseSet, ctx: &ExclusionContext) -> ExclusionResult {
        for clause in &set.clauses {
            if Self::evaluate_condition(&clause.condition, ctx) {
                return ExclusionResult::ShouldSkip {
                    reason: clause.reason.clone(),
                };
            }
        }
        ExclusionResult::Proceed
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_exclusions() {
        let clauses = ExclusionClauseSet::from_patterns(&[]);
        assert!(clauses.is_empty());
    }

    #[test]
    fn parse_string_exclusions() {
        let clauses = ExclusionClauseSet::from_patterns(&[
            "blog articles".to_string(),
            "documentation generation".to_string(),
            "newsletter".to_string(),
        ]);
        assert_eq!(clauses.len(), 3);
    }

    #[test]
    fn exclusion_matching_is_case_insensitive() {
        let clauses = ExclusionClauseSet::from_patterns(&["BLOG ARTICLES".to_string()]);
        let ctx = ExclusionContext::from_task("write a blog article");
        let result = ConditionEvaluator::evaluate(&clauses, &ctx);
        assert!(result.should_skip());
    }

    #[test]
    fn exclusion_matches_partial_words() {
        let clauses = ExclusionClauseSet::from_patterns(&["newsletter".to_string()]);
        let ctx = ExclusionContext::from_task("send newsletters");
        let result = ConditionEvaluator::evaluate(&clauses, &ctx);
        assert!(result.should_skip());
    }

    #[test]
    fn from_patterns_with_whitespace() {
        let clauses = ExclusionClauseSet::from_patterns(&[
            "  blog articles  ".to_string(),
            String::new(),
            "documentation  ".to_string(),
        ]);
        assert_eq!(clauses.len(), 2); // empty string excluded
    }

    #[test]
    fn platform_exclusion_matches() {
        let mut clauses = ExclusionClauseSet::new();
        clauses.add(ExclusionClause::platform("windows"));

        let ctx = ExclusionContext {
            platform: Some("windows".to_string()),
            ..ExclusionContext::default()
        };
        let result = ConditionEvaluator::evaluate(&clauses, &ctx);
        assert!(result.should_skip());
    }

    #[test]
    fn platform_exclusion_no_match() {
        let mut clauses = ExclusionClauseSet::new();
        clauses.add(ExclusionClause::platform("windows"));

        let ctx = ExclusionContext {
            platform: Some("linux".to_string()),
            ..ExclusionContext::default()
        };
        let result = ConditionEvaluator::evaluate(&clauses, &ctx);
        assert!(!result.should_skip());
    }

    #[test]
    fn language_exclusion_matches() {
        let mut clauses = ExclusionClauseSet::new();
        clauses.add(ExclusionClause::language("python"));

        let ctx = ExclusionContext {
            language: Some("python".to_string()),
            ..ExclusionContext::default()
        };
        let result = ConditionEvaluator::evaluate(&clauses, &ctx);
        assert!(result.should_skip());
    }

    #[test]
    fn context_match_no_hit_proceeds() {
        let clauses = ExclusionClauseSet::from_patterns(&["blog articles".to_string()]);
        let ctx = ExclusionContext::from_task("implement a sorting algorithm");
        let result = ConditionEvaluator::evaluate(&clauses, &ctx);
        assert_eq!(result, ExclusionResult::Proceed);
    }

    #[test]
    fn multiple_clauses_first_match_wins() {
        let mut clauses = ExclusionClauseSet::new();
        clauses.add(ExclusionClause::from_pattern("blog"));
        clauses.add(ExclusionClause::from_pattern("documentation"));

        let ctx = ExclusionContext::from_task("write documentation");
        let result = ConditionEvaluator::evaluate(&clauses, &ctx);
        assert!(result.should_skip());
        if let ExclusionResult::ShouldSkip { reason } = result {
            assert!(!reason.is_empty());
        }
    }

    #[test]
    fn version_comparison_gte() {
        assert!(ConditionEvaluator::version_gte("1.70.0", "1.70.0"));
        assert!(ConditionEvaluator::version_gte("1.71.0", "1.70.0"));
        assert!(!ConditionEvaluator::version_gte("1.69.0", "1.70.0"));
        assert!(ConditionEvaluator::version_gte("2.0.0", "1.99.99"));
    }

    #[test]
    fn version_comparison_different_lengths() {
        assert!(ConditionEvaluator::version_gte("1.70", "1.70.0"));
        assert!(ConditionEvaluator::version_gte("1.70.0", "1.70"));
    }

    #[test]
    fn tool_version_exclusion() {
        let clause = ExclusionClause {
            reason: "requires newer rustc".to_string(),
            condition: ExclusionCondition::ToolVersion {
                tool: "rustc".to_string(),
                min_version: Some("1.75.0".to_string()),
                max_version: None,
            },
        };
        let mut clauses = ExclusionClauseSet::new();
        clauses.add(clause);

        let ctx_new = ExclusionContext {
            tool_versions: HashMap::from([("rustc".to_string(), "1.76.0".to_string())]),
            ..ExclusionContext::default()
        };
        assert!(ConditionEvaluator::evaluate(&clauses, &ctx_new).should_skip());

        let ctx_old = ExclusionContext {
            tool_versions: HashMap::from([("rustc".to_string(), "1.70.0".to_string())]),
            ..ExclusionContext::default()
        };
        assert!(!ConditionEvaluator::evaluate(&clauses, &ctx_old).should_skip());
    }

    #[test]
    fn exclusion_clause_serialization_roundtrip() {
        let clause = ExclusionClause::from_pattern("test pattern");
        let json = serde_json::to_string(&clause).unwrap();
        let back: ExclusionClause = serde_json::from_str(&json).unwrap();
        assert_eq!(back.reason, clause.reason);
    }

    #[test]
    fn exclusion_clause_set_serialization_roundtrip() {
        let clauses =
            ExclusionClauseSet::from_patterns(&["blog".to_string(), "newsletter".to_string()]);
        let json = serde_json::to_string(&clauses).unwrap();
        let back: ExclusionClauseSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
    }

    #[test]
    fn detect_platform_returns_non_empty() {
        let platform = ExclusionContext::detect_platform();
        assert!(!platform.is_empty());
    }

    #[test]
    fn env_var_exclusion_absent() {
        let mut clauses = ExclusionClauseSet::new();
        clauses.add(ExclusionClause::env_var(
            "RUSTYCODE_DEFinitely_NOT_SET_12345",
            None,
        ));
        let ctx = ExclusionContext::default();
        let result = ConditionEvaluator::evaluate(&clauses, &ctx);
        assert!(!result.should_skip());
    }
}
