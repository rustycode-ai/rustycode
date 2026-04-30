//! Graph pruning: Intelligent removal of low-quality thoughts and dead ends

use crate::thinking::core::error::Result;
use crate::thinking::core::graph::ReasoningGraph;
use crate::thinking::core::scoring::ConfidenceScorer;
use crate::thinking::core::types::ThoughtId;

/// Pruning strategy
#[derive(Debug, Clone, Copy)]
pub enum PruningStrategy {
    /// Remove thoughts below confidence threshold
    ConfidenceThreshold,
    /// Remove redundant thoughts (same content, lower confidence)
    Redundancy,
    /// Remove dead-end thoughts (no successors, low confidence)
    DeadEnds,
    /// Aggressive: combine all strategies
    Aggressive,
}

/// Manages graph pruning and cleanup
pub struct GraphPruner {
    scorer: ConfidenceScorer,
    confidence_threshold: f64,
    max_nodes: usize,
}

impl GraphPruner {
    #[must_use]
    pub const fn new(scorer: ConfidenceScorer) -> Self {
        Self {
            scorer,
            confidence_threshold: 0.3,
            max_nodes: 200,
        }
    }

    #[must_use]
    pub const fn with_threshold(mut self, threshold: f64) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    #[must_use]
    pub const fn with_max_nodes(mut self, max: usize) -> Self {
        self.max_nodes = max;
        self
    }

    /// Apply pruning strategy to graph.
    ///
    /// # Errors
    ///
    /// Returns an error if individual pruning operations fail due to invalid graph state.
    pub fn prune(&self, graph: &mut ReasoningGraph, strategy: PruningStrategy) -> Result<usize> {
        let pruned_count = match strategy {
            PruningStrategy::ConfidenceThreshold => self.prune_low_confidence(graph),
            PruningStrategy::Redundancy => self.prune_redundant(graph),
            PruningStrategy::DeadEnds => self.prune_dead_ends(graph),
            PruningStrategy::Aggressive => {
                self.prune_low_confidence(graph)
                    + self.prune_redundant(graph)
                    + self.prune_dead_ends(graph)
            }
        };

        Ok(pruned_count)
    }

    /// Remove thoughts below confidence threshold
    fn prune_low_confidence(&self, graph: &mut ReasoningGraph) -> usize {
        let low_conf_ids = self
            .scorer
            .low_confidence_thoughts(graph, self.confidence_threshold);

        let mut removed = 0;
        for id in low_conf_ids {
            if graph.remove_thought(id).is_ok() {
                removed += 1;
            }
        }

        removed
    }

    /// Remove redundant thoughts (same content, different confidence)
    fn prune_redundant(&self, graph: &mut ReasoningGraph) -> usize {
        let mut content_map: std::collections::HashMap<String, Vec<ThoughtId>> =
            std::collections::HashMap::new();

        // Group thoughts by content
        for thought in graph.thoughts() {
            content_map
                .entry(thought.content.clone())
                .or_default()
                .push(thought.id);
        }

        let mut removed = 0;

        // For each content group, keep best, remove others
        for ids in content_map.values() {
            if ids.len() > 1 {
                // Find thought with highest confidence.
                // ids.len() > 1 guarantees at least one element, but use
                // defensive iteration to avoid panicking.
                let Some(&best_id) = ids.iter().max_by(|&&a, &&b| {
                    let a_score = self.scorer.score(a, graph);
                    let b_score = self.scorer.score(b, graph);
                    a_score
                        .partial_cmp(&b_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }) else {
                    continue;
                };

                // Remove the others
                for &id in ids {
                    if id != best_id && graph.remove_thought(id).is_ok() {
                        removed += 1;
                    }
                }
            }
        }

        removed
    }

    /// Remove dead-end thoughts (no successors, low value)
    fn prune_dead_ends(&self, graph: &mut ReasoningGraph) -> usize {
        let mut dead_ends = Vec::new();

        for thought in graph.thoughts() {
            let successors = graph.successors(thought.id);
            let score = self.scorer.score(thought.id, graph);

            // A thought is a dead-end if:
            // 1. It has no successors
            // 2. Its confidence is below 0.4
            if successors.is_empty() && score < 0.4 {
                dead_ends.push(thought.id);
            }
        }

        let mut removed = 0;
        for id in dead_ends {
            if graph.remove_thought(id).is_ok() {
                removed += 1;
            }
        }

        removed
    }

    /// Enforce maximum node limit by removing lowest-scoring thoughts.
    ///
    /// # Errors
    ///
    /// Returns an error if removing thoughts fails due to invalid graph state.
    pub fn enforce_max_nodes(&self, graph: &mut ReasoningGraph) -> Result<usize> {
        if graph.len() <= self.max_nodes {
            return Ok(0);
        }

        let excess = graph.len() - self.max_nodes;
        let scores = self.scorer.score_all(graph);

        // Sort by score (ascending)
        let mut scored_ids: Vec<_> = scores.iter().collect();
        scored_ids.sort_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut removed = 0;
        for (id, _) in scored_ids.iter().take(excess) {
            if graph.remove_thought(**id).is_ok() {
                removed += 1;
            }
        }

        Ok(removed)
    }

    /// Get thoughts that would be pruned (dry run)
    #[must_use]
    pub fn would_prune(&self, graph: &ReasoningGraph, strategy: PruningStrategy) -> Vec<ThoughtId> {
        match strategy {
            PruningStrategy::ConfidenceThreshold => self
                .scorer
                .low_confidence_thoughts(graph, self.confidence_threshold),
            PruningStrategy::DeadEnds => {
                let mut dead_ends = Vec::new();
                for thought in graph.thoughts() {
                    let successors = graph.successors(thought.id);
                    let score = self.scorer.score(thought.id, graph);
                    if successors.is_empty() && score < 0.4 {
                        dead_ends.push(thought.id);
                    }
                }
                dead_ends
            }
            _ => Vec::new(), // More complex for redundancy and aggressive
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::thinking::core::types::{Thought, ThoughtKind};

    #[test]
    fn test_prune_low_confidence() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_threshold(0.5);

        let mut graph = ReasoningGraph::new();

        let low = Thought::new(ThoughtKind::Analysis, "Low".to_string()).with_confidence(0.2);
        let high = Thought::new(ThoughtKind::Analysis, "High".to_string()).with_confidence(0.8);

        graph.add_thought(low)?;
        graph.add_thought(high)?;

        let initial_count = graph.len();
        let removed = pruner.prune(&mut graph, PruningStrategy::ConfidenceThreshold)?;

        assert_eq!(removed, 1);
        assert_eq!(graph.len(), initial_count - 1);
        Ok(())
    }

    #[test]
    fn test_enforce_max_nodes() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_max_nodes(2);

        let mut graph = ReasoningGraph::new();

        for i in 0..5 {
            let t = Thought::new(ThoughtKind::Analysis, format!("Thought {i}"))
                .with_confidence(f64::from(i).mul_add(0.1, 0.5));
            graph.add_thought(t)?;
        }

        let removed = pruner.enforce_max_nodes(&mut graph)?;
        assert!(removed > 0);
        assert!(graph.len() <= 2);
        Ok(())
    }

    #[test]
    fn test_prune_redundant_keeps_best() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer);

        let mut graph = ReasoningGraph::new();
        graph.add_thought(
            Thought::new(ThoughtKind::Analysis, "same content".into()).with_confidence(0.3),
        )?;
        graph.add_thought(
            Thought::new(ThoughtKind::Analysis, "same content".into()).with_confidence(0.9),
        )?;
        graph.add_thought(
            Thought::new(ThoughtKind::Analysis, "same content".into()).with_confidence(0.5),
        )?;

        let removed = pruner.prune(&mut graph, PruningStrategy::Redundancy)?;
        assert_eq!(removed, 2);
        assert_eq!(graph.len(), 1);
        Ok(())
    }

    #[test]
    fn test_prune_dead_ends() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer);

        let mut graph = ReasoningGraph::new();
        // Dead end: no successors, low score
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "dead".into()).with_confidence(0.1))?;
        // Good thought
        graph.add_thought(
            Thought::new(ThoughtKind::Synthesis, "good".into()).with_confidence(0.9),
        )?;

        let removed = pruner.prune(&mut graph, PruningStrategy::DeadEnds)?;
        assert_eq!(removed, 1);
        assert_eq!(graph.len(), 1);
        Ok(())
    }

    #[test]
    fn test_prune_aggressive_combines_all() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_threshold(0.5);

        let mut graph = ReasoningGraph::new();
        graph.add_thought(
            Thought::new(ThoughtKind::Analysis, "low conf".into()).with_confidence(0.1),
        )?;
        graph.add_thought(
            Thought::new(ThoughtKind::Synthesis, "good".into()).with_confidence(0.9),
        )?;

        let removed = pruner.prune(&mut graph, PruningStrategy::Aggressive)?;
        assert!(removed >= 1);
        Ok(())
    }

    #[test]
    fn test_would_prune_dry_run() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_threshold(0.5);

        let mut graph = ReasoningGraph::new();
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "low".into()).with_confidence(0.1))?;
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "high".into()).with_confidence(0.9))?;

        let would_remove = pruner.would_prune(&graph, PruningStrategy::ConfidenceThreshold);
        assert_eq!(would_remove.len(), 1);
        // Graph should be unchanged
        assert_eq!(graph.len(), 2);
        Ok(())
    }

    #[test]
    fn test_enforce_max_nodes_no_change_if_under() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_max_nodes(100);

        let mut graph = ReasoningGraph::new();
        graph.add_thought(Thought::new(ThoughtKind::Analysis, "t1".into()))?;
        graph.add_thought(Thought::new(ThoughtKind::Analysis, "t2".into()))?;

        let removed = pruner.enforce_max_nodes(&mut graph)?;
        assert_eq!(removed, 0);
        assert_eq!(graph.len(), 2);
        Ok(())
    }

    #[test]
    fn test_prune_empty_graph() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer);

        let mut graph = ReasoningGraph::new();
        let removed = pruner.prune(&mut graph, PruningStrategy::Aggressive)?;
        assert_eq!(removed, 0);
        Ok(())
    }

    #[test]
    fn test_pruning_strategy_debug() {
        assert!(
            format!("{:?}", PruningStrategy::ConfidenceThreshold).contains("ConfidenceThreshold")
        );
        assert!(format!("{:?}", PruningStrategy::Redundancy).contains("Redundancy"));
        assert!(format!("{:?}", PruningStrategy::DeadEnds).contains("DeadEnds"));
        assert!(format!("{:?}", PruningStrategy::Aggressive).contains("Aggressive"));
    }

    #[test]
    fn test_pruning_strategy_clone_copy() {
        let s = PruningStrategy::DeadEnds;
        let s2 = s; // Copy
        assert_eq!(
            format!("{s2:?}"),
            format!("{:?}", PruningStrategy::DeadEnds)
        );
    }

    #[test]
    fn test_builder_chaining() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer)
            .with_threshold(0.7)
            .with_max_nodes(5);

        let mut graph = ReasoningGraph::new();
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "low".into()).with_confidence(0.1))?;
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "mid".into()).with_confidence(0.5))?;
        graph.add_thought(
            Thought::new(ThoughtKind::Synthesis, "high".into()).with_confidence(0.9),
        )?;

        let removed = pruner.prune(&mut graph, PruningStrategy::ConfidenceThreshold)?;
        assert_eq!(removed, 2);
        assert_eq!(graph.len(), 1);
        Ok(())
    }

    #[test]
    fn test_would_prune_dead_ends() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer);

        let mut graph = ReasoningGraph::new();
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "dead".into()).with_confidence(0.1))?;
        graph.add_thought(
            Thought::new(ThoughtKind::Synthesis, "alive".into()).with_confidence(0.9),
        )?;

        let candidates = pruner.would_prune(&graph, PruningStrategy::DeadEnds);
        assert_eq!(candidates.len(), 1);
        assert_eq!(graph.len(), 2, "would_prune should not modify graph");
        Ok(())
    }

    #[test]
    fn test_would_prune_redundancy_returns_empty() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer);

        let mut graph = ReasoningGraph::new();
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "same".into()).with_confidence(0.3))?;
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "same".into()).with_confidence(0.9))?;

        let candidates = pruner.would_prune(&graph, PruningStrategy::Redundancy);
        assert!(
            candidates.is_empty(),
            "Redundancy strategy not supported in would_prune"
        );
        Ok(())
    }

    #[test]
    fn test_would_prune_aggressive_returns_empty() {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer);

        let graph = ReasoningGraph::new();
        let candidates = pruner.would_prune(&graph, PruningStrategy::Aggressive);
        assert!(
            candidates.is_empty(),
            "Aggressive strategy not supported in would_prune"
        );
    }

    #[test]
    fn test_enforce_max_nodes_exact_removal() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_max_nodes(3);

        let mut graph = ReasoningGraph::new();
        for i in 0..6 {
            let t = Thought::new(ThoughtKind::Analysis, format!("t{i}"))
                .with_confidence(0.1 * f64::from(i));
            graph.add_thought(t)?;
        }

        assert_eq!(graph.len(), 6);
        let removed = pruner.enforce_max_nodes(&mut graph)?;
        assert_eq!(removed, 3, "Should remove exactly 3 to reach max_nodes=3");
        assert_eq!(graph.len(), 3);
        Ok(())
    }

    #[test]
    fn test_prune_dead_ends_keeps_high_confidence_leaf() -> Result<()> {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer);

        let mut graph = ReasoningGraph::new();
        // Leaf with high confidence — should NOT be pruned
        graph.add_thought(
            Thought::new(ThoughtKind::Synthesis, "good leaf".into()).with_confidence(0.9),
        )?;
        // Leaf with low confidence — should be pruned
        graph.add_thought(
            Thought::new(ThoughtKind::Analysis, "bad leaf".into()).with_confidence(0.1),
        )?;

        let removed = pruner.prune(&mut graph, PruningStrategy::DeadEnds)?;
        assert_eq!(removed, 1);
        assert_eq!(graph.len(), 1);
        let remaining = graph.thoughts().next().expect("one remains");
        assert!(remaining.content.contains("good"));
        Ok(())
    }
}
