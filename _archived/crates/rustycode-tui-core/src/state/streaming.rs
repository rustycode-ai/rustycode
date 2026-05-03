//! Streaming State Management
//!
//! Manages LLM streaming, tool execution, and real-time content updates.

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Streaming state for real-time content updates
#[derive(Debug)]
pub struct StreamingState {
    /// Current streaming content
    pub current_stream_content: String,

    /// Streaming render buffer
    pub streaming_render_buffer: crate::StreamingRenderBuffer,

    /// Streaming status
    pub is_streaming: bool,
    pub stream_cancelled: bool,
    pub chunks_received: usize,

    /// Queued message during streaming
    pub queued_message: Option<String>,

    /// Background command results
    pub pending_bash_result: Arc<Mutex<Option<String>>>,

    /// Performance tracking
    pub stream_start_time: Option<std::time::Instant>,
    pub last_response_duration: Option<Duration>,
}

impl Default for StreamingState {
    fn default() -> Self {
        Self {
            current_stream_content: String::new(),
            streaming_render_buffer: crate::StreamingRenderBuffer,
            is_streaming: false,
            stream_cancelled: false,
            chunks_received: 0,
            queued_message: None,
            pending_bash_result: Arc::new(Mutex::new(None)),
            stream_start_time: None,
            last_response_duration: None,
        }
    }
}

/// Tool execution state for tracking active tools
#[derive(Debug, Default)]
pub struct ToolExecutionState {
    /// Active tool execution names (placeholder: tracking names only)
    pub active_tool_names: Vec<String>,

    /// Tool panel visibility
    pub showing_tool_panel: bool,

    /// Tool panel history names
    pub tool_panel_history: Vec<String>,

    /// Tool panel selected index
    pub tool_panel_selected_index: Option<usize>,

    /// Tool result display
    pub showing_tool_result: bool,
    pub tool_result_show_full: bool,
    pub tool_result_scroll_offset: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_state_default() {
        let state = StreamingState::default();
        assert!(state.current_stream_content.is_empty());
        assert!(!state.is_streaming);
        assert!(!state.stream_cancelled);
        assert_eq!(state.chunks_received, 0);
        assert!(state.queued_message.is_none());
        assert!(state.stream_start_time.is_none());
        assert!(state.last_response_duration.is_none());
    }

    #[test]
    fn test_streaming_state_active() {
        let state = StreamingState {
            is_streaming: true,
            current_stream_content: "Hello...".to_string(),
            chunks_received: 5,
            stream_start_time: Some(std::time::Instant::now()),
            ..StreamingState::default()
        };
        assert!(state.is_streaming);
        assert_eq!(state.chunks_received, 5);
    }

    #[test]
    fn test_streaming_cancelled() {
        let state = StreamingState {
            is_streaming: true,
            stream_cancelled: true,
            ..StreamingState::default()
        };
        assert!(state.stream_cancelled);
    }

    #[test]
    fn test_pending_bash_result() {
        let state = StreamingState::default();
        assert!(state
            .pending_bash_result
            .try_lock()
            .map_or(true, |g| g.is_none()));
    }

    #[test]
    fn test_tool_execution_state_default() {
        let state = ToolExecutionState::default();
        assert!(state.active_tool_names.is_empty());
        assert!(!state.showing_tool_panel);
        assert!(state.tool_panel_history.is_empty());
        assert!(state.tool_panel_selected_index.is_none());
        assert!(!state.showing_tool_result);
        assert!(!state.tool_result_show_full);
        assert_eq!(state.tool_result_scroll_offset, 0);
    }

    #[test]
    fn test_tool_execution_active_tools() {
        let state = ToolExecutionState {
            active_tool_names: vec!["bash".to_string(), "read".to_string()],
            showing_tool_panel: true,
            ..ToolExecutionState::default()
        };
        assert_eq!(state.active_tool_names.len(), 2);
        assert!(state.showing_tool_panel);
    }

    #[test]
    fn test_queued_message() {
        let state = StreamingState {
            queued_message: Some("pending input".to_string()),
            ..StreamingState::default()
        };
        assert_eq!(state.queued_message.as_deref(), Some("pending input"));
    }
}
