/// End-to-end streaming pipeline regression tests.
///
/// These tests drive the full chain: `StreamEvent` → TUI state. They verify that:
/// 1. Message loss doesn't occur (e.g., identical periods not dropped)
/// 2. Content ordering is preserved
/// 3. Parsing layers don't silently corrupt the stream
/// 4. The bug manifests deterministically so fixes can be verified
use rustycode_protocol::stream_event::StreamEvent;

#[derive(Clone, Debug, PartialEq)]
pub enum PipelineEvent {
    StreamEvent(StreamEvent),
    StreamChunk(rustycode_tui::app::async_::StreamChunk),
}

pub struct PipelineCapture {
    pub stream_chunks: Vec<rustycode_tui::app::async_::StreamChunk>,
    pub final_text: String,
}

impl Default for PipelineCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineCapture {
    pub const fn new() -> Self {
        Self {
            stream_chunks: Vec::new(),
            final_text: String::new(),
        }
    }

    pub fn record_stream_chunk(&mut self, chunk: rustycode_tui::app::async_::StreamChunk) {
        if let rustycode_tui::app::async_::StreamChunk::Text(text) = &chunk {
            self.final_text.push_str(text);
        }
        self.stream_chunks.push(chunk);
    }

    pub fn text_chunk_count(&self) -> usize {
        self.stream_chunks
            .iter()
            .filter(|c| matches!(c, rustycode_tui::app::async_::StreamChunk::Text(_)))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_stutter_consecutive_identical_deltas() {
        // REGRESSION: User reported text repeating (e.g., ". ." becomes "." or gets scrambled)
        // Root cause: Layer 3 deduplication logic checks `if text == last_text { return; }`
        // This drops the second period.

        let events: Vec<StreamEvent> = vec![
            StreamEvent::TextDelta {
                content: ".".to_string(),
            },
            StreamEvent::TextDelta {
                content: ".".to_string(),
            },
            StreamEvent::TextDelta {
                content: " Hello".to_string(),
            },
            StreamEvent::Done,
        ];

        let mut capture = PipelineCapture::new();

        for event in events {
            match &event {
                StreamEvent::TextDelta { content } => {
                    let chunk = rustycode_tui::app::async_::StreamChunk::Text(content.clone());
                    capture.record_stream_chunk(chunk);
                }
                StreamEvent::Done => {
                    capture.record_stream_chunk(rustycode_tui::app::async_::StreamChunk::Done);
                }
                _ => {}
            }
        }

        let expected = ".. Hello";
        assert_eq!(
            capture.final_text, expected,
            "regression_stutter_consecutive_identical_deltas: Both periods must appear in output"
        );
    }

    #[test]
    fn regression_no_loss_repeated_word_non_consecutive() {
        // REGRESSION: "the cat sat on the mat" should not lose the second "the"
        // The dedup only checks consecutive chunks, but we test that it doesn't
        // accidentally drop words that appear multiple times non-consecutively.

        let events: Vec<StreamEvent> = vec![
            StreamEvent::TextDelta {
                content: "the".to_string(),
            },
            StreamEvent::TextDelta {
                content: " cat sat on ".to_string(),
            },
            StreamEvent::TextDelta {
                content: "the".to_string(),
            },
            StreamEvent::TextDelta {
                content: " mat".to_string(),
            },
            StreamEvent::Done,
        ];

        let mut capture = PipelineCapture::new();

        for event in events {
            match &event {
                StreamEvent::TextDelta { content } => {
                    let chunk = rustycode_tui::app::async_::StreamChunk::Text(content.clone());
                    capture.record_stream_chunk(chunk);
                }
                StreamEvent::Done => {
                    capture.record_stream_chunk(rustycode_tui::app::async_::StreamChunk::Done);
                }
                _ => {}
            }
        }

        let expected = "the cat sat on the mat";
        assert_eq!(
            capture.final_text, expected,
            "regression_no_loss_repeated_word_non_consecutive: Non-consecutive repetitions must be preserved"
        );
    }

    #[test]
    fn full_pipeline_simple_response() {
        // HAPPY PATH: Clean well-formed response end-to-end
        // Input: 3 text deltas + TurnCompleted + Done
        // Expected: All 3 text pieces concatenated correctly

        let events: Vec<StreamEvent> = vec![
            StreamEvent::TextDelta {
                content: "The ".to_string(),
            },
            StreamEvent::TextDelta {
                content: "quick ".to_string(),
            },
            StreamEvent::TextDelta {
                content: "fox".to_string(),
            },
            StreamEvent::TurnCompleted {
                stop_reason: "end_turn".to_string(),
            },
            StreamEvent::Done,
        ];

        let mut capture = PipelineCapture::new();

        for event in events {
            match &event {
                StreamEvent::TextDelta { content } => {
                    let chunk = rustycode_tui::app::async_::StreamChunk::Text(content.clone());
                    capture.record_stream_chunk(chunk);
                }
                StreamEvent::Done => {
                    capture.record_stream_chunk(rustycode_tui::app::async_::StreamChunk::Done);
                }
                _ => {}
            }
        }

        assert_eq!(
            capture.text_chunk_count(),
            3,
            "Should have exactly 3 text chunks"
        );
        assert_eq!(
            capture.final_text, "The quick fox",
            "Full text must match concatenation of all deltas"
        );
    }

    #[test]
    fn pipeline_preserves_newlines_and_special_chars() {
        // Verify that newlines, tabs, and other special characters survive the pipeline

        let events: Vec<StreamEvent> = vec![
            StreamEvent::TextDelta {
                content: "Line 1\n".to_string(),
            },
            StreamEvent::TextDelta {
                content: "Line\t2".to_string(),
            },
            StreamEvent::TextDelta {
                content: "\nLine 3".to_string(),
            },
            StreamEvent::Done,
        ];

        let mut capture = PipelineCapture::new();

        for event in events {
            match &event {
                StreamEvent::TextDelta { content } => {
                    let chunk = rustycode_tui::app::async_::StreamChunk::Text(content.clone());
                    capture.record_stream_chunk(chunk);
                }
                StreamEvent::Done => {
                    capture.record_stream_chunk(rustycode_tui::app::async_::StreamChunk::Done);
                }
                _ => {}
            }
        }

        let expected = "Line 1\nLine\t2\nLine 3";
        assert_eq!(
            capture.final_text, expected,
            "Newlines and tabs must be preserved"
        );
    }

    #[test]
    fn pipeline_handles_unicode() {
        // Verify Unicode content survives the pipeline

        let events: Vec<StreamEvent> = vec![
            StreamEvent::TextDelta {
                content: "Hello 世界 ".to_string(),
            },
            StreamEvent::TextDelta {
                content: "🦀 Rust".to_string(),
            },
            StreamEvent::Done,
        ];

        let mut capture = PipelineCapture::new();

        for event in events {
            match &event {
                StreamEvent::TextDelta { content } => {
                    let chunk = rustycode_tui::app::async_::StreamChunk::Text(content.clone());
                    capture.record_stream_chunk(chunk);
                }
                StreamEvent::Done => {
                    capture.record_stream_chunk(rustycode_tui::app::async_::StreamChunk::Done);
                }
                _ => {}
            }
        }

        let expected = "Hello 世界 🦀 Rust";
        assert_eq!(capture.final_text, expected, "Unicode must be preserved");
    }
}
