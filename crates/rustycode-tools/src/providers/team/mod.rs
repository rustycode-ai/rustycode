use anyhow::{anyhow, Result};
use schemars::JsonSchema;

// Re-export all tools
pub use create::*;
pub use delete::*;

pub mod create;
pub mod delete;

#[derive(serde::Deserialize, JsonSchema)]
pub struct TeamCreateParams {
    /// Name for the new team. Used as directory name under ~/.claude/teams/
    pub team_name: String,
    /// Team description/purpose
    pub description: Option<String>,
    /// Type/role of the team lead (e.g., 'researcher', 'test-runner')
    pub agent_type: Option<String>,
}

#[derive(serde::Deserialize, JsonSchema)]
pub struct TeamDeleteParams {}

pub(crate) fn validate_team_name(name: &str) -> Result<()> {
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

pub(crate) fn chrono_now_rfc3339() -> String {
    // Avoid depending on chrono; use std time
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}s since epoch", duration.as_secs())
}

#[cfg(test)]
pub(crate) mod tests_common {
    use crate::ToolContext;

    pub fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }
}
