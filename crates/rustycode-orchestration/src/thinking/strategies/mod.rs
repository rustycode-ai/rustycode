//! Five adaptive reasoning strategies for problem solving

use crate::thinking::core::error::Result;
use async_trait::async_trait;

pub mod abductive;
pub mod analogical;
pub mod dialectic;
pub mod parallel;
pub mod sequential;

/// Base trait for reasoning strategies
#[async_trait]
pub trait ReasoningStrategy: Send + Sync {
    /// Strategy name (Sequential, Dialectic, Parallel, Analogical, Abductive)
    fn name(&self) -> &'static str;

    /// Execute the strategy with given prompt
    async fn execute(&self, prompt: &str) -> Result<Vec<String>>;

    /// Deterministic check: does this strategy match the given problem?
    /// First primary keyword match wins in the selection decision tree.
    fn matches_problem(&self, problem: &str) -> bool;

    /// Deprecated: kept for backward compatibility.
    /// Returns 1.0 if `matches_problem` is true, 0.0 otherwise.
    #[deprecated(note = "Use matches_problem instead")]
    fn is_suitable_for(&self, problem: &str) -> f64 {
        if self.matches_problem(problem) {
            1.0
        } else {
            0.0
        }
    }
}

/// Factory for creating strategy instances
pub struct StrategyFactory;

impl StrategyFactory {
    #[must_use]
    pub const fn sequential() -> sequential::SequentialStrategy {
        sequential::SequentialStrategy
    }

    #[must_use]
    pub const fn dialectic() -> dialectic::DialecticStrategy {
        dialectic::DialecticStrategy
    }

    #[must_use]
    pub const fn parallel() -> parallel::ParallelStrategy {
        parallel::ParallelStrategy
    }

    #[must_use]
    pub const fn analogical() -> analogical::AnalogicalStrategy {
        analogical::AnalogicalStrategy
    }

    #[must_use]
    pub const fn abductive() -> abductive::AbductiveStrategy {
        abductive::AbductiveStrategy
    }

    #[must_use]
    pub fn all() -> Vec<Box<dyn ReasoningStrategy>> {
        vec![
            Box::new(sequential::SequentialStrategy),
            Box::new(dialectic::DialecticStrategy),
            Box::new(parallel::ParallelStrategy),
            Box::new(analogical::AnalogicalStrategy),
            Box::new(abductive::AbductiveStrategy),
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, deprecated)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_creates_all_strategies() {
        let all = StrategyFactory::all();
        assert_eq!(all.len(), 5);
        let names: Vec<&str> = all.iter().map(|s| s.name()).collect();
        assert!(names.contains(&"Sequential"));
        assert!(names.contains(&"Dialectic"));
        assert!(names.contains(&"Parallel"));
        assert!(names.contains(&"Analogical"));
        assert!(names.contains(&"Abductive"));
    }

    #[test]
    fn test_factory_unique_names() {
        let all = StrategyFactory::all();
        let names: Vec<&str> = all.iter().map(|s| s.name()).collect();
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), 5);
    }

    #[test]
    fn test_factory_individual_constructors() {
        assert_eq!(StrategyFactory::sequential().name(), "Sequential");
        assert_eq!(StrategyFactory::dialectic().name(), "Dialectic");
        assert_eq!(StrategyFactory::parallel().name(), "Parallel");
        assert_eq!(StrategyFactory::analogical().name(), "Analogical");
        assert_eq!(StrategyFactory::abductive().name(), "Abductive");
    }

    #[tokio::test]
    async fn test_all_strategies_execute_successfully() {
        for strategy in StrategyFactory::all() {
            let result = strategy.execute("test prompt").await.unwrap();
            assert!(!result.is_empty(), "{} returned empty", strategy.name());
        }
    }

    #[test]
    fn test_exactly_one_strategy_matches_debug() {
        let all = StrategyFactory::all();
        assert!(
            all.iter()
                .any(|s| s.matches_problem("debug the error") && s.name() == "Abductive"),
            "Abductive should match debug"
        );
    }

    #[test]
    fn test_no_strategy_matches_gibberish() {
        let all = StrategyFactory::all();
        let count = all.iter().filter(|s| s.matches_problem("xyzzy123")).count();
        assert_eq!(count, 0, "No strategy should match gibberish");
    }

    #[test]
    fn test_dialectic_matches_tradeoff() {
        let all = StrategyFactory::all();
        assert!(
            all.iter()
                .filter(|s| s.matches_problem("tradeoff between performance and readability"))
                .map(|s| s.name())
                .any(|x| x == "Dialectic"),
            "Dialectic should match tradeoff"
        );
    }

    #[test]
    fn test_analogical_matches_similar() {
        let all = StrategyFactory::all();
        assert!(
            all.iter()
                .filter(|s| s.matches_problem("similar to how Kubernetes works"))
                .map(|s| s.name())
                .any(|x| x == "Analogical"),
            "Analogical should match 'similar to'"
        );
    }

    #[test]
    fn test_parallel_matches_comprehensive() {
        let all = StrategyFactory::all();
        assert!(
            all.iter()
                .filter(|s| s.matches_problem("need comprehensive analysis"))
                .map(|s| s.name())
                .any(|x| x == "Parallel"),
            "Parallel should match 'comprehensive analysis'"
        );
    }

    #[test]
    fn test_empty_string_matches_nothing() {
        let all = StrategyFactory::all();
        let count = all.iter().filter(|s| s.matches_problem("")).count();
        assert_eq!(count, 0, "No strategy should match empty string");
    }
}
