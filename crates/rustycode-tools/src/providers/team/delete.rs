use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};

rustycode_tools_api::define_tool! {
    pub struct TeamDeleteTool;

    name: "TeamDelete",
    description: r#"Remove team and task directories when the swarm work is complete.

This operation:
- Removes the team directory (~/.claude/teams/{team-name}/)
- Removes the task directory (~/.claude/tasks/{team-name}/)
- Clears team context from the current session

IMPORTANT: TeamDelete will fail if the team still has active members. Gracefully terminate teammates first, then call TeamDelete after all teammates have shut down."#,
    permission: ToolPermission::Write,
    tags: [ToolTag::Ops],

    execute(_params: TeamDeleteParams, _ctx) {
        // In production, the team name comes from session context.
        // For now, return a message indicating the operation needs context.
        Ok(ToolOutput::text(
            "Team delete requires active team context from the session",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;
    use serde_json::json;

    #[test]
    fn test_team_delete_metadata() {
        let tool = TeamDeleteTool;
        assert_eq!(tool.name(), "TeamDelete");
        assert_eq!(tool.permission(), ToolPermission::Write);
    }

    #[test]
    fn test_team_delete_returns_message() {
        let tool = TeamDeleteTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_ok());
    }
}
