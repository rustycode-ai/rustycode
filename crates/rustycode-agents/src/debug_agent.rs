//! Debug Agent - Debugging and troubleshooting agent
//!
//! This agent specializes in debugging tasks including:
//! - Error analysis and diagnosis
//! - Root cause identification
//! - Debugging strategy suggestions
//! - Fix recommendations

use crate::agent::{Agent, AgentConfig, AgentResult};
use anyhow::Result;
use async_trait::async_trait;

/// Debug agent for error analysis and troubleshooting
#[derive(Default)]
pub struct DebugAgent;

impl DebugAgent {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Agent for DebugAgent {
    fn name(&self) -> &'static str {
        "debug_agent"
    }

    fn description(&self) -> &'static str {
        "Debugging and troubleshooting agent for error analysis and fixes"
    }

    async fn execute(
        &self,
        prompt: &str,
        _context: Option<&str>,
        _config: &AgentConfig,
    ) -> Result<AgentResult> {
        let content = format!("Debug Agent analyzing: {prompt}");
        Ok(AgentResult::success(content))
    }

    fn can_handle(&self, task_description: &str) -> bool {
        let keywords = [
            "debug",
            "error",
            "bug",
            "fix",
            "troubleshoot",
            "crash",
            "failure",
        ];

        keywords
            .iter()
            .any(|&kw| task_description.to_lowercase().contains(kw))
    }
}
