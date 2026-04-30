use crate::thinking::core::graph::ReasoningGraph;
use crate::thinking::core::scoring::ConfidenceScorer;
use crate::thinking::strategies::{ReasoningStrategy, StrategyFactory};

pub struct StrategySelector;

impl StrategySelector {
    #[must_use]
    pub fn select(problem: &str, graph: &ReasoningGraph) -> Box<dyn ReasoningStrategy> {
        let strategies: Vec<Box<dyn ReasoningStrategy>> = vec![
            Box::new(StrategyFactory::abductive()),
            Box::new(StrategyFactory::dialectic()),
            Box::new(StrategyFactory::parallel()),
            Box::new(StrategyFactory::analogical()),
        ];

        for strategy in &strategies {
            if strategy.matches_problem(problem) {
                if let Some(override_strategy) = Self::metadata_override(strategy.name(), graph) {
                    return override_strategy;
                }
                return Self::get_strategy(strategy.name());
            }
        }

        Self::get_strategy("Sequential")
    }

    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::similar_names)]
    fn metadata_override(
        current_strategy: &str,
        graph: &ReasoningGraph,
    ) -> Option<Box<dyn ReasoningStrategy>> {
        if graph.is_empty() {
            return None;
        }

        let scorer = ConfidenceScorer::new();
        let scores = scorer.score_all(graph);
        let avg_confidence: f64 = scores.values().sum::<f64>() / scores.len().max(1) as f64;

        let contradictions = graph
            .edges()
            .iter()
            .filter(|e| {
                use crate::thinking::core::types::EdgeKind;
                matches!(e.kind, EdgeKind::Contradicts)
            })
            .count();

        if contradictions > 0 && current_strategy != "Dialectic" {
            return Some(Box::new(StrategyFactory::dialectic()));
        }

        if avg_confidence < 0.4 && current_strategy != "Abductive" {
            return Some(Box::new(StrategyFactory::abductive()));
        }

        None
    }

    fn get_strategy(name: &str) -> Box<dyn ReasoningStrategy> {
        match name {
            "Dialectic" => Box::new(StrategyFactory::dialectic()),
            "Parallel" => Box::new(StrategyFactory::parallel()),
            "Analogical" => Box::new(StrategyFactory::analogical()),
            "Abductive" => Box::new(StrategyFactory::abductive()),
            _ => Box::new(StrategyFactory::sequential()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::thinking::core::types::{Thought, ThoughtKind};

    #[test]
    fn test_strategy_selection_sequential() {
        let mut graph = ReasoningGraph::new();
        let thought = Thought::new(ThoughtKind::Initial, "Step by step process".to_string());
        graph.add_thought(thought).unwrap();

        let strategy = StrategySelector::select("Describe a step by step process", &graph);
        assert_eq!(strategy.name(), "Sequential");
    }

    #[test]
    fn test_strategy_selection_dialectic() {
        let mut graph = ReasoningGraph::new();
        let thought = Thought::new(ThoughtKind::Initial, "Tradeoff between X and Y".to_string());
        graph.add_thought(thought).unwrap();

        let strategy = StrategySelector::select(
            "Analyze the tradeoff, conflict, and tension between approaches",
            &graph,
        );
        assert_eq!(strategy.name(), "Dialectic");
    }

    #[test]
    fn test_deterministic_abductive() {
        let graph = ReasoningGraph::new();
        let strategy = StrategySelector::select("debug the error in my code", &graph);
        assert_eq!(strategy.name(), "Abductive");
    }

    #[test]
    fn test_deterministic_analogical() {
        let graph = ReasoningGraph::new();
        let strategy = StrategySelector::select("Use an analogy from nature to solve this", &graph);
        assert_eq!(strategy.name(), "Analogical");
    }

    #[test]
    fn test_deterministic_parallel() {
        let graph = ReasoningGraph::new();
        let strategy =
            StrategySelector::select("Analyze from multiple perspectives thoroughly", &graph);
        assert_eq!(strategy.name(), "Parallel");
    }

    #[test]
    fn test_fallback_to_sequential() {
        let graph = ReasoningGraph::new();
        let strategy = StrategySelector::select("Describe a simple fact", &graph);
        assert_eq!(strategy.name(), "Sequential");
    }

    #[test]
    fn test_metadata_override_contradictions_switch_to_dialectic() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "analyze".to_string()).with_confidence(0.9);
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Critique, "critique".to_string()).with_confidence(0.8);
        let id2 = t2.id;

        graph.add_thought(t1).unwrap();
        graph.add_thought(t2).unwrap();
        graph
            .add_edge(
                id1,
                id2,
                crate::thinking::core::types::EdgeKind::Contradicts,
            )
            .unwrap();

        // "multiple perspectives" matches Parallel, but contradictions
        // should override to Dialectic
        let strategy =
            StrategySelector::select("Analyze from multiple perspectives thoroughly", &graph);
        assert_eq!(
            strategy.name(),
            "Dialectic",
            "Should override to Dialectic when contradictions exist"
        );
    }

    #[test]
    fn test_metadata_override_low_confidence_switches_to_abductive() {
        let mut graph = ReasoningGraph::new();
        // Very low confidence thoughts
        let t1 = Thought::new(ThoughtKind::Initial, "analyze".to_string()).with_confidence(0.1);
        graph.add_thought(t1).unwrap();

        // "multiple perspectives" matches Parallel, but low confidence
        // should override to Abductive
        let strategy =
            StrategySelector::select("Analyze from multiple perspectives thoroughly", &graph);
        assert_eq!(
            strategy.name(),
            "Abductive",
            "Should override to Abductive when confidence is very low"
        );
    }

    #[test]
    fn test_metadata_no_override_when_strategy_already_matches() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "debug error".to_string()).with_confidence(0.9);
        let t2 = Thought::new(ThoughtKind::Critique, "critique".to_string()).with_confidence(0.8);
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_thought(t1).unwrap();
        graph.add_thought(t2).unwrap();
        graph
            .add_edge(
                id1,
                id2,
                crate::thinking::core::types::EdgeKind::Contradicts,
            )
            .unwrap();

        // "debug" matches Abductive, contradictions would suggest Dialectic
        // but current_strategy is already checked — since Abductive != Dialectic, it should switch
        let strategy = StrategySelector::select("debug the error in my code", &graph);
        assert_eq!(strategy.name(), "Dialectic");
    }

    #[test]
    fn test_select_empty_graph_no_override() {
        let graph = ReasoningGraph::new();
        // Empty graph — metadata_override should return None
        let strategy =
            StrategySelector::select("Analyze the tradeoff and tension between X and Y", &graph);
        assert_eq!(strategy.name(), "Dialectic");
    }
}
