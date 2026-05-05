//! Review Agent - Code review and analysis agent
//!
//! This agent specializes in code review tasks including:
//! - Code quality assessment
//! - Best practices evaluation
//! - Security vulnerability detection
//! - Performance analysis

use crate::agent::{Agent, AgentConfig, AgentResult};
use anyhow::Result;
use async_trait::async_trait;

/// Review agent for code analysis and review
#[derive(Default)]
pub struct ReviewAgent;

impl ReviewAgent {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Agent for ReviewAgent {
    fn name(&self) -> &'static str {
        "review_agent"
    }

    fn description(&self) -> &'static str {
        "Code review and analysis agent for quality assessment and best practices"
    }

    async fn execute(
        &self,
        prompt: &str,
        _context: Option<&str>,
        _config: &AgentConfig,
    ) -> Result<AgentResult> {
        let content = format!("Review Agent analyzing: {prompt}");
        Ok(AgentResult::success(content))
    }

    fn can_handle(&self, task_description: &str) -> bool {
        let keywords = [
            "review",
            "analyze",
            "quality",
            "security",
            "performance",
            "audit",
        ];

        keywords
            .iter()
            .any(|&kw| task_description.to_lowercase().contains(kw))
    }
}
