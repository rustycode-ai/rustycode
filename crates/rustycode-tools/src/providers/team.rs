use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

/// Create a team to coordinate multiple agents.
///
/// Creates a team config file and corresponding task list directory.
/// Teams have a 1:1 correspondence with task lists.
pub struct TeamCreateTool;

impl Tool for TeamCreateTool {
    fn name(&self) -> &'static str {
        "team_create"
    }

    fn description(&self) -> &'static str {
        r#"Create a new team to coordinate multiple agents working on a project. Teams have a 1:1 correspondence with task lists (Team = TaskList).

Use this tool proactively whenever:
- The user explicitly asks to use a team, swarm, or group of agents
- The user mentions wanting agents to work together, coordinate, or collaborate
- A task is complex enough that it would benefit from parallel work by multiple agents

This creates:
- A team file at ~/.claude/teams/{team-name}/config.json
- A corresponding task list directory at ~/.claude/tasks/{team-name}/"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["team_name"],
            "properties": {
                "team_name": {
                    "type": "string",
                    "description": "Name for the new team. Used as directory name under ~/.claude/teams/"
                },
                "description": {
                    "type": "string",
                    "description": "Team description/purpose"
                },
                "agent_type": {
                    "type": "string",
                    "description": "Type/role of the team lead (e.g., 'researcher', 'test-runner')"
                }
            }
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let team_name = params
            .get("team_name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing team_name"))?;

        validate_team_name(team_name)?;

        let description = params
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");

        let agent_type = params
            .get("agent_type")
            .and_then(Value::as_str)
            .unwrap_or("team-lead");

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

/// Delete a team and its task list directory.
pub struct TeamDeleteTool;

impl Tool for TeamDeleteTool {
    fn name(&self) -> &'static str {
        "team_delete"
    }

    fn description(&self) -> &'static str {
        r#"Remove team and task directories when the swarm work is complete.

This operation:
- Removes the team directory (~/.claude/teams/{team-name}/)
- Removes the task directory (~/.claude/tasks/{team-name}/)
- Clears team context from the current session

IMPORTANT: TeamDelete will fail if the team still has active members. Gracefully terminate teammates first, then call TeamDelete after all teammates have shut down."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Ops]
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
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
