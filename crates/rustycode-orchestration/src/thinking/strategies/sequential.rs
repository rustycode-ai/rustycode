use super::ReasoningStrategy;
use crate::thinking::core::error::Result;
use async_trait::async_trait;

/// Sequential reasoning: step-by-step linear reasoning for ordered problems
pub struct SequentialStrategy;

#[async_trait]
impl ReasoningStrategy for SequentialStrategy {
    fn name(&self) -> &'static str {
        "Sequential"
    }

    async fn execute(&self, _prompt: &str) -> Result<Vec<String>> {
        // Phase 1: Stub implementation
        Ok(vec!["Step 1: Initial analysis".to_string()])
    }

    fn matches_problem(&self, _problem: &str) -> bool {
        false
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
    async fn test_sequential_execute() {
        let s = SequentialStrategy;
        let result = s.execute("test").await.unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_sequential_name() {
        assert_eq!(SequentialStrategy.name(), "Sequential");
    }

    #[test]
    fn test_sequential_no_match() {
        assert!(!SequentialStrategy.matches_problem("anything"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_sequential_is_suitable_always_zero() {
        assert!((SequentialStrategy.is_suitable_for("anything") - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_sequential_execute_returns_initial_analysis() {
        let s = SequentialStrategy;
        let result = s.execute("test").await.unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("Step 1"));
        assert!(result[0].contains("Initial analysis"));
    }

    #[test]
    fn test_sequential_matches_problem_always_false() {
        // Sequential is the default/fallback strategy — it never matches keywords
        assert!(!SequentialStrategy.matches_problem("step by step"));
        assert!(!SequentialStrategy.matches_problem("first then next"));
        assert!(!SequentialStrategy.matches_problem("sequential process"));
    }
}
