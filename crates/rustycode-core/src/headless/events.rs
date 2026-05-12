use rustycode_agent_runtime::{AgentEvents, AgentResult};
use rustycode_protocol::stream_event::{ApprovalDecision, StreamEvent};
use rustycode_protocol::tool_names as tn;

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

    async fn on_approval_needed(
        &mut self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> ApprovalDecision {
        use rustycode_tools_security::approve::SmartApprove;

        let sa = SmartApprove::new();
        let command = if tool_name == tn::BASH {
            input
                .as_object()
                .and_then(|obj| obj.get("command"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        } else {
            input.to_string()
        };

        match sa.classify(tool_name, Some(&command)) {
            rustycode_protocol::permission_modes::OperationClass::ReadOnly => {
                tracing::debug!("Headless auto-approved (read-only): {}", tool_name);
            }
            rustycode_protocol::permission_modes::OperationClass::Write => {
                tracing::info!(
                    "Headless auto-approved (write): {} {}",
                    tool_name,
                    truncate_cmd(&command, 60)
                );
            }
            rustycode_protocol::permission_modes::OperationClass::Destructive => {
                tracing::warn!(
                    "Headless auto-approved (DESTRUCTIVE): {} {}",
                    tool_name,
                    truncate_cmd(&command, 60)
                );
            }
            rustycode_protocol::permission_modes::OperationClass::Unknown => {
                tracing::info!("Headless auto-approved (unknown): {}", tool_name);
            }
        }
        ApprovalDecision::AutoApproved
    }

    async fn on_done(&mut self, _result: &AgentResult) {}
}

fn truncate_cmd(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let boundary = s.floor_char_boundary(max);
        format!("{}…", &s[..boundary])
    }
}
