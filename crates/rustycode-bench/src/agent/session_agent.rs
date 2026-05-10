use crate::agent::BenchAgent;
use crate::environment::BenchEnvironment;
use anyhow::Result;
use rustycode_agent_runtime::{AgentConfig, AgentEvents, AgentResult, AgentSession};
use rustycode_llm::provider::LLMProvider;
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_tools::ToolRegistry;
use std::path::PathBuf;
use std::sync::Arc;

pub struct SessionBenchAgent {
    name: &'static str,
    model: String,
    session: AgentSession,
    provider: Arc<dyn LLMProvider>,
    tool_registry: ToolRegistry,
}

impl SessionBenchAgent {
    pub fn new(name: &'static str, model: String, provider: Arc<dyn LLMProvider>, tool_registry: ToolRegistry, cwd: PathBuf) -> Self {
        Self {
            name,
            model,
            session: AgentSession::new(AgentConfig::default(), cwd),
            provider,
            tool_registry,
        }
    }
}

struct BenchAgentEvents;
#[async_trait::async_trait]
impl AgentEvents for BenchAgentEvents {
    async fn on_event(&mut self, _event: StreamEvent) {}
    async fn on_done(&mut self, _result: &AgentResult) {}
}

#[async_trait::async_trait]
impl BenchAgent for SessionBenchAgent {
    fn name(&self) -> &'static str { self.name }
    async fn setup(&mut self, _env: &mut dyn BenchEnvironment) -> Result<()> {
        Ok(())
    }
    async fn run(&mut self, instruction: &str, _env: &mut dyn BenchEnvironment) -> Result<()> {
        let mut events = BenchAgentEvents;
        self.session.run(
            &*self.provider,
            &self.model,
            "You are an autonomous development agent.",
            vec![rustycode_llm::provider::ChatMessage::user(instruction.to_string())],
            &[],
            &self.tool_registry,
            &mut events
        ).await?;
        Ok(())
    }

    fn token_usage(&self) -> (u64, u64) {
        // SessionBenchAgent does not currently track token usage
        (0, 0)
    }
}
