use rustycode_agent_runtime::{AgentResult, StoppedReason};
use rustycode_protocol::{reasoning_summary::ReasoningSummary, token_usage::TokenUsage};

pub use rustycode_protocol::agent_outcome::AgentOutcome;

/// Convert an [`AgentResult`] into an [`AgentOutcome`].
///
/// This is a free function because `AgentOutcome` is defined in `rustycode-protocol`
/// and `StoppedReason` is defined in `rustycode-agent-runtime` — both are available
/// here in orchestration which sits above both.
pub fn agent_outcome_from_result(
    result: &AgentResult,
    agent_id: impl Into<String>,
    task_id: impl Into<String>,
) -> AgentOutcome {
    let success = !matches!(
        result.stopped_reason,
        StoppedReason::MaxTurnsReached | StoppedReason::TimeoutExceeded
    );
    let mut usage = TokenUsage::zero();
    usage.input_tokens = result.total_input_tokens;
    usage.output_tokens = result.total_output_tokens;
    usage.cache_read_tokens = result.total_cache_read_tokens;
    usage.cache_creation_tokens = result.total_cache_creation_tokens;

    AgentOutcome {
        agent_id: agent_id.into(),
        task_id: task_id.into(),
        success,
        output_text: result.final_text.clone(),
        files_changed: vec![],
        usage,
        reasoning_summary: ReasoningSummary::empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_outcome() {
        let outcome = AgentOutcome::failed("agent_1", "task_1", "timeout exceeded");
        assert!(!outcome.success);
        assert_eq!(outcome.output_text, "timeout exceeded");
        assert!(outcome.files_changed.is_empty());
        assert_eq!(outcome.usage.total(), 0);
    }

    #[test]
    fn serialization_round_trip() {
        use rustycode_protocol::agent_protocol::FileChange;

        let outcome = AgentOutcome {
            agent_id: "agent_2".into(),
            task_id: "task_2".into(),
            success: true,
            output_text: "Done".into(),
            files_changed: vec![FileChange {
                path: "src/lib.rs".into(),
                summary: "Added new function".into(),
                diff_hunk: "+fn new() {}".into(),
                lines_added: 1,
                lines_removed: 0,
            }],
            usage: TokenUsage::zero(),
            reasoning_summary: ReasoningSummary::empty(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let deserialized: AgentOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent_id, outcome.agent_id);
        assert!(deserialized.success);
        assert_eq!(deserialized.files_changed.len(), 1);
    }

    #[test]
    fn from_agent_result_success() {
        let result = AgentResult {
            final_text: "All done".into(),
            messages: vec![],
            stopped_reason: StoppedReason::NoToolCalls,
            total_input_tokens: 500,
            total_output_tokens: 100,
            total_cache_read_tokens: 200,
            total_cache_creation_tokens: 50,
        };
        let outcome = agent_outcome_from_result(&result, "agent_1", "task_1");
        assert!(outcome.success);
        assert_eq!(outcome.output_text, "All done");
        assert_eq!(outcome.usage.input_tokens, 500);
        assert_eq!(outcome.usage.output_tokens, 100);
    }

    #[test]
    fn from_agent_result_failure() {
        let result = AgentResult {
            final_text: String::new(),
            messages: vec![],
            stopped_reason: StoppedReason::MaxTurnsReached,
            total_input_tokens: 1000,
            total_output_tokens: 200,
            total_cache_read_tokens: 0,
            total_cache_creation_tokens: 0,
        };
        let outcome = agent_outcome_from_result(&result, "agent_1", "task_1");
        assert!(!outcome.success);
        assert_eq!(outcome.usage.total(), 1200);
    }
}
