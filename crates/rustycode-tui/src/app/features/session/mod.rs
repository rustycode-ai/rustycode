//! Session feature module
//!
//! Handles session state management: messages, active tools, message selection.
//! Owns session-related state (messages, selected_message, active_tools, etc.)
//!
//! ## State
//! - `SessionState`: Wraps session messages, tool tracking, selection state
//!
//! ## Events Handled
//! - `TuiEvent::Service(EventMsg)`: Message and event updates from the orchestration layer
//! - `TuiEvent::Stream(StreamChunk)`: Stream completion signals
//! - `TuiEvent::Tick`: Periodic updates
//!
//! ## Surfaces
//! - "session": Main conversation view where messages appear
//!
//! ## Rendering
//! Messages are rendered in the main session surface with selection highlighting

use crate::app::features::{
    FeatureRegistry, RenderCtx, SurfaceId, TuiAction, TuiEvent, TuiFeature, UpdateCtx,
};
use chrono::{DateTime, Utc};
use ratatui::Frame;
use rustycode_protocol::Message;

/// Tool execution status for tracking in-progress operations
#[derive(Debug, Clone)]
pub struct ToolStatus {
    /// Tool ID from tool use block
    pub tool_id: String,
    /// Tool name
    pub tool_name: String,
    /// When execution started
    pub started_at: DateTime<Utc>,
    /// Current status: "pending", "running", "complete", "error"
    pub status: String,
}

/// Session state management
#[derive(Default)]
pub struct SessionState {
    /// Conversation messages
    pub messages: Vec<Message>,
    /// Currently selected message index for inspection/navigation
    pub selected_message: usize,
    /// Active tool executions (tool_id, status)
    pub active_tools: Vec<ToolStatus>,
    /// Total tokens in message context (for display)
    pub context_tokens: usize,
    /// Whether session has unsaved changes
    pub has_unsaved_changes: bool,
}

impl SessionState {
    /// Create a new session state
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a message to the session
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.has_unsaved_changes = true;
        // Auto-select new message if not already scrolled
        if self.selected_message == 0 || self.selected_message >= self.messages.len() - 1 {
            self.selected_message = self.messages.len() - 1;
        }
    }

    /// Clear all messages (for new session)
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.selected_message = 0;
        self.has_unsaved_changes = false;
    }

    /// Add an active tool execution
    pub fn start_tool(&mut self, tool_id: impl Into<String>, tool_name: impl Into<String>) {
        let status = ToolStatus {
            tool_id: tool_id.into(),
            tool_name: tool_name.into(),
            started_at: Utc::now(),
            status: "running".to_string(),
        };
        self.active_tools.push(status);
    }

    /// Mark a tool as complete
    pub fn complete_tool(&mut self, tool_id: &str) {
        if let Some(pos) = self.active_tools.iter().position(|t| t.tool_id == tool_id) {
            self.active_tools[pos].status = "complete".to_string();
        }
    }

    /// Remove a tool from active list
    pub fn remove_tool(&mut self, tool_id: &str) {
        self.active_tools.retain(|t| t.tool_id != tool_id);
    }

    /// Get the count of active tools
    pub fn active_tool_count(&self) -> usize {
        self.active_tools.len()
    }

    /// Select next message
    pub fn select_next_message(&mut self) {
        if !self.messages.is_empty() && self.selected_message < self.messages.len() - 1 {
            self.selected_message += 1;
        }
    }

    /// Select previous message
    pub fn select_previous_message(&mut self) {
        if self.selected_message > 0 {
            self.selected_message -= 1;
        }
    }

    /// Get the currently selected message, if any
    pub fn current_message(&self) -> Option<&Message> {
        self.messages.get(self.selected_message)
    }

    /// Reset session state for a new conversation
    pub fn reset(&mut self) {
        self.messages.clear();
        self.selected_message = 0;
        self.active_tools.clear();
        self.context_tokens = 0;
        self.has_unsaved_changes = false;
    }
}

/// Session feature for message management
pub struct SessionFeature {
    state: SessionState,
    surface: SurfaceId,
}

impl SessionFeature {
    /// Create a new session feature
    pub fn new() -> Self {
        Self {
            state: SessionState::new(),
            surface: SurfaceId::new("session"),
        }
    }
}

impl Default for SessionFeature {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiFeature for SessionFeature {
    fn id(&self) -> &'static str {
        "session"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(self.surface, self.id());
    }

    fn update(&mut self, event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
        match event {
            TuiEvent::Service(_event_msg) => {
                // TODO: Handle ServiceEvent variants
                // - MessageEvent: new message arrives → add_message()
                // - ToolEvent: tool updates → start_tool() / complete_tool()
                // - CompletionEvent: stream complete → trigger final message update
                Vec::new()
            }
            TuiEvent::Stream(_chunk) => {
                // Stream completion is handled by SessionStreamingFeature
                // This feature receives the signal but doesn't process stream chunks directly
                Vec::new()
            }
            TuiEvent::Key(_)
            | TuiEvent::Tick
            | TuiEvent::Resize { .. }
            | TuiEvent::FocusGained
            | TuiEvent::FocusLost => {
                // Handled by other features or not applicable
                Vec::new()
            }
        }
    }

    fn render(&self, surface: SurfaceId, _frame: &mut Frame, _ctx: &RenderCtx) {
        if surface == self.surface {
            // TODO: Implement session message rendering
            // - Render messages with proper styling
            // - Highlight selected message
            // - Show tool status indicators
            // - Render active tools list
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_new_is_empty() {
        let state = SessionState::new();
        assert_eq!(state.messages.len(), 0);
        assert_eq!(state.selected_message, 0);
        assert_eq!(state.active_tools.len(), 0);
    }

    #[test]
    fn session_state_clears_messages() {
        let mut state = SessionState::new();
        state.context_tokens = 100;
        state.has_unsaved_changes = true;

        state.clear_messages();

        assert_eq!(state.messages.len(), 0);
        assert_eq!(state.selected_message, 0);
        assert!(!state.has_unsaved_changes);
    }

    #[test]
    fn session_state_resets_all_fields() {
        let mut state = SessionState::new();
        state.context_tokens = 200;
        state.selected_message = 5;
        state.active_tools.push(ToolStatus {
            tool_id: "test".to_string(),
            tool_name: "tool".to_string(),
            started_at: Utc::now(),
            status: "running".to_string(),
        });

        state.reset();

        assert_eq!(state.messages.len(), 0);
        assert_eq!(state.selected_message, 0);
        assert_eq!(state.active_tools.len(), 0);
        assert_eq!(state.context_tokens, 0);
        assert!(!state.has_unsaved_changes);
    }

    #[test]
    fn session_feature_has_id() {
        let feature = SessionFeature::new();
        assert_eq!(feature.id(), "session");
    }

    #[test]
    fn feature_registers_session_surface() {
        let feature = SessionFeature::new();
        let mut reg = crate::app::features::FeatureRegistry::new();
        feature.register(&mut reg);
        assert_eq!(
            reg.surface_feature(SurfaceId::new("session")),
            Some("session")
        );
    }

    #[test]
    fn session_state_tracks_active_tools() {
        let mut state = SessionState::new();
        state.start_tool("tool-1", "echo");
        state.start_tool("tool-2", "bash");

        assert_eq!(state.active_tool_count(), 2);

        state.complete_tool("tool-1");
        assert_eq!(state.active_tools[0].status, "complete");

        state.remove_tool("tool-1");
        assert_eq!(state.active_tool_count(), 1);
    }

    #[test]
    fn session_state_message_navigation() {
        let mut state = SessionState::new();
        let msg1 = Message::user("hello");
        let msg2 = Message::user("world");

        state.add_message(msg1);
        state.add_message(msg2);

        assert_eq!(state.selected_message, 1);

        state.select_previous_message();
        assert_eq!(state.selected_message, 0);

        state.select_next_message();
        assert_eq!(state.selected_message, 1);

        state.select_next_message();
        assert_eq!(state.selected_message, 1); // Clamped at end
    }
}
