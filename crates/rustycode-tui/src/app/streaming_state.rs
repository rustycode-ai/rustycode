//! Streaming state sub-struct for the TUI.
//!
//! Groups all fields related to active LLM streaming into one cohesive unit,
//! keeping the main `TUI` struct definition manageable.

use std::sync::Arc;
use std::time::{Duration, Instant};

/// Streaming-related state for the TUI.
///
/// All fields here track the lifecycle of an active LLM response stream,
/// from the first chunk received through completion.
pub(crate) struct StreamingState {
    /// Whether an LLM response stream is currently active.
    pub(crate) is_streaming: bool,
    /// Set by Esc/Ctrl+C to cooperatively cancel the stream.
    pub(crate) stream_cancelled: bool,
    /// Number of text content chunks received in the current stream.
    pub(crate) chunks_received: usize,
    /// Number of thinking/reasoning chunks received in the current stream.
    pub(crate) thinking_chunks_received: usize,
    /// Accumulated text content from the current stream.
    pub(crate) current_stream_content: String,
    /// Render buffer for incremental streaming display.
    pub(crate) streaming_render_buffer: crate::app::streaming_render_buffer::StreamingRenderBuffer,
    /// Message queued by the user while a stream is active (goose pattern).
    pub(crate) queued_message: Option<String>,
    /// Shared store for background bash command results.
    pub(crate) pending_bash_result: Arc<std::sync::Mutex<Option<String>>>,
    /// Instant when the current stream started (for elapsed timing).
    pub(crate) stream_start_time: Option<Instant>,
    /// Duration of the most recently completed response (shown in status bar).
    pub(crate) last_response_duration: Option<Duration>,
}

impl StreamingState {
    /// Create a new `StreamingState` with all fields at their default/empty values.
    pub(crate) fn new() -> Self {
        Self {
            is_streaming: false,
            stream_cancelled: false,
            chunks_received: 0,
            thinking_chunks_received: 0,
            current_stream_content: String::new(),
            streaming_render_buffer:
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new(),
            queued_message: None,
            pending_bash_result: Arc::new(std::sync::Mutex::new(None)),
            stream_start_time: None,
            last_response_duration: None,
        }
    }

    /// Reset all streaming state back to idle defaults.
    ///
    /// Called when a stream completes or is cancelled.
    pub(crate) fn reset(&mut self) {
        self.is_streaming = false;
        self.stream_cancelled = false;
        self.chunks_received = 0;
        self.thinking_chunks_received = 0;
        self.current_stream_content.clear();
        self.streaming_render_buffer =
            crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
        self.stream_start_time = None;
        // Intentionally NOT clearing last_response_duration — it persists
        // to show timing in the status bar after streaming ends.
        // Intentionally NOT clearing queued_message — it's handled separately.
    }
}
