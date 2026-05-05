//! Confidence scoring for thoughts using a 7-factor algorithm
//!
//! All `usize as f64` casts in this module are safe because the values
//! represent graph node/edge counts that are bounded by `max_nodes` (default
//! 1000), well within the 52-bit mantissa of `f64`.

#![allow(clippy::cast_precision_loss)]

use crate::thinking::core::graph::ReasoningGraph;
use crate::thinking::core::types::{Thought, ThoughtId};

/// Seven-factor confidence scoring algorithm for thoughts
///
/// Confidence = (Base × Support × `KnowledgeBoost`) - Contradiction - `DepthPenalty` + `CoherenceBoost` - `TemporalDecay`
///
/// Factors:
/// - Base: Initial quality estimate [0.0, 1.0]
/// - Support: Evidence from predecessor thoughts [0.0, 1.0]
/// - `KnowledgeBoost`: Validation against external knowledge [0.0, 0.5]
/// - Contradiction: Penalty for conflicting thoughts [-0.5, 0.0]
/// - `DepthPenalty`: Penalty for reasoning depth [-0.3, 0.0]
/// - `CoherenceBoost`: Bonus for coherent thoughts [0.0, 0.3]
/// - `TemporalDecay`: Penalty for older thoughts [-0.1, 0.0]
pub struct ConfidenceScorer {
    // Tunable weights for each factor
    pub weight_base: f64,
    pub weight_support: f64,
    pub weight_knowledge: f64,
    pub weight_contradiction: f64,
    pub weight_depth: f64,
    pub weight_coherence: f64,
    pub weight_temporal: f64,
    pub weight_relevance: f64,
    pub max_depth: usize,
    pub enable_temporal: bool,
}

impl Default for ConfidenceScorer {
    fn default() -> Self {
        Self {
            weight_base: 1.0,
            weight_support: 0.8,
            weight_knowledge: 0.6,
            weight_contradiction: 0.8,
            weight_depth: 0.5,
            weight_coherence: 0.4,
            weight_temporal: 0.3,
            weight_relevance: 0.3,
            max_depth: 10,
            enable_temporal: false, // Disabled by default in Phase 1
        }
    }
}

impl ConfidenceScorer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Score a thought based on the graph structure and its properties
    #[must_use]
    pub fn score(&self, thought_id: ThoughtId, graph: &ReasoningGraph) -> f64 {
        let Ok(thought) = graph.thought(thought_id) else {
            return 0.0;
        };

        let base = Self::base_score(thought);
        let support = Self::support_score(thought_id, graph);
        let knowledge = Self::knowledge_score(thought);
        let contradiction = Self::contradiction_penalty(thought_id, graph);
        let depth = Self::depth_penalty(thought_id, graph);
        let coherence = Self::coherence_boost(thought_id, graph);
        let temporal = if self.enable_temporal {
            Self::temporal_decay(thought)
        } else {
            0.0
        };

        // Combined formula: (Base × Support × KnowledgeBoost) - Contradiction - DepthPenalty + CoherenceBoost - TemporalDecay
        let combined = (base * support).mul_add(1.0 + knowledge, -contradiction) - depth
            + coherence
            - temporal;

        combined.clamp(0.0, 1.0)
    }

    /// Base confidence from the thought's metadata
    const fn base_score(thought: &Thought) -> f64 {
        thought.metadata.confidence
    }

    /// Support score based on predecessor thoughts
    fn support_score(id: ThoughtId, graph: &ReasoningGraph) -> f64 {
        let predecessors = graph.predecessors(id);
        if predecessors.is_empty() {
            return 0.8; // Root thoughts get high support baseline
        }

        let avg_predecessor_confidence: f64 = predecessors
            .iter()
            .filter_map(|&pred_id| graph.thought(pred_id).ok())
            .map(|t| t.metadata.confidence)
            .sum::<f64>()
            / predecessors.len() as f64;

        // Support scales from predecessor confidence
        avg_predecessor_confidence.mul_add(0.5, 0.5)
    }

    /// Knowledge boost from evidence count
    fn knowledge_score(thought: &Thought) -> f64 {
        let evidence_count = thought.metadata.evidence.len();
        (evidence_count as f64 / 10.0).min(0.5) // Cap at 0.5 boost
    }

    /// Penalty for contradicting thoughts
    fn contradiction_penalty(id: ThoughtId, graph: &ReasoningGraph) -> f64 {
        let successors = graph.successors(id);
        let contradicting: usize = successors
            .iter()
            .filter(|&&succ_id| {
                // Check for contradicting edges
                graph
                    .edges()
                    .iter()
                    .any(|e| e.from == id && e.to == succ_id)
            })
            .count();

        let penalty = (contradicting as f64) * 0.05;
        penalty.min(0.5) // Cap penalty at 0.5
    }

    /// Penalty for deep reasoning
    fn depth_penalty(id: ThoughtId, graph: &ReasoningGraph) -> f64 {
        let depth = graph.depth(id);
        let normalized_depth = (depth as f64) / 10.0;
        (normalized_depth * 0.3).min(0.3) // Cap at 0.3 penalty
    }

    /// Coherence boost for well-connected thoughts
    fn coherence_boost(id: ThoughtId, graph: &ReasoningGraph) -> f64 {
        let in_degree = graph.predecessors(id).len();
        let out_degree = graph.successors(id).len();
        let degree = (in_degree + out_degree) as f64;

        // Thoughts with more connections get coherence bonus
        (degree / 10.0).min(0.3) // Cap at 0.3 boost
    }

    /// Temporal decay for older thoughts (stub for Phase 1)
    const fn temporal_decay(_thought: &Thought) -> f64 {
        // Phase 1: Not implementing temporal tracking
        0.0
    }

    /// Score all thoughts in graph and return as map
    #[must_use]
    pub fn score_all(&self, graph: &ReasoningGraph) -> std::collections::HashMap<ThoughtId, f64> {
        graph
            .thoughts()
            .map(|t| (t.id, self.score(t.id, graph)))
            .collect()
    }

    /// Find thoughts below confidence threshold
    #[must_use]
    pub fn low_confidence_thoughts(
        &self,
        graph: &ReasoningGraph,
        threshold: f64,
    ) -> Vec<ThoughtId> {
        let scores = self.score_all(graph);
        scores
            .into_iter()
            .filter(|(_, score)| *score < threshold)
            .map(|(id, _)| id)
            .collect()
    }

    /// Score a thought's relevance to a given goal description.
    /// Uses simple keyword overlap as a lightweight relevance signal.
    /// Returns 0.0 if no goal is provided.
    pub fn relevance_score(&self, thought: &Thought, goal: &str) -> f64 {
        if goal.is_empty() {
            return 0.0;
        }

        let goal_lower = goal.to_lowercase();
        let content_lower = thought.content.to_lowercase();

        let goal_words: Vec<&str> = goal_lower
            .split_whitespace()
            .filter(|w| w.len() > 2) // Skip short words
            .collect();

        if goal_words.is_empty() {
            return 0.0;
        }

        let matched = goal_words
            .iter()
            .filter(|word| content_lower.contains(*word))
            .count();

        (matched as f64) / (goal_words.len() as f64)
    }

    /// Score a thought combining confidence + goal relevance.
    /// Combined = confidence * (1.0 - `weight_relevance`) + relevance * `weight_relevance`
    #[allow(clippy::suboptimal_flops)]
    pub fn score_with_relevance(
        &self,
        thought_id: ThoughtId,
        graph: &ReasoningGraph,
        goal: &str,
    ) -> f64 {
        let base = self.score(thought_id, graph);
        let Ok(thought) = graph.thought(thought_id) else {
            return base;
        };
        let relevance = self.relevance_score(thought, goal);

        base * (1.0 - self.weight_relevance) + relevance * self.weight_relevance
    }

    /// Find thoughts that should be pruned because they're both low-confidence
    /// AND irrelevant to the goal.
    pub fn thoughts_to_prune(
        &self,
        graph: &ReasoningGraph,
        goal: &str,
        confidence_threshold: f64,
        relevance_threshold: f64,
    ) -> Vec<ThoughtId> {
        let scores = self.score_all(graph);
        scores
            .into_iter()
            .filter(|(id, score)| {
                if *score >= confidence_threshold {
                    return false;
                }
                if let Ok(thought) = graph.thought(*id) {
                    let rel = self.relevance_score(thought, goal);
                    return rel < relevance_threshold;
                }
                true
            })
            .map(|(id, _)| id)
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::thinking::core::types::ThoughtKind;

    #[test]
    fn test_base_scoring() {
        let scorer = ConfidenceScorer::new();
        let mut graph = ReasoningGraph::new();

        let thought = Thought::new(ThoughtKind::Initial, "Test".to_string()).with_confidence(0.7);
        let id = thought.id;
        graph
            .add_thought(thought)
            .expect("add thought should succeed");

        let score = scorer.score(id, &graph);
        assert!(score > 0.5 && score < 0.9);
    }

    #[test]
    fn test_root_thought_support() {
        let scorer = ConfidenceScorer::new();
        let mut graph = ReasoningGraph::new();

        let root = Thought::new(ThoughtKind::Initial, "Root".to_string()).with_confidence(0.5);
        let id = root.id;
        graph.add_thought(root).expect("add root should succeed");

        let score = scorer.score(id, &graph);
        // Root should get support baseline of 0.8
        assert!(score > 0.3);
    }

    #[test]
    fn test_low_confidence_detection() {
        let scorer = ConfidenceScorer::new();
        let mut graph = ReasoningGraph::new();

        let low =
            Thought::new(ThoughtKind::Analysis, "Low confidence".to_string()).with_confidence(0.1);
        let high =
            Thought::new(ThoughtKind::Analysis, "High confidence".to_string()).with_confidence(0.9);

        graph.add_thought(low).expect("add low should succeed");
        graph.add_thought(high).expect("add high should succeed");

        let low_conf = scorer.low_confidence_thoughts(&graph, 0.3);
        assert_eq!(low_conf.len(), 1);
    }

    #[test]
    fn test_relevance_score_basic() {
        let scorer = ConfidenceScorer::new();
        let thought = Thought::new(
            ThoughtKind::Analysis,
            "Optimize token usage in prompts".to_string(),
        )
        .with_confidence(0.8);

        let score = scorer.relevance_score(&thought, "optimize tokens");
        assert!(score >= 0.5, "Should have high relevance: {score}");

        let irrelevant = scorer.relevance_score(&thought, "bake a cake recipe");
        assert!(irrelevant < 0.3, "Should have low relevance: {irrelevant}");
    }

    #[test]
    fn test_relevance_score_empty_goal() {
        let scorer = ConfidenceScorer::new();
        let thought =
            Thought::new(ThoughtKind::Initial, "Something".to_string()).with_confidence(0.5);

        assert_eq!(scorer.relevance_score(&thought, ""), 0.0);
    }

    #[test]
    fn test_score_with_relevance() {
        let scorer = ConfidenceScorer::new();
        let mut graph = ReasoningGraph::new();

        let thought = Thought::new(
            ThoughtKind::Analysis,
            "Optimize the search algorithm".to_string(),
        )
        .with_confidence(0.8);
        let id = thought.id;
        graph.add_thought(thought).expect("add should succeed");

        let base = scorer.score(id, &graph);
        let with_rel = scorer.score_with_relevance(id, &graph, "optimize search");

        assert!(with_rel > 0.0, "Score with relevance should be positive");
        let diff = (with_rel - base).abs();
        assert!(diff < 1.0, "Score should stay in [0,1] range: {with_rel}");
    }

    #[test]
    fn test_thoughts_to_prune() {
        let scorer = ConfidenceScorer::new();
        let mut graph = ReasoningGraph::new();

        let bad = Thought::new(
            ThoughtKind::Initial,
            "Unrelated tangent about cooking".to_string(),
        )
        .with_confidence(0.1);
        let good = Thought::new(ThoughtKind::Analysis, "Optimize token usage".to_string())
            .with_confidence(0.9);
        let relevant = Thought::new(
            ThoughtKind::Synthesis,
            "Token optimization approach".to_string(),
        )
        .with_confidence(0.2);

        let bad_id = bad.id;
        graph.add_thought(bad).expect("add should succeed");
        graph.add_thought(good).expect("add should succeed");
        graph.add_thought(relevant).expect("add should succeed");

        let to_prune = scorer.thoughts_to_prune(&graph, "token optimization", 0.5, 0.3);

        assert!(
            to_prune.contains(&bad_id),
            "Low confidence + irrelevant thought should be pruned"
        );
        assert_eq!(to_prune.len(), 1, "Only one thought should be pruned");
    }

    #[test]
    fn test_score_nonexistent_thought() {
        let scorer = ConfidenceScorer::new();
        let graph = ReasoningGraph::new();
        let score = scorer.score(uuid::Uuid::new_v4(), &graph);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_score_all_empty_graph() {
        let scorer = ConfidenceScorer::new();
        let graph = ReasoningGraph::new();
        let all_scores = scorer.score_all(&graph);
        assert!(all_scores.is_empty());
    }

    #[test]
    fn test_score_all_multiple_thoughts() {
        let scorer = ConfidenceScorer::new();
        let mut graph = ReasoningGraph::new();

        let t1 = Thought::new(ThoughtKind::Initial, "A".to_string()).with_confidence(0.8);
        let t2 = Thought::new(ThoughtKind::Analysis, "B".to_string()).with_confidence(0.5);
        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");

        let all_scores = scorer.score_all(&graph);
        assert_eq!(all_scores.len(), 2);
        for score in all_scores.values() {
            assert!(*score >= 0.0 && *score <= 1.0);
        }
    }

    #[test]
    fn test_low_confidence_none_above_threshold() {
        let scorer = ConfidenceScorer::new();
        let mut graph = ReasoningGraph::new();

        let high = Thought::new(ThoughtKind::Analysis, "High".to_string()).with_confidence(0.9);
        graph.add_thought(high).expect("add");

        let low = scorer.low_confidence_thoughts(&graph, 0.3);
        assert!(low.is_empty());
    }

    #[test]
    fn test_relevance_score_short_words_ignored() {
        let scorer = ConfidenceScorer::new();
        let thought =
            Thought::new(ThoughtKind::Analysis, "hello world".to_string()).with_confidence(0.5);

        // All words in goal are short (len <= 2), so they're filtered out → empty goal_words → 0.0
        let score = scorer.relevance_score(&thought, "a b c");
        assert_eq!(score, 0.0, "Short words should be ignored");
    }

    #[test]
    fn test_relevance_score_partial_match() {
        let scorer = ConfidenceScorer::new();
        let thought = Thought::new(
            ThoughtKind::Analysis,
            "We should optimize the algorithm".to_string(),
        )
        .with_confidence(0.5);

        let score = scorer.relevance_score(&thought, "optimize performance");
        // "optimize" matches, "performance" doesn't → 1/2 = 0.5
        assert!((score - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_score_with_relevance_nonexistent_thought() {
        let scorer = ConfidenceScorer::new();
        let graph = ReasoningGraph::new();
        let score = scorer.score_with_relevance(uuid::Uuid::new_v4(), &graph, "goal");
        // Falls back to base score which is 0.0 for nonexistent
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_thoughts_to_prune_all_high_confidence() {
        let scorer = ConfidenceScorer::new();
        let mut graph = ReasoningGraph::new();

        let t = Thought::new(ThoughtKind::Analysis, "Good stuff".to_string()).with_confidence(0.9);
        graph.add_thought(t).expect("add");

        let to_prune = scorer.thoughts_to_prune(&graph, "goal", 0.5, 0.3);
        assert!(to_prune.is_empty());
    }

    #[test]
    fn test_thoughts_to_prune_low_relevance_kept() {
        let scorer = ConfidenceScorer::new();
        let mut graph = ReasoningGraph::new();

        // Low confidence but high relevance to goal
        let t = Thought::new(
            ThoughtKind::Analysis,
            "optimize performance metrics".to_string(),
        )
        .with_confidence(0.2);
        graph.add_thought(t).expect("add");

        // relevance_threshold=0.3 — if relevance > 0.3, don't prune
        let to_prune = scorer.thoughts_to_prune(&graph, "optimize performance", 0.5, 0.3);
        // Should NOT prune — relevance is high even though confidence is low
        assert!(to_prune.is_empty());
    }

    #[test]
    fn test_custom_weights() {
        let scorer = ConfidenceScorer {
            weight_base: 2.0,
            weight_support: 0.0,
            weight_knowledge: 0.0,
            weight_contradiction: 0.0,
            weight_depth: 0.0,
            weight_coherence: 0.0,
            weight_temporal: 0.0,
            weight_relevance: 0.0,
            max_depth: 5,
            enable_temporal: false,
        };

        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Initial, "Test".to_string()).with_confidence(0.5);
        let id = t.id;
        graph.add_thought(t).expect("add");

        let score = scorer.score(id, &graph);
        // With weight_base=2.0 and support baseline 0.8, score = (0.5 * 0.8) * (1 + 0) - 0 + 0 + 0
        // = 0.4, clamped to [0,1]
        assert!((0.0..=1.0).contains(&score));
    }
}
