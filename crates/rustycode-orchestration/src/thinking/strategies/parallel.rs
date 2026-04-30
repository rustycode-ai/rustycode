use super::ReasoningStrategy;
use crate::thinking::core::error::Result;
use async_trait::async_trait;

/// Parallel reasoning: Multiple independent analyses merged for multi-aspect problems
pub struct ParallelStrategy;

#[async_trait]
impl ReasoningStrategy for ParallelStrategy {
    fn name(&self) -> &'static str {
        "Parallel"
    }

    async fn execute(&self, _prompt: &str) -> Result<Vec<String>> {
        // Phase 1: Stub implementation
        Ok(vec![
            "Analysis A: First perspective".to_string(),
            "Analysis B: Second perspective".to_string(),
            "Merged: Combined insights".to_string(),
        ])
    }

    fn matches_problem(&self, problem: &str) -> bool {
        let lower = problem.to_lowercase();
        [
            "multiple aspects",
            "multiple perspectives",
            "several angles",
            "comprehensive analysis",
            "all sides",
            "from every",
        ]
        .iter()
        .any(|kw| lower.contains(kw))
    }

    fn is_suitable_for(&self, problem: &str) -> f64 {
        if self.matches_problem(problem) {
            1.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parallel_execute() {
        let s = ParallelStrategy;
        let result = s.execute("test").await.unwrap();
        assert!(result.len() >= 2);
    }

    #[test]
    fn test_parallel_name() {
        assert_eq!(ParallelStrategy.name(), "Parallel");
    }

    #[test]
    fn test_parallel_matches() {
        assert!(ParallelStrategy.matches_problem("need comprehensive analysis"));
        assert!(ParallelStrategy.matches_problem("multiple aspects to consider"));
    }

    #[test]
    fn test_parallel_no_match() {
        assert!(!ParallelStrategy.matches_problem("simple task"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_parallel_is_suitable_matching() {
        assert!((ParallelStrategy.is_suitable_for("multiple aspects") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(deprecated)]
    fn test_parallel_is_suitable_non_matching() {
        assert!((ParallelStrategy.is_suitable_for("add field") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parallel_matches_all_keywords() {
        assert!(ParallelStrategy.matches_problem("multiple perspectives on this"));
        assert!(ParallelStrategy.matches_problem("several angles to consider"));
        assert!(ParallelStrategy.matches_problem("comprehensive analysis needed"));
        assert!(ParallelStrategy.matches_problem("from every direction"));
        assert!(ParallelStrategy.matches_problem("all sides of the argument"));
    }

    #[test]
    fn test_parallel_matches_case_insensitive() {
        assert!(ParallelStrategy.matches_problem("MULTIPLE ASPECTS of the problem"));
        assert!(ParallelStrategy.matches_problem("COMPREHENSIVE ANALYSIS"));
    }

    #[tokio::test]
    async fn test_parallel_execute_returns_three_steps() {
        let s = ParallelStrategy;
        let result = s.execute("parallel test").await.unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].contains("Analysis A"));
        assert!(result[1].contains("Analysis B"));
        assert!(result[2].contains("Merged"));
    }
}
