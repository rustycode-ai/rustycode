//! Test Agent - Test generation and execution agent
//!
//! This agent specializes in testing tasks including:
//! - Unit test generation
//! - Integration test creation
//! - Test execution and analysis
//! - Test coverage improvement

use crate::agent::{Agent, AgentConfig, AgentResult};
use anyhow::Result;
use async_trait::async_trait;

/// Test agent for test generation and execution
pub struct TestAgent;

impl TestAgent {
    /// Create a new test agent
    pub const fn new() -> Self {
        Self
    }
}

impl Default for TestAgent {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Agent for TestAgent {
    fn name(&self) -> &'static str {
        "test_agent"
    }

    fn description(&self) -> &'static str {
        "Test generation and execution agent for comprehensive testing"
    }

    async fn execute(
        &self,
        prompt: &str,
        _context: Option<&str>,
        _config: &AgentConfig,
    ) -> Result<AgentResult> {
        let content = format!("Test Agent processing: {prompt}");
        Ok(AgentResult::success(content))
    }

    fn can_handle(&self, task_description: &str) -> bool {
        let keywords = [
            "test",
            "testing",
            "unit test",
            "integration",
            "coverage",
            "spec",
        ];

        keywords
            .iter()
            .any(|&kw| task_description.to_lowercase().contains(kw))
    }
}
