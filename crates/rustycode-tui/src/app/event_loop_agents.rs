//! Agent-related functionality for the event loop
//!
//! Extracted from event_loop.rs to separate agent management logic.

use crate::agents::{AgentStatus, AgentTask};
use anyhow::Result;
use rustycode_runtime::multi_agent::{AgentRole, AgentRoleExt};

/// Parse an agent role from string
pub fn parse_agent_role(role_str: &str) -> Result<AgentRole, String> {
    match role_str.to_lowercase().as_str() {
        "factual" | "senior" | "test" => Ok(AgentRole::Reviewer),
        "security" => Ok(AgentRole::Skeptic),
        "consistency" | "redundancy" => Ok(AgentRole::Judge),
        "performance" => Ok(AgentRole::Researcher),
        "docs" | "doc" => Ok(AgentRole::Worker),
        _ => Err(format!(
            "Unknown role: {}. Available: factual, senior, security, consistency, redundancy, performance, test, docs",
            role_str
        )),
    }
}

/// Format agent information for display
pub fn format_agent_list(agents: &[AgentTask]) -> String {
    if agents.is_empty() {
        return "No agents running".to_string();
    }

    let mut output = String::from("Active Agents:\n\n");

    for agent in agents {
        output.push_str(&format!(
            "{} [{}] {} - {}\n",
            agent.status_icon(),
            agent.id,
            agent.role.name(),
            agent.task
        ));

        if agent.status == AgentStatus::Running {
            output.push_str(&format!("   Running for {}\n", agent.formatted_time()));
        } else if agent.status == AgentStatus::Completed {
            if let Some(result) = &agent.result {
                output.push_str("[OK] Completed\n");
                if !result.issues.is_empty() {
                    output.push_str(&format!("   Issues: {}\n", result.issues.len()));
                }
                if !result.suggestions.is_empty() {
                    output.push_str(&format!("   Suggestions: {}\n", result.suggestions.len()));
                }
            }
        } else if agent.status == AgentStatus::Failed {
            if let Some(error) = &agent.error {
                output.push_str(&format!("   [X] Error: {}\n", error));
            }
        }
    }

    output
}

/// Get agent command help text
pub fn agent_command_help() -> &'static str {
    "Agent commands:\n\
     • /agent list - Show all agents\n\
     • /agent spawn <role> <task> - Spawn a new agent\n\
     • /agent cancel <id> - Cancel a running agent\n\
     \n\
     Available roles: factual, senior, security, consistency, redundancy, performance, test, docs"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_agent_roles() {
        assert!(matches!(
            parse_agent_role("factual").unwrap(),
            AgentRole::Reviewer
        ));
        assert!(matches!(
            parse_agent_role("senior").unwrap(),
            AgentRole::Reviewer
        ));
        assert!(matches!(
            parse_agent_role("security").unwrap(),
            AgentRole::Skeptic
        ));
        assert!(matches!(
            parse_agent_role("docs").unwrap(),
            AgentRole::Worker
        ));
        assert!(matches!(
            parse_agent_role("doc").unwrap(),
            AgentRole::Worker
        ));
    }

    #[test]
    fn test_parse_invalid_role() {
        assert!(parse_agent_role("invalid").is_err());
        assert!(parse_agent_role("").is_err());
    }

    #[test]
    fn test_case_insensitive_role_parsing() {
        assert!(matches!(
            parse_agent_role("FACTUAL").unwrap(),
            AgentRole::Reviewer
        ));
        assert!(matches!(
            parse_agent_role("SeNiOr").unwrap(),
            AgentRole::Reviewer
        ));
    }
}
