use rustycode_llm::provider::{ChatMessage, MessageRole};
use rustycode_protocol::{ContentBlock, MessageContent};

/// Remove hallucinated tool-use markers from assistant text.
/// The LLM sometimes outputs `[Tool use]` or `[tool_result:...]` patterns
/// that look like structured tool calls but are just text. Strip these
/// so they don't pollute the conversation history.
pub fn clean_assistant_text(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with("[Tool use]") && !line.starts_with("[tool_result:"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Context window budget in characters. Leave ~50K tokens (175K chars) for
/// the response and system overhead. With a 200K token context window,
/// this keeps the conversation under ~150K tokens.
const CONTEXT_BUDGET_CHARS: usize = 400_000;

/// Number of recent turn pairs (assistant + user tool results) to always preserve.
const PRESERVED_RECENT_TURNS: usize = 6;

/// Maximum chars for a tool result in pruned (old) turns.
const PRUNED_TOOL_RESULT_MAX: usize = 500;

/// Maximum chars for error tool results in pruned turns (errors need more context).
const PRUNED_ERROR_RESULT_MAX: usize = 1200;

/// Truncate a string to a maximum character count, preserving the head and tail
/// with a truncation marker in between.
fn truncate_content(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        return content.to_string();
    }
    let half = max_chars / 2;
    let head_end = content
        .char_indices()
        .take_while(|(i, _)| *i < half)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(half.min(content.len()));
    let tail_byte = content.len().saturating_sub(half);
    let tail_start = content
        .char_indices()
        .find(|(i, _)| *i >= tail_byte)
        .map(|(i, _)| i)
        .unwrap_or(tail_byte);
    if tail_start <= head_end {
        content[..head_end].to_string()
    } else {
        format!(
            "{}\n\n... [truncated, {} chars total] ...\n\n{}",
            &content[..head_end],
            content.len(),
            &content[tail_start..]
        )
    }
}

/// Estimate the total character count of a message (rough token proxy).
fn estimate_message_chars(msg: &ChatMessage) -> usize {
    match &msg.content {
        MessageContent::Simple(text) => text.len(),
        MessageContent::Blocks(blocks) => blocks.iter().map(block_char_size).sum(),
        _ => 0,
    }
}

/// Estimate character count of a single content block.
fn block_char_size(block: &ContentBlock) -> usize {
    match block {
        ContentBlock::Text { text, .. } => text.len(),
        ContentBlock::ToolUse { name, input, .. } => name.len() + input.to_string().len(),
        ContentBlock::ToolResult { content, .. } => content.len(),
        ContentBlock::Thinking { thinking, .. } => thinking.len(),
        ContentBlock::Image { .. } => 200,
        _ => 0,
    }
}

/// Prune a tool result block to a short summary, preserving the tool_use_id.
fn summarize_tool_result(block: &ContentBlock) -> ContentBlock {
    match block {
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => {
            let max_chars = if *is_error {
                PRUNED_ERROR_RESULT_MAX
            } else {
                PRUNED_TOOL_RESULT_MAX
            };
            let summarized = if content.len() > max_chars {
                truncate_content(content, max_chars)
            } else {
                content.clone()
            };
            ContentBlock::ToolResult {
                tool_use_id: tool_use_id.clone(),
                content: summarized,
                is_error: *is_error,
            }
        }
        other => other.clone(),
    }
}

/// Prune message history to fit within the context window budget.
///
/// Strategy:
/// 1. Always keep the system prompt (first message)
/// 2. Always keep the most recent `PRESERVED_RECENT_TURNS` turn pairs
/// 3. For older messages, truncate tool results to short summaries
/// 4. If still over budget, remove the oldest prunable messages entirely
pub fn prune_messages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let total_chars: usize = messages.iter().map(estimate_message_chars).sum();

    // If within budget, no pruning needed
    if total_chars <= CONTEXT_BUDGET_CHARS {
        return messages;
    }

    tracing::info!(
        "Pruning messages: {} chars over {} budget",
        total_chars,
        CONTEXT_BUDGET_CHARS
    );

    if messages.is_empty() {
        return messages;
    }

    // Identify boundaries:
    // - msg[0] = initial user task (always keep)
    // - messages[1..] = turn pairs (assistant + tool results)

    let mut result = Vec::with_capacity(messages.len());

    // Always keep initial user task
    let task_end = if !messages.is_empty() {
        result.push(messages[0].clone());
        1
    } else {
        0
    };

    // Remaining messages are turn pairs
    let remaining = &messages[task_end..];
    if remaining.is_empty() {
        return result;
    }

    // Collect turn pairs as (start, len) index ranges
    let turn_ranges = collect_turn_ranges(remaining);

    let total_turns = turn_ranges.len();
    let preserved_from_end = PRESERVED_RECENT_TURNS.min(total_turns);
    let prunable_turns = total_turns.saturating_sub(preserved_from_end);

    // Phase 1: Truncate tool results in old turns
    for &(start, len) in &turn_ranges[..prunable_turns] {
        for msg in &remaining[start..start + len] {
            result.push(prune_message_tool_results(msg));
        }
    }

    // Phase 2: Keep recent turns intact
    for &(start, len) in &turn_ranges[prunable_turns..] {
        for msg in &remaining[start..start + len] {
            result.push(msg.clone());
        }
    }

    // Check if we're now within budget
    let new_total: usize = result.iter().map(estimate_message_chars).sum();
    if new_total <= CONTEXT_BUDGET_CHARS {
        tracing::info!(
            "After pruning tool results: {} chars (within budget)",
            new_total
        );
        return result;
    }

    // Phase 3: Still over budget — remove oldest prunable turns entirely,
    // but first extract a structured summary so the model doesn't lose context.
    tracing::info!(
        "Still over budget ({} chars), removing old turns with summary extraction",
        new_total
    );

    // Determine which old turns will be dropped vs kept
    let preserved_chars: usize = turn_ranges[prunable_turns..]
        .iter()
        .flat_map(|&(start, len)| &remaining[start..start + len])
        .map(estimate_message_chars)
        .sum();

    let chars_so_far: usize = 1; // task message
    let budget_for_old = CONTEXT_BUDGET_CHARS
        .saturating_sub(chars_so_far)
        .saturating_sub(preserved_chars);

    // Find how many old turns fit in budget
    let mut old_chars: usize = 0;
    let mut kept_old_count = 0;
    for &(start, len) in &turn_ranges[..prunable_turns] {
        let pair_chars: usize = remaining[start..start + len]
            .iter()
            .map(estimate_message_chars)
            .sum();
        if old_chars + pair_chars > budget_for_old {
            break;
        }
        old_chars += pair_chars;
        kept_old_count += 1;
    }

    // Extract structured summary from turns that will be dropped
    let dropped_start = kept_old_count;
    let summary = extract_context_summary(remaining, &turn_ranges[dropped_start..prunable_turns]);

    let mut final_result = Vec::new();
    // Keep the initial user task message
    if !result.is_empty() {
        final_result.push(result[0].clone());
    }

    // Inject the structured summary if any turns were dropped
    if !summary.is_empty() {
        tracing::info!(
            "Injecting context summary ({} chars) from {} dropped turns",
            summary.len(),
            prunable_turns - dropped_start
        );
        final_result.push(ChatMessage::user(format!(
            "[CONTEXT SUMMARY — earlier work was trimmed to save space]\n{summary}\n\nContinue from where you left off."
        )));
    }

    // Add pruned old turns that fit within remaining budget
    for &(start, len) in &turn_ranges[..kept_old_count] {
        for msg in &remaining[start..start + len] {
            final_result.push(prune_message_tool_results(msg));
        }
    }

    // Add preserved recent turns
    for &(start, len) in &turn_ranges[prunable_turns..] {
        for msg in &remaining[start..start + len] {
            final_result.push(msg.clone());
        }
    }

    let final_total: usize = final_result.iter().map(estimate_message_chars).sum();
    tracing::info!("After aggressive pruning: {} chars", final_total);

    final_result
}

/// Extract a structured summary from turns that are being dropped.
///
/// Instead of regex-based pattern matching, this inspects the actual tool calls
/// and their results to build a concise summary of what happened:
/// - Files created (write_file with new content)
/// - Files edited (edit_file operations)
/// - Packages installed (bash commands containing "install")
/// - Last error seen (error tool results)
/// - Last success seen (successful bash/write operations)
fn extract_context_summary(messages: &[ChatMessage], turn_ranges: &[(usize, usize)]) -> String {
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
                                    "write_file" | "text_editor_20250728" if !path.is_empty() => {
                                        let entry = path.to_string();
                                        if !files_written.contains(&entry) {
                                            files_written.push(entry);
                                        }
                                    }
                                    "edit_file" | "apply_patch" if !path.is_empty() => {
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
                    if text.contains("write_file") || text.contains("edit_file") {
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
fn collect_turn_ranges(messages: &[ChatMessage]) -> Vec<(usize, usize)> {
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

/// Prune tool results in a single message to reduce size.
fn prune_message_tool_results(msg: &ChatMessage) -> ChatMessage {
    match &msg.content {
        MessageContent::Simple(text) => {
            if text.len() > 300 {
                ChatMessage {
                    role: msg.role.clone(),
                    content: MessageContent::Simple(truncate_content(text, 200)),
                }
            } else {
                msg.clone()
            }
        }
        MessageContent::Blocks(blocks) => {
            let pruned_blocks: Vec<ContentBlock> =
                blocks.iter().map(summarize_tool_result).collect();
            ChatMessage {
                role: msg.role.clone(),
                content: MessageContent::Blocks(pruned_blocks),
            }
        }
        _ => msg.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_assistant_text_removes_tool_markers() {
        let input =
            "Here is my analysis.\n[Tool use]\n[tool_result:bash:abc123] hello\nMore text.\n";
        let cleaned = clean_assistant_text(input);
        assert!(!cleaned.contains("[Tool use]"));
        assert!(!cleaned.contains("[tool_result:"));
        assert!(cleaned.contains("Here is my analysis."));
        assert!(cleaned.contains("More text."));
    }

    #[test]
    fn test_clean_assistant_text_preserves_normal_text() {
        let input = "Hello world\nThis is normal text\nNo markers here";
        let cleaned = clean_assistant_text(input);
        assert_eq!(cleaned, input);
    }

    #[test]
    fn test_truncate_content_short() {
        let content = "short";
        let result = truncate_content(content, 100);
        assert_eq!(result, "short");
    }

    #[test]
    fn test_truncate_content_long() {
        let content = "a".repeat(1000);
        let result = truncate_content(&content, 100);
        assert!(result.contains("[truncated"));
        assert!(result.len() < 200);
    }

    #[test]
    fn test_prune_messages_under_budget() {
        let messages = vec![ChatMessage::user("do the task")];
        let result = prune_messages(messages);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_prune_messages_preserves_task() {
        let mut messages = vec![ChatMessage::user("do the task")];
        // Add many turns with very large tool results to exceed budget
        for i in 0..30 {
            let large_result = format!("Result {}: {}", i, "x".repeat(10000));
            messages.push(ChatMessage::assistant(MessageContent::Blocks(vec![
                ContentBlock::text("thinking..."),
                ContentBlock::ToolUse {
                    id: format!("tool_{}", i),
                    name: "bash".to_string(),
                    input: serde_json::json!({"command": format!("echo {}", i)}),
                },
            ])));
            messages.push(ChatMessage::user(MessageContent::Blocks(vec![
                ContentBlock::tool_result(format!("tool_{}", i), large_result.clone()),
            ])));
        }

        let result = prune_messages(messages);

        // Initial user task preserved
        assert_eq!(result[0].role, MessageRole::User);
        assert!(
            matches!(&result[0].content, MessageContent::Simple(t) if t.contains("do the task"))
        );

        // Total chars should be within budget
        let total: usize = result.iter().map(estimate_message_chars).sum();
        assert!(
            total <= CONTEXT_BUDGET_CHARS + 1000,
            "Total {} exceeds budget {}",
            total,
            CONTEXT_BUDGET_CHARS
        );
    }

    #[test]
    fn test_collect_turn_ranges() {
        let messages = vec![
            ChatMessage::assistant(MessageContent::Simple("a1".into())),
            ChatMessage::user(MessageContent::Simple("u1".into())),
            ChatMessage::assistant(MessageContent::Simple("a2".into())),
            ChatMessage::user(MessageContent::Simple("u2".into())),
        ];
        let ranges = collect_turn_ranges(&messages);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], (0, 2)); // [a1, u1]
        assert_eq!(ranges[1], (2, 2)); // [a2, u2]
    }

    #[test]
    fn test_collect_turn_ranges_with_course_correction() {
        let messages = vec![
            ChatMessage::assistant(MessageContent::Simple("a1".into())),
            ChatMessage::user(MessageContent::Simple("u1".into())),
            // Course correction injected as User message
            ChatMessage::user(MessageContent::Simple("WARNING: stuck".into())),
            ChatMessage::assistant(MessageContent::Simple("a2".into())),
        ];
        let ranges = collect_turn_ranges(&messages);
        assert_eq!(ranges.len(), 3); // [a1,u1], [WARNING], [a2]
    }

    #[test]
    fn test_extract_context_summary_files_and_errors() {
        let messages = vec![
            // Turn 1: write a file
            ChatMessage::assistant(MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "t1".into(),
                name: "write_file".into(),
                input: serde_json::json!({"path": "/app/solution.py", "content": "print('hi')"}),
            }])),
            ChatMessage::user(MessageContent::Blocks(vec![ContentBlock::tool_result(
                "t1",
                "File written successfully",
            )])),
            // Turn 2: edit a file
            ChatMessage::assistant(MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "t2".into(),
                name: "edit_file".into(),
                input: serde_json::json!({"path": "/app/solution.py", "old_string": "hi", "new_string": "hello"}),
            }])),
            ChatMessage::user(MessageContent::Blocks(vec![ContentBlock::tool_result(
                "t2",
                "Edit applied",
            )])),
            // Turn 3: bash error
            ChatMessage::assistant(MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "t3".into(),
                name: "bash".into(),
                input: serde_json::json!({"command": "python3 solution.py"}),
            }])),
            ChatMessage::user(MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "t3".into(),
                content: "Traceback: NameError: name 'x' is not defined".into(),
                is_error: true,
            }])),
        ];

        let ranges = vec![(0, 2), (2, 2), (4, 2)];
        let summary = extract_context_summary(&messages, &ranges);

        assert!(
            summary.contains("Files created: /app/solution.py"),
            "Summary: {summary}"
        );
        assert!(
            summary.contains("Files edited: /app/solution.py"),
            "Summary: {summary}"
        );
        assert!(summary.contains("Last error:"), "Summary: {summary}");
        assert!(summary.contains("NameError"), "Summary: {summary}");
    }

    #[test]
    fn test_extract_context_summary_empty_turns() {
        let messages: Vec<ChatMessage> = vec![];
        let ranges: Vec<(usize, usize)> = vec![];
        let summary = extract_context_summary(&messages, &ranges);
        assert!(summary.is_empty());
    }
}
