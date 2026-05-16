use rustycode_protocol::AgentOutcome;
use rustycode_protocol::{agent_protocol::FileChange, token_usage::TokenUsage};

use crate::convergence::ConvergenceView;

/// Aggregated context from a completed team execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TeamContext {
    /// Unique team identifier.
    pub team_id: String,
    /// Task this team worked on.
    pub task_id: String,
    /// Outcomes from each agent in the team.
    pub agent_outcomes: Vec<AgentOutcome>,
    /// Aggregated convergence view across agents.
    pub convergence: ConvergenceView,
    /// Combined file changes from all agents (deduplicated).
    pub combined_changes: Vec<FileChange>,
    /// Total token usage across all agents.
    pub total_usage: TokenUsage,
}

impl TeamContext {
    /// Aggregate total usage from all agent outcomes.
    pub fn aggregate_usage(&self) -> TokenUsage {
        self.agent_outcomes
            .iter()
            .map(|o| o.usage)
            .fold(TokenUsage::zero(), |acc, u| acc.saturating_add(u))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::reasoning_summary::ReasoningSummary;
    use rustycode_protocol::AgentOutcome;

    #[test]
    fn empty_team_context() {
        let ctx = TeamContext {
            team_id: "team_1".into(),
            task_id: "task_1".into(),
            agent_outcomes: vec![],
            convergence: ConvergenceView::empty(),
            combined_changes: vec![],
            total_usage: TokenUsage::zero(),
        };
        assert_eq!(ctx.agent_outcomes.len(), 0);
        assert_eq!(ctx.aggregate_usage().total(), 0);
    }

    #[test]
    fn usage_aggregation() {
        let mut usage1 = TokenUsage::zero();
        usage1.input_tokens = 100;
        usage1.output_tokens = 50;
        let mut usage2 = TokenUsage::zero();
        usage2.input_tokens = 200;
        usage2.output_tokens = 75;

        let ctx = TeamContext {
            team_id: "team_2".into(),
            task_id: "task_2".into(),
            agent_outcomes: vec![
                AgentOutcome {
                    agent_id: "a1".into(),
                    task_id: "task_2".into(),
                    success: true,
                    output_text: String::new(),
                    files_changed: vec![],
                    usage: usage1,
                    reasoning_summary: ReasoningSummary::empty(),
                },
                AgentOutcome {
                    agent_id: "a2".into(),
                    task_id: "task_2".into(),
                    success: true,
                    output_text: String::new(),
                    files_changed: vec![],
                    usage: usage2,
                    reasoning_summary: ReasoningSummary::empty(),
                },
            ],
            convergence: ConvergenceView::empty(),
            combined_changes: vec![],
            total_usage: TokenUsage::zero(),
        };
        let agg = ctx.aggregate_usage();
        assert_eq!(agg.input_tokens, 300);
        assert_eq!(agg.output_tokens, 125);
    }

    #[test]
    fn serialization_round_trip() {
        let ctx = TeamContext {
            team_id: "team_3".into(),
            task_id: "task_3".into(),
            agent_outcomes: vec![],
            convergence: ConvergenceView::empty(),
            combined_changes: vec![],
            total_usage: TokenUsage::zero(),
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let deserialized: TeamContext = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.team_id, ctx.team_id);
    }
}
