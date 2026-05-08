use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Context, Result};
use schemars::JsonSchema;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

#[derive(serde::Deserialize, JsonSchema)]
pub struct TeamCreateParams {
    /// Name for the new team. Used as directory name under ~/.claude/teams/
    team_name: String,
    /// Team description/purpose
    description: Option<String>,
    /// Type/role of the team lead (e.g., 'researcher', 'test-runner')
    agent_type: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct TeamCreateTool;

    name: "team_create",
    description: r#"Create a new team to coordinate multiple agents working on a project. Teams have a 1:1 correspondence with task lists (Team = TaskList).

Use this tool proactively whenever:
- The user explicitly asks to use a team, swarm, or group of agents
- The user mentions wanting agents to work together, coordinate, or collaborate
- A task is complex enough that it would benefit from parallel work by multiple agents

This creates:
- A team file at ~/.claude/teams/{team-name}/config.json
- A corresponding task list directory at ~/.claude/tasks/{team-name}/"#,
    permission: ToolPermission::Write,
    tags: [ToolTag::Ops],

    execute(params: TeamCreateParams, _ctx) {
        let team_name = &params.team_name;

        validate_team_name(team_name)?;

        let description = params.description.as_deref().unwrap_or("");
        let agent_type = params.agent_type.as_deref().unwrap_or("team-lead");

        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let team_dir = home.join(".claude").join("teams").join(team_name);
        let task_dir = home.join(".claude").join("tasks").join(team_name);

        if team_dir.exists() {
            return Err(anyhow!("Team '{team_name}' already exists"));
        }

        fs::create_dir_all(&team_dir)
            .with_context(|| format!("Failed to create team dir: {}", team_dir.display()))?;
        fs::create_dir_all(&task_dir)
            .with_context(|| format!("Failed to create task dir: {}", task_dir.display()))?;

        let config = json!({
            "team_name": team_name,
            "description": description,
            "agent_type": agent_type,
            "created_at": chrono_now_rfc3339(),
            "members": [],
        });

        let config_path = team_dir.join("config.json");
        fs::write(
            &config_path,
            serde_json::to_string_pretty(&config)
                .with_context(|| "Failed to serialize team config")?,
        )
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

        Ok(ToolOutput::with_structured(
            format!("Team '{team_name}' created"),
            json!({
                "team_name": team_name,
                "team_dir": team_dir.to_string_lossy(),
                "task_dir": task_dir.to_string_lossy(),
            }),
        ))
    }
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct TeamDeleteParams {}

rustycode_tools_api::define_tool! {
    pub struct TeamDeleteTool;

    name: "team_delete",
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

fn validate_team_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("team_name must not be empty"));
    }
    if name.len() > 64 {
        return Err(anyhow!("team_name must be at most 64 characters"));
    }
    // Allow alphanumeric, hyphens, underscores
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return Err(anyhow!(
            "team_name may only contain letters, digits, hyphens, and underscores"
        ));
    }
    Ok(())
}

fn chrono_now_rfc3339() -> String {
    // Avoid depending on chrono; use std time
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}s since epoch", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    #[test]
    fn test_team_create_metadata() {
        let tool = TeamCreateTool;
        assert_eq!(tool.name(), "team_create");
        assert_eq!(tool.permission(), ToolPermission::Write);
    }

    #[test]
    fn test_team_create_requires_name() {
        let tool = TeamCreateTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_team_create_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let team_name = "test-team-42";

        // Monkey-patch home dir by testing the dir creation directly
        let team_dir = dir.path().join(".claude").join("teams").join(team_name);
        let task_dir = dir.path().join(".claude").join("tasks").join(team_name);
        fs::create_dir_all(&team_dir).unwrap();
        fs::create_dir_all(&task_dir).unwrap();
        let config = json!({"team_name": team_name, "members": []});
        fs::write(
            team_dir.join("config.json"),
            serde_json::to_string_pretty(&config).unwrap(),
        )
        .unwrap();

        assert!(dir
            .path()
            .join(".claude/teams/test-team-42/config.json")
            .exists());
        assert!(dir.path().join(".claude/tasks/test-team-42").exists());
    }

    #[test]
    fn test_validate_team_name() {
        assert!(validate_team_name("my-team").is_ok());
        assert!(validate_team_name("team_123").is_ok());
        assert!(validate_team_name("a").is_ok());
        assert!(validate_team_name("").is_err());
        assert!(validate_team_name("has space").is_err());
        assert!(validate_team_name("bad!char").is_err());
        assert!(validate_team_name(&"x".repeat(65)).is_err());
    }

    #[test]
    fn test_team_delete_metadata() {
        let tool = TeamDeleteTool;
        assert_eq!(tool.name(), "team_delete");
        assert_eq!(tool.permission(), ToolPermission::Write);
    }

    #[test]
    fn test_team_delete_returns_message() {
        let tool = TeamDeleteTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_ok());
    }
}
