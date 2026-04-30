use rustycode_agent::{AgentEvents, AgentResult};
use rustycode_protocol::stream_event::StreamEvent;

#[derive(Default)]
pub struct HeadlessAgentBridge;

impl HeadlessAgentBridge {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl AgentEvents for HeadlessAgentBridge {
    async fn on_event(&mut self, _event: StreamEvent) {}

    async fn on_done(&mut self, _result: &AgentResult) {}
}
