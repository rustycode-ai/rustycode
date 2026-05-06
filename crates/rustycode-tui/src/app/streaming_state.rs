//! Streaming state sub-struct for the TUI.

use std::sync::Arc;
use std::time::{Duration, Instant};

/// Streaming-related state for the TUI.
///
/// All fields here track the lifecycle of an active LLM response stream,
/// from the first chunk received through completion.
#[non_exhaustive]
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

    /// Prepare state for a new stream, resetting counters and buffers.
    ///
    /// Call this before initiating a new LLM response stream.
    pub(crate) fn begin_streaming(&mut self) {
        self.is_streaming = true;
        self.chunks_received = 0;
        self.thinking_chunks_received = 0;
        self.stream_start_time = Some(Instant::now());
        self.current_stream_content.clear();
        self.streaming_render_buffer =
            crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_idle() {
        let state = StreamingState::new();
        assert!(!state.is_streaming);
        assert!(!state.stream_cancelled);
        assert_eq!(state.chunks_received, 0);
        assert_eq!(state.thinking_chunks_received, 0);
        assert!(state.current_stream_content.is_empty());
        assert!(state.queued_message.is_none());
        assert!(state.stream_start_time.is_none());
        assert!(state.last_response_duration.is_none());
    }

    #[test]
    fn reset_preserves_last_response_duration() {
        let mut state = StreamingState::new();
        state.last_response_duration = Some(Duration::from_secs(5));

        state.reset();

        assert_eq!(state.last_response_duration, Some(Duration::from_secs(5)));
    }

    #[test]
    fn reset_preserves_queued_message() {
        let mut state = StreamingState::new();
        state.queued_message = Some("hello".into());

        state.reset();

        assert_eq!(state.queued_message.as_deref(), Some("hello"));
    }

    #[test]
    fn reset_clears_streaming_fields() {
        let mut state = StreamingState::new();
        state.is_streaming = true;
        state.stream_cancelled = true;
        state.chunks_received = 42;
        state.thinking_chunks_received = 10;
        state.current_stream_content = "some content".into();
        state.stream_start_time = Some(Instant::now());

        state.reset();

        assert!(!state.is_streaming);
        assert!(!state.stream_cancelled);
        assert_eq!(state.chunks_received, 0);
        assert_eq!(state.thinking_chunks_received, 0);
        assert!(state.current_stream_content.is_empty());
        assert!(state.stream_start_time.is_none());
    }
}
