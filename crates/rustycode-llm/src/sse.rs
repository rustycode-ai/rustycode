//! Reusable SSE (Server-Sent Events) line-buffered parser.
//!
//! Handles the common case where TCP chunks split SSE `data:` lines
//! across multiple reads. Maintains an internal buffer of incomplete
//! lines and only yields complete lines for parsing.

use std::sync::{Arc, Mutex};

/// Maximum SSE buffer size (1 MiB). Guards against malformed streams with no newlines.
const MAX_SSE_BUFFER: usize = 1 << 20;

/// Shared mutable state for the SSE line buffer.
/// Uses `Arc<Mutex<>>` so it can be captured by value in `map` closures.
#[derive(Debug)]
pub struct SseLineBuffer(Arc<Mutex<String>>);

impl SseLineBuffer {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(String::new())))
    }

    /// Feed a raw chunk, extract all complete lines, returning them as a Vec.
    /// Incomplete trailing data is buffered for the next call.
    pub fn feed_chunk(&self, raw: &str) -> Vec<String> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.push_str(raw);

        if state.len() > MAX_SSE_BUFFER {
            tracing::warn!("SSE buffer exceeded {} bytes, truncating", MAX_SSE_BUFFER);
            state.clear();
            return Vec::new();
        }

        let buffer = std::mem::take(&mut *state);
        let (complete_lines, remainder) = match buffer.rfind('\n') {
            Some(pos) => (buffer[..pos].to_string(), buffer[pos + 1..].to_string()),
            None => (String::new(), buffer),
        };
        *state = remainder;

        complete_lines
            .lines()
            .map(|l| l.trim_end_matches('\r').to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }
}

impl Clone for SseLineBuffer {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl Default for SseLineBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_line_single_chunk() {
        let buf = SseLineBuffer::new();
        let lines = buf.feed_chunk("data: {\"text\":\"hello\"}\n\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "data: {\"text\":\"hello\"}");
    }

    #[test]
    fn test_split_line_across_chunks() {
        let buf = SseLineBuffer::new();
        let lines1 = buf.feed_chunk("data: {\"text\":\"hel");
        assert!(lines1.is_empty()); // incomplete, buffered
        let lines2 = buf.feed_chunk("lo\"}\n\n");
        assert_eq!(lines2.len(), 1);
        assert_eq!(lines2[0], "data: {\"text\":\"hello\"}");
    }

    #[test]
    fn test_multiple_lines_in_one_chunk() {
        let buf = SseLineBuffer::new();
        let lines = buf.feed_chunk("data: line1\n\ndata: line2\n\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "data: line1");
        assert_eq!(lines[1], "data: line2");
    }

    #[test]
    fn test_empty_chunks() {
        let buf = SseLineBuffer::new();
        let lines = buf.feed_chunk("");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_carriage_return_stripped() {
        let buf = SseLineBuffer::new();
        let lines = buf.feed_chunk("data: test\r\n\r\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "data: test");
    }

    #[test]
    fn test_buffer_overflow_protection() {
        let buf = SseLineBuffer::new();
        // Feed a huge string with no newlines
        let huge = "x".repeat(MAX_SSE_BUFFER + 100);
        let lines = buf.feed_chunk(&huge);
        assert!(lines.is_empty());
        // Buffer should be cleared
        let lines2 = buf.feed_chunk("data: ok\n\n");
        assert_eq!(lines2.len(), 1);
    }

    #[test]
    fn test_clone_shares_state() {
        let buf1 = SseLineBuffer::new();
        let buf2 = buf1.clone();
        let _ = buf1.feed_chunk("data: partial");
        let lines = buf2.feed_chunk(" done\n\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "data: partial done");
    }
}
