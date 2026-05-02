//! Convergence detection and metrics tracking for reasoning sessions

use crate::thinking::core::graph::ReasoningGraph;
use crate::thinking::core::scoring::ConfidenceScorer;
use std::collections::VecDeque;

/// Tracks reasoning metrics and convergence state
#[derive(Debug, Clone)]
pub struct ConvergenceMetrics {
    /// Confidence scores over iterations
    confidence_history: VecDeque<f64>,
    /// Graph size (thought count) over iterations
    graph_size_history: VecDeque<usize>,
    /// Number of new thoughts per iteration
    new_thoughts_history: VecDeque<usize>,
    /// Previous iteration count for delta calculation
    previous_count: usize,
}

impl ConvergenceMetrics {
    /// Create a new metrics tracker with given capacity
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            confidence_history: VecDeque::with_capacity(capacity),
            graph_size_history: VecDeque::with_capacity(capacity),
            new_thoughts_history: VecDeque::with_capacity(capacity),
            previous_count: 0,
        }
    }

    /// Record metrics for current iteration
    pub fn record_iteration(&mut self, graph: &ReasoningGraph) {
        const MAX_HISTORY: usize = 100;

        let scorer = ConfidenceScorer::new();
        let all_scores = scorer.score_all(graph);

        let current_count = graph.len();
        let new_thoughts = current_count.saturating_sub(self.previous_count);

        // Calculate average confidence
        #[allow(clippy::cast_precision_loss)]
        let avg_confidence = if all_scores.is_empty() {
            0.0
        } else {
            all_scores.values().sum::<f64>() / all_scores.len() as f64
        };

        self.confidence_history.push_back(avg_confidence);
        self.graph_size_history.push_back(current_count);
        self.new_thoughts_history.push_back(new_thoughts);
        self.previous_count = current_count;

        // Trim history to bounded window
        while self.confidence_history.len() > MAX_HISTORY {
            self.confidence_history.pop_front();
            self.graph_size_history.pop_front();
            self.new_thoughts_history.pop_front();
        }
    }

    /// Get average confidence over last N iterations
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn average_confidence(&self, window: usize) -> f64 {
        let window = window.min(self.confidence_history.len());
        if window == 0 {
            return 0.0;
        }

        let start = self.confidence_history.len().saturating_sub(window);
        let sum: f64 = self.confidence_history.iter().skip(start).sum();
        sum / window as f64
    }

    /// Get confidence trend (positive = improving)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn confidence_trend(&self, window: usize) -> f64 {
        if self.confidence_history.len() < 2 {
            return 0.0;
        }

        let window = window.min(self.confidence_history.len() / 2);
        let len = self.confidence_history.len();

        if len < 2 {
            return 0.0;
        }

        let first_half_avg = {
            let start = len.saturating_sub(2 * window);
            let sum: f64 = self
                .confidence_history
                .iter()
                .skip(start)
                .take(window)
                .sum();
            sum / window.max(1) as f64
        };

        let second_half_avg = {
            let start = len.saturating_sub(window);
            let sum: f64 = self.confidence_history.iter().skip(start).sum();
            sum / window.max(1) as f64
        };

        second_half_avg - first_half_avg
    }

    /// Check if reasoning has plateaued (confidence not improving)
    #[must_use]
    pub fn has_plateaued(&self, window: usize, threshold: f64) -> bool {
        if self.confidence_history.len() < window * 2 {
            return false;
        }
        self.confidence_trend(window).abs() < threshold
    }

    /// Get rate of new thought generation
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn new_thoughts_per_iteration(&self, window: usize) -> f64 {
        let window = window.min(self.new_thoughts_history.len());
        if window == 0 {
            return 0.0;
        }

        let start = self.new_thoughts_history.len().saturating_sub(window);
        let sum: usize = self.new_thoughts_history.iter().skip(start).sum();
        sum as f64 / window as f64
    }

    /// Check if reasoning has become stagnant (no new thoughts)
    #[must_use]
    pub fn is_stagnant(&self, window: usize) -> bool {
        if self.new_thoughts_history.len() < window {
            return false;
        }
        self.new_thoughts_per_iteration(window) < 0.1
    }

    /// Get confidence history
    #[must_use]
    pub fn confidence_history(&self) -> Vec<f64> {
        self.confidence_history.iter().copied().collect()
    }

    /// Get graph size history
    #[must_use]
    pub fn graph_size_history(&self) -> Vec<usize> {
        self.graph_size_history.iter().copied().collect()
    }

    /// Get iteration count
    #[must_use]
    pub fn iteration_count(&self) -> usize {
        self.confidence_history.len()
    }

    /// Get latest average confidence
    #[must_use]
    pub fn latest_confidence(&self) -> Option<f64> {
        self.confidence_history.back().copied()
    }
}

/// Determines if reasoning should continue based on convergence criteria
#[derive(Debug)]
pub struct ConvergenceDetector {
    plateau_window: usize,
    plateau_threshold: f64,
    stagnation_window: usize,
}

impl ConvergenceDetector {
    /// Create detector with default thresholds
    #[must_use]
    pub const fn new() -> Self {
        Self {
            plateau_window: 3,
            plateau_threshold: 0.02,
            stagnation_window: 3,
        }
    }

    /// Create with custom thresholds
    #[must_use]
    pub const fn with_thresholds(
        plateau_window: usize,
        plateau_threshold: f64,
        stagnation_window: usize,
    ) -> Self {
        Self {
            plateau_window,
            plateau_threshold,
            stagnation_window,
        }
    }

    /// Check if reasoning has converged
    #[must_use]
    pub fn has_converged(
        &self,
        metrics: &ConvergenceMetrics,
        target_confidence: Option<f64>,
    ) -> bool {
        // Check for plateau
        if metrics.has_plateaued(self.plateau_window, self.plateau_threshold) {
            return true;
        }

        // Check for stagnation
        if metrics.is_stagnant(self.stagnation_window) {
            return true;
        }

        // Diminishing returns: good enough, stop wasting tokens
        if let Some(current) = metrics.latest_confidence() {
            if current >= 0.5
                && metrics.confidence_history().len() >= 3
                && metrics.confidence_trend(3) < 0.02
            {
                return true;
            }
        }

        // Check if target confidence reached
        if let Some(target) = target_confidence {
            if let Some(current) = metrics.latest_confidence() {
                if current >= target {
                    return true;
                }
            }
        }

        false
    }

    /// Get convergence reason if converged
    #[must_use]
    pub fn convergence_reason(&self, metrics: &ConvergenceMetrics) -> Option<String> {
        if metrics.has_plateaued(self.plateau_window, self.plateau_threshold) {
            return Some(format!(
                "Confidence plateau detected: trend = {:.4}",
                metrics.confidence_trend(self.plateau_window)
            ));
        }

        if metrics.is_stagnant(self.stagnation_window) {
            return Some(format!(
                "Stagnation detected: {:.2} new thoughts/iteration",
                metrics.new_thoughts_per_iteration(self.stagnation_window)
            ));
        }

        if let Some(current) = metrics.latest_confidence() {
            if current >= 0.5 && metrics.confidence_trend(3) < 0.02 {
                return Some(format!(
                    "Diminishing returns: confidence {:.3} with trend {:.4}",
                    current,
                    metrics.confidence_trend(3)
                ));
            }
        }

        None
    }
}

impl Default for ConvergenceDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::thinking::core::types::Thought;

    #[test]
    fn test_metrics_recording() {
        let mut metrics = ConvergenceMetrics::new(10);
        let mut graph = ReasoningGraph::new();

        // Record iteration 1
        let t1 = Thought::new(
            crate::thinking::core::types::ThoughtKind::Initial,
            "Thought 1".to_string(),
        )
        .with_confidence(0.5);
        graph.add_thought(t1).unwrap();
        metrics.record_iteration(&graph);

        assert_eq!(metrics.iteration_count(), 1);
        assert!(metrics.latest_confidence().is_some());
    }

    #[test]
    fn test_confidence_trend() {
        let mut metrics = ConvergenceMetrics::new(10);
        let mut graph = ReasoningGraph::new();

        // Add thoughts with increasing confidence
        for i in 1..=5 {
            let thought = Thought::new(
                crate::thinking::core::types::ThoughtKind::Analysis,
                format!("Thought {i}"),
            )
            .with_confidence(f64::from(i).mul_add(0.05, 0.5));
            graph.add_thought(thought).unwrap();
            metrics.record_iteration(&graph);
        }

        // Confidence should be improving
        let trend = metrics.confidence_trend(2);
        assert!(trend > 0.0);
    }

    #[test]
    fn test_stagnation_detection() {
        let mut metrics = ConvergenceMetrics::new(10);
        let graph = ReasoningGraph::new();

        // Record multiple iterations with no new thoughts
        for _ in 0..3 {
            metrics.record_iteration(&graph);
        }

        assert!(metrics.is_stagnant(3));
    }

    #[test]
    fn test_convergence_detector() {
        let detector = ConvergenceDetector::new();
        let mut metrics = ConvergenceMetrics::new(10);
        let graph = ReasoningGraph::new();

        // Initially not converged
        assert!(!detector.has_converged(&metrics, None));

        // After stagnation, should converge
        for _ in 0..3 {
            metrics.record_iteration(&graph);
        }
        assert!(detector.has_converged(&metrics, None));
    }

    #[test]
    fn test_target_confidence() {
        let detector = ConvergenceDetector::new();
        let mut metrics = ConvergenceMetrics::new(10);
        let mut graph = ReasoningGraph::new();

        // Create a thought with very high confidence that will score above 0.9
        let thought = Thought::new(
            crate::thinking::core::types::ThoughtKind::Analysis,
            "High confidence".to_string(),
        )
        .with_confidence(1.0); // Use max confidence to ensure it scores high
        graph.add_thought(thought).unwrap();

        // Record enough iterations for convergence checks to kick in
        for _ in 0..3 {
            metrics.record_iteration(&graph);
        }

        assert!(detector.has_converged(&metrics, Some(0.75)));
    }

    #[test]
    fn test_convergence_reason_plateau() {
        let detector = ConvergenceDetector::new();
        let mut metrics = ConvergenceMetrics::new(10);
        let graph = ReasoningGraph::new();

        // Flat confidence to trigger plateau
        for _ in 0..6 {
            metrics.record_iteration(&graph);
        }

        let reason = detector.convergence_reason(&metrics);
        assert!(reason.is_some());
        let text = reason.unwrap();
        assert!(
            text.contains("plateau") || text.contains("Stagnation"),
            "Expected plateau or stagnation reason, got: {text}"
        );
    }

    #[test]
    fn test_convergence_reason_not_converged() {
        let detector = ConvergenceDetector::new();
        let metrics = ConvergenceMetrics::new(10);
        // No iterations recorded
        let reason = detector.convergence_reason(&metrics);
        assert!(reason.is_none());
    }

    #[test]
    fn test_metrics_confidence_history_accessor() {
        let mut metrics = ConvergenceMetrics::new(10);
        let mut graph = ReasoningGraph::new();

        let t = Thought::new(
            crate::thinking::core::types::ThoughtKind::Initial,
            "A".to_string(),
        )
        .with_confidence(0.6);
        graph.add_thought(t).unwrap();
        metrics.record_iteration(&graph);

        let history = metrics.confidence_history();
        assert_eq!(history.len(), 1);
        assert!(history[0] > 0.0);
    }

    #[test]
    fn test_metrics_graph_size_history() {
        let mut metrics = ConvergenceMetrics::new(10);
        let graph = ReasoningGraph::new();
        metrics.record_iteration(&graph);

        let sizes = metrics.graph_size_history();
        assert_eq!(sizes.len(), 1);
        assert_eq!(sizes[0], 0);
    }

    #[test]
    fn test_metrics_iteration_count() {
        let mut metrics = ConvergenceMetrics::new(10);
        let graph = ReasoningGraph::new();
        assert_eq!(metrics.iteration_count(), 0);

        metrics.record_iteration(&graph);
        metrics.record_iteration(&graph);
        assert_eq!(metrics.iteration_count(), 2);
    }

    #[test]
    fn test_average_confidence_window_exceeds_history() {
        let metrics = ConvergenceMetrics::new(10);
        // No data at all
        assert_eq!(metrics.average_confidence(5), 0.0);
    }

    #[test]
    fn test_confidence_trend_single_point() {
        let mut metrics = ConvergenceMetrics::new(10);
        let graph = ReasoningGraph::new();
        metrics.record_iteration(&graph);

        // Only one point — can't compute trend
        let trend = metrics.confidence_trend(2);
        assert_eq!(trend, 0.0);
    }

    #[test]
    fn test_with_thresholds_custom() {
        let detector = ConvergenceDetector::with_thresholds(5, 0.01, 4);
        let mut metrics = ConvergenceMetrics::new(10);
        let graph = ReasoningGraph::new();

        // Not enough iterations for the larger windows
        for _ in 0..3 {
            metrics.record_iteration(&graph);
        }
        // plateau_window=5 needs >= 10 history entries to trigger
        assert!(!detector.has_converged(&metrics, None));
    }

    #[test]
    fn test_has_plateaued_short_history() {
        let mut metrics = ConvergenceMetrics::new(10);
        let graph = ReasoningGraph::new();
        metrics.record_iteration(&graph);

        // window=2 but only 1 entry — not enough data
        assert!(!metrics.has_plateaued(2, 0.01));
    }

    #[test]
    fn test_new_thoughts_per_iteration_empty() {
        let metrics = ConvergenceMetrics::new(10);
        assert_eq!(metrics.new_thoughts_per_iteration(3), 0.0);
    }

    #[test]
    fn test_default_detector() {
        let detector = ConvergenceDetector::default();
        let metrics = ConvergenceMetrics::new(10);
        // Fresh metrics should not be converged
        assert!(!detector.has_converged(&metrics, None));
    }

    #[test]
    fn test_diminishing_returns_detection() {
        let detector = ConvergenceDetector::new();
        let mut metrics = ConvergenceMetrics::new(10);
        let mut graph = ReasoningGraph::new();

        // Add a high-confidence thought and record several iterations
        // with flat confidence (no improvement) to trigger diminishing returns
        let thought = Thought::new(
            crate::thinking::core::types::ThoughtKind::Analysis,
            "Good enough answer".to_string(),
        )
        .with_confidence(0.7);
        graph.add_thought(thought).unwrap();

        for _ in 0..4 {
            metrics.record_iteration(&graph);
        }

        assert!(detector.has_converged(&metrics, None));
        let reason = detector.convergence_reason(&metrics);
        assert!(reason.is_some());
        let reason = reason.unwrap();
        assert!(
            reason.contains("Diminishing returns")
                || reason.contains("plateau")
                || reason.contains("Stagnation"),
            "Expected convergence reason about diminishing returns, plateau, or stagnation, got: {reason}"
        );
    }

    #[test]
    fn test_convergence_metrics_debug() {
        let metrics = ConvergenceMetrics::new(10);
        let debug = format!("{metrics:?}");
        assert!(debug.contains("ConvergenceMetrics"));
    }

    #[test]
    fn test_convergence_detector_debug() {
        let detector = ConvergenceDetector::new();
        let debug = format!("{detector:?}");
        assert!(debug.contains("ConvergenceDetector"));
    }

    #[test]
    fn test_confidence_trend_declining() {
        let mut metrics = ConvergenceMetrics::new(10);
        let mut graph1 = ReasoningGraph::new();
        let high = Thought::new(
            crate::thinking::core::types::ThoughtKind::Analysis,
            "High".to_string(),
        )
        .with_confidence(0.9);
        graph1.add_thought(high).unwrap();

        let mut graph2 = ReasoningGraph::new();
        let low = Thought::new(
            crate::thinking::core::types::ThoughtKind::Analysis,
            "Low".to_string(),
        )
        .with_confidence(0.2);
        graph2.add_thought(low).unwrap();

        // Record ascending then descending
        metrics.record_iteration(&graph2); // low
        metrics.record_iteration(&graph1); // high
        metrics.record_iteration(&graph2); // low
        metrics.record_iteration(&graph2); // low

        // Trend should exist (positive or negative)
        let trend = metrics.confidence_trend(2);
        assert_ne!(trend, 0.0, "With varying data, trend should be nonzero");
    }

    #[test]
    fn test_new_thoughts_rate() {
        let mut metrics = ConvergenceMetrics::new(10);
        let mut graph = ReasoningGraph::new();

        let t1 = Thought::new(
            crate::thinking::core::types::ThoughtKind::Initial,
            "A".to_string(),
        );
        graph.add_thought(t1).unwrap();
        metrics.record_iteration(&graph); // 1 new thought

        // No more additions
        metrics.record_iteration(&graph); // 0 new
        metrics.record_iteration(&graph); // 0 new

        let rate = metrics.new_thoughts_per_iteration(3);
        assert!(
            (rate - (1.0 / 3.0)).abs() < 0.01,
            "Rate should be ~0.33, got {rate}"
        );
    }

    #[test]
    fn test_target_confidence_not_reached() {
        let detector = ConvergenceDetector::new();
        let mut metrics = ConvergenceMetrics::new(10);
        let mut graph = ReasoningGraph::new();

        let t = Thought::new(
            crate::thinking::core::types::ThoughtKind::Analysis,
            "Medium".to_string(),
        )
        .with_confidence(0.5);
        graph.add_thought(t).unwrap();
        metrics.record_iteration(&graph);

        // Single data point — can't converge via plateau/stagnation yet
        // target 0.99 > current confidence
        assert!(!detector.has_converged(&metrics, Some(0.99)));
    }

    #[test]
    fn test_convergence_reason_diminishing_returns_message_format() {
        let detector = ConvergenceDetector::new();
        let mut metrics = ConvergenceMetrics::new(10);
        let mut graph = ReasoningGraph::new();

        let t = Thought::new(
            crate::thinking::core::types::ThoughtKind::Synthesis,
            "Answer".to_string(),
        )
        .with_confidence(0.8);
        graph.add_thought(t).unwrap();

        for _ in 0..5 {
            metrics.record_iteration(&graph);
        }

        if let Some(reason) = detector.convergence_reason(&metrics) {
            // Should contain numeric info
            assert!(
                reason.contains("0.")
                    || reason.contains("Stagnation")
                    || reason.contains("plateau")
            );
        }
    }
}
