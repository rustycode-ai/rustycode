//! Streaming Markdown Processor
//!
//! Handles partial markdown deltas for real-time TUI rendering.
//! Processes incomplete markdown (e.g., unclosed code blocks, tables)
//! that arrives during LLM streaming.
//!
//! Inspired by forgecode's `forge_markdown_stream`.

/// A chunk of processed markdown
#[derive(Debug, Clone)]
pub struct MarkdownChunk {
    /// The raw content received
    pub raw: String,
    /// The processed/sanitized content for display
    pub display: String,
    /// Whether this completes a markdown element
    pub is_complete: bool,
    /// Type of element being streamed
    pub element_type: MarkdownElement,
}

/// Types of markdown elements
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MarkdownElement {
    /// Regular paragraph text
    Text,
    /// Code block (possibly incomplete)
    CodeBlock {
        language: Option<String>,
        closed: bool,
    },
    /// Inline code
    InlineCode,
    /// Header
    Header { level: usize },
    /// List item
    ListItem,
    /// Table (possibly incomplete)
    Table { rows: usize, closed: bool },
    /// Block quote
    BlockQuote,
    /// Unknown/incomplete
    Unknown,
}

/// State tracked during streaming
#[derive(Debug, Default)]
struct StreamState {
    /// Accumulated buffer
    buffer: String,
    /// Whether we're inside a code block
    in_code_block: bool,
    code_block_language: Option<String>,
    /// Backtick count for current code fence
    code_fence_len: usize,
    /// Whether we're inside a table
    in_table: bool,
    /// Table row count
    table_rows: usize,
}

/// Streaming markdown processor
#[derive(Debug)]
pub struct MarkdownStream {
    state: StreamState,
}

impl Default for MarkdownStream {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownStream {
    pub fn new() -> Self {
        Self {
            state: StreamState::default(),
        }
    }

    /// Process an incoming text delta from LLM streaming
    pub fn push(&mut self, delta: &str) -> MarkdownChunk {
        self.state.buffer.push_str(delta);
        self.update_state();

        let element_type = self.detect_element_type();
        let is_complete = self.is_element_complete();
        let display = self.sanitize_for_display();

        MarkdownChunk {
            raw: delta.to_string(),
            display,
            is_complete,
            element_type,
        }
    }

    /// Get the full accumulated content
    pub fn content(&self) -> &str {
        &self.state.buffer
    }

    /// Reset state for a new stream
    pub fn reset(&mut self) {
        self.state = StreamState::default();
    }

    /// Check if we're currently in an incomplete element
    pub const fn is_incomplete(&self) -> bool {
        self.state.in_code_block || self.state.in_table
    }

    /// Get a sanitized version suitable for display
    /// Closes unclosed code blocks and tables
    pub fn sanitized_output(&self) -> String {
        let mut output = self.state.buffer.clone();

        // Close unclosed code blocks
        if self.state.in_code_block {
            output.push_str(&"`".repeat(self.state.code_fence_len));
            output.push('\n');
        }

        output
    }

    fn detect_element_type(&self) -> MarkdownElement {
        let last_line = self.state.buffer.lines().last().unwrap_or("");

        if self.state.in_code_block {
            return MarkdownElement::CodeBlock {
                language: self.state.code_block_language.clone(),
                closed: false,
            };
        }

        // Check for code block start
        if last_line.starts_with("```") {
            let lang = last_line.strip_prefix("```").map(|s| s.trim().to_string());
            return MarkdownElement::CodeBlock {
                language: lang,
                closed: false,
            };
        }

        // Check for headers
        let header_level = last_line.chars().take_while(|c| *c == '#').count();
        if header_level > 0 && header_level <= 6 && last_line.chars().nth(header_level) == Some(' ')
        {
            return MarkdownElement::Header {
                level: header_level,
            };
        }

        // Check for list items
        if last_line.trim_start().starts_with("- ") || last_line.trim_start().starts_with("* ") {
            return MarkdownElement::ListItem;
        }

        // Check for tables
        if last_line.contains('|') && last_line.trim().starts_with('|') {
            return MarkdownElement::Table {
                rows: self.state.table_rows,
                closed: !self.state.in_table,
            };
        }

        // Check for block quotes
        if last_line.starts_with("> ") {
            return MarkdownElement::BlockQuote;
        }

        MarkdownElement::Text
    }

    fn is_element_complete(&self) -> bool {
        // Code blocks are complete when closed
        if self.state.in_code_block {
            // Check if the last line closes the code block
            let last_line = self.state.buffer.lines().last().unwrap_or("");
            if last_line
                .trim()
                .starts_with(&"`".repeat(self.state.code_fence_len))
                && last_line.trim().len() == self.state.code_fence_len
            {
                return true;
            }
            return false;
        }

        // Check for code block opening
        let last_line = self.state.buffer.lines().last().unwrap_or("");
        if let Some(rest) = last_line.strip_prefix("```") {
            if !rest.contains('`') {
                let fence_len = last_line.chars().take_while(|c| *c == '`').count();
                if fence_len >= 3 {
                    return false;
                }
            }
        }

        true
    }

    /// Update parsing state by scanning the buffer for code fences and table markers.
    fn update_state(&mut self) {
        // Reset tracked state
        self.state.in_code_block = false;
        self.state.code_block_language = None;
        self.state.code_fence_len = 0;
        self.state.in_table = false;
        self.state.table_rows = 0;

        for line in self.state.buffer.lines() {
            // Detect code fences
            let fence_len = line.chars().take_while(|c| *c == '`').count();
            if fence_len >= 3 {
                let rest = line.get(fence_len..).unwrap_or("");
                if rest.contains('`') {
                    continue;
                }

                if self.state.in_code_block {
                    // Closing fence — must match the opening fence length
                    if fence_len >= self.state.code_fence_len {
                        self.state.in_code_block = false;
                        self.state.code_block_language = None;
                        self.state.code_fence_len = 0;
                    }
                } else {
                    // Opening fence
                    self.state.in_code_block = true;
                    self.state.code_fence_len = fence_len;
                    self.state.code_block_language = {
                        let lang = rest.trim();
                        if lang.is_empty() {
                            None
                        } else {
                            Some(lang.to_string())
                        }
                    };
                }
            } else if self.state.in_code_block {
                // Inside code block, skip other detection
                continue;
            } else if line.trim().starts_with('|') && line.trim().ends_with('|') {
                self.state.in_table = true;
                self.state.table_rows += 1;
            } else if !line.trim().is_empty() && !line.trim().starts_with('|') {
                // Non-table line outside a code block ends the table
                if self.state.in_table && !line.contains('|') {
                    self.state.in_table = false;
                }
            }
        }
    }

    fn sanitize_for_display(&self) -> String {
        // For now, pass through. The TUI renderer handles the actual formatting.
        self.state.buffer.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_streaming() {
        let mut stream = MarkdownStream::new();
        let chunk = stream.push("Hello, ");
        assert_eq!(chunk.element_type, MarkdownElement::Text);

        let chunk = stream.push("world!");
        assert_eq!(chunk.element_type, MarkdownElement::Text);
        assert!(stream.content().contains("Hello, world!"));
    }

    #[test]
    fn test_code_block_streaming() {
        let mut stream = MarkdownStream::new();
        stream.push("```rust\n");
        assert!(stream.is_incomplete());

        stream.push("fn main() {}\n");
        assert!(stream.is_incomplete());

        stream.push("```");
        assert!(!stream.is_incomplete());
    }

    #[test]
    fn test_code_block_language_detection_handles_blank_fence() {
        let mut stream = MarkdownStream::new();
        let chunk = stream.push("```\n");
        assert_eq!(
            chunk.element_type,
            MarkdownElement::CodeBlock {
                language: None,
                closed: false
            }
        );
    }

    #[test]
    fn test_header_detection() {
        let mut stream = MarkdownStream::new();
        let chunk = stream.push("## Hello\n");
        assert_eq!(chunk.element_type, MarkdownElement::Header { level: 2 });
    }

    #[test]
    fn test_list_detection() {
        let mut stream = MarkdownStream::new();
        let chunk = stream.push("- Item one\n");
        assert_eq!(chunk.element_type, MarkdownElement::ListItem);
    }

    #[test]
    fn test_sanitized_output_closes_code_block() {
        let mut stream = MarkdownStream::new();
        stream.push("```python\nprint('hello')\n");

        let output = stream.sanitized_output();
        assert!(output.ends_with("```\n"));
    }

    #[test]
    fn test_reset() {
        let mut stream = MarkdownStream::new();
        stream.push("```rust\ncode");
        assert!(stream.is_incomplete());

        stream.reset();
        assert!(!stream.is_incomplete());
        assert!(stream.content().is_empty());
    }
}
