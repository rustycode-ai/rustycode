//! Reusable SSE (Server-Sent Events) line-buffered parser.
//!
//! Handles the common case where TCP chunks split SSE `data:` lines
//! across multiple reads. Maintains an internal buffer of incomplete
//! lines and only yields complete lines for parsing.

use std::sync::{Arc, Mutex};

/// Maximum SSE buffer size (1 MiB). Guards against malformed streams with no newlines.
const MAX_SSE_BUFFER: usize = 1 << 20;

/// Byte-level SSE line buffer that correctly handles multi-byte UTF-8 split across TCP chunks.
///
/// Unlike the legacy `SseLineBuffer` which converts each chunk to a string independently
/// (potentially corrupting multi-byte UTF-8 characters at chunk boundaries), this buffer
/// operates on raw bytes and only converts to String after splitting on complete `\n` boundaries.
///
/// This is safe because `\n` (0x0A) is a single-byte ASCII value that never appears inside
/// multi-byte UTF-8 continuation bytes (which are always 0x80–0xBF).
#[derive(Debug)]
pub struct SseByteBuffer(Arc<Mutex<Vec<u8>>>);

impl SseByteBuffer {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    /// Feed raw bytes, extract all complete lines as Strings.
    /// Incomplete trailing data (no trailing `\n`) is buffered for the next call.
    pub fn feed_chunk(&self, bytes: &[u8]) -> Vec<String> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.extend_from_slice(bytes);

        if state.len() > MAX_SSE_BUFFER {
            tracing::warn!("SSE buffer exceeded {} bytes, truncating", MAX_SSE_BUFFER);
            state.clear();
            return Vec::new();
        }

        // Find the last `\n` to split complete vs incomplete data.
        let newline_pos = state.iter().rposition(|&b| b == b'\n');
        let (complete, remainder) = match newline_pos {
            Some(pos) => {
                let complete = state[..pos].to_vec();
                let remainder = state[pos + 1..].to_vec();
                (complete, remainder)
            }
            None => return Vec::new(), // No complete lines yet
        };
        *state = remainder;

        complete
            .split(|&b| b == b'\n')
            .map(|line| {
                // Strip trailing \r
                let trimmed = line.strip_suffix(b"\r").unwrap_or(line);
                String::from_utf8_lossy(trimmed).into_owned()
            })
            .filter(|l| !l.is_empty())
            .collect()
    }
}

impl Clone for SseByteBuffer {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl Default for SseByteBuffer {
    fn default() -> Self {
        Self::new()
    }
}

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

    // === SseByteBuffer tests ===

    #[test]
    fn byte_buffer_complete_line_single_chunk() {
        let buf = SseByteBuffer::new();
        let lines = buf.feed_chunk(b"data: {\"text\":\"hello\"}\n\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "data: {\"text\":\"hello\"}");
    }

    #[test]
    fn byte_buffer_split_line_across_chunks() {
        let buf = SseByteBuffer::new();
        let lines1 = buf.feed_chunk(b"data: {\"text\":\"hel");
        assert!(lines1.is_empty()); // incomplete, buffered
        let lines2 = buf.feed_chunk(b"lo\"}\n\n");
        assert_eq!(lines2.len(), 1);
        assert_eq!(lines2[0], "data: {\"text\":\"hello\"}");
    }

    #[test]
    fn byte_buffer_multiple_lines_in_one_chunk() {
        let buf = SseByteBuffer::new();
        let lines = buf.feed_chunk(b"data: line1\n\ndata: line2\n\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "data: line1");
        assert_eq!(lines[1], "data: line2");
    }

    #[test]
    fn byte_buffer_carriage_return_stripped() {
        let buf = SseByteBuffer::new();
        let lines = buf.feed_chunk(b"data: test\r\n\r\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "data: test");
    }

    #[test]
    fn byte_buffer_buffer_overflow_protection() {
        let buf = SseByteBuffer::new();
        // Feed a huge string with no newlines
        let huge = vec![b'x'; MAX_SSE_BUFFER + 100];
        let lines = buf.feed_chunk(&huge);
        assert!(lines.is_empty());
        // Buffer should be cleared
        let lines2 = buf.feed_chunk(b"data: ok\n\n");
        assert_eq!(lines2.len(), 1);
    }

    #[test]
    fn byte_buffer_clone_shares_state() {
        let buf1 = SseByteBuffer::new();
        let buf2 = buf1.clone();
        let _ = buf1.feed_chunk(b"data: partial");
        let lines = buf2.feed_chunk(b" done\n\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "data: partial done");
    }

    #[test]
    fn byte_buffer_split_multibyte_utf8_across_chunks() {
        // "你好世界" encoded in UTF-8, split mid-character across chunks
        let full = "data: 你好世界\n\n";
        let full_bytes = full.as_bytes();

        // Find a split point inside a multi-byte character (after "好" which is 3 bytes)
        // "你" = E4 BD A0 (3 bytes), "好" = E5 A5 BD (3 bytes)
        // Split after the first byte of "好" (E5) so the chunk ends mid-character
        let prefix = "data: 你"; // "data: " + "你" = 6 + 3 = 9 bytes
        let prefix_bytes = prefix.as_bytes();
        // Next character is "好" (E5 A5 BD). Split after E5 (byte 9)
        let split_at = prefix_bytes.len() + 1; // Inside "好"'s first byte is at index 9, so split at 10

        let buf = SseByteBuffer::new();
        let lines1 = buf.feed_chunk(&full_bytes[..split_at]);
        assert!(lines1.is_empty(), "No complete line yet");

        let lines2 = buf.feed_chunk(&full_bytes[split_at..]);
        assert_eq!(
            lines2.len(),
            1,
            "Should have one complete line after second chunk"
        );
        assert_eq!(
            lines2[0], "data: 你好世界",
            "Multi-byte UTF-8 should be preserved across chunk boundaries"
        );
    }

    #[test]
    fn byte_buffer_split_emoji_across_chunks() {
        // "🌍" is 4 bytes: F0 9F 8C 8D
        let full = "data: hello🌍world\n\n";
        let full_bytes = full.as_bytes();

        // Find where the emoji starts
        let hello_prefix = "data: hello";
        let emoji_start = hello_prefix.len();

        // Split after first byte of emoji (F0)
        let buf = SseByteBuffer::new();
        let lines1 = buf.feed_chunk(&full_bytes[..=emoji_start]);
        assert!(lines1.is_empty());

        let lines2 = buf.feed_chunk(&full_bytes[emoji_start + 1..]);
        assert_eq!(lines2.len(), 1);
        assert_eq!(
            lines2[0], "data: hello🌍world",
            "4-byte emoji should be preserved across chunk boundaries"
        );
    }

    #[test]
    fn byte_buffer_empty_chunks() {
        let buf = SseByteBuffer::new();
        let lines = buf.feed_chunk(b"");
        assert!(lines.is_empty());
    }

    #[test]
    fn byte_buffer_no_newline_returns_empty() {
        let buf = SseByteBuffer::new();
        let lines = buf.feed_chunk(b"data: incomplete no newline");
        assert!(lines.is_empty());
    }

    // === Comprehensive Unicode / binary / image tests ===

    #[test]
    fn byte_buffer_russian_cyrillic_across_chunks() {
        // Russian: "Привет мир" (Hello world) — each char is 2 bytes UTF-8
        let full = "data: Привет мир\n\n";
        let full_bytes = full.as_bytes();
        // "data: " is 6 bytes. "П" is D0 9F (2 bytes). Split after D0 (incomplete char).
        let buf = SseByteBuffer::new();
        let lines1 = buf.feed_chunk(&full_bytes[..7]); // "data: \xD0" — incomplete "П"
        assert!(lines1.is_empty());
        let lines2 = buf.feed_chunk(&full_bytes[7..]);
        assert_eq!(lines2.len(), 1);
        assert_eq!(lines2[0], "data: Привет мир");
    }

    #[test]
    fn byte_buffer_chinese_across_chunks() {
        // Chinese: "数据" — each char is 3 bytes
        let full = "data: 数据处理\n\n";
        let full_bytes = full.as_bytes();
        let prefix = "data: ";
        let split_at = prefix.len() + 1; // Inside first byte of "数" (E6 95 B0)

        let buf = SseByteBuffer::new();
        let lines1 = buf.feed_chunk(&full_bytes[..split_at]);
        assert!(lines1.is_empty());
        let lines2 = buf.feed_chunk(&full_bytes[split_at..]);
        assert_eq!(lines2.len(), 1);
        assert_eq!(lines2[0], "data: 数据处理");
    }

    #[test]
    fn byte_buffer_japanese_katakana_across_chunks() {
        // Japanese: "データ" — katakana chars are 3 bytes each
        let full = "data: データ\n\n";
        let full_bytes = full.as_bytes();
        // Split mid-character of "ー" (E3 83 BC)
        let prefix = "data: デ";
        let split_at = prefix.len() + 2; // Inside "ー", after first 2 bytes

        let buf = SseByteBuffer::new();
        let lines1 = buf.feed_chunk(&full_bytes[..split_at]);
        assert!(lines1.is_empty());
        let lines2 = buf.feed_chunk(&full_bytes[split_at..]);
        assert_eq!(lines2.len(), 1);
        assert_eq!(lines2[0], "data: データ");
    }

    #[test]
    fn byte_buffer_emoji_family_across_chunks() {
        // "👨‍👩‍👧‍👦" is a complex emoji sequence (7 bytes per base + ZWJ + other chars)
        let full = "data: hello 👨‍👩‍👧‍👦 world\n\n";
        let full_bytes = full.as_bytes();
        // Split somewhere in the middle of the emoji sequence
        let split_at = "data: hello ".len() + 4;

        let buf = SseByteBuffer::new();
        let _ = buf.feed_chunk(&full_bytes[..split_at]);
        let lines = buf.feed_chunk(&full_bytes[split_at..]);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "data: hello 👨‍👩‍👧‍👦 world");
    }

    #[test]
    fn byte_buffer_multiple_multibyte_splits() {
        // Simulate real SSE with JSON containing mixed scripts
        let payload = r#"data: {"content":"Hello 世界 🌍 Привет 日本語"}"#;
        let full = format!("{payload}\n\n");
        let full_bytes = full.as_bytes();

        // Split into 3 chunks at arbitrary byte boundaries
        let third = full_bytes.len() / 3;
        let buf = SseByteBuffer::new();
        let l1 = buf.feed_chunk(&full_bytes[..third]);
        assert!(l1.is_empty());
        let l2 = buf.feed_chunk(&full_bytes[third..third * 2]);
        assert!(l2.is_empty());
        let l3 = buf.feed_chunk(&full_bytes[third * 2..]);
        assert_eq!(l3.len(), 1);
        assert_eq!(l3[0], payload);
    }

    #[test]
    fn byte_buffer_binary_null_bytes_in_json() {
        // JSON with escaped unicode containing null-like patterns
        let full = "data: {\"content\":\"test\\u0000value\"}\n\n";
        let buf = SseByteBuffer::new();
        let lines = buf.feed_chunk(full.as_bytes());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "data: {\"content\":\"test\\u0000value\"}");
    }

    #[test]
    fn byte_buffer_base64_image_data_across_chunks() {
        // Simulate tool call with base64-encoded image data in arguments
        let b64_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let full = format!("data: {{\"tool_calls\":[{{\"function\":{{\"arguments\":\"{{\\\"content_base64\\\":\\\"{b64_data}\\\"}}\"}}}}]}}\n\n");
        let full_bytes = full.as_bytes();

        // Split in the middle of the base64 string
        let mid = full_bytes.len() / 2;
        let buf = SseByteBuffer::new();
        let l1 = buf.feed_chunk(&full_bytes[..mid]);
        assert!(l1.is_empty());
        let l2 = buf.feed_chunk(&full_bytes[mid..]);
        assert_eq!(l2.len(), 1);
        assert!(l2[0].contains("iVBORw0KGgo"));
    }

    #[test]
    fn byte_buffer_many_rapid_small_chunks() {
        // Simulate slow network with many 1-2 byte chunks
        let full = "data: 你好🌍世界\n\n";
        let full_bytes = full.as_bytes();
        let buf = SseByteBuffer::new();

        let mut received = Vec::new();
        for i in 0..full_bytes.len() {
            let lines = buf.feed_chunk(&full_bytes[i..=i]);
            received.extend(lines);
        }
        assert_eq!(received.len(), 1);
        assert_eq!(received[0], "data: 你好🌍世界");
    }

    #[test]
    fn byte_buffer_preserves_valid_non_ascii_in_single_chunk() {
        // Single chunk with various non-ASCII — should pass through unchanged
        let content = "data: Ångström résumé naïve café\n\n";
        let buf = SseByteBuffer::new();
        let lines = buf.feed_chunk(content.as_bytes());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "data: Ångström résumé naïve café");
    }

    #[test]
    fn byte_buffer_mixed_sse_lines_with_unicode() {
        // Multiple SSE events in one chunk with mixed languages
        let chunk = "data: line1 日本語\n\ndata: line2 한국어\n\ndata: line3 العربية\n\n";
        let buf = SseByteBuffer::new();
        let lines = buf.feed_chunk(chunk.as_bytes());
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "data: line1 日本語");
        assert_eq!(lines[1], "data: line2 한국어");
        assert_eq!(lines[2], "data: line3 العربية");
    }
}
