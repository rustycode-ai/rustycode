use std::sync::Arc;

use tracing::warn;
use crate::session::SessionManager;

pub struct EventBridge {
    _session_manager: Arc<SessionManager>,
    _session_token: String,
}

impl EventBridge {
    pub fn new(_session_manager: Arc<SessionManager>, _session_token: String) -> Self {
        // TODO: Subscribe to EventBus for StreamEvents matching session_token.
        // The bridge will:
        // 1. Subscribe to rustycode-bus with pattern matching the session
        // 2. For each StreamEvent, wrap in EventPayload with next_seq()
        // 3. Send as ServerMessage::Event over the WebSocket sink
        // 4. Periodically send StateSnapshot (every N seconds or on TurnCompleted)
        //
        // This requires the EventBus to be injected as a dependency.
        // For Phase 1, the bridge is a placeholder that holds the session context.
        Self {
            _session_manager,
            _session_token,
        }
    }
}

impl Drop for EventBridge {
    fn drop(&mut self) {
        // TODO: Unsubscribe from EventBus when the WebSocket disconnects
        warn!(
            session_id = %self._session_token,
            "event bridge dropped — EventBus cleanup needed when integrated"
        );
    }
}
