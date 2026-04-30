//! Prompt context and builder for rendering strategy templates

use crate::thinking::core::types::Thought;
use std::collections::HashMap;
use std::fmt::Write;

/// Context for rendering strategy prompts
#[derive(Debug, Clone)]
pub struct PromptContext {
    pub problem: String,
    pub previous_thoughts: Vec<Thought>,
    pub constraints: Vec<String>,
    pub current_depth: usize,
    pub iteration: usize,
    pub graph_summary: String,
    pub metadata: HashMap<String, String>,
    /// What we're trying to achieve (for goal-oriented reasoning)
    pub goal_description: Option<String>,
    /// Criteria for knowing we've succeeded
    pub success_criteria: Vec<String>,
}

impl PromptContext {
    pub fn new(problem: impl Into<String>) -> Self {
        Self {
            problem: problem.into(),
            previous_thoughts: Vec::new(),
            constraints: Vec::new(),
            current_depth: 0,
            iteration: 0,
            graph_summary: String::new(),
            metadata: HashMap::new(),
            goal_description: None,
            success_criteria: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_previous_thoughts(mut self, thoughts: Vec<Thought>) -> Self {
        self.previous_thoughts = thoughts;
        self
    }

    #[must_use]
    pub fn with_constraints(mut self, constraints: Vec<String>) -> Self {
        self.constraints = constraints;
        self
    }

    #[must_use]
    pub const fn with_depth(mut self, depth: usize) -> Self {
        self.current_depth = depth;
        self
    }

    #[must_use]
    pub const fn with_iteration(mut self, iteration: usize) -> Self {
        self.iteration = iteration;
        self
    }

    #[must_use]
    pub fn with_graph_summary(mut self, summary: impl Into<String>) -> Self {
        self.graph_summary = summary.into();
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set the goal description for goal-oriented reasoning
    #[must_use]
    pub fn with_goal(mut self, goal: impl Into<String>) -> Self {
        self.goal_description = Some(goal.into());
        self
    }

    /// Set success criteria for knowing when we've achieved the goal
    #[must_use]
    pub fn with_success_criteria(mut self, criteria: Vec<String>) -> Self {
        self.success_criteria = criteria;
        self
    }

    /// Top N most recent thoughts by confidence
    #[must_use]
    pub fn top_thoughts(&self, n: usize) -> Vec<&Thought> {
        let mut sorted = self.previous_thoughts.iter().collect::<Vec<_>>();
        sorted.sort_by(|a, b| {
            b.metadata
                .confidence
                .partial_cmp(&a.metadata.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.into_iter().take(n).collect()
    }

    /// Summarize previous thoughts for inclusion in prompt
    #[must_use]
    pub fn thoughts_summary(&self, max_length: usize) -> String {
        if self.previous_thoughts.is_empty() {
            return "No previous thoughts yet.".to_string();
        }

        let top = self.top_thoughts(2);
        let mut summary = "Previous thoughts:\n".to_string();

        for (idx, thought) in top.iter().enumerate() {
            let content = if thought.content.len() > 100 {
                let mut end = 97;
                while end > 0 && !thought.content.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}...", &thought.content[..end])
            } else {
                thought.content.clone()
            };

            let _ = writeln!(
                summary,
                "{}. [{:?}] {} (confidence: {:.2})",
                idx + 1,
                thought.kind,
                content,
                thought.metadata.confidence
            );
        }

        if summary.len() > max_length {
            let mut end = max_length;
            while end > 0 && !summary.is_char_boundary(end) {
                end -= 1;
            }
            summary.truncate(end);
            summary.push_str("\n[truncated]");
        }

        summary
    }

    /// Format constraints for display
    #[must_use]
    pub fn format_constraints(&self) -> String {
        if self.constraints.is_empty() {
            return String::new();
        }

        let mut result = "Constraints:\n".to_string();
        for (idx, constraint) in self.constraints.iter().enumerate() {
            let _ = writeln!(result, "  {}. {}", idx + 1, constraint);
        }
        result
    }

    /// Format the goal description for inclusion in prompts.
    /// Returns empty string if no goal is set (so templates can skip the section).
    #[allow(clippy::option_if_let_else)]
    #[must_use]
    pub fn format_goal(&self) -> String {
        if let Some(ref goal) = self.goal_description {
            let mut result = format!("## Goal\n{goal}");
            if !self.success_criteria.is_empty() {
                result.push_str("\n### Success Criteria\n");
                for (idx, criterion) in self.success_criteria.iter().enumerate() {
                    let _ = writeln!(result, "  {}. {criterion}", idx + 1);
                }
            }
            result
        } else {
            String::new()
        }
    }

    /// Prune low-confidence thoughts, keeping only those above the threshold.
    /// Returns a new `PromptContext` with only the high-confidence thoughts.
    #[must_use]
    pub fn prune_low_confidence(&self, threshold: f64) -> Self {
        let pruned: Vec<Thought> = self
            .previous_thoughts
            .iter()
            .filter(|t| t.metadata.confidence >= threshold)
            .cloned()
            .collect();
        Self {
            previous_thoughts: pruned,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thinking::core::types::ThoughtKind;

    #[test]
    fn test_context_builder() {
        let context = PromptContext::new("Test problem")
            .with_depth(2)
            .with_iteration(1);

        assert_eq!(context.problem, "Test problem");
        assert_eq!(context.current_depth, 2);
        assert_eq!(context.iteration, 1);
    }

    #[test]
    fn test_top_thoughts_sorting() {
        let t1 = Thought::new(ThoughtKind::Initial, "Low".to_string()).with_confidence(0.3);
        let t2 = Thought::new(ThoughtKind::Analysis, "High".to_string()).with_confidence(0.9);
        let t3 = Thought::new(ThoughtKind::Synthesis, "Mid".to_string()).with_confidence(0.6);

        let context = PromptContext::new("Test").with_previous_thoughts(vec![t1, t2, t3]);
        let top = context.top_thoughts(2);

        assert_eq!(top.len(), 2);
        assert!(top[0].metadata.confidence > top[1].metadata.confidence);
    }

    #[test]
    fn test_thoughts_summary_truncation() {
        let long_thought = Thought::new(ThoughtKind::Initial, "a".repeat(500)).with_confidence(0.8);

        let context = PromptContext::new("Test").with_previous_thoughts(vec![long_thought]);
        let summary = context.thoughts_summary(50);

        assert!(
            summary.contains("[truncated]"),
            "Long summary should be truncated"
        );
        assert!(
            summary.len() <= 100,
            "Summary should not exceed reasonable bounds"
        );
    }

    #[test]
    fn test_format_constraints() {
        let context = PromptContext::new("Test").with_constraints(vec![
            "Max depth: 5".to_string(),
            "Time limit: 30s".to_string(),
        ]);

        let formatted = context.format_constraints();
        assert!(formatted.contains("Max depth"));
        assert!(formatted.contains("Time limit"));
    }

    #[test]
    fn test_with_goal_and_success_criteria() {
        let context = PromptContext::new("Test problem")
            .with_goal("Find the optimal solution")
            .with_success_criteria(vec!["Cost < 100".to_string(), "Time < 5s".to_string()]);

        assert_eq!(
            context.goal_description,
            Some("Find the optimal solution".to_string())
        );
        assert_eq!(context.success_criteria.len(), 2);
    }

    #[test]
    fn test_format_goal_with_criteria() {
        let context = PromptContext::new("Test")
            .with_goal("Solve X")
            .with_success_criteria(vec!["Criterion A".to_string()]);

        let formatted = context.format_goal();
        assert!(formatted.contains("## Goal"));
        assert!(formatted.contains("Solve X"));
        assert!(formatted.contains("### Success Criteria"));
        assert!(formatted.contains("Criterion A"));
    }

    #[test]
    fn test_format_goal_without_goal() {
        let context = PromptContext::new("Test");
        assert!(context.format_goal().is_empty());
    }

    #[test]
    fn test_prune_low_confidence() {
        let t1 = Thought::new(ThoughtKind::Initial, "Low".to_string()).with_confidence(0.2);
        let t2 = Thought::new(ThoughtKind::Analysis, "High".to_string()).with_confidence(0.9);
        let t3 = Thought::new(ThoughtKind::Synthesis, "Mid".to_string()).with_confidence(0.6);

        let context = PromptContext::new("Test").with_previous_thoughts(vec![t1, t2, t3]);
        let pruned = context.prune_low_confidence(0.5);

        assert_eq!(pruned.previous_thoughts.len(), 2);
        assert!(pruned
            .previous_thoughts
            .iter()
            .all(|t| t.metadata.confidence >= 0.5));
    }

    #[test]
    fn test_format_constraints_empty() {
        let context = PromptContext::new("Test");
        assert!(context.format_constraints().is_empty());
    }

    #[test]
    fn test_context_debug() {
        let context = PromptContext::new("Debug test");
        let debug = format!("{context:?}");
        assert!(debug.contains("Debug test"));
    }

    #[test]
    fn test_context_clone() {
        let ctx = PromptContext::new("Clone me").with_depth(3);
        #[allow(clippy::redundant_clone)]
        let cloned = ctx.clone();
        assert_eq!(cloned.problem, "Clone me");
        assert_eq!(cloned.current_depth, 3);
    }

    #[test]
    fn test_thoughts_summary_empty() {
        let context = PromptContext::new("Test");
        let summary = context.thoughts_summary(100);
        assert!(summary.contains("No previous thoughts"));
    }

    #[test]
    fn test_thoughts_summary_with_multiple_thoughts() {
        let t1 = Thought::new(ThoughtKind::Initial, "First idea".to_string()).with_confidence(0.5);
        let t2 =
            Thought::new(ThoughtKind::Analysis, "Second idea".to_string()).with_confidence(0.9);

        let ctx = PromptContext::new("Test").with_previous_thoughts(vec![t1, t2]);
        let summary = ctx.thoughts_summary(500);
        assert!(summary.contains("Previous thoughts"));
        assert!(summary.contains("Second idea"));
    }

    #[test]
    fn test_with_metadata() {
        let ctx = PromptContext::new("Test")
            .with_metadata("key1", "value1")
            .with_metadata("key2", "value2");

        assert_eq!(ctx.metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(ctx.metadata.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_with_graph_summary() {
        let ctx = PromptContext::new("Test").with_graph_summary("3 nodes, 2 edges");
        assert_eq!(ctx.graph_summary, "3 nodes, 2 edges");
    }

    #[test]
    fn test_prune_low_confidence_all_above() {
        let t = Thought::new(ThoughtKind::Analysis, "Good".to_string()).with_confidence(0.9);
        let ctx = PromptContext::new("Test").with_previous_thoughts(vec![t]);
        let pruned = ctx.prune_low_confidence(0.5);
        assert_eq!(pruned.previous_thoughts.len(), 1);
    }

    #[test]
    fn test_prune_low_confidence_all_below() {
        let t = Thought::new(ThoughtKind::Analysis, "Bad".to_string()).with_confidence(0.1);
        let ctx = PromptContext::new("Test").with_previous_thoughts(vec![t]);
        let pruned = ctx.prune_low_confidence(0.5);
        assert!(pruned.previous_thoughts.is_empty());
    }

    #[test]
    fn test_top_thoughts_empty() {
        let ctx = PromptContext::new("Test");
        assert!(ctx.top_thoughts(5).is_empty());
    }

    #[test]
    fn test_top_thoughts_more_than_available() {
        let t = Thought::new(ThoughtKind::Initial, "Only".to_string()).with_confidence(0.7);
        let ctx = PromptContext::new("Test").with_previous_thoughts(vec![t]);
        let top = ctx.top_thoughts(10);
        assert_eq!(top.len(), 1);
    }
}
