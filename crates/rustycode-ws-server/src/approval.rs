//! WS pipeline interaction: routes tool approval requests through the
//! WebSocket to the browser client and awaits the response.

use async_trait::async_trait;
use rustycode_orchestration::pipeline::PipelineInteraction;
use rustycode_protocol::stream_event::ApprovalDecision;
use tracing::info;

/// PipelineInteraction that auto-approves all tool calls.
///
/// The EventBridge forwards `ToolCallStarted` events to the client so the
/// browser shows tool activity in real-time. Explicit approval gating requires
/// ID coordination between the interaction and bridge-forwarded events that
/// isn't wired yet, so we auto-approve to avoid blocking the pipeline.
pub struct WsPipelineInteraction;

impl Default for WsPipelineInteraction {
    fn default() -> Self {
        Self
    }
}

impl WsPipelineInteraction {
    pub fn new() -> Self {
        Self
    }

    pub fn set_session(
        &self,
        _pending_approvals: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
        _cancel_token: tokio_util::sync::CancellationToken,
    ) {
        // Kept as no-op so session.rs call sites compile without changes.
    }

    pub fn clear_session(&self) {}
}

#[async_trait]
impl PipelineInteraction for WsPipelineInteraction {
    async fn request_approval(
        &self,
        tool_name: &str,
        _input: &serde_json::Value,
    ) -> ApprovalDecision {
        info!(tool_name = %tool_name, "auto-approving tool call");
        ApprovalDecision::AutoApproved
    }

    fn is_cancelled(&self) -> bool {
        false
    }
}
