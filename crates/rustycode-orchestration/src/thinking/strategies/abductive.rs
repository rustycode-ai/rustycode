use super::ReasoningStrategy;
use crate::thinking::core::error::Result;
use async_trait::async_trait;

/// Abductive reasoning: Hypothesis generation and inference for debugging and diagnosis
pub struct AbductiveStrategy;

#[async_trait]
impl ReasoningStrategy for AbductiveStrategy {
    fn name(&self) -> &'static str {
        "Abductive"
    }

    async fn execute(&self, _prompt: &str) -> Result<Vec<String>> {
        // Phase 1: Stub implementation
        Ok(vec![
            "Observations: Known facts".to_string(),
            "Hypotheses: Possible explanations".to_string(),
            "Best explanation: Most likely cause".to_string(),
        ])
    }

    fn matches_problem(&self, problem: &str) -> bool {
        let lower = problem.to_lowercase();
        [
            "why",
            "debug",
            "diagnose",
            "root cause",
            "error",
            "bug",
            "fix",
            "investigate",
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
    async fn test_abductive_execute() {
        let s = AbductiveStrategy;
        let result = s.execute("test").await.unwrap();
        assert!(result.len() >= 2);
    }

    #[test]
    fn test_abductive_name() {
        assert_eq!(AbductiveStrategy.name(), "Abductive");
    }

    #[test]
    fn test_abductive_matches() {
        assert!(AbductiveStrategy.matches_problem("why does this error"));
        assert!(AbductiveStrategy.matches_problem("debug the crash"));
        assert!(AbductiveStrategy.matches_problem("investigate root cause"));
    }

    #[test]
    fn test_abductive_no_match() {
        assert!(!AbductiveStrategy.matches_problem("build a feature"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_abductive_is_suitable_matching() {
        assert!((AbductiveStrategy.is_suitable_for("debug crash") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(deprecated)]
    fn test_abductive_is_suitable_non_matching() {
        assert!((AbductiveStrategy.is_suitable_for("create user") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_abductive_matches_all_keywords() {
        assert!(AbductiveStrategy.matches_problem("why does it fail"));
        assert!(AbductiveStrategy.matches_problem("diagnose the issue"));
        assert!(AbductiveStrategy.matches_problem("root cause analysis"));
        assert!(AbductiveStrategy.matches_problem("fix the error"));
        assert!(AbductiveStrategy.matches_problem("bug in parser"));
    }

    #[test]
    fn test_abductive_matches_case_insensitive() {
        assert!(AbductiveStrategy.matches_problem("DEBUG the system"));
        assert!(AbductiveStrategy.matches_problem("WHY this happens"));
    }

    #[tokio::test]
    async fn test_abductive_execute_returns_three_steps() {
        let s = AbductiveStrategy;
        let result = s.execute("diagnose timeout").await.unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].contains("Observations"));
        assert!(result[1].contains("Hypotheses"));
        assert!(result[2].contains("Best explanation"));
    }
}
