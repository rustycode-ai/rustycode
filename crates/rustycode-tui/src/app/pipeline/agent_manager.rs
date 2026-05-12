use anyhow::Result;
use rustycode_agent_runtime::{AgentConfig, AgentEvents, AgentResult, AgentSession};
use rustycode_protocol::permission_modes::PermissionMode;
use rustycode_protocol::stream_event::{ApprovalDecision, StreamEvent};
use rustycode_tools_api::tiers::ToolTier;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;

use crate::app::async_::StreamChunk;
use crate::app::pipeline::tool_registry::ToolRegistry;
use crate::app::streaming::adapter::StreamEventAdapter;
use anyhow::Context as _;
use rustycode_tools_api::{Tool as RustyCodeTool, ToolOutput, ToolPermission};
use serde_json::json;
use std::sync::Arc as StdArc;

struct PipelineToolAdapter {
    name: String,
    description: String,
    parameters_schema: serde_json::Value,
    permission: ToolPermission,
    tool: StdArc<dyn crate::app::pipeline::tool_registry::Tool>,
}

impl RustyCodeTool for PipelineToolAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn permission(&self) -> ToolPermission {
        self.permission
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters_schema.clone()
    }

    fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &rustycode_tools_api::ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let output = self
            .tool
            .execute(params)
            .with_context(|| format!("pipeline tool '{}' failed", self.name))?;
        let text = output
            .as_str()
            .map(ToString::to_string)
            .unwrap_or_else(|| output.to_string());
        Ok(ToolOutput::text(text))
    }
}

fn browser_tool_adapters(tool_registry: &ToolRegistry) -> Vec<Box<dyn RustyCodeTool>> {
    let mut tools = Vec::new();

    if let Some(tool) = tool_registry.get("browser", "goto") {
        tools.push(Box::new(PipelineToolAdapter {
            name: "browser.goto".to_string(),
            description: "Navigate to a URL in the browser and return the title plus final URL"
                .to_string(),
            parameters_schema: json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": {"type": "string", "description": "URL to navigate to"}
                }
            }),
            permission: ToolPermission::Network,
            tool,
        }) as Box<dyn RustyCodeTool>);
    }

    if let Some(tool) = tool_registry.get("browser", "extract") {
        tools.push(Box::new(PipelineToolAdapter {
            name: "browser.extract".to_string(),
            description: "Extract page content or a selector from the active browser tab"
                .to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "selector": {"type": "string", "description": "CSS selector to extract"},
                    "screenshot": {"type": "boolean", "description": "Return a screenshot instead of content"}
                }
            }),
            permission: ToolPermission::Network,
            tool,
        }) as Box<dyn RustyCodeTool>);
    }

    tools
}

pub struct TuiAgentManager {
    session: Arc<tokio::sync::Mutex<AgentSession>>,
    provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
    /// Phase 1C shadow mode: broadcast receiver for EventMsg validation.
    event_rx: tokio::sync::broadcast::Receiver<rustycode_protocol::EventMsg>,
}

pub struct TuiAgentBridge {
    final_text: String,
    adapter: StreamEventAdapter,
    permission_mode: PermissionMode,
    /// Phase 1C shadow mode: broadcast receiver for EventMsg validation.
    event_rx: Option<tokio::sync::broadcast::Receiver<rustycode_protocol::EventMsg>>,
    /// Shadow mode validation counters.
    shadow_matches: usize,
    shadow_mismatches: usize,
    shadow_misses: usize,
}

impl TuiAgentBridge {
    pub fn new(stream_tx: SyncSender<StreamChunk>) -> Self {
        Self {
            final_text: String::new(),
            adapter: StreamEventAdapter::new(stream_tx),
            permission_mode: PermissionMode::Default,
            event_rx: None,
            shadow_matches: 0,
            shadow_mismatches: 0,
            shadow_misses: 0,
        }
    }

    pub fn with_approval_rx(mut self, approval_rx: Receiver<(String, bool)>) -> Self {
        self.adapter = self.adapter.with_approval_rx(approval_rx);
        self
    }

    pub fn with_question_rx(mut self, question_rx: Receiver<String>) -> Self {
        self.adapter = self.adapter.with_question_rx(question_rx);
        self
    }

    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = mode;
        self.adapter = self.adapter.with_permission_mode(mode);
        self
    }

    pub fn with_event_rx(
        mut self,
        event_rx: tokio::sync::broadcast::Receiver<rustycode_protocol::EventMsg>,
    ) -> Self {
        self.event_rx = Some(event_rx);
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

        // Existing callback flow (authoritative)
        self.adapter.on_event(event.clone()).await;

        // Phase 1C shadow mode: validate broadcast matches callback
        if let Some(ref mut rx) = self.event_rx {
            match rx.try_recv() {
                Ok(msg) => {
                    tracing::debug!(
                        target: "rustycode_tui::shadow",
                        "EventMsg shadow received: {:?}",
                        msg
                    );
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                    tracing::debug!(
                        target: "rustycode_tui::shadow",
                        "EventMsg shadow: no broadcast event available (may arrive later)"
                    );
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                    tracing::warn!(
                        target: "rustycode_tui::shadow",
                        "EventMsg shadow lagged, skipped {n} events"
                    );
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    tracing::debug!(
                        target: "rustycode_tui::shadow",
                        "EventMsg shadow channel closed"
                    );
                }
            }
        }
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
        // Phase 1C shadow mode: subscribe to EventMsg broadcast
        let event_rx = session.subscribe();
        Self {
            session: Arc::new(tokio::sync::Mutex::new(session)),
            provider,
            event_rx,
        }
    }

    pub fn create_bridge(&self, stream_tx: SyncSender<StreamChunk>) -> TuiAgentBridge {
        // Phase 1C shadow mode: pass broadcast receiver to bridge
        TuiAgentBridge::new(stream_tx).with_event_rx(self.event_rx.resubscribe())
    }

    pub async fn run_task<E: AgentEvents>(
        &self,
        model: &str,
        task: &str,
        tool_registry: &ToolRegistry,
        events: &mut E,
    ) -> Result<AgentResult> {
        let mut registry = rustycode_tools::default_registry();
        for adapter in browser_tool_adapters(tool_registry) {
            registry.register_boxed(adapter);
        }
        let mut session = self.session.lock().await;
        session
            .run(
                &*self.provider,
                model,
                "You are an autonomous development agent.",
                vec![rustycode_llm::provider::ChatMessage::user(task.to_string())],
                &[],
                &registry,
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
