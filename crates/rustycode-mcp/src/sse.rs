//! Server-Sent Events (SSE) parser for MCP Streamable HTTP transport.
//!
//! Implements the W3C SSE parsing algorithm for streaming consumption.
//! Feed chunks as they arrive; complete events are returned from `feed()`.

/// A parsed SSE event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// Event type (defaults to `"message"` per SSE spec).
    pub event_type: String,
    /// Event data (multiple `data:` fields joined with `\n`).
    pub data: String,
    /// Last event ID (from `id:` field).
    pub id: Option<String>,
    /// Server-suggested reconnect interval in milliseconds (from `retry:` field).
    pub retry: Option<u64>,
}

/// Streaming SSE parser. Feed chunks incrementally; extract complete events.
pub struct SseParser {
    event_type: String,
    data_lines: Vec<String>,
    last_event_id: Option<String>,
    retry: Option<u64>,
    buffer: String,
    saw_first_chunk: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            event_type: String::new(),
            data_lines: Vec::new(),
            last_event_id: None,
            retry: None,
            buffer: String::new(),
            saw_first_chunk: false,
        }
    }

    /// Feed a chunk of SSE text. Returns any complete events parsed from it.
    pub fn feed(&mut self, chunk: &str) -> Vec<SseEvent> {
        let mut events = Vec::new();

        // Strip BOM from the very first chunk
        if self.saw_first_chunk {
            self.buffer.push_str(chunk);
        } else {
            self.buffer
                .push_str(chunk.strip_prefix('\u{feff}').unwrap_or(chunk));
            self.saw_first_chunk = true;
        }

        while let Some(line_end) = self.find_line_end() {
            let line = self.buffer[..line_end].to_string();
            let sep_len = self.line_separator_len(line_end);
            self.buffer = self.buffer[line_end + sep_len..].to_string();

            self.process_line(&line, &mut events);
        }

        events
    }

    /// Flush any buffered partial event. Call when the stream ends.
    pub fn flush(&mut self) -> Option<SseEvent> {
        if !self.buffer.is_empty() {
            // Process remaining buffer as a final line
            let mut events = Vec::new();
            let remaining = std::mem::take(&mut self.buffer);
            self.process_line(&remaining, &mut events);
            return events.into_iter().next();
        }
        self.take_event()
    }

    /// Returns the most recent retry interval seen.
    pub fn retry_interval(&self) -> Option<u64> {
        self.retry
    }

    fn process_line(&mut self, line: &str, events: &mut Vec<SseEvent>) {
        if line.is_empty() {
            // Empty line dispatches the current event
            if let Some(event) = self.take_event() {
                events.push(event);
            }
            return;
        }

        if line.starts_with(':') {
            // Comment, ignore
            return;
        }

        let (field, value) = match line.find(':') {
            Some(pos) => {
                let value = &line[pos + 1..];
                // Per spec: strip single leading space from value
                let value = value.strip_prefix(' ').unwrap_or(value);
                (&line[..pos], value)
            }
            None => {
                // No colon: field name is the whole line, value is empty
                (line, "")
            }
        };

        match field {
            "event" => self.event_type = value.to_string(),
            "data" => self.data_lines.push(value.to_string()),
            "id" => {
                // Per spec: ignore id if it contains null
                if !value.contains('\0') {
                    self.last_event_id = Some(value.to_string());
                }
            }
            "retry" => {
                if let Ok(ms) = value.parse::<u64>() {
                    self.retry = Some(ms);
                }
            }
            _ => {} // Ignore unknown fields
        }
    }

    /// Build and return the current event, resetting accumulators.
    /// Returns None if data buffer is empty (per SSE spec).
    fn take_event(&mut self) -> Option<SseEvent> {
        let data = self.data_lines.join("\n");
        self.data_lines.clear();

        let event_type = if self.event_type.is_empty() {
            "message".to_string()
        } else {
            std::mem::take(&mut self.event_type)
        };

        // Per SSE spec: do not fire events with empty data
        if data.is_empty() {
            self.event_type.clear();
            return None;
        }

        Some(SseEvent {
            event_type,
            data,
            id: self.last_event_id.take(),
            retry: self.retry,
        })
    }

    /// Find the position of the next line separator (\n, \r, or \r\n).
    fn find_line_end(&self) -> Option<usize> {
        let bytes = self.buffer.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\r' || b == b'\n' {
                return Some(i);
            }
        }
        None
    }

    /// Return the byte length of the line separator at the given position.
    fn line_separator_len(&self, pos: usize) -> usize {
        let bytes = self.buffer.as_bytes();
        if bytes[pos] == b'\r' {
            if bytes.get(pos + 1) == Some(&b'\n') {
                2 // \r\n
            } else {
                1 // \r alone
            }
        } else {
            1 // \n
        }
    }
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_data_line_event() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
        assert_eq!(events[0].event_type, "message");
    }

    #[test]
    fn multi_line_data_joined() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: line1\ndata: line2\ndata: line3\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2\nline3");
    }

    #[test]
    fn multiple_events_in_one_chunk() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: first\n\ndata: second\n\n");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "first");
        assert_eq!(events[1].data, "second");
    }

    #[test]
    fn event_type_field() {
        let mut parser = SseParser::new();
        let events = parser.feed("event: custom\ndata: payload\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "custom");
        assert_eq!(events[0].data, "payload");
    }

    #[test]
    fn id_field() {
        let mut parser = SseParser::new();
        let events = parser.feed("id: 42\ndata: hello\n\n");
        assert_eq!(events[0].id.as_deref(), Some("42"));
    }

    #[test]
    fn retry_field() {
        let mut parser = SseParser::new();
        let events = parser.feed("retry: 5000\ndata: hello\n\n");
        assert_eq!(events[0].retry, Some(5000));
    }

    #[test]
    fn comment_lines_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed(": this is a comment\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn empty_data_event_skipped() {
        let mut parser = SseParser::new();
        let events = parser.feed("data:\n\n");
        assert!(events.is_empty());
    }

    #[test]
    fn no_value_field() {
        // Field with no colon after value gets empty value
        let mut parser = SseParser::new();
        let events = parser.feed("data\n\n");
        // Empty data → event skipped
        assert!(events.is_empty());
    }

    #[test]
    fn value_no_leading_space() {
        // "data:value" → value is "value" (no space to strip)
        let mut parser = SseParser::new();
        let events = parser.feed("data:nospace\n\n");
        assert_eq!(events[0].data, "nospace");
    }

    #[test]
    fn value_leading_space_stripped() {
        // "data: value" → value is "value" (single leading space stripped)
        let mut parser = SseParser::new();
        let events = parser.feed("data: hello\n\n");
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn value_multiple_spaces_preserved() {
        // "data:  two spaces" → only first space stripped → " two spaces"
        let mut parser = SseParser::new();
        let events = parser.feed("data:  two spaces\n\n");
        assert_eq!(events[0].data, " two spaces");
    }

    #[test]
    fn fragmented_chunks() {
        let mut parser = SseParser::new();
        let mut events = Vec::new();

        events.extend(parser.feed("dat"));
        assert!(events.is_empty());

        events.extend(parser.feed("a: hel"));
        assert!(events.is_empty());

        events.extend(parser.feed("lo\n\n"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn crlf_line_endings() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: hello\r\n\r\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn cr_only_line_endings() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: hello\r\r");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn bom_stripped_from_first_chunk() {
        let mut parser = SseParser::new();
        let events = parser.feed("\u{feff}data: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn bom_only_stripped_from_first_chunk() {
        let mut parser = SseParser::new();
        let mut events = Vec::new();

        // First chunk with BOM — stripped, produces valid event
        events.extend(parser.feed("\u{feff}data: first\n\n"));
        // Second chunk — BOM NOT stripped, field name becomes "\u{feff}data" (unknown)
        // No "data:" lines accumulated → event skipped
        events.extend(parser.feed("\u{feff}data: second\n\n"));

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "first");
    }

    #[test]
    fn null_in_id_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed("id: bad\u{0}id\ndata: hello\n\n");
        assert_eq!(events[0].id, None);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn invalid_retry_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed("retry: notanumber\ndata: hello\n\n");
        assert_eq!(events[0].retry, None);
    }

    #[test]
    fn unknown_fields_ignored() {
        let mut parser = SseParser::new();
        let events = parser.feed("foo: bar\ndata: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn flush_pending_event() {
        let mut parser = SseParser::new();
        parser.feed("data: hello\n");
        // No empty line yet, so no event dispatched
        let event = parser.flush();
        assert!(event.is_some());
        assert_eq!(event.unwrap().data, "hello");
    }

    #[test]
    fn flush_empty_does_nothing() {
        let mut parser = SseParser::new();
        assert!(parser.flush().is_none());
    }

    #[test]
    fn retry_interval_persists_across_events() {
        let mut parser = SseParser::new();
        parser.feed("retry: 3000\ndata: first\n\ndata: second\n\n");
        assert_eq!(parser.retry_interval(), Some(3000));
    }

    #[test]
    fn default_impl() {
        let mut parser = SseParser::default();
        let events = parser.feed("data: test\n\n");
        assert_eq!(events.len(), 1);
    }
}
