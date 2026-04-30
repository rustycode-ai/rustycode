//! Conversation Fixer Pipeline
//!
//! Validates and fixes conversation structure before sending to LLMs.
//! Ported from goose's conversation fixing pipeline with RustyCode type adaptation.
//!
//! ## Pipeline Order
//!
//! 1. `merge_text_content_blocks` - Merge consecutive text blocks within messages
//! 2. `trim_assistant_whitespace` - Trim trailing whitespace from assistant text
//! 3. `remove_empty_messages` - Drop messages with empty content
//! 4. `fix_empty_tool_results` - Ensure tool results have non-empty content
//! 5. `fix_tool_calling` - Ensure tool_use/tool_result pairs are matched
//! 6. `merge_consecutive_messages` - Merge same-role consecutive messages
//! 7. `fix_lead_trail` - Remove leading/trailing assistant messages
//! 8. `populate_if_empty` - Add placeholder if conversation is empty
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_protocol::conversation_fixer::fix_conversation;
//!
//! let (fixed, warnings) = fix_conversation(messages);
//! for warning in &warnings {
//!     tracing::warn!("Conversation fix: {}", warning);
//! }
//! ```

use crate::{ContentBlock, Message, MessageContent};

/// Result of fixing a conversation: (fixed_messages, warnings).
pub type FixResult = (Vec<Message>, Vec<String>);

/// Run the full conversation fixing pipeline.
///
/// Returns the fixed messages and a list of warnings describing what was changed.
pub fn fix_conversation(messages: Vec<Message>) -> FixResult {
    let (messages, w1) = merge_text_content_blocks(messages);
    let (messages, w2) = trim_assistant_whitespace(messages);
    let (messages, w3) = remove_empty_messages(messages);
    let (messages, w4) = fix_empty_tool_results(messages);
    let (messages, w5) = fix_tool_calling(messages);
    let (messages, w6) = merge_consecutive_messages(messages);
    let (messages, w7) = fix_lead_trail(messages);
    let (messages, w8) = populate_if_empty(messages);

    let mut warnings = Vec::new();
    warnings.extend(w1);
    warnings.extend(w2);
    warnings.extend(w3);
    warnings.extend(w4);
    warnings.extend(w5);
    warnings.extend(w6);
    warnings.extend(w7);
    warnings.extend(w8);

    (messages, warnings)
}

/// Merge consecutive text content blocks within each message.
///
/// When an assistant message has multiple adjacent text blocks
/// (e.g., from streaming assembly), this merges them into one.
pub fn merge_text_content_blocks(messages: Vec<Message>) -> FixResult {
    let mut warnings = Vec::new();
    let mut result = Vec::with_capacity(messages.len());

    for msg in messages {
        let content = match &msg.content {
            MessageContent::Blocks(blocks) => {
                let mut merged: Vec<ContentBlock> = Vec::new();
                let mut text_buf = String::new();
                let mut cache_ctrl = None;

                for block in blocks {
                    match block {
                        ContentBlock::Text {
                            text,
                            cache_control,
                        } => {
                            if !text_buf.is_empty() {
                                text_buf.push('\n');
                            }
                            text_buf.push_str(text);
                            if cache_control.is_some() {
                                cache_ctrl = *cache_control;
                            }
                        }
                        other => {
                            // Flush accumulated text before non-text block
                            if !text_buf.is_empty() {
                                merged.push(ContentBlock::Text {
                                    text: std::mem::take(&mut text_buf),
                                    cache_control: cache_ctrl.take(),
                                });
                            }
                            merged.push(other.clone());
                        }
                    }
                }
                // Flush remaining text
                if !text_buf.is_empty() {
                    merged.push(ContentBlock::Text {
                        text: text_buf,
                        cache_control: cache_ctrl,
                    });
                }

                if merged.len() < blocks.len() {
                    warnings.push(format!(
                        "Merged {} text blocks into {} in {} message",
                        blocks.len(),
                        merged.len(),
                        msg.role
                    ));
                }

                MessageContent::Blocks(merged)
            }
            other => other.clone(),
        };

        result.push(Message {
            role: msg.role.clone(),
            content,
            timestamp: msg.timestamp,
            metadata: msg.metadata,
        });
    }

    (result, warnings)
}

/// Trim trailing whitespace from assistant message text.
///
/// LLM responses sometimes have trailing newlines or spaces that waste tokens.
pub fn trim_assistant_whitespace(messages: Vec<Message>) -> FixResult {
    let mut warnings = Vec::new();
    let mut result = Vec::with_capacity(messages.len());

    for msg in messages {
        if !msg.is_assistant() {
            result.push(msg);
            continue;
        }

        let content = match &msg.content {
            MessageContent::Simple(text) => {
                let trimmed = text.trim_end().to_string();
                if trimmed.len() != text.len() {
                    warnings.push("Trimmed trailing whitespace from assistant message".to_string());
                }
                MessageContent::Simple(trimmed)
            }
            MessageContent::Blocks(blocks) => {
                let mut fixed_blocks: Vec<ContentBlock> = Vec::with_capacity(blocks.len());
                let mut did_trim = false;

                for block in blocks {
                    match block {
                        ContentBlock::Text {
                            text,
                            cache_control,
                        } => {
                            let trimmed = text.trim_end().to_string();
                            if trimmed.len() != text.len() {
                                did_trim = true;
                            }
                            fixed_blocks.push(ContentBlock::Text {
                                text: trimmed,
                                cache_control: *cache_control,
                            });
                        }
                        other => fixed_blocks.push(other.clone()),
                    }
                }

                if did_trim {
                    warnings
                        .push("Trimmed trailing whitespace from assistant text blocks".to_string());
                }
                MessageContent::Blocks(fixed_blocks)
            }
        };

        result.push(Message {
            role: msg.role,
            content,
            timestamp: msg.timestamp,
            metadata: msg.metadata,
        });
    }

    (result, warnings)
}

/// Remove messages with empty content.
///
/// Empty messages waste tokens and can confuse some LLM providers.
pub fn remove_empty_messages(messages: Vec<Message>) -> FixResult {
    let mut warnings = Vec::new();
    let mut result = Vec::new();

    for msg in messages {
        if is_content_empty(&msg.content) {
            warnings.push(format!("Removed empty {} message", msg.role));
            continue;
        }
        result.push(msg);
    }

    (result, warnings)
}

/// Fix empty tool use inputs and ensure tool_use blocks have valid input.
///
/// Some LLM providers reject tool_use blocks with empty or null input.
/// This fixer ensures all tool_use blocks have at least an empty JSON object `{}`.
///
/// Inspired by goose's `fix_empty_tool_results` in conversation/mod.rs.
pub fn fix_empty_tool_results(messages: Vec<Message>) -> FixResult {
    let mut warnings = Vec::new();
    let mut result = Vec::with_capacity(messages.len());

    for msg in messages {
        let content = match &msg.content {
            MessageContent::Blocks(blocks) => {
                let mut fixed = false;
                let new_blocks: Vec<ContentBlock> = blocks
                    .iter()
                    .map(|block| match block {
                        ContentBlock::ToolUse { id, name, input } if input.is_null() => {
                            fixed = true;
                            ContentBlock::ToolUse {
                                id: id.clone(),
                                name: name.clone(),
                                input: serde_json::json!({}),
                            }
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } if content.is_empty() => {
                            fixed = true;
                            ContentBlock::ToolResult {
                                tool_use_id: tool_use_id.clone(),
                                content: "(empty)".to_string(),
                                is_error: *is_error,
                            }
                        }
                        other => other.clone(),
                    })
                    .collect();

                if fixed {
                    warnings.push(format!("Fixed null tool input in {} message", msg.role));
                }
                MessageContent::Blocks(new_blocks)
            }
            other => other.clone(),
        };

        result.push(Message {
            role: msg.role,
            content,
            timestamp: msg.timestamp,
            metadata: msg.metadata,
        });
    }

    (result, warnings)
}

/// Fix tool calling structure.
///
/// Ensures:
/// - Tool_use blocks in assistant messages have matching tool_result responses
/// - Removes orphaned tool_use or tool_result blocks
/// - Tool results are in user-role messages (per Anthropic API convention)
pub fn fix_tool_calling(messages: Vec<Message>) -> FixResult {
    let mut warnings = Vec::new();

    // Collect all tool_use IDs from assistant messages
    let mut pending_tool_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for msg in &messages {
        if msg.is_assistant() {
            if let MessageContent::Blocks(blocks) = &msg.content {
                for block in blocks {
                    if let ContentBlock::ToolUse { id, .. } = block {
                        pending_tool_ids.insert(id.clone());
                    }
                }
            }
        }
    }

    // Collect all tool_result IDs from user messages
    let mut responded_tool_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for msg in &messages {
        if msg.is_user() {
            if let MessageContent::Blocks(blocks) = &msg.content {
                for block in blocks {
                    match block {
                        // Tool results using the ToolResult variant
                        ContentBlock::ToolResult { tool_use_id, .. } => {
                            responded_tool_ids.insert(tool_use_id.clone());
                        }
                        // Tool results using the ToolUse variant (legacy format)
                        ContentBlock::ToolUse { id, .. } => {
                            responded_tool_ids.insert(id.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Find orphaned IDs (requests without responses)
    let orphaned_requests: std::collections::HashSet<String> = pending_tool_ids
        .difference(&responded_tool_ids)
        .cloned()
        .collect();

    // Find orphaned responses (results without requests)
    let orphaned_responses: std::collections::HashSet<String> = responded_tool_ids
        .difference(&pending_tool_ids)
        .cloned()
        .collect();

    if !orphaned_requests.is_empty() {
        warnings.push(format!(
            "Found {} orphaned tool call(s) without responses",
            orphaned_requests.len()
        ));
    }

    if !orphaned_responses.is_empty() {
        warnings.push(format!(
            "Found {} orphaned tool result(s) without requests",
            orphaned_responses.len()
        ));
    }

    // Filter out orphaned tool_use and tool_result blocks
    let mut result = Vec::with_capacity(messages.len());
    for msg in messages {
        let content = match &msg.content {
            MessageContent::Blocks(blocks) => {
                let filtered: Vec<ContentBlock> = blocks
                    .iter()
                    .filter(|block| match block {
                        ContentBlock::ToolUse { id, .. } => !orphaned_requests.contains(id),
                        ContentBlock::ToolResult { tool_use_id, .. } => {
                            !orphaned_responses.contains(tool_use_id)
                        }
                        _ => true,
                    })
                    .cloned()
                    .collect();
                MessageContent::Blocks(filtered)
            }
            other => other.clone(),
        };

        // Skip messages that became empty after filtering
        if is_content_empty(&content) && !msg.is_user() {
            warnings.push(format!(
                "Removed {} message that became empty after tool fix",
                msg.role
            ));
            continue;
        }

        result.push(Message {
            role: msg.role,
            content,
            timestamp: msg.timestamp,
            metadata: msg.metadata,
        });
    }

    (result, warnings)
}

/// Merge consecutive messages with the same role.
///
/// Some LLM providers require alternating user/assistant messages.
/// This merges consecutive same-role messages to satisfy that constraint.
pub fn merge_consecutive_messages(messages: Vec<Message>) -> FixResult {
    let mut warnings = Vec::new();
    if messages.is_empty() {
        return (messages, warnings);
    }

    let mut result: Vec<Message> = Vec::with_capacity(messages.len());
    result.push(messages[0].clone());

    for msg in messages.into_iter().skip(1) {
        let last = match result.last() {
            Some(l) => l,
            None => continue,
        };

        if msg.role == last.role {
            // Merge content
            let merged_content = merge_content(&last.content, &msg.content);
            let merged_msg = Message {
                role: last.role.clone(),
                content: merged_content,
                timestamp: last.timestamp,
                metadata: last.metadata,
            };
            warnings.push(format!("Merged consecutive {} messages", msg.role));
            result.pop();
            result.push(merged_msg);
        } else {
            result.push(msg);
        }
    }

    (result, warnings)
}

/// Remove leading and trailing assistant messages.
///
/// Conversations should start with a user or system message and end with
/// a user message (for the next turn). Leading/trailing assistant messages
/// can cause API errors with some providers.
pub fn fix_lead_trail(messages: Vec<Message>) -> FixResult {
    let mut warnings = Vec::new();
    let mut msgs = messages;

    // Remove leading assistant messages
    let leading = msgs.iter().take_while(|m| m.is_assistant()).count();
    if leading > 0 {
        warnings.push(format!("Removed {leading} leading assistant message(s)"));
        msgs.drain(0..leading);
    }

    // Remove trailing assistant messages
    while msgs.last().is_some_and(|m| m.is_assistant()) {
        warnings.push("Removed trailing assistant message".to_string());
        msgs.pop();
    }

    (msgs, warnings)
}

/// Ensure the conversation is not empty.
///
/// If all messages were removed by previous fixers, add a placeholder user message.
pub fn populate_if_empty(messages: Vec<Message>) -> FixResult {
    if messages.is_empty() {
        return (
            vec![Message::user("(conversation continued)")],
            vec!["Conversation was empty, added placeholder message".to_string()],
        );
    }
    (messages, Vec::new())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Check if message content is effectively empty.
fn is_content_empty(content: &MessageContent) -> bool {
    match content {
        MessageContent::Simple(text) => text.trim().is_empty(),
        MessageContent::Blocks(blocks) => {
            if blocks.is_empty() {
                return true;
            }
            blocks.iter().all(|b| match b {
                ContentBlock::Text { text, .. } => text.trim().is_empty(),
                _ => false, // Non-text blocks (images, tool_use) are never "empty"
            })
        }
    }
}

/// Merge two MessageContent values into one.
fn merge_content(a: &MessageContent, b: &MessageContent) -> MessageContent {
    let a_blocks = content_to_blocks(a);
    let b_blocks = content_to_blocks(b);
    let mut merged = a_blocks;
    merged.extend(b_blocks);
    MessageContent::Blocks(merged)
}

/// Convert any MessageContent to a `Vec<ContentBlock>`.
fn content_to_blocks(content: &MessageContent) -> Vec<ContentBlock> {
    match content {
        MessageContent::Simple(text) => {
            if text.is_empty() {
                vec![]
            } else {
                vec![ContentBlock::text(text)]
            }
        }
        MessageContent::Blocks(blocks) => blocks.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn test_merge_text_content_blocks_merges_adjacent() {
        let messages = vec![Message::assistant(MessageContent::Blocks(vec![
            ContentBlock::text("Hello"),
            ContentBlock::text(" World"),
            ContentBlock::tool_use("id1", "bash", json!({"cmd": "ls"})),
            ContentBlock::text(" More text"),
        ]))];

        let (fixed, warnings) = merge_text_content_blocks(messages);
        assert_eq!(fixed.len(), 1);

        match &fixed[0].content {
            MessageContent::Blocks(blocks) => {
                // Should be: merged text, tool_use, more text = 3 blocks
                assert_eq!(blocks.len(), 3);
                assert_eq!(
                    blocks[0],
                    ContentBlock::Text {
                        text: "Hello\n World".to_string(),
                        cache_control: None,
                    }
                );
                assert!(blocks[1].is_tool_use());
                assert_eq!(
                    blocks[2],
                    ContentBlock::Text {
                        text: " More text".to_string(),
                        cache_control: None,
                    }
                );
            }
            _ => panic!("Expected blocks"),
        }
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_merge_text_content_no_merge_needed() {
        let messages = vec![Message::user("Hello")];
        let (fixed, warnings) = merge_text_content_blocks(messages);
        assert_eq!(fixed.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_trim_assistant_whitespace() {
        let messages = vec![
            Message::assistant("Hello   \n\n"),
            Message::user("  keep spaces  "),
        ];

        let (fixed, warnings) = trim_assistant_whitespace(messages);
        assert_eq!(fixed.len(), 2);
        assert_eq!(fixed[0].content.as_text(), "Hello");
        // User messages should NOT be trimmed
        assert_eq!(fixed[1].content.as_text(), "  keep spaces  ");
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_trim_assistant_text_blocks() {
        let messages = vec![Message::assistant(MessageContent::Blocks(vec![
            ContentBlock::text("Line 1  \n"),
            ContentBlock::text("Line 2\n\n"),
        ]))];

        let (fixed, _) = trim_assistant_whitespace(messages);
        match &fixed[0].content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(
                    blocks[0],
                    ContentBlock::Text {
                        text: "Line 1".to_string(),
                        cache_control: None,
                    }
                );
                assert_eq!(
                    blocks[1],
                    ContentBlock::Text {
                        text: "Line 2".to_string(),
                        cache_control: None,
                    }
                );
            }
            _ => panic!("Expected blocks"),
        }
    }

    #[test]
    fn test_remove_empty_messages() {
        let messages = vec![
            Message::user(""),
            Message::assistant("content"),
            Message::user("   "),
            Message::user("real content"),
        ];

        let (fixed, warnings) = remove_empty_messages(messages);
        assert_eq!(fixed.len(), 2);
        assert_eq!(fixed[0].content.as_text(), "content");
        assert_eq!(fixed[1].content.as_text(), "real content");
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_remove_empty_blocks() {
        let messages = vec![Message::assistant(MessageContent::Blocks(vec![
            ContentBlock::text(""),
            ContentBlock::text(""),
        ]))];

        let (fixed, warnings) = remove_empty_messages(messages);
        assert!(fixed.is_empty());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_fix_tool_calling_removes_orphans() {
        let messages = vec![
            Message::user("do something"),
            Message::assistant(MessageContent::Blocks(vec![
                ContentBlock::text("Let me check"),
                ContentBlock::tool_use("orphan-1", "bash", json!({"cmd": "ls"})),
                ContentBlock::tool_use("orphan-2", "read_file", json!({"path": "x"})),
            ])),
            Message::user("next turn"),
        ];

        let (fixed, warnings) = fix_tool_calling(messages);
        assert_eq!(fixed.len(), 3);

        // Assistant message should have text only (tool_use removed)
        match &fixed[1].content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                assert!(blocks[0].is_text());
            }
            _ => panic!("Expected blocks"),
        }
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_fix_tool_calling_keeps_matched() {
        let messages = vec![
            Message::user("do something"),
            Message::assistant(MessageContent::Blocks(vec![ContentBlock::tool_use(
                "call-1",
                "bash",
                json!({"cmd": "ls"}),
            )])),
            Message::user(MessageContent::Blocks(vec![ContentBlock::tool_use(
                "call-1",
                "bash",
                json!({"result": "ok"}),
            )])),
        ];

        let (fixed, warnings) = fix_tool_calling(messages);
        assert_eq!(fixed.len(), 3);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_merge_consecutive_messages() {
        let messages = vec![
            Message::user("first"),
            Message::user("second"),
            Message::assistant("response"),
            Message::assistant("more response"),
        ];

        let (fixed, warnings) = merge_consecutive_messages(messages);
        assert_eq!(fixed.len(), 2);
        assert_eq!(fixed[0].content.as_text(), "first\nsecond");
        assert_eq!(fixed[1].content.as_text(), "response\nmore response");
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_merge_consecutive_no_merge_needed() {
        let messages = vec![Message::user("first"), Message::assistant("response")];

        let (fixed, warnings) = merge_consecutive_messages(messages);
        assert_eq!(fixed.len(), 2);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_fix_lead_trail_removes_leading_assistant() {
        let messages = vec![
            Message::assistant("spurious"),
            Message::user("real question"),
            Message::assistant("answer"),
        ];

        let (fixed, warnings) = fix_lead_trail(messages);
        // Both leading and trailing assistant messages are removed
        assert_eq!(fixed.len(), 1);
        assert!(fixed[0].is_user());
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_fix_lead_trail_removes_trailing_assistant() {
        let messages = vec![
            Message::user("question"),
            Message::assistant("answer"),
            Message::assistant("more answer"),
        ];

        let (fixed, _) = fix_lead_trail(messages);
        assert_eq!(fixed.len(), 1);
        assert!(fixed[0].is_user());
    }

    #[test]
    fn test_fix_lead_trail_preserves_system_first() {
        let messages = vec![
            Message::system("system prompt"),
            Message::user("question"),
            Message::assistant("answer"),
        ];

        let (fixed, _warnings) = fix_lead_trail(messages);
        assert_eq!(fixed.len(), 2); // trailing assistant removed
        assert!(fixed[0].is_system());
        assert!(fixed[1].is_user());
    }

    #[test]
    fn test_populate_if_empty_adds_placeholder() {
        let (fixed, _warnings) = populate_if_empty(vec![]);
        assert_eq!(fixed.len(), 1);
        assert!(fixed[0].is_user());
    }

    #[test]
    fn test_populate_if_empty_preserves_content() {
        let messages = vec![Message::user("existing")];
        let (fixed, warnings) = populate_if_empty(messages);
        assert_eq!(fixed.len(), 1);
        assert!(warnings.is_empty());
    }

    // ── Full Pipeline Tests ──────────────────────────────────────────────────

    #[test]
    fn test_full_pipeline_fixes_malformed_conversation() {
        let messages = vec![
            Message::assistant(""),
            Message::assistant("I'll help  \n"),
            Message::user(""),
            Message::user("do something"),
            Message::assistant(MessageContent::Blocks(vec![
                ContentBlock::text("Let me"),
                ContentBlock::text(" check"),
                ContentBlock::tool_use("orphan", "bash", json!({"cmd": "ls"})),
            ])),
            Message::assistant("Here's more"),
            Message::assistant("  \n"),
        ];

        let (fixed, warnings) = fix_conversation(messages);

        // Should have: user "do something", assistant merged text (orphan tool_use removed)
        assert!(!fixed.is_empty());
        assert!(!warnings.is_empty());

        // First message should NOT be assistant
        assert!(
            fixed.first().is_none_or(|m| !m.is_assistant()),
            "First message should not be assistant"
        );

        // Last message should NOT be assistant
        assert!(
            fixed.last().is_none_or(|m| !m.is_assistant()),
            "Last message should not be assistant"
        );
    }

    #[test]
    fn test_full_pipeline_preserves_good_conversation() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there! How can I help?"),
            Message::user("What is 2+2?"),
            Message::assistant("4"),
        ];

        let (fixed, _warnings) = fix_conversation(messages);
        // Trailing assistant "4" is removed by fix_lead_trail
        // This is correct: conversations should end with user message for next turn
        assert_eq!(fixed.len(), 4);
        assert!(fixed[0].is_system());
        assert!(fixed[1].is_user());
        assert!(fixed[2].is_assistant());
        assert!(fixed[3].is_user());
    }

    #[test]
    fn test_full_pipeline_handles_empty_input() {
        let (fixed, warnings) = fix_conversation(vec![]);
        assert_eq!(fixed.len(), 1);
        assert!(fixed[0].is_user());
        assert!(warnings.iter().any(|w| w.contains("placeholder")));
    }

    #[test]
    fn test_fix_empty_tool_results_fixes_null_input() {
        let messages = vec![
            Message::user("do something"),
            Message::assistant(MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                input: Value::Null,
            }])),
        ];

        let (fixed, warnings) = fix_empty_tool_results(messages);
        assert_eq!(fixed.len(), 2);
        // The tool use should now have empty object instead of null
        match &fixed[1].content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolUse { input, .. } => {
                        assert_eq!(*input, Value::Object(serde_json::Map::new()));
                    }
                    _ => panic!("Expected ToolUse block"),
                }
            }
            _ => panic!("Expected blocks"),
        }
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_fix_empty_tool_results_preserves_valid() {
        let messages = vec![
            Message::user("do something"),
            Message::assistant(MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                input: json!({"command": "ls"}),
            }])),
        ];

        let (fixed, warnings) = fix_empty_tool_results(messages);
        assert_eq!(fixed.len(), 2);
        assert!(warnings.is_empty());
    }
}
