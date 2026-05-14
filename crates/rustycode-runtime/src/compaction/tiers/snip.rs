//! SnipTier — free compaction pass that trims tool output and removes thinking blocks.
//!
//! This tier costs zero LLM tokens. It operates purely by:
//!
//! 1. **Trimming tool-result content** that exceeds `max_tool_output_lines`,
//!    appending a `"... N lines truncated ..."` footer.
//! 2. **Removing `<thinking>...</thinking>` and `<analysis>...</analysis>`
//!    XML blocks** from text content (artifacts the LLM sometimes emits).

use rustycode_protocol::{ContentBlock, Message, MessageContent};

use super::TierResult;

/// Free compaction pass: trims long tool output and strips thinking/analysis blocks.
#[derive(Debug, Clone)]
pub struct SnipTier {
    /// Maximum number of lines retained from each tool-result block.
    max_tool_output_lines: usize,
}

impl SnipTier {
    pub fn new(max_tool_output_lines: usize) -> Self {
        Self {
            max_tool_output_lines,
        }
    }

    /// Run the Snip pass over `messages`, returning the transformed list and
    /// an estimated count of tokens removed.
    pub fn compact(&self, messages: Vec<Message>) -> TierResult {
        let tokens_before: usize = messages
            .iter()
            .map(|m| rustycode_protocol::estimate_tokens(&m.content.as_text()))
            .sum();
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

        let tokens_after: usize = transformed
            .iter()
            .map(|m| rustycode_protocol::estimate_tokens(&m.content.as_text()))
            .sum();

        TierResult {
            messages: transformed,
            tokens_removed: tokens_before.saturating_sub(tokens_after),
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
    use rustycode_protocol::MessageRole;

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
        let input = "word one two three four five six seven\nword eight nine ten eleven twelve thirteen\nword fourteen fifteen sixteen seventeen eighteen\nword nineteen twenty twentyone twentytwo twentythree\nword twentyfour twentyfive twentysix twentyseven twentyeight";
        let msgs = vec![tool_result_msg(input)];
        let result = tier.compact(msgs);

        assert!(result.tokens_removed > 0);
        let content = &result.messages[0].content;
        match content {
            MessageContent::Blocks(blocks) => {
                if let ContentBlock::ToolResult { content, .. } = &blocks[0] {
                    assert!(content.starts_with("word one two three"));
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
        // 10 lines, each with 20 words -> ~180 words removed from lines 2-10.
        let long_line: String = (0..20)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let input: String = (0..10)
            .map(|i| format!("{long_line} line{i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let msgs = vec![tool_result_msg(&input)];
        let result = tier.compact(msgs);

        assert!(
            result.tokens_removed > 0,
            "should report positive tokens_removed when content was trimmed"
        );
        assert!(
            result.tokens_removed >= 100,
            "expected substantial token removal, got {}",
            result.tokens_removed
        );
    }

    // -- Extended edge-case tests -------------------------------------------

    #[test]
    fn empty_messages_returns_empty() {
        let tier = SnipTier::new(10);
        let msgs: Vec<Message> = Vec::new();
        let result = tier.compact(msgs);
        assert!(result.messages.is_empty());
        assert_eq!(result.tokens_removed, 0);
    }

    #[test]
    fn multiple_thinking_blocks_in_one_message() {
        let tier = SnipTier::new(50);
        let input = "before <thinking>first</thinking> middle <thinking>second</thinking> after";
        let msgs = vec![user_msg(input)];
        let result = tier.compact(msgs);

        let text = result.messages[0].content.as_text();
        assert!(
            !text.contains("first"),
            "first thinking block should be removed"
        );
        assert!(
            !text.contains("second"),
            "second thinking block should be removed"
        );
        assert!(text.contains("before"), "text before blocks preserved");
        assert!(text.contains("middle"), "text between blocks preserved");
        assert!(text.contains("after"), "text after blocks preserved");
    }

    #[test]
    fn unclosed_thinking_tag_removes_rest() {
        let tier = SnipTier::new(50);
        let input = "start <thinking>this never closes";
        let msgs = vec![user_msg(input)];
        let result = tier.compact(msgs);

        let text = result.messages[0].content.as_text();
        assert!(
            !text.contains("this never closes"),
            "unclosed thinking should be removed"
        );
        assert!(text.contains("start"), "text before unclosed tag preserved");
    }

    #[test]
    fn nested_analysis_inside_thinking() {
        let tier = SnipTier::new(50);
        // <thinking> wraps <analysis> — the outer thinking tag removes everything.
        let input = "before <thinking>outer <analysis>inner</analysis> rest</thinking> after";
        let msgs = vec![user_msg(input)];
        let result = tier.compact(msgs);

        let text = result.messages[0].content.as_text();
        assert!(
            !text.contains("outer") && !text.contains("inner"),
            "nested blocks should be fully removed"
        );
        assert!(text.contains("before") && text.contains("after"));
    }

    #[test]
    fn thinking_block_with_newlines() {
        let tier = SnipTier::new(50);
        let input = "before\n<thinking>\nline1\nline2\nline3\n</thinking>\nafter";
        let msgs = vec![user_msg(input)];
        let result = tier.compact(msgs);

        let text = result.messages[0].content.as_text();
        assert!(
            !text.contains("line1"),
            "multiline thinking should be removed"
        );
        assert!(text.contains("before") && text.contains("after"));
    }

    #[test]
    fn tool_output_exactly_at_limit_not_truncated() {
        let tier = SnipTier::new(3);
        let input = "line1\nline2\nline3"; // exactly 3 lines
        let msgs = vec![tool_result_msg(input)];
        let result = tier.compact(msgs);

        assert_eq!(
            result.tokens_removed, 0,
            "no truncation when exactly at limit"
        );
        match &result.messages[0].content {
            MessageContent::Blocks(blocks) => {
                if let ContentBlock::ToolResult { content, .. } = &blocks[0] {
                    assert!(
                        !content.contains("truncated"),
                        "no truncation footer expected"
                    );
                    assert_eq!(content, input);
                }
            }
            other => panic!("expected Blocks, got {other:?}"),
        }
    }

    #[test]
    fn tool_output_one_over_limit_is_truncated() {
        let tier = SnipTier::new(3);
        let input = "line1\nline2\nline3\nline4"; // 4 lines, limit 3
        let msgs = vec![tool_result_msg(input)];
        let result = tier.compact(msgs);

        // Note: removing 1 short line may not save tokens because the truncation
        // footer ("... 1 lines truncated ...") can be longer than the removed
        // content. Verify the truncation happened, not that tokens decreased.
        match &result.messages[0].content {
            MessageContent::Blocks(blocks) => {
                if let ContentBlock::ToolResult { content, .. } = &blocks[0] {
                    assert!(
                        content.contains("1 lines truncated"),
                        "should report 1 line truncated"
                    );
                    assert!(
                        content.contains("line1")
                            && content.contains("line2")
                            && content.contains("line3")
                    );
                    assert!(!content.contains("line4"), "line4 should have been removed");
                }
            }
            other => panic!("expected Blocks, got {other:?}"),
        }
    }

    #[test]
    fn tool_output_with_crlf_line_endings() {
        let tier = SnipTier::new(2);
        let input = "line1\r\nline2\r\nline3\r\nline4"; // 4 lines with CRLF
        let msgs = vec![tool_result_msg(input)];
        let result = tier.compact(msgs);

        // Verify truncation happened (footer present), regardless of net token
        // savings — the footer can be longer than the removed short lines.
        match &result.messages[0].content {
            MessageContent::Blocks(blocks) => {
                if let ContentBlock::ToolResult { content, .. } = &blocks[0] {
                    assert!(
                        content.contains("2 lines truncated"),
                        "should report truncation count"
                    );
                    assert!(content.contains("line1"), "first 2 lines should be kept");
                    assert!(content.contains("line2"), "first 2 lines should be kept");
                    assert!(!content.contains("line4"), "line4 should have been removed");
                }
            }
            other => panic!("expected Blocks, got {other:?}"),
        }
    }

    #[test]
    fn message_with_only_thinking_block_produces_minimal_content() {
        let tier = SnipTier::new(50);
        let input = "<thinking>all my deep thoughts</thinking>";
        let msgs = vec![user_msg(input)];
        let result = tier.compact(msgs);

        let text = result.messages[0].content.as_text();
        assert!(
            text.trim().is_empty(),
            "message with only thinking block should be empty after snip, got: '{text}'"
        );
    }

    #[test]
    fn mixed_blocks_thinking_in_text_not_in_tool_result() {
        let tier = SnipTier::new(2);
        // A message with both Text and ToolResult blocks.
        // Thinking should be stripped from Text but ToolResult should be trimmed.
        let long_tool = (0..10)
            .map(|i| format!("tool line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let msgs = vec![Message {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "preamble <thinking>secret</thinking> postamble".to_string(),
                    cache_control: None,
                },
                ContentBlock::ToolResult {
                    tool_use_id: "t1".to_string(),
                    content: long_tool,
                    is_error: false,
                },
            ]),
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        }];

        let result = tier.compact(msgs);
        let text_block = match &result.messages[0].content {
            MessageContent::Blocks(blocks) => blocks.clone(),
            other => panic!("expected Blocks, got {other:?}"),
        };

        // Text block: thinking stripped.
        if let ContentBlock::Text { text, .. } = &text_block[0] {
            assert!(
                !text.contains("secret"),
                "thinking should be removed from Text block"
            );
            assert!(text.contains("preamble") && text.contains("postamble"));
        }

        // ToolResult: trimmed to 2 lines.
        if let ContentBlock::ToolResult { content, .. } = &text_block[1] {
            assert!(
                content.contains("8 lines truncated"),
                "tool output should be trimmed"
            );
        }
    }

    #[test]
    fn user_and_assistant_text_preserved_verbatim_when_no_trimming_needed() {
        let tier = SnipTier::new(50);
        let msgs = vec![
            user_msg("I need to fix the auth bug in src/auth.rs"),
            assistant_msg("I'll read the file and check the token validation logic."),
            user_msg("Also check the session expiry handling"),
            assistant_msg("Found it: the session cookie wasn't being refreshed."),
        ];
        let result = tier.compact(msgs);

        assert_eq!(result.messages.len(), 4, "all messages preserved");
        assert_eq!(
            result.tokens_removed, 0,
            "no tokens removed from short text"
        );
        assert_eq!(
            result.messages[0].content.as_text(),
            "I need to fix the auth bug in src/auth.rs"
        );
        assert_eq!(
            result.messages[1].content.as_text(),
            "I'll read the file and check the token validation logic."
        );
    }

    #[test]
    fn tool_error_result_is_trimmed_but_not_stripped() {
        let tier = SnipTier::new(2);
        let long_error: String = (0..10)
            .map(|i| format!("error line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let msgs = vec![Message {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "err_1".to_string(),
                content: long_error.clone(),
                is_error: true,
            }]),
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        }];

        let result = tier.compact(msgs);
        match &result.messages[0].content {
            MessageContent::Blocks(blocks) => {
                if let ContentBlock::ToolResult {
                    content, is_error, ..
                } = &blocks[0]
                {
                    assert!(*is_error, "is_error flag should be preserved");
                    assert!(
                        content.contains("8 lines truncated"),
                        "error output should also be trimmed"
                    );
                }
            }
            other => panic!("expected Blocks, got {other:?}"),
        }
    }
}
