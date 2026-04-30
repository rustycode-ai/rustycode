use super::ReasoningStrategy;
use crate::thinking::core::error::Result;
use async_trait::async_trait;

/// Analogical reasoning: Pattern mapping from known domains for creative solutions
pub struct AnalogicalStrategy;

#[async_trait]
impl ReasoningStrategy for AnalogicalStrategy {
    fn name(&self) -> &'static str {
        "Analogical"
    }

    async fn execute(&self, _prompt: &str) -> Result<Vec<String>> {
        // Phase 1: Stub implementation
        Ok(vec![
            "Known domain: Analogous situation".to_string(),
            "Pattern mapping: Shared structure".to_string(),
            "Application: Mapped to current problem".to_string(),
        ])
    }

    fn matches_problem(&self, problem: &str) -> bool {
        let lower = problem.to_lowercase();
        [
            "analogy",
            "similar to",
            "like how",
            "pattern from",
            "borrow from",
            "inspired by",
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
    async fn test_analogical_execute() {
        let s = AnalogicalStrategy;
        let result = s.execute("test").await.unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_analogical_name() {
        assert_eq!(AnalogicalStrategy.name(), "Analogical");
    }

    #[test]
    fn test_analogical_matches() {
        assert!(AnalogicalStrategy.matches_problem("similar to the cache problem"));
        assert!(AnalogicalStrategy.matches_problem("inspired by microservices"));
    }

    #[test]
    fn test_analogical_no_match() {
        assert!(!AnalogicalStrategy.matches_problem("fix typo"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_analogical_is_suitable_matching() {
        assert!(
            (AnalogicalStrategy.is_suitable_for("similar to cache") - 1.0).abs() < f64::EPSILON
        );
    }

    #[test]
    #[allow(deprecated)]
    fn test_analogical_is_suitable_non_matching() {
        assert!((AnalogicalStrategy.is_suitable_for("build widget") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_analogical_matches_all_keywords() {
        assert!(AnalogicalStrategy.matches_problem("analogy with event sourcing"));
        assert!(AnalogicalStrategy.matches_problem("like how DNS works"));
        assert!(AnalogicalStrategy.matches_problem("pattern from distributed systems"));
        assert!(AnalogicalStrategy.matches_problem("borrow from functional programming"));
        assert!(AnalogicalStrategy.matches_problem("inspired by Erlang"));
    }

    #[test]
    fn test_analogical_matches_case_insensitive() {
        assert!(AnalogicalStrategy.matches_problem("SIMILAR TO the old system"));
        assert!(AnalogicalStrategy.matches_problem("INSPIRED BY nature"));
    }

    #[tokio::test]
    async fn test_analogical_execute_returns_three_steps() {
        let s = AnalogicalStrategy;
        let result = s.execute("analogy test").await.unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].contains("Known domain"));
        assert!(result[1].contains("Pattern mapping"));
        assert!(result[2].contains("Application"));
    }
}
