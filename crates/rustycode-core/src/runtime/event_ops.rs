//! Runtime event publishing operations.

use std::sync::Arc;

use rustycode_bus::{SessionCompletedEvent, SessionStartedEvent, ToolBlockedEvent};
use rustycode_protocol::SessionId;
use tracing::warn;

use super::Runtime;

impl Runtime {
    /// Helper to publish events from sync code.
    pub(crate) fn publish_event<E: rustycode_bus::Event + Clone + Send + 'static>(&self, event: E) {
        let bus = Arc::clone(&self.bus);

        // Try to use existing runtime; otherwise use the shared runtime
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // We're inside a runtime, spawn a task to publish
            handle.spawn(async move {
                if let Err(e) = bus.publish(event).await {
                    warn!("Failed to publish event to bus: {}", e);
                }
            });
        } else {
            // No runtime exists — run on the workspace-wide shared runtime.
            crate::shared_runtime::block_on_shared(async move {
                if let Err(e) = bus.publish(event).await {
                    warn!("Failed to publish event to bus: {}", e);
                }
            });
        }
    }

    /// Publish session started event
    pub fn publish_session_started(&self, session_id: SessionId, task: String, detail: String) {
        self.publish_event(SessionStartedEvent::new(session_id, task, detail));
    }

    /// Publish session completed event
    pub fn publish_session_completed(
        &self,
        session_id: SessionId,
        task: String,
        status: String,
        detail: String,
    ) {
        self.publish_event(SessionCompletedEvent::new(session_id, task, status, detail));
    }

    /// Publish tool blocked event
    pub fn publish_tool_blocked(
        &self,
        session_id: SessionId,
        tool_name: String,
        arguments: serde_json::Value,
        reason: String,
        detail: String,
    ) {
        self.publish_event(ToolBlockedEvent::new(
            session_id, tool_name, arguments, reason, detail,
        ));
    }
}
