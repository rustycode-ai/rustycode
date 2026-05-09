//! Context summary extraction for compaction.
//!
//! Shared utility used by `rustycode-core` and `rustycode-agent-runtime`
//! to extract structured summaries from conversation messages during
//! context trimming (compaction).

use crate::provider::{ChatMessage, MessageRole};
use rustycode_protocol::{ContentBlock, MessageContent};

/// Extract a structured summary from message turns that are about to be dropped.
///
/// Instead of regex-based pattern matching, this inspects the actual tool calls
/// and their results to build a concise summary of what happened:
/// - Files created (write_file with new content)
/// - Files edited (edit_file operations)
/// - Packages installed (bash commands containing "install")
/// - Last error seen (error tool results)
/// - Last success seen (successful bash/write operations)
pub fn extract_context_summary(
    messages: &[ChatMessage],
    turn_ranges: &[(usize, usize)],
) -> String {
    let mut files_written: Vec<String> = Vec::new();
    let mut files_edited: Vec<String> = Vec::new();
    let mut last_error: Option<String> = None;
    let mut last_success: Option<String> = None;

    for &(start, len) in turn_ranges {
        let slice = &messages[start..start + len];
        for msg in slice {
            match &msg.content {
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::ToolUse { name, input, .. } => {
                                let path = input
                                    .get("path")
                                    .or_else(|| input.get("file_path"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                match name.as_str() {
                                    "Write" | "text_editor_20250728" if !path.is_empty() => {
                                        let entry = path.to_string();
                                        if !files_written.contains(&entry) {
                                            files_written.push(entry);
                                        }
                                    }
                                    "Edit" | "apply_patch" if !path.is_empty() => {
                                        let entry = path.to_string();
                                        if !files_edited.contains(&entry) {
                                            files_edited.push(entry);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            ContentBlock::ToolResult {
                                content, is_error, ..
                            } => {
                                if *is_error {
                                    // Keep last ~200 chars of error
                                    let err: String = content
                                        .chars()
                                        .rev()
                                        .take(200)
                                        .collect::<Vec<_>>()
                                        .into_iter()
                                        .rev()
                                        .collect();
                                    last_error = Some(err);
                                } else if content.lines().count() <= 5 && content.len() < 200 {
                                    // Short successful results are useful context
                                    last_success = Some(content.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                MessageContent::Simple(text) => {
                    // Check assistant text for tool calls embedded as text
                    if text.contains("Write") || text.contains("Edit") {
                        let lower = text.to_lowercase();
                        if lower.contains("success") || lower.contains("wrote") {
                            let info: String = text.chars().take(150).collect();
                            last_success = Some(info);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let mut parts = Vec::new();
    if !files_written.is_empty() {
        parts.push(format!("Files created: {}", files_written.join(", ")));
    }
    if !files_edited.is_empty() {
        parts.push(format!("Files edited: {}", files_edited.join(", ")));
    }
    if let Some(ref err) = last_error {
        let err_trimmed = err.trim();
        if !err_trimmed.is_empty() {
            parts.push(format!("Last error: {err_trimmed}"));
        }
    }
    if let Some(ref succ) = last_success {
        let succ_trimmed = succ.trim();
        if !succ_trimmed.is_empty() {
            parts.push(format!("Last success: {succ_trimmed}"));
        }
    }

    parts.join("\n")
}

/// Collect turn ranges as (start_index, length) pairs from a message slice.
pub fn collect_turn_ranges(messages: &[ChatMessage]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        if messages[i].role == MessageRole::Assistant {
            let start = i;
            i += 1;
            // May have user message following
            if i < messages.len() && messages[i].role == MessageRole::User {
                i += 1;
            }
            ranges.push((start, i - start));
        } else {
            // Unexpected role (course correction), treat as single
            ranges.push((i, 1));
            i += 1;
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_turn_ranges() {
        let messages = vec![
            ChatMessage::user(MessageContent::Simple("u1".into())),
            ChatMessage::assistant(MessageContent::Simple("a1".into())),
            ChatMessage::user(MessageContent::Simple("u2".into())),
        ];
        let ranges = collect_turn_ranges(&messages);
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn test_collect_turn_ranges_with_course_correction() {
        let messages = vec![
            ChatMessage::user(MessageContent::Simple("u1".into())),
            ChatMessage::user(MessageContent::Simple("WARNING: stuck".into())),
            ChatMessage::assistant(MessageContent::Simple("a1".into())),
        ];
        let ranges = collect_turn_ranges(&messages);
        assert_eq!(ranges.len(), 3);
    }

    #[test]
    fn test_extract_context_summary_files_and_errors() {
        let messages = vec![
            ChatMessage::assistant(MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "1".into(),
                name: "Write".into(),
                input: serde_json::json!({"path": "/tmp/test.rs", "content": "fn main() {}"}),
            }])),
            ChatMessage::user(MessageContent::Blocks(vec![ContentBlock::tool_result(
                "1",
                "wrote file",
            )])),
            ChatMessage::assistant(MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "2".into(),
                name: "Edit".into(),
                input: serde_json::json!({"path": "/tmp/test.rs", "old": "fn", "new": "pub fn"}),
            }])),
            ChatMessage::user(MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "2".into(),
                content: "error: not found".into(),
                is_error: true,
            }])),
        ];
        let ranges = collect_turn_ranges(&messages);
        let summary = extract_context_summary(&messages, &ranges);
        assert!(summary.contains("Files created: /tmp/test.rs"));
        assert!(summary.contains("Files edited: /tmp/test.rs"));
        assert!(summary.contains("Last error:"));
    }

    #[test]
    fn test_extract_context_summary_empty_turns() {
        let messages: Vec<ChatMessage> = vec![];
        let summary = extract_context_summary(&messages, &[]);
        assert!(summary.is_empty());
    }
}
