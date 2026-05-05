//! SnipTier — free compaction pass that trims tool output and removes thinking blocks.
//!
//! This tier costs zero LLM tokens. It operates purely by:
//!
//! 1. **Trimming tool-result content** that exceeds `max_tool_output_lines`,
//!    appending a `"... N lines truncated ..."` footer.
//! 2. **Removing `<thinking>...</thinking>` and `<analysis>...</analysis>`
//!    XML blocks** from text content (artifacts the LLM sometimes emits).

use rustycode_protocol::{ContentBlock, Message, MessageContent, MessageRole};

use super::TierResult;

/// Free compaction pass: trims long tool output and strips thinking/analysis blocks.
#[derive(Debug, Clone)]
pub struct SnipTier {
    /// Maximum number of lines retained from each tool-result block.
    max_tool_output_lines: usize,
}

impl SnipTier {
    /// Create a new SnipTier with the given line limit for tool output.
    pub fn new(max_tool_output_lines: usize) -> Self {
        Self {
            max_tool_output_lines,
        }
    }

    /// Run the Snip pass over `messages`, returning the transformed list and
    /// an estimated count of tokens removed.
    pub fn compact(&self, messages: Vec<Message>) -> TierResult {
        let mut total_chars_removed: usize = 0;

        let transformed: Vec<Message> = messages
            .into_iter()
            .map(|mut msg| {
                let original_len = msg.content.len();
                let new_content = self.transform_content(&msg.content, &mut total_chars_removed);
                msg.content = new_content;
                // Also count chars saved from content-length change due to
                // block-level removals (e.g. thinking blocks).
                let new_len = msg.content.len();
                if new_len < original_len {
                    total_chars_removed += original_len - new_len;
                }
                msg
            })
            .collect();

        TierResult {
            messages: transformed,
            tokens_removed: total_chars_removed / 4,
        }
    }

    /// Transform a single [`MessageContent`], accumulating chars removed.
    fn transform_content(
        &self,
        content: &MessageContent,
        chars_removed: &mut usize,
    ) -> MessageContent {
        match content {
            MessageContent::Simple(text) => {
                let cleaned = strip_thinking_blocks(text, chars_removed);
                MessageContent::Simple(cleaned)
            }
            MessageContent::Blocks(blocks) => {
                let new_blocks: Vec<ContentBlock> = blocks
                    .iter()
                    .map(|block| self.transform_block(block, chars_removed))
                    .collect();
                MessageContent::Blocks(new_blocks)
            }
            // Future variants pass through unchanged.
            _ => content.clone(),
        }
    }

    /// Transform a single [`ContentBlock`].
    fn transform_block(&self, block: &ContentBlock, chars_removed: &mut usize) -> ContentBlock {
        match block {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let trimmed = self.trim_tool_output(content, chars_removed);
                ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: trimmed,
                    is_error: *is_error,
                }
            }
            ContentBlock::Text {
                text,
                cache_control,
            } => {
                let cleaned = strip_thinking_blocks(text, chars_removed);
                ContentBlock::Text {
                    text: cleaned,
                    cache_control: *cache_control,
                }
            }
            // All other block types pass through unchanged.
            other => other.clone(),
        }
    }

    /// Trim tool output to `max_tool_output_lines`, appending a truncation footer
    /// when lines are removed.
    fn trim_tool_output(&self, text: &str, chars_removed: &mut usize) -> String {
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() <= self.max_tool_output_lines {
            return text.to_string();
        }

        let kept: Vec<&str> = lines[..self.max_tool_output_lines].to_vec();
        let removed_count = lines.len() - self.max_tool_output_lines;

        let removed_text_len: usize = lines[self.max_tool_output_lines..]
            .iter()
            .map(|l| l.len() + 1) // +1 for the newline
            .sum();
        *chars_removed += removed_text_len;

        let mut result = kept.join("\n");
        result.push_str(&format!("\n... {} lines truncated ...", removed_count));
        result
    }
}

/// Remove `<thinking>...</thinking>` and `<analysis>...</analysis>` blocks
/// from a string, accumulating the approximate character count removed.
fn strip_thinking_blocks(text: &str, chars_removed: &mut usize) -> String {
    let mut result = text.to_string();
    for tag in &["thinking", "analysis"] {
        let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
        let mut cleaned = String::with_capacity(result.len());
        let mut pos = 0;
        let bytes = result.as_bytes();
        while pos < bytes.len() {
            if let Some(start) = result[pos..].find(&open) {
                let abs_start = pos + start;
                // Append everything before the opening tag.
                cleaned.push_str(&result[pos..abs_start]);
                // Find the closing tag.
                let after_open = abs_start + open.len();
                if let Some(end_offset) = result[after_open..].find(&close) {
                    let abs_end = after_open + end_offset + close.len();
                    *chars_removed += abs_end - abs_start;
                    pos = abs_end;
                } else {
                    // No closing tag — treat rest of string as the block.
                    *chars_removed += result.len() - abs_start;
                    pos = result.len();
                }
            } else {
                cleaned.push_str(&result[pos..]);
                break;
            }
        }
        result = cleaned;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a user message with a single ToolResult block.
    fn tool_result_msg(content: &str) -> Message {
        Message {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "tool_123".to_string(),
                content: content.to_string(),
                is_error: false,
            }]),
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        }
    }

    /// Helper: build a user message with simple text content.
    fn user_msg(text: &str) -> Message {
        Message::user(text)
    }

    /// Helper: build an assistant message with simple text content.
    fn assistant_msg(text: &str) -> Message {
        Message::assistant(text)
    }

    #[test]
    fn short_output_unchanged() {
        let tier = SnipTier::new(10);
        let lines = "line 1\nline 2\nline 3";
        let msgs = vec![tool_result_msg(lines)];
        let result = tier.compact(msgs);

        assert_eq!(result.tokens_removed, 0);
        let content = &result.messages[0].content;
        match content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                if let ContentBlock::ToolResult { content, .. } = &blocks[0] {
                    assert_eq!(content, lines);
                }
            }
            other => panic!("expected Blocks, got {other:?}"),
        }
    }

    #[test]
    fn long_output_truncated_with_count() {
        let tier = SnipTier::new(3);
        let input = "line 1\nline 2\nline 3\nline 4\nline 5";
        let msgs = vec![tool_result_msg(input)];
        let result = tier.compact(msgs);

        assert!(result.tokens_removed > 0);
        let content = &result.messages[0].content;
        match content {
            MessageContent::Blocks(blocks) => {
                if let ContentBlock::ToolResult { content, .. } = &blocks[0] {
                    assert!(content.starts_with("line 1\nline 2\nline 3"));
                    assert!(content.contains("2 lines truncated"));
                }
            }
            other => panic!("expected Blocks, got {other:?}"),
        }
    }

    #[test]
    fn thinking_removed() {
        let tier = SnipTier::new(50);
        let input = "before <thinking>deep thoughts</thinking> after";
        let msgs = vec![user_msg(input)];
        let result = tier.compact(msgs);

        let text = result.messages[0].content.as_text();
        assert!(
            !text.contains("<thinking>"),
            "thinking tag should be removed, got: {text}"
        );
        assert!(
            !text.contains("deep thoughts"),
            "thinking content should be removed, got: {text}"
        );
        assert!(text.contains("before"), "surrounding text preserved");
        assert!(text.contains("after"), "surrounding text preserved");
    }

    #[test]
    fn analysis_removed() {
        let tier = SnipTier::new(50);
        let input = "start <analysis>detailed analysis</analysis> end";
        let msgs = vec![user_msg(input)];
        let result = tier.compact(msgs);

        let text = result.messages[0].content.as_text();
        assert!(
            !text.contains("<analysis>"),
            "analysis tag should be removed, got: {text}"
        );
        assert!(
            !text.contains("detailed analysis"),
            "analysis content should be removed, got: {text}"
        );
        assert!(text.contains("start"), "surrounding text preserved");
        assert!(text.contains("end"), "surrounding text preserved");
    }

    #[test]
    fn user_assistant_preserved() {
        let tier = SnipTier::new(5);
        let msgs = vec![
            user_msg("Hello from user"),
            assistant_msg("Hello from assistant"),
        ];
        let result = tier.compact(msgs);

        assert_eq!(result.tokens_removed, 0);
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].content.as_text(), "Hello from user");
        assert_eq!(result.messages[1].content.as_text(), "Hello from assistant");
    }

    #[test]
    fn tokens_removed_estimated() {
        let tier = SnipTier::new(1);
        // 10 lines, each 100 chars -> ~900 chars removed from lines 2-10.
        let long_line: String = "x".repeat(100);
        let input: String = (0..10)
            .map(|i| format!("{long_line}_{i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let msgs = vec![tool_result_msg(&input)];
        let result = tier.compact(msgs);

        assert!(
            result.tokens_removed > 0,
            "should report positive tokens_removed when content was trimmed"
        );
        // Rough check: at least 500 chars removed / 4 = 125 tokens.
        assert!(
            result.tokens_removed >= 100,
            "expected substantial token removal, got {}",
            result.tokens_removed
        );
    }
}
