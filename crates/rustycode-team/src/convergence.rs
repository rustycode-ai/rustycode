use rustycode_protocol::reasoning_summary::Insight;

/// Aggregated view across multiple teams working on related sub-tasks.
/// Used by the ensemble layer to assess overall convergence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConvergenceView {
    /// Number of teams contributing to this view.
    pub team_count: usize,
    /// Highest confidence reported by any team.
    pub max_confidence: f64,
    /// Mean confidence across all teams.
    pub mean_confidence: f64,
    /// Top insights aggregated across teams (deduplicated, ranked).
    pub top_insights: Vec<Insight>,
    /// Opinions from teams that disagree with the majority.
    pub dissenting_opinions: Vec<DissentingOpinion>,
    /// Whether all teams converged on a consistent answer.
    pub convergence_achieved: bool,
}

impl ConvergenceView {
    /// Empty convergence view for zero-team cases.
    pub fn empty() -> Self {
        Self {
            team_count: 0,
            max_confidence: 0.0,
            mean_confidence: 0.0,
            top_insights: vec![],
            dissenting_opinions: vec![],
            convergence_achieved: false,
        }
    }
}

/// An opinion from a team that disagrees with the majority consensus.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DissentingOpinion {
    /// The agent that holds this opinion.
    pub agent_id: String,
    /// The team this agent belongs to.
    pub team_id: String,
    /// Description of the disagreement.
    pub opinion: String,
    /// How confident the dissenting agent is (0.0–1.0).
    pub confidence: f64,
    /// Evidence supporting the dissenting opinion.
    pub evidence: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_view() {
        let view = ConvergenceView::empty();
        assert_eq!(view.team_count, 0);
        assert!(!view.convergence_achieved);
        assert!(view.top_insights.is_empty());
    }

    #[test]
    fn convergence_construction() {
        let view = ConvergenceView {
            team_count: 3,
            max_confidence: 0.95,
            mean_confidence: 0.87,
            top_insights: vec![Insight::new("Use hashmap", 0.9, "sequential", 0)],
            dissenting_opinions: vec![DissentingOpinion {
                agent_id: "agent_3".into(),
                team_id: "team_2".into(),
                opinion: "Prefer B-tree for sorted access".into(),
                confidence: 0.7,
                evidence: vec!["Range queries are common".into()],
            }],
            convergence_achieved: false,
        };
        assert_eq!(view.team_count, 3);
        assert!(!view.convergence_achieved);
        assert_eq!(view.dissenting_opinions.len(), 1);
    }

    #[test]
    fn serialization_round_trip() {
        let view = ConvergenceView {
            team_count: 2,
            max_confidence: 0.8,
            mean_confidence: 0.75,
            top_insights: vec![],
            dissenting_opinions: vec![],
            convergence_achieved: true,
        };
        let json = serde_json::to_string(&view).unwrap();
        let deserialized: ConvergenceView = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.team_count, view.team_count);
        assert_eq!(deserialized.mean_confidence, view.mean_confidence);
    }
}
