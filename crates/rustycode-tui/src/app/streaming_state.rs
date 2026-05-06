use std::sync::Arc;
use std::time::{Duration, Instant};

#[non_exhaustive]
pub(crate) struct StreamingState {
    pub(crate) is_streaming: bool,
    /// Set by Esc/Ctrl+C to cooperatively cancel the stream.
    pub(crate) stream_cancelled: bool,
    pub(crate) chunks_received: usize,
    pub(crate) thinking_chunks_received: usize,
    pub(crate) current_stream_content: String,
    pub(crate) streaming_render_buffer: crate::app::streaming_render_buffer::StreamingRenderBuffer,
    pub(crate) queued_message: Option<String>,
    pub(crate) pending_bash_result: Arc<std::sync::Mutex<Option<String>>>,
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

    pub(crate) fn begin_streaming(&mut self) {
        self.is_streaming = true;
        self.chunks_received = 0;
        self.thinking_chunks_received = 0;
        self.stream_start_time = Some(Instant::now());
        self.current_stream_content.clear();
        self.streaming_render_buffer =
            crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
    }

    pub(crate) fn is_active(&self) -> bool {
        self.is_streaming
    }

    pub(crate) fn cancel(&mut self) {
        self.stream_cancelled = true;
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.stream_cancelled
    }

    pub(crate) fn elapsed(&self) -> Option<Duration> {
        self.stream_start_time.map(|t| t.elapsed())
    }

    pub(crate) fn record_chunk(&mut self) {
        self.chunks_received = self.chunks_received.saturating_add(1);
    }

    pub(crate) fn record_thinking_chunk(&mut self) {
        self.thinking_chunks_received = self.thinking_chunks_received.saturating_add(1);
    }

    pub(crate) fn append_content(&mut self, text: &str) {
        self.current_stream_content.push_str(text);
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
    fn is_active_reflects_streaming_state() {
        let mut state = StreamingState::new();
        assert!(!state.is_active());
        state.begin_streaming();
        assert!(state.is_active());
        state.reset();
        assert!(!state.is_active());
    }

    #[test]
    fn cancel_sets_flag() {
        let mut state = StreamingState::new();
        assert!(!state.is_cancelled());
        state.cancel();
        assert!(state.is_cancelled());
    }

    #[test]
    fn elapsed_none_when_not_streaming() {
        let state = StreamingState::new();
        assert!(state.elapsed().is_none());
    }

    #[test]
    fn elapsed_some_when_streaming() {
        let mut state = StreamingState::new();
        state.begin_streaming();
        assert!(state.elapsed().is_some());
    }

    #[test]
    fn record_chunk_increments() {
        let mut state = StreamingState::new();
        state.record_chunk();
        state.record_chunk();
        state.record_chunk();
        assert_eq!(state.chunks_received, 3);
    }

    #[test]
    fn record_thinking_chunk_increments() {
        let mut state = StreamingState::new();
        state.record_thinking_chunk();
        state.record_thinking_chunk();
        assert_eq!(state.thinking_chunks_received, 2);
    }

    #[test]
    fn append_content_accumulates() {
        let mut state = StreamingState::new();
        state.append_content("hello ");
        state.append_content("world");
        assert_eq!(state.current_stream_content, "hello world");
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
