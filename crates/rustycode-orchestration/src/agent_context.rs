use rustycode_protocol::{
    agent_protocol::{AgentRole, FileSnippet},
    cost_budget::CostBudget,
    reasoning_summary::ReasoningSummary,
    tool_scope::ToolScope,
    Message,
};

/// Inbound context for an agent execution. Constructed by the orchestration layer
/// and passed to `AgentSession` at call time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentContext {
    /// Unique task identifier for tracing.
    pub task_id: String,
    /// Session this agent belongs to.
    pub session_id: String,
    /// The role this agent plays (Architect, Builder, Skeptic, etc.).
    pub agent_role: AgentRole,
    /// Tools this agent is permitted to use.
    pub tool_scope: ToolScope,
    /// Token and cost budget for this execution.
    pub budget: CostBudget,
    /// Prior conversation history to carry forward.
    pub conversation_history: Vec<Message>,
    /// File snippets relevant to the current task.
    pub files_in_scope: Vec<FileSnippet>,
    /// Reasoning summary from the parent agent (if this is a sub-agent).
    pub reasoning_from_parent: Option<ReasoningSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_with_defaults() {
        let ctx = AgentContext {
            task_id: "task_1".into(),
            session_id: "sess_1".into(),
            agent_role: AgentRole::Builder,
            tool_scope: ToolScope::full(),
            budget: CostBudget::new(100_000, 1.0),
            conversation_history: vec![],
            files_in_scope: vec![],
            reasoning_from_parent: None,
        };
        assert_eq!(ctx.task_id, "task_1");
        assert!(ctx.tool_scope.is_allowed("bash"));
        assert!(!ctx.budget.is_exhausted());
    }

    #[test]
    fn serialization_round_trip() {
        let ctx = AgentContext {
            task_id: "task_2".into(),
            session_id: "sess_2".into(),
            agent_role: AgentRole::Skeptic,
            tool_scope: ToolScope::allow_only(["read_file".to_string()]),
            budget: CostBudget::new(50_000, 0.5),
            conversation_history: vec![],
            files_in_scope: vec![FileSnippet {
                path: "src/main.rs".into(),
                content: "fn main() {}".into(),
                line_range: Some((1, 3)),
            }],
            reasoning_from_parent: None,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let deserialized: AgentContext = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task_id, ctx.task_id);
        assert_eq!(deserialized.agent_role, ctx.agent_role);
        assert_eq!(deserialized.files_in_scope.len(), 1);
    }
}
