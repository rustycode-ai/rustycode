//! Metacognitive monitoring: Self-awareness about reasoning quality

use crate::thinking::core::graph::ReasoningGraph;
use crate::thinking::core::scoring::ConfidenceScorer;

/// Indicators of reasoning health
#[derive(Debug, Clone)]
pub struct ReasoningMetrics {
    /// Average confidence across all thoughts
    pub avg_confidence: f64,
    /// Trend in confidence (positive = improving, negative = declining)
    pub confidence_trend: f64,
    /// Whether reasoning appears stagnant (no progress)
    pub is_stagnant: bool,
    /// Number of contradictions detected
    pub contradiction_count: usize,
    pub declining_confidence: bool,
    /// Suggested strategy to try next
    pub suggested_strategy: Option<String>,
}

/// Monitors the metacognitive state of reasoning
pub struct MetacognitiveMonitor {
    scorer: ConfidenceScorer,
    history: Vec<f64>, // Historical average confidences
    max_history: usize,
    stagnation_threshold: usize, // How many iterations without improvement
}

impl Default for MetacognitiveMonitor {
    fn default() -> Self {
        Self::new(ConfidenceScorer::new())
    }
}

impl MetacognitiveMonitor {
    #[must_use]
    pub const fn new(scorer: ConfidenceScorer) -> Self {
        Self {
            scorer,
            history: Vec::new(),
            max_history: 10,
            stagnation_threshold: 3,
        }
    }

    /// Analyze the current reasoning state
    #[allow(clippy::cast_precision_loss)]
    pub fn analyze(&mut self, graph: &ReasoningGraph) -> ReasoningMetrics {
        let scores = self.scorer.score_all(graph);

        // Calculate average confidence
        let avg_confidence = if scores.is_empty() {
            0.0
        } else {
            scores.values().sum::<f64>() / scores.len() as f64
        };

        // Track history and detect trend
        self.history.push(avg_confidence);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }

        let confidence_trend = self.calculate_trend();
        let is_stagnant = self.detect_stagnation();
        let declining_confidence = confidence_trend < -0.05;
        let contradiction_count = Self::count_contradictions(graph);

        let suggested_strategy = if is_stagnant {
            Some(Self::suggest_strategy_for_stagnation())
        } else if declining_confidence {
            Some(Self::suggest_strategy_for_decline())
        } else {
            None
        };

        ReasoningMetrics {
            avg_confidence,
            confidence_trend,
            is_stagnant,
            contradiction_count,
            declining_confidence,
            suggested_strategy,
        }
    }

    /// Calculate confidence trend from history
    #[allow(clippy::cast_precision_loss)]
    fn calculate_trend(&self) -> f64 {
        if self.history.len() < 2 {
            return 0.0;
        }

        let recent_avg = self.history[self.history.len() / 2..].iter().sum::<f64>()
            / (self.history.len() / 2) as f64;
        let older_avg = self.history[..self.history.len() / 2].iter().sum::<f64>()
            / (self.history.len() / 2) as f64;

        recent_avg - older_avg
    }

    /// Detect if reasoning has stagnated
    fn detect_stagnation(&self) -> bool {
        if self.history.len() < self.stagnation_threshold {
            return false;
        }

        let recent = &self.history[self.history.len() - self.stagnation_threshold..];
        let variance: f64 = recent
            .iter()
            .zip(recent.iter().skip(1))
            .map(|(a, b)| (a - b).abs())
            .sum();

        variance < 0.01 // Very small changes in confidence
    }

    /// Count contradictions in the graph
    fn count_contradictions(graph: &ReasoningGraph) -> usize {
        graph
            .edges()
            .iter()
            .filter(|e| {
                // Check if edge kind indicates contradiction
                use crate::thinking::core::types::EdgeKind;
                matches!(e.kind, EdgeKind::Contradicts)
            })
            .count()
    }

    /// Suggest strategy for stagnant reasoning
    fn suggest_strategy_for_stagnation() -> String {
        // Try different strategies when stuck
        "Try a different strategy (Dialectic for contradictions, Parallel for multi-aspect)"
            .to_string()
    }

    /// Suggest strategy for declining confidence
    fn suggest_strategy_for_decline() -> String {
        "Confidence declining - try Abductive strategy for root-cause analysis".to_string()
    }

    /// Reset monitoring state
    pub fn reset(&mut self) {
        self.history.clear();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::thinking::core::types::{Thought, ThoughtKind};

    #[test]
    fn test_average_confidence_calculation() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);
        let mut graph = ReasoningGraph::new();

        let t1 = Thought::new(ThoughtKind::Initial, "A".to_string()).with_confidence(0.8);
        let t2 = Thought::new(ThoughtKind::Analysis, "B".to_string()).with_confidence(0.6);

        graph.add_thought(t1).expect("add t1 should succeed");
        graph.add_thought(t2).expect("add t2 should succeed");

        let metrics = monitor.analyze(&graph);
        assert!(
            metrics.avg_confidence > 0.0,
            "Average confidence should be non-zero"
        );
    }

    #[test]
    fn test_stagnation_detection() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        // Simulate flat confidence trend (must be >= stagnation_threshold = 3)
        monitor.history = vec![0.5, 0.50001, 0.5];

        // Call detect_stagnation directly
        let is_stagnant = monitor.detect_stagnation();
        assert!(
            is_stagnant,
            "Nearly flat history should be detected as stagnant"
        );
    }

    #[test]
    fn test_trend_calculation() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        // Simulate improving trend
        monitor.history = vec![0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
        let trend = monitor.calculate_trend();
        assert!(trend > 0.1, "Positive trend should be detected");
    }

    #[test]
    fn test_empty_graph_analysis() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);
        let graph = ReasoningGraph::new();

        let metrics = monitor.analyze(&graph);
        assert_eq!(metrics.avg_confidence, 0.0);
        assert_eq!(metrics.contradiction_count, 0);
        assert!(!metrics.is_stagnant);
    }

    #[test]
    fn test_contradiction_counting() {
        use crate::thinking::core::types::EdgeKind;

        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);
        let mut graph = ReasoningGraph::new();

        let t1 = Thought::new(ThoughtKind::Initial, "A".into());
        let t2 = Thought::new(ThoughtKind::Critique, "B".into());
        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");

        // Add a contradiction edge
        let ids: Vec<_> = graph.thoughts().map(|t| t.id).collect();
        graph
            .add_edge(ids[0], ids[1], EdgeKind::Contradicts)
            .expect("add edge");

        let metrics = monitor.analyze(&graph);
        assert_eq!(metrics.contradiction_count, 1);
    }

    #[test]
    fn test_declining_confidence_suggests_abductive() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        // Simulate declining confidence with enough history to not be stagnant
        monitor.history = vec![0.8, 0.7, 0.6, 0.5, 0.3, 0.1];

        let graph = ReasoningGraph::new();
        let metrics = monitor.analyze(&graph);
        assert!(metrics.declining_confidence);
        assert!(metrics.suggested_strategy.is_some());
        let strategy = metrics.suggested_strategy.expect("should have suggestion");
        assert!(
            strategy.contains("Abductive"),
            "Expected Abductive suggestion, got: {strategy}"
        );
    }

    #[test]
    fn test_reset_clears_history() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        let t = Thought::new(ThoughtKind::Initial, "A".into());
        let mut g = ReasoningGraph::new();
        g.add_thought(t).expect("add");
        monitor.analyze(&g);
        assert!(!monitor.history.is_empty());

        monitor.reset();
        assert!(monitor.history.is_empty());
    }

    #[test]
    fn test_default_monitor() {
        let mut monitor = MetacognitiveMonitor::default();
        let graph = ReasoningGraph::new();
        let metrics = monitor.analyze(&graph);
        assert_eq!(metrics.avg_confidence, 0.0);
    }

    #[test]
    fn test_reasoning_metrics_debug() {
        let metrics = ReasoningMetrics {
            avg_confidence: 0.5,
            confidence_trend: 0.1,
            is_stagnant: false,
            contradiction_count: 2,
            declining_confidence: false,
            suggested_strategy: Some("test".to_string()),
        };
        let debug = format!("{metrics:?}");
        assert!(debug.contains("0.5"));
        assert!(debug.contains("stagnant"));
    }

    #[test]
    fn test_trend_single_point_returns_zero() {
        let monitor = MetacognitiveMonitor::new(ConfidenceScorer::new());
        // history has 0 elements
        assert_eq!(monitor.calculate_trend(), 0.0);

        let mut monitor = MetacognitiveMonitor::new(ConfidenceScorer::new());
        monitor.history.push(0.5);
        // history has 1 element — not enough for trend
        assert_eq!(monitor.calculate_trend(), 0.0);
    }

    #[test]
    fn test_history_trimming_at_max() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        let graph = ReasoningGraph::new();
        // max_history is 10, add 12 entries
        for _ in 0..12 {
            monitor.analyze(&graph);
        }

        // History should be trimmed to max_history
        assert!(
            monitor.history.len() <= 10,
            "History should be capped at max_history=10, got {}",
            monitor.history.len()
        );
    }

    #[test]
    fn test_suggest_strategy_stagnation_content() {
        // Test the suggestion functions directly since analyze() modifies history
        let stagnation_suggestion = MetacognitiveMonitor::suggest_strategy_for_stagnation();
        assert!(
            stagnation_suggestion.contains("Dialectic")
                || stagnation_suggestion.contains("Parallel"),
            "Stagnation strategy should suggest alternatives, got: {stagnation_suggestion}"
        );
    }

    #[test]
    fn test_suggest_strategy_decline_content() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        // Declining trend but not stagnant (enough variance)
        monitor.history = vec![0.9, 0.85, 0.8, 0.75, 0.5, 0.2];

        let graph = ReasoningGraph::new();
        let metrics = monitor.analyze(&graph);

        assert!(metrics.declining_confidence);
        let strategy = metrics
            .suggested_strategy
            .expect("decline should suggest strategy");
        assert!(
            strategy.contains("Abductive"),
            "Decline strategy should suggest Abductive, got: {strategy}"
        );
    }

    #[test]
    fn test_no_suggestion_when_healthy() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Analysis, "good".into()).with_confidence(0.8);
        graph.add_thought(t).expect("add");

        // Single analysis — no stagnation, no decline
        let metrics = monitor.analyze(&graph);
        assert!(!metrics.is_stagnant);
        assert!(!metrics.declining_confidence);
        assert!(metrics.suggested_strategy.is_none());
    }

    #[test]
    fn test_reset_clears_and_allows_fresh_analysis() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        let mut graph = ReasoningGraph::new();
        graph
            .add_thought(Thought::new(ThoughtKind::Initial, "A".into()))
            .expect("add");
        monitor.analyze(&graph);
        monitor.analyze(&graph);
        assert_eq!(monitor.history.len(), 2);

        monitor.reset();
        assert!(monitor.history.is_empty());

        // Can analyze again after reset
        let metrics = monitor.analyze(&graph);
        assert!(metrics.avg_confidence > 0.0);
        assert_eq!(monitor.history.len(), 1);
    }
}
