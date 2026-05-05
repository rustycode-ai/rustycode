//! Streaming markdown buffer for safe incremental rendering.
//!
//! Ported from goose's `MarkdownBuffer`. Provides a buffer that accumulates streaming
//! markdown chunks and determines safe points to flush content for rendering. It tracks
//! open markdown constructs (code blocks, bold, links, etc.) to ensure we only output
//! complete, well-formed markdown.
//!
//! Unlike `rustycode_tools::markdown_stream::MarkdownStream` which focuses on element
//! classification, this module focuses on **safe render boundaries** - finding the latest
//! position in a buffer where all markdown constructs are balanced/closed.

use regex::Regex;
use std::io::Write;
use std::sync::LazyLock;

const MAX_CODE_BLOCK_LINES: usize = 50;
const TRUNCATED_SHOW_LINES: usize = 20;

/// Regex that tokenizes markdown inline elements.
/// Order matters: longer/more-specific patterns first.
static INLINE_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(",
        r"\\.",                 // Escaped char
        r"|`+",                 // Inline code
        r"|\*\*\*",             // Bold+italic
        r"|\*\*",               // Bold
        r"|\*",                 // Italic
        r"|___",                // Bold+italic (underscore)
        r"|__",                 // Bold (underscore)
        r"|_",                  // Italic (underscore)
        r"|~~",                 // Strikethrough
        r"|\!\[",               // Image start
        r"|\]\(",               // Link URL start
        r"|\[",                 // Link text start
        r"|\]",                 // Bracket close
        r"|\)",                 // Link URL end
        r"|[^\\\*_`~\[\]!()]+", // Plain text
        r"|.",                  // Any other single char
        r")"
    ))
    .expect("INLINE_TOKEN_RE is a compile-time-verified static regex")
});

/// Truncate large code blocks in content, saving full content to temp file.
///
/// Processes ALL code blocks in the content, not just the first one.
fn truncate_code_blocks(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut pos = 0;
    let len = content.len();

    while pos < len {
        let remaining = &content[pos..];

        let (fence_offset, fence_str) = match (remaining.find("```"), remaining.find("~~~")) {
            (Some(a), Some(b)) if a <= b => (a, "```"),
            (Some(a), None) => (a, "```"),
            (None, Some(b)) => (b, "~~~"),
            (None, None) => {
                result.push_str(remaining);
                break;
            }
            (Some(_), Some(b)) => (b, "~~~"),
        };

        result.push_str(&remaining[..fence_offset]);

        let fence_abs = pos + fence_offset;
        let after_fence = match content.get(fence_abs + 3..) {
            Some(s) => s,
            None => {
                result.push_str(&remaining[fence_offset..]);
                break;
            }
        };

        let newline_pos = match after_fence.find('\n') {
            Some(p) => p,
            None => {
                result.push_str(&remaining[fence_offset..]);
                break;
            }
        };

        let code_start = fence_abs + 3 + newline_pos + 1;

        let code_region = match content.get(code_start..) {
            Some(s) => s,
            None => {
                result.push_str(&remaining[fence_offset..]);
                break;
            }
        };

        let close_pattern = format!("\n{}", fence_str);
        let close_offset = match code_region.find(&close_pattern) {
            Some(p) => p,
            None => {
                result.push_str(&remaining[fence_offset..]);
                break;
            }
        };

        let code_content = match code_region.get(..close_offset) {
            Some(s) => s,
            None => {
                result.push_str(&remaining[fence_offset..]);
                break;
            }
        };

        let lines: Vec<&str> = code_content.lines().collect();

        if lines.len() <= MAX_CODE_BLOCK_LINES {
            let block_end = code_start + close_offset + 1 + fence_str.len();
            result.push_str(match content.get(..block_end) {
                Some(s) => &s[fence_abs..],
                None => break,
            });
        } else {
            let truncated: String = lines
                .iter()
                .take(TRUNCATED_SHOW_LINES)
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            let remaining_lines = lines.len() - TRUNCATED_SHOW_LINES;

            let file_msg = save_to_temp_file(code_content)
                .map(|p| format!(" -> {}", p))
                .unwrap_or_default();

            let fence_header_end = fence_abs + 3 + newline_pos;
            let fence_header = &content[fence_abs..fence_header_end];

            let close_abs = code_start + close_offset + 1;
            let suffix: &str = content
                .get(close_abs + fence_str.len()..)
                .unwrap_or_default();

            result.push_str(fence_header);
            result.push('\n');
            result.push_str(&truncated);
            result.push_str(&format!(
                "\n... ({} more lines{})\n{}{}",
                remaining_lines, file_msg, fence_str, suffix
            ));

            pos = close_abs + fence_str.len();
            continue;
        }

        let close_abs = code_start + close_offset + 1;
        pos = close_abs + fence_str.len();
    }

    result
}

fn save_to_temp_file(content: &str) -> Option<String> {
    let mut file = tempfile::Builder::new()
        .prefix("rustycode-")
        .suffix(".txt")
        .tempfile()
        .ok()?;

    file.write_all(content.as_bytes()).ok()?;
    let (_, path) = file.keep().ok()?;
    Some(path.display().to_string())
}

/// Tracks the current parsing state for markdown constructs.
#[derive(Default, Debug, Clone, PartialEq)]
struct ParseState {
    in_code_block: bool,
    code_fence_char: char,
    code_fence_len: usize,
    in_table: bool,
    pending_heading: bool,
    in_inline_code: bool,
    inline_code_len: usize,
    in_bold: bool,
    in_italic: bool,
    in_strikethrough: bool,
    in_link_text: bool,
    in_link_url: bool,
    in_image_alt: bool,
}

impl ParseState {
    /// Returns true if no markdown constructs are currently open.
    fn is_clean(&self) -> bool {
        !self.in_code_block
            && !self.in_table
            && !self.pending_heading
            && !self.in_inline_code
            && !self.in_bold
            && !self.in_italic
            && !self.in_strikethrough
            && !self.in_link_text
            && !self.in_link_url
            && !self.in_image_alt
    }

    /// Returns true when a complete table row has been consumed.
    ///
    /// Table rows are safe to flush incrementally as long as all inline
    /// constructs inside the row are closed.
    fn is_table_row_safe(&self) -> bool {
        self.in_table
            && !self.in_code_block
            && !self.pending_heading
            && !self.in_inline_code
            && !self.in_bold
            && !self.in_italic
            && !self.in_strikethrough
            && !self.in_link_text
            && !self.in_link_url
            && !self.in_image_alt
    }
}

/// Close unclosed code fences in content to prevent broken markdown rendering.
///
/// At stream end, an unclosed fence (``` or ~~~) would cause everything
/// after it to render as a code block. This function detects and closes them.
///
/// Properly tracks open/close state so that fence-like content inside a code
/// block (e.g., a markdown tutorial showing ``` examples) doesn't cause false
/// positives.
fn close_unclosed_fences(content: &str) -> String {
    let mut result = content.to_string();

    let mut open_backtick = false;
    let mut open_tilde = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if open_backtick {
            let fence_len = trimmed.chars().take_while(|&c| c == '`').count();
            if fence_len >= 3 {
                let after = trimmed.get(fence_len..).map(|s| s.trim()).unwrap_or("");
                if after.is_empty() {
                    open_backtick = false;
                }
            }
        } else if open_tilde {
            let fence_len = trimmed.chars().take_while(|&c| c == '~').count();
            if fence_len >= 3 {
                let after = trimmed.get(fence_len..).map(|s| s.trim()).unwrap_or("");
                if after.is_empty() {
                    open_tilde = false;
                }
            }
        } else {
            if trimmed.starts_with("```") {
                open_backtick = true;
            } else if trimmed.starts_with("~~~") {
                open_tilde = true;
            }
        }
    }

    if open_backtick {
        result.push_str("\n```\n");
    }
    if open_tilde {
        result.push_str("\n~~~\n");
    }

    truncate_code_blocks(&result)
}

/// A streaming markdown buffer that tracks open constructs.
///
/// Accumulates chunks and returns content that is safe to render,
/// holding back any incomplete markdown constructs. Large code blocks
/// are automatically truncated with full content saved to a temp file.
#[derive(Default)]
pub struct StreamingRenderBuffer {
    buffer: String,
}

#[allow(clippy::string_slice)]
impl StreamingRenderBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a chunk of markdown text to the buffer.
    ///
    /// Returns any content that is safe to render, or None if the buffer
    /// contains only incomplete constructs.
    pub fn push(&mut self, chunk: &str) -> Option<String> {
        self.buffer.push_str(chunk);
        let safe_end = self.find_safe_end();

        if safe_end > 0 && self.buffer.is_char_boundary(safe_end) {
            let to_render = self.buffer[..safe_end].to_string();
            self.buffer = self.buffer[safe_end..].to_string();
            Some(to_render)
        } else if safe_end > 0 {
            // safe_end landed mid-character; walk back to the nearest char boundary
            let mut end = safe_end;
            while end > 0 && !self.buffer.is_char_boundary(end) {
                end -= 1;
            }
            if end > 0 {
                let to_render = self.buffer[..end].to_string();
                self.buffer = self.buffer[end..].to_string();
                Some(to_render)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Flush any remaining content from the buffer.
    ///
    /// Call this at the end of a stream to get any buffered content.
    /// Closes any unclosed code fences to prevent broken rendering.
    pub fn flush(&mut self) -> String {
        let content = std::mem::take(&mut self.buffer);
        close_unclosed_fences(&content)
    }

    /// Check if there is buffered content waiting for safe boundaries.
    pub fn has_pending(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Get the length of buffered content.
    pub fn pending_len(&self) -> usize {
        self.buffer.len()
    }

    /// Find the last byte position where the parse state is "clean".
    fn find_safe_end(&self) -> usize {
        let mut state = ParseState::default();
        let mut last_safe: usize = 0;
        let bytes = self.buffer.as_bytes();
        let len = bytes.len();
        let mut pos: usize = 0;

        while pos < len {
            let at_line_start = pos == 0 || bytes[pos - 1] == b'\n';

            if at_line_start {
                if let Some(new_pos) = self.process_line_start(&mut state, pos) {
                    pos = new_pos;
                    if state.is_clean() {
                        last_safe = pos;
                    }
                    continue;
                }
            }

            if state.in_code_block {
                while pos < len && bytes[pos] != b'\n' {
                    pos += 1;
                }
                if pos < len {
                    pos += 1;
                }
                continue;
            }

            let remaining = &self.buffer[pos..];
            let line_end = remaining.find('\n').map(|i| pos + i + 1).unwrap_or(len);
            let line_content = &self.buffer[pos..line_end];

            for cap in INLINE_TOKEN_RE.find_iter(line_content) {
                let token = cap.as_str();

                self.process_inline_token(&mut state, token);

                if state.is_clean() {
                    last_safe = pos + cap.end();
                }
            }

            if line_end <= len && line_end > pos && bytes[line_end - 1] == b'\n' {
                state.pending_heading = false;
                if state.is_table_row_safe() {
                    last_safe = line_end;
                }
                if state.is_clean() {
                    last_safe = line_end;
                }
            }

            pos = line_end;
        }

        last_safe
    }

    /// Process block-level constructs at the start of a line.
    fn process_line_start(&self, state: &mut ParseState, pos: usize) -> Option<usize> {
        let remaining = &self.buffer[pos..];

        if state.pending_heading {
            state.pending_heading = false;
        }

        if let Some(fence_result) = self.check_code_fence(remaining, state) {
            return Some(pos + fence_result);
        }

        if state.in_code_block {
            return None;
        }

        if remaining.starts_with('#') {
            let hashes = remaining.chars().take_while(|&c| c == '#').count();
            if hashes <= 6 {
                let after_hashes = &remaining[hashes..];
                if after_hashes.is_empty()
                    || after_hashes.starts_with(' ')
                    || after_hashes.starts_with('\n')
                {
                    state.pending_heading = true;
                    return None;
                }
            }
        }

        if remaining.starts_with('|') {
            state.in_table = true;
            return None;
        }

        if (remaining.starts_with('\n') || remaining.is_empty()) && state.in_table {
            state.in_table = false;
            return Some(pos + 1);
        }

        if state.in_table && !remaining.starts_with('|') {
            state.in_table = false;
        }

        None
    }

    /// Check for a code fence and update state.
    fn check_code_fence(&self, line: &str, state: &mut ParseState) -> Option<usize> {
        let trimmed = line.trim_start();

        let fence_char = trimmed.chars().next()?;
        if fence_char != '`' && fence_char != '~' {
            return None;
        }

        let fence_len = trimmed.chars().take_while(|&c| c == fence_char).count();
        if fence_len < 3 {
            return None;
        }

        let after_fence = &trimmed[fence_len..];

        if state.in_code_block {
            if fence_char == state.code_fence_char
                && fence_len >= state.code_fence_len
                && (after_fence.is_empty()
                    || after_fence.starts_with('\n')
                    || after_fence.trim().is_empty())
            {
                state.in_code_block = false;
                state.code_fence_char = '\0';
                state.code_fence_len = 0;

                return line.find('\n').map(|p| p + 1).or(Some(line.len()));
            }
        } else {
            state.in_code_block = true;
            state.code_fence_char = fence_char;
            state.code_fence_len = fence_len;

            return line.find('\n').map(|p| p + 1).or(Some(line.len()));
        }

        None
    }

    /// Process an inline token and update state.
    fn process_inline_token(&self, state: &mut ParseState, token: &str) {
        if token.starts_with('\\') && token.len() == 2 {
            return;
        }

        if token.starts_with('`') {
            let tick_count = token.len();
            if state.in_inline_code {
                if tick_count == state.inline_code_len {
                    state.in_inline_code = false;
                    state.inline_code_len = 0;
                }
            } else {
                state.in_inline_code = true;
                state.inline_code_len = tick_count;
            }
            return;
        }

        if state.in_inline_code {
            return;
        }

        match token {
            "***" | "___" => {
                if state.in_bold && state.in_italic {
                    state.in_bold = false;
                    state.in_italic = false;
                } else if state.in_bold {
                    state.in_italic = !state.in_italic;
                } else if state.in_italic {
                    state.in_bold = !state.in_bold;
                } else {
                    state.in_bold = true;
                    state.in_italic = true;
                }
            }
            "**" | "__" => {
                state.in_bold = !state.in_bold;
            }
            "*" | "_" => {
                state.in_italic = !state.in_italic;
            }
            "~~" => {
                state.in_strikethrough = !state.in_strikethrough;
            }
            "![" => {
                state.in_image_alt = true;
            }
            "[" => {
                if !state.in_link_text && !state.in_image_alt {
                    state.in_link_text = true;
                }
            }
            "](" => {
                if state.in_link_text {
                    state.in_link_text = false;
                    state.in_link_url = true;
                } else if state.in_image_alt {
                    state.in_image_alt = false;
                    state.in_link_url = true;
                }
            }
            "]" => {}
            ")" => {
                if state.in_link_url {
                    state.in_link_url = false;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Process chunks through the buffer and return all outputs (skipping None, including flush)
    fn stream(chunks: &[&str]) -> Vec<String> {
        let mut buf = StreamingRenderBuffer::new();
        let mut results: Vec<String> = chunks.iter().filter_map(|chunk| buf.push(chunk)).collect();
        let remaining = buf.flush();
        if !remaining.is_empty() {
            results.push(remaining);
        }
        results
    }

    #[test]
    fn test_simple_text_streams_immediately() {
        let result = stream(&["Hello", " world", "!"]);
        assert_eq!(result, vec!["Hello", " world", "!"]);
    }

    #[test]
    fn test_bold_split_mid_word() {
        let result = stream(&["Here's the **important", "** part."]);
        assert_eq!(result, vec!["Here's the ", "**important** part."]);
    }

    #[test]
    fn test_inline_code_split() {
        let result = stream(&["Use the `println!", "` macro."]);
        assert_eq!(result, vec!["Use the ", "`println!` macro."]);
    }

    #[test]
    fn test_code_block_streamed_complete() {
        let result = stream(&[
            "```rust\n",
            "fn main() {\n",
            "    println!(\"hello\");\n",
            "}\n",
            "```\n",
        ]);
        assert_eq!(
            result,
            vec!["```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n"]
        );
    }

    #[test]
    fn test_link_url_split() {
        let result = stream(&["Check [the docs](https://doc", "s.rs) for more."]);
        assert_eq!(
            result,
            vec!["Check ", "[the docs](https://docs.rs) for more."]
        );
    }

    #[test]
    fn test_table_streamed_complete() {
        let result = stream(&[
            "| Name | Value |\n",
            "|------|-------|\n",
            "| foo  | 42    |\n",
            "\nMore text",
        ]);
        assert!(result
            .iter()
            .any(|chunk| chunk.contains("| Name | Value |")));
        assert!(result
            .iter()
            .any(|chunk| chunk.contains("|------|-------|")));
        assert!(result
            .iter()
            .any(|chunk| chunk.contains("| foo  | 42    |")));
        assert!(result.iter().any(|chunk| chunk.contains("More text")));
    }

    #[test]
    fn test_heading_split() {
        let result = stream(&["# Getting St", "arted\n\nFirst, install..."]);
        assert_eq!(result, vec!["# Getting Started\n\nFirst, install..."]);
    }

    #[test]
    fn test_unclosed_bold_flushes() {
        let result = stream(&["This is **incomplete bold"]);
        assert_eq!(result, vec!["This is ", "**incomplete bold"]);
    }

    #[test]
    fn test_unclosed_code_block_flushes() {
        let result = stream(&["```\ncode"]);
        // flush() should close the unclosed fence
        assert_eq!(result, vec!["```\ncode\n```\n"]);
    }

    #[test]
    fn test_strikethrough_and_bold_split() {
        let result = stream(&["~~stri", "ke~~ and **bo", "ld**"]);
        assert_eq!(result, vec!["~~strike~~ and ", "**bold**"]);
    }

    #[test]
    fn test_empty_input() {
        let result = stream(&[""]);
        let expected: Vec<String> = vec![];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_unicode_content() {
        let result = stream(&["Hello 世界! Here's some **太字** text."]);
        assert_eq!(result, vec!["Hello 世界! Here's some **太字** text."]);
    }

    #[test]
    fn test_combined_bold_italic() {
        let result = stream(&["***bold italic***"]);
        assert_eq!(result, vec!["***bold italic***"]);
    }

    #[test]
    fn test_mixed_content_typical_response() {
        let result = stream(&[
            "Here's how to do it:\n\n",
            "1. First, run `cargo",
            " build`\n",
            "2. Then check the **out",
            "put**\n\n",
            "```rust\n",
            "fn main() {}\n",
            "```\n",
        ]);
        assert_eq!(
            result,
            vec![
                "Here's how to do it:\n\n",
                "1. First, run ",
                "`cargo build`\n",
                "2. Then check the ",
                "**output**\n\n",
                "```rust\nfn main() {}\n```\n"
            ]
        );
    }

    #[test]
    fn test_has_pending_and_pending_len() {
        let mut buf = StreamingRenderBuffer::new();
        assert!(!buf.has_pending());
        assert_eq!(buf.pending_len(), 0);

        // Push incomplete bold - "Hello " is safe, "**wor" stays pending
        let flushed = buf.push("Hello **wor");
        assert!(flushed.is_some()); // "Hello " was flushed
        assert!(buf.has_pending());
        assert_eq!(buf.pending_len(), 5); // "**wor" remaining

        // Close bold - flushes
        let flushed = buf.push("ld**!");
        assert!(flushed.is_some());
        assert!(!buf.has_pending());
    }

    #[test]
    fn test_flush_closes_unclosed_code_fence() {
        let mut buf = StreamingRenderBuffer::new();
        buf.push("```rust\nfn main() {}");
        // No closing fence - flush should add one
        let result = buf.flush();
        assert!(
            result.contains("```rust\nfn main() {}"),
            "content preserved"
        );
        assert!(
            result.ends_with("```\n"),
            "closing fence added: {:?}",
            result
        );
    }

    #[test]
    fn test_flush_doesnt_add_extra_fence_when_closed() {
        let result = stream(&["```rust\nfn main() {}\n```"]);
        // The whole thing is held back then flushed, but fence is already closed
        // Count fences - should be exactly 2 (open + close)
        let combined: String = result.join("");
        let fence_count = combined.lines().filter(|l| l.starts_with("```")).count();
        assert_eq!(
            fence_count, 2,
            "should have exactly 2 fences, got {}: {:?}",
            fence_count, combined
        );
    }

    #[test]
    fn test_flush_handles_empty_buffer() {
        let mut buf = StreamingRenderBuffer::new();
        let result = buf.flush();
        assert!(result.is_empty());
    }

    // === UTF-8 and special character tests ===

    #[test]
    fn test_push_thai_characters() {
        let result = stream(&["สวัสดี", "ครับ", " **bold**"]);
        let combined: String = result.join("");
        assert!(combined.contains("สวัสดีครับ"));
        assert!(combined.contains("**bold**"));
    }

    #[test]
    fn test_push_emoji() {
        let result = stream(&["Hello 🌍", " World 🚀"]);
        let combined: String = result.join("");
        assert!(combined.contains("🌍"));
        assert!(combined.contains("🚀"));
    }

    #[test]
    fn test_push_zwj_emoji() {
        let family = "👨‍👩‍👧‍👦";
        let result = stream(&[family]);
        let combined: String = result.join("");
        assert!(combined.contains(family), "ZWJ family emoji preserved");
    }

    #[test]
    fn test_push_chinese_characters() {
        let result = stream(&["你好世界", " **粗体**"]);
        let combined: String = result.join("");
        assert!(combined.contains("你好世界"));
        assert!(combined.contains("**粗体**"));
    }

    #[test]
    fn test_push_arabic_rtl() {
        let result = stream(&["مرحبا", " بالعالم"]);
        let combined: String = result.join("");
        assert!(combined.contains("مرحبا"));
        assert!(combined.contains("بالعالم"));
    }

    #[test]
    fn test_push_japanese() {
        let result = stream(&["こんにちは", "世界"]);
        let combined: String = result.join("");
        assert!(combined.contains("こんにちは世界"));
    }

    #[test]
    fn test_push_null_byte() {
        let result = stream(&["Hello\0World"]);
        let combined: String = result.join("");
        assert!(combined.contains("Hello\0World"), "null byte preserved");
    }

    #[test]
    fn test_push_control_characters() {
        let result = stream(&["Hello\tWorld\n", "Next line\r\n"]);
        let combined: String = result.join("");
        assert!(combined.contains('\t'));
        assert!(combined.contains('\n'));
        assert!(combined.contains('\r'));
    }

    #[test]
    fn test_push_mixed_scripts() {
        let result = stream(&["Hello สวัสดี 你好 🌍", " **bold**"]);
        let combined: String = result.join("");
        assert!(combined.contains("Hello"));
        assert!(combined.contains("สวัสดี"));
        assert!(combined.contains("你好"));
        assert!(combined.contains("🌍"));
        assert!(combined.contains("**bold**"));
    }

    #[test]
    fn test_push_combining_diacritics() {
        // e + combining acute accent = é
        let combined_char = "e\u{0301}";
        let result = stream(&[combined_char, " text"]);
        let joined: String = result.join("");
        assert!(
            joined.contains(combined_char) || joined.contains("é"),
            "combining diacritics preserved"
        );
    }

    #[test]
    fn test_push_incomplete_bold_with_thai() {
        let mut buf = StreamingRenderBuffer::new();
        // Thai text followed by incomplete bold
        let first = buf.push("สวัสดี **ที");
        assert!(first.is_some(), "Thai text before bold is safe");
        let first_text = first.unwrap();
        assert!(first_text.contains("สวัสดี"));

        // Complete the bold
        let second = buf.push("กษณ์**");
        assert!(second.is_some());
        let second_text = second.unwrap();
        assert!(second_text.contains("**ทีกษณ์**"));
    }

    #[test]
    fn test_push_code_block_with_chinese() {
        let result = stream(&["```rust\n", "fn 你好() {}\n", "```\n"]);
        let combined: String = result.join("");
        assert!(combined.contains("你好"));
    }

    #[test]
    fn test_push_very_long_multibyte_string() {
        // 1000 Thai characters
        let thai: String = "ส".repeat(1000);
        let result = stream(&[&thai]);
        let combined: String = result.join("");
        assert_eq!(combined.chars().filter(|c| *c == 'ส').count(), 1000);
    }

    #[test]
    fn test_push_emoji_in_code_block() {
        let result = stream(&["```\n", "print 🌍\n", "```\n"]);
        let combined: String = result.join("");
        assert!(combined.contains("🌍"));
    }

    #[test]
    fn test_push_surrogate_safe_unicode() {
        // Characters at the edges of BMP
        let result = stream(&["\u{FFFF}", " text ", "\u{10000}"]);
        let combined: String = result.join("");
        assert!(combined.contains('\u{FFFF}'));
        assert!(combined.contains('\u{10000}'));
    }
}
