use super::ReasoningStrategy;
use crate::thinking::core::error::Result;
use async_trait::async_trait;

/// Dialectic reasoning: Thesis→Antithesis→Synthesis for contradictions and tradeoffs
pub struct DialecticStrategy;

#[async_trait]
impl ReasoningStrategy for DialecticStrategy {
    fn name(&self) -> &'static str {
        "Dialectic"
    }

    async fn execute(&self, _prompt: &str) -> Result<Vec<String>> {
        // Phase 1: Stub implementation
        Ok(vec![
            "Thesis: Initial position".to_string(),
            "Antithesis: Opposing view".to_string(),
            "Synthesis: Reconciled position".to_string(),
        ])
    }

    fn matches_problem(&self, problem: &str) -> bool {
        let lower = problem.to_lowercase();
        [
            "tradeoff", "conflict", "tension", "versus", "vs", "dilemma", "balance", "weigh",
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
    async fn test_dialectic_execute() {
        let s = DialecticStrategy;
        let result = s.execute("test").await.unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_dialectic_name() {
        assert_eq!(DialecticStrategy.name(), "Dialectic");
    }

    #[test]
    fn test_dialectic_matches() {
        assert!(DialecticStrategy.matches_problem("tradeoff between speed and quality"));
        assert!(DialecticStrategy.matches_problem("performance vs readability"));
        assert!(DialecticStrategy.matches_problem("weigh the dilemma"));
    }

    #[test]
    fn test_dialectic_no_match() {
        assert!(!DialecticStrategy.matches_problem("implement feature"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_dialectic_is_suitable_matching() {
        assert!(
            (DialecticStrategy.is_suitable_for("tradeoff analysis") - 1.0).abs() < f64::EPSILON
        );
    }

    #[test]
    #[allow(deprecated)]
    fn test_dialectic_is_suitable_non_matching() {
        assert!((DialecticStrategy.is_suitable_for("write code") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dialectic_matches_all_keywords() {
        assert!(DialecticStrategy.matches_problem("conflict between modules"));
        assert!(DialecticStrategy.matches_problem("tension in design"));
        assert!(DialecticStrategy.matches_problem("versus alternative"));
        assert!(DialecticStrategy.matches_problem("balance speed and safety"));
    }

    #[test]
    fn test_dialectic_matches_case_insensitive() {
        assert!(DialecticStrategy.matches_problem("TRADEOFF between approaches"));
        assert!(DialecticStrategy.matches_problem("VS the other option"));
    }

    #[tokio::test]
    async fn test_dialectic_execute_returns_three_steps() {
        let s = DialecticStrategy;
        let result = s.execute("tradeoff test").await.unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].contains("Thesis"));
        assert!(result[1].contains("Antithesis"));
        assert!(result[2].contains("Synthesis"));
    }
}
