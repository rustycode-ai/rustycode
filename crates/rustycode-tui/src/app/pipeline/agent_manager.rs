use anyhow::Result;
use rustycode_agent::{AgentConfig, AgentEvents, AgentResult, AgentSession};
use rustycode_protocol::stream_event::{ApprovalDecision, StreamEvent};
use rustycode_tools_api::tiers::ToolTier;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;

use crate::app::async_::StreamChunk;
use crate::app::pipeline::tool_registry::ToolRegistry;
use crate::app::streaming::adapter::StreamEventAdapter;

pub struct TuiAgentManager {
    session: Arc<tokio::sync::Mutex<AgentSession>>,
    provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
}

pub struct TuiAgentBridge {
    final_text: String,
    adapter: StreamEventAdapter,
}

impl TuiAgentBridge {
    pub fn new(stream_tx: SyncSender<StreamChunk>) -> Self {
        Self {
            final_text: String::new(),
            adapter: StreamEventAdapter::new(stream_tx),
        }
    }

    pub fn with_approval_rx(mut self, approval_rx: Receiver<bool>) -> Self {
        self.adapter = self.adapter.with_approval_rx(approval_rx);
        self
    }

    pub fn with_question_rx(mut self, question_rx: Receiver<String>) -> Self {
        self.adapter = self.adapter.with_question_rx(question_rx);
        self
    }

    pub fn final_text(&self) -> &str {
        &self.final_text
    }
}

#[async_trait::async_trait]
impl AgentEvents for TuiAgentBridge {
    async fn on_event(&mut self, event: StreamEvent) {
        if let StreamEvent::TextDelta { content } = &event {
            self.final_text.push_str(content);
        }

        self.adapter.on_event(event).await;
    }

    async fn on_approval_needed(
        &mut self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> ApprovalDecision {
        self.adapter.on_approval_needed(tool_name, input).await
    }

    async fn on_question(&mut self, question: &str, options: &[String]) -> Option<String> {
        self.adapter.on_question(question, options).await
    }

    async fn on_done(&mut self, result: &AgentResult) {
        if self.final_text.is_empty() {
            self.final_text = result.final_text.clone();
        }
        self.adapter.on_done(result).await;
    }
}

impl TuiAgentManager {
    pub fn new(
        cwd: PathBuf,
        provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
        agent_config: AgentConfig,
    ) -> Self {
        let mut session = AgentSession::new(agent_config, cwd);
        session.activation.promote(ToolTier::Full);
        Self {
            session: Arc::new(tokio::sync::Mutex::new(session)),
            provider,
        }
    }

    pub fn create_bridge(&self, stream_tx: SyncSender<StreamChunk>) -> TuiAgentBridge {
        TuiAgentBridge::new(stream_tx)
    }

    pub async fn run_task<E: AgentEvents>(
        &self,
        model: &str,
        task: &str,
        _tool_registry: &ToolRegistry,
        events: &mut E,
    ) -> Result<AgentResult> {
        let tool_registry = rustycode_tools::default_registry();
        let mut session = self.session.lock().await;
        session
            .run(
                &*self.provider,
                model,
                "You are an autonomous development agent.",
                vec![rustycode_llm::provider::ChatMessage::user(task.to_string())],
                &[],
                &tool_registry,
                events,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::async_::StreamChunk;
    use std::sync::mpsc::sync_channel;

    #[tokio::test]
    async fn test_adapter_text_delta() {
        let (tx, rx) = sync_channel(1);
        let bridge = TuiAgentBridge::new(tx);
        let mut bridge = bridge;
        bridge
            .on_event(StreamEvent::TextDelta {
                content: "Hello".to_string(),
            })
            .await;
        let chunk = rx.recv().unwrap();
        match chunk {
            StreamChunk::Text(s) => assert_eq!(s, "Hello"),
            _ => panic!("Expected Text chunk"),
        }
    }

    #[tokio::test]
    async fn test_adapter_done() {
        let (tx, rx) = sync_channel(1);
        let bridge = TuiAgentBridge::new(tx);
        let mut bridge = bridge;
        bridge.on_event(StreamEvent::Done).await;
        let chunk = rx.recv().unwrap();
        match chunk {
            StreamChunk::Done => {}
            _ => panic!("Expected Done"),
        }
    }
}
