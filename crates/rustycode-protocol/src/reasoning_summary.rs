//! Lightweight representation of an agent's ReasoningGraph for upward communication.
//! Used in AgentOutcome and ConvergenceView for cross-agent reasoning aggregation.

use serde::{Deserialize, Serialize};

/// Condensed summary of an agent's ReasoningGraph for parent/team consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningSummary {
    /// Number of thoughts in the original graph.
    pub thought_count: usize,
    /// Highest confidence score across all thoughts.
    pub max_confidence: f64,
    /// Mean confidence score across all thoughts.
    pub mean_confidence: f64,
    /// Top-N insights sorted by confidence (descending).
    pub top_insights: Vec<Insight>,
    /// Strategy that was primarily used.
    pub strategy_used: String,
    /// Whether the agent's reasoning converged (confidence stabilized).
    pub convergence_achieved: bool,
}

impl ReasoningSummary {
    /// Create an empty summary (no reasoning performed).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            thought_count: 0,
            max_confidence: 0.0,
            mean_confidence: 0.0,
            top_insights: Vec::new(),
            strategy_used: String::new(),
            convergence_achieved: false,
        }
    }

    /// Create a summary from pre-computed values.
    #[must_use]
    pub fn from_parts(
        thought_count: usize,
        max_confidence: f64,
        mean_confidence: f64,
        top_insights: Vec<Insight>,
        strategy_used: impl Into<String>,
        convergence_achieved: bool,
    ) -> Self {
        Self {
            thought_count,
            max_confidence,
            mean_confidence,
            top_insights,
            strategy_used: strategy_used.into(),
            convergence_achieved,
        }
    }

    /// Merge another summary into this one, combining insights.
    pub fn merge(&mut self, other: &ReasoningSummary) {
        self.thought_count += other.thought_count;
        self.max_confidence = self.max_confidence.max(other.max_confidence);
        // Weighted mean by thought count
        let total = self.thought_count;
        if total > 0 {
            self.mean_confidence = (self.mean_confidence * (total - other.thought_count) as f64
                + other.mean_confidence * other.thought_count as f64)
                / total as f64;
        }
        self.top_insights.extend(other.top_insights.iter().cloned());
        self.top_insights.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.top_insights.truncate(10); // Keep top 10
        self.convergence_achieved = self.convergence_achieved && other.convergence_achieved;
    }
}

/// A single high-confidence insight from an agent's reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    /// The insight content (condensed thought).
    pub content: String,
    /// Confidence score [0.0, 1.0].
    pub confidence: f64,
    /// Strategy that generated this insight.
    pub strategy: String,
    /// Reasoning depth (distance from root thought).
    pub depth: usize,
}

impl Insight {
    /// Create a new insight.
    #[must_use]
    pub fn new(
        content: impl Into<String>,
        confidence: f64,
        strategy: impl Into<String>,
        depth: usize,
    ) -> Self {
        Self {
            content: content.into(),
            confidence: confidence.clamp(0.0, 1.0),
            strategy: strategy.into(),
            depth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_summary() {
        let s = ReasoningSummary::empty();
        assert_eq!(s.thought_count, 0);
        assert_eq!(s.max_confidence, 0.0);
        assert!(s.top_insights.is_empty());
        assert!(!s.convergence_achieved);
    }

    #[test]
    fn from_parts_round_trip() {
        let insights = vec![Insight::new("Test insight", 0.9, "sequential", 1)];
        let s = ReasoningSummary::from_parts(5, 0.9, 0.7, insights.clone(), "dialectic", true);
        assert_eq!(s.thought_count, 5);
        assert_eq!(s.max_confidence, 0.9);
        assert_eq!(s.top_insights.len(), 1);
        assert_eq!(s.strategy_used, "dialectic");
    }

    #[test]
    fn merge_combines_insights() {
        let mut a = ReasoningSummary::from_parts(
            3,
            0.8,
            0.6,
            vec![Insight::new("A1", 0.8, "seq", 1)],
            "sequential",
            true,
        );
        let b = ReasoningSummary::from_parts(
            2,
            0.9,
            0.7,
            vec![Insight::new("B1", 0.9, "dialectic", 2)],
            "dialectic",
            false,
        );
        a.merge(&b);
        assert_eq!(a.thought_count, 5);
        assert_eq!(a.max_confidence, 0.9);
        assert_eq!(a.top_insights.len(), 2);
        // Top insight should be B1 (higher confidence)
        assert_eq!(a.top_insights[0].content, "B1");
        assert!(!a.convergence_achieved); // true && false = false
    }

    #[test]
    fn merge_truncates_to_top_10() {
        let mut a = ReasoningSummary::empty();
        let mut b = ReasoningSummary::empty();
        for i in 0..8 {
            a.top_insights
                .push(Insight::new(format!("A{i}"), 0.5 + i as f64 * 0.01, "s", 1));
        }
        for i in 0..8 {
            b.top_insights
                .push(Insight::new(format!("B{i}"), 0.5 + i as f64 * 0.02, "s", 1));
        }
        a.thought_count = 8;
        a.mean_confidence = 0.5;
        b.thought_count = 8;
        b.mean_confidence = 0.6;
        a.merge(&b);
        assert_eq!(a.top_insights.len(), 10);
        // Highest confidence should be first
        let max_conf = a.top_insights.first().unwrap().confidence;
        assert!(a
            .top_insights
            .iter()
            .all(|i| i.confidence <= max_conf + f64::EPSILON));
    }

    #[test]
    fn insight_confidence_clamped() {
        let i = Insight::new("test", 1.5, "s", 0);
        assert_eq!(i.confidence, 1.0);
        let i = Insight::new("test", -0.5, "s", 0);
        assert_eq!(i.confidence, 0.0);
    }

    #[test]
    fn serialization_round_trip() {
        let s = ReasoningSummary::from_parts(
            10,
            0.95,
            0.8,
            vec![Insight::new("key finding", 0.95, "abductive", 3)],
            "abductive",
            true,
        );
        let json = serde_json::to_string(&s).unwrap();
        let deserialized: ReasoningSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.thought_count, s.thought_count);
        assert_eq!(deserialized.top_insights.len(), s.top_insights.len());
    }
}
