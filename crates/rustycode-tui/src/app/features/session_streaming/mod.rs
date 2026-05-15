//! Session Streaming feature module
//!
//! Handles streaming of LLM responses and token updates.
//! Owns all streaming-related state (is_streaming, stream_content, chunks_received, etc.)
//!
//! ## State
//! - `SessionStreamingState`: Wraps streaming-related fields from session.streaming
//!
//! ## Events Handled
//! - `TuiEvent::Stream(StreamChunk)`: LLM stream output chunks
//! - `TuiEvent::Tick`: Periodic updates (streaming timeout checks)
//!
//! ## Surfaces
//! - "session": Main conversation view where streaming text appears
//!
//! ## Rendering
//! Streaming text is rendered in the main session surface

use crate::app::async_::StreamChunk;
use crate::app::features::{
    FeatureRegistry, RenderCtx, SurfaceId, TuiAction, TuiEvent, TuiFeature, UpdateCtx,
};
use ratatui::Frame;

/// Streaming state for a session
#[derive(Default)]
pub struct SessionStreamingState {
    /// Whether streaming is currently active
    pub is_streaming: bool,
    /// Current stream content accumulated so far
    pub current_stream_content: String,
    /// Number of stream chunks received
    pub chunks_received: usize,
    /// Number of thinking chunks received
    pub thinking_chunks_received: usize,
    /// When streaming started
    pub stream_start_time: Option<std::time::Instant>,
    /// Render buffer for streaming text display
    pub streaming_render_buffer: Vec<String>,
    /// Whether stream was cancelled
    pub stream_cancelled: bool,
}

impl SessionStreamingState {
    /// Create a new streaming state
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a new stream
    pub fn begin_streaming(&mut self) {
        self.is_streaming = true;
        self.current_stream_content.clear();
        self.chunks_received = 0;
        self.thinking_chunks_received = 0;
        self.stream_start_time = Some(std::time::Instant::now());
        self.streaming_render_buffer.clear();
        self.stream_cancelled = false;
    }

    /// Complete streaming (mark as done)
    pub fn complete_streaming(&mut self) {
        self.is_streaming = false;
    }

    /// Cancel streaming
    pub fn cancel_streaming(&mut self) {
        self.stream_cancelled = true;
        self.is_streaming = false;
    }

    /// Reset streaming state
    pub fn reset_streaming(&mut self) {
        self.is_streaming = false;
        self.current_stream_content.clear();
        self.chunks_received = 0;
        self.thinking_chunks_received = 0;
        self.stream_start_time = None;
        self.streaming_render_buffer.clear();
        self.stream_cancelled = false;
    }
}

/// Session streaming feature
pub struct SessionStreamingFeature {
    state: SessionStreamingState,
    surface: SurfaceId,
}

impl SessionStreamingFeature {
    /// Create a new session streaming feature
    pub fn new() -> Self {
        Self {
            state: SessionStreamingState::new(),
            surface: SurfaceId::new("session"),
        }
    }

    /// Handle a stream chunk event
    fn handle_stream_chunk(&mut self, chunk: StreamChunk) -> Vec<TuiAction> {
        // TODO: Implement actual stream chunk handling
        // For now, this is a placeholder showing the pattern
        let mut actions = Vec::new();

        match chunk {
            StreamChunk::Text(text) => {
                // Update streaming state
                if !self.state.is_streaming {
                    self.state.begin_streaming();
                }
                self.state.chunks_received += 1;
                self.state.current_stream_content.push_str(&text);
                self.state.streaming_render_buffer.push(text);

                // Mark UI as dirty to trigger redraw
                actions.push(TuiAction::MarkDirty);
            }
            StreamChunk::Thinking(thinking) => {
                if !self.state.is_streaming {
                    self.state.begin_streaming();
                }
                self.state.thinking_chunks_received += 1;
                // Thinking content handling would go here
                self.state
                    .streaming_render_buffer
                    .push(format!("[THINKING] {}", thinking));
                actions.push(TuiAction::MarkDirty);
            }
            StreamChunk::ToolStart { tool_id, .. } => {
                // Tool execution started - might emit action to tool panel feature
                self.state
                    .streaming_render_buffer
                    .push(format!("[TOOL START] {}", tool_id));
                actions.push(TuiAction::MarkDirty);
            }
            StreamChunk::ToolProgress { tool_id, .. } => {
                // Tool progress update
                self.state
                    .streaming_render_buffer
                    .push(format!("[TOOL PROGRESS] {:?}", tool_id));
                actions.push(TuiAction::MarkDirty);
            }
            StreamChunk::ToolComplete { .. } => {
                // Tool result handling
                actions.push(TuiAction::MarkDirty);
            }
            StreamChunk::Done => {
                // Complete streaming
                self.state.complete_streaming();
                actions.push(TuiAction::MarkDirty);
            }
            StreamChunk::Stopped { .. } => {
                // Stream was stopped
                self.state.cancel_streaming();
                actions.push(TuiAction::MarkDirty);
            }
            StreamChunk::Error(_) => {
                // Error handling
                self.state.cancel_streaming();
                actions.push(TuiAction::MarkDirty);
            }
            _ => {
                // Other variants (QuestionRequest, ApprovalRequest, ExecutionTrace, etc.)
                // would be handled by other features or special case handling
                actions.push(TuiAction::MarkDirty);
            }
        }

        actions
    }
}

impl Default for SessionStreamingFeature {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiFeature for SessionStreamingFeature {
    fn id(&self) -> &'static str {
        "session-streaming"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        // Register the session surface for this feature
        reg.register_surface(self.surface, self.id());
    }

    fn update(&mut self, event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
        match event {
            TuiEvent::Stream(chunk) => self.handle_stream_chunk(chunk.clone()),
            TuiEvent::Tick => {
                // Handle periodic updates (e.g., streaming timeout)
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn render(&self, surface: SurfaceId, _frame: &mut Frame, _ctx: &RenderCtx) {
        // Only render if this is our surface
        if surface != self.surface {
            return;
        }

        // TODO: Implement streaming text rendering
        // For now, placeholder
        if self.state.is_streaming {
            // Render streaming indicator and buffered text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_streaming_state_new_is_not_streaming() {
        let state = SessionStreamingState::new();
        assert!(!state.is_streaming);
        assert_eq!(state.chunks_received, 0);
    }

    #[test]
    fn begin_streaming_sets_flags() {
        let mut state = SessionStreamingState::new();
        state.begin_streaming();
        assert!(state.is_streaming);
        assert!(state.stream_start_time.is_some());
    }

    #[test]
    fn complete_streaming_clears_flag() {
        let mut state = SessionStreamingState::new();
        state.begin_streaming();
        state.complete_streaming();
        assert!(!state.is_streaming);
    }

    #[test]
    fn session_streaming_feature_has_id() {
        let feature = SessionStreamingFeature::new();
        assert_eq!(feature.id(), "session-streaming");
    }

    #[test]
    fn feature_registers_session_surface() {
        let feature = SessionStreamingFeature::new();
        let mut reg = crate::app::features::FeatureRegistry::new();
        feature.register(&mut reg);
        // Verify the feature can register surfaces (no assertions needed, just verify no panic)
    }

    #[test]
    fn reset_streaming_clears_all_state() {
        let mut state = SessionStreamingState::new();
        state.begin_streaming();
        state.current_stream_content.push_str("test content");
        state.chunks_received = 5;

        state.reset_streaming();

        assert!(!state.is_streaming);
        assert_eq!(state.current_stream_content, "");
        assert_eq!(state.chunks_received, 0);
        assert!(state.stream_start_time.is_none());
    }
}
