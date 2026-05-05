//! Code Agent - General-purpose coding agent
//!
//! This agent specializes in code-related tasks including:
//! - Code generation and modification
//! - Code review and analysis
//! - Bug fixing and debugging
//! - Refactoring suggestions

use crate::agent::{Agent, AgentConfig, AgentResult};
use anyhow::Result;
use async_trait::async_trait;

/// Code agent for general coding tasks
#[derive(Default)]
pub struct CodeAgent;

impl CodeAgent {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Agent for CodeAgent {
    fn name(&self) -> &'static str {
        "code_agent"
    }

    fn description(&self) -> &'static str {
        "General-purpose coding agent for code generation, review, and modification"
    }

    async fn execute(
        &self,
        prompt: &str,
        _context: Option<&str>,
        _config: &AgentConfig,
    ) -> Result<AgentResult> {
        let content = format!("Code Agent processing: {prompt}");

        Ok(AgentResult::success(content))
    }

    fn can_handle(&self, task_description: &str) -> bool {
        let keywords = [
            "code",
            "programming",
            "function",
            "class",
            "bug",
            "fix",
            "refactor",
            "implement",
            "write",
            "generate",
            "review",
            "debug",
            "test",
        ];

        keywords
            .iter()
            .any(|&kw| task_description.to_lowercase().contains(kw))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn code_agent_can_handle_coding_tasks() {
        let agent = CodeAgent::new();

        assert!(agent.can_handle("write a function to calculate fibonacci"));
        assert!(agent.can_handle("fix this bug in the code"));
        assert!(agent.can_handle("review this pull request"));
        assert!(agent.can_handle("refactor this class"));
        assert!(!agent.can_handle("what is the weather today"));
    }

    #[tokio::test]
    async fn code_agent_execute_returns_result() {
        let agent = CodeAgent::new();
        let config = AgentConfig::default();

        let result = agent.execute("test prompt", None, &config).await.unwrap();

        assert!(result.success);
        assert!(result.content.contains("Code Agent processing"));
        assert!(result.error.is_none());
    }
}
