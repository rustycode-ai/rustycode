#![allow(
    clippy::bool_to_int_with_if,
    clippy::branches_sharing_code,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::collapsible_else_if,
    clippy::collection_is_never_read,
    clippy::default_trait_access,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::ignore_without_reason,
    clippy::ignored_unit_patterns,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::items_after_statements,
    clippy::iter_on_single_items,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::option_if_let_else,
    clippy::range_plus_one,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::ref_option,
    clippy::search_is_some,
    clippy::semicolon_if_nothing_returned,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_peekable,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::used_underscore_binding,
    clippy::useless_let_if_seq
)]

//! Tests for the conversation message fixing logic in streaming::response.
//!
//! Validates that `fix_conversation_messages` correctly:
//! - Removes leading orphan assistant messages
//! - Removes leading orphaned tool_result messages
//! - Removes trailing assistant messages (without tool_use)
//! - Preserves trailing assistant messages (with tool_use)
//! - Merges consecutive same-role messages
//! - Does not merge tool-related messages
//! - Handles edge cases (empty, all-removed, etc.)

use rustycode_llm::provider::ChatMessage;
use rustycode_protocol::{ContentBlock, MessageContent};

/// Reimplements the fix logic from streaming::response for testing.
/// This must stay in sync with the production implementation.
fn fix_conversation_messages(messages: &mut Vec<ChatMessage>) {
    use rustycode_llm::provider::MessageRole;

    // Remove leading non-system/non-user messages
    while messages
        .first()
        .is_some_and(|m| !matches!(m.role, MessageRole::System | MessageRole::User))
    {
        messages.remove(0);
    }

    // Remove leading orphaned tool_result messages
    while let Some(msg) = messages.first() {
        if msg.role != MessageRole::User {
            break;
        }
        let text = msg.content.as_text();
        if text.contains("\"type\":\"tool_result\"") {
            messages.remove(0);
        } else {
            break;
        }
    }

    // Remove trailing assistant messages only if they DON'T contain tool_use
    while messages.last().is_some_and(|m| {
        if m.role != MessageRole::Assistant {
            return false;
        }
        !matches!(&m.content, MessageContent::Blocks(blocks) if blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { .. })))
    }) {
        messages.pop();
    }

    // Merge consecutive same-role messages (except system and tool-related)
    let mut i = 1;
    while i < messages.len() {
        let prev_has_tools = matches!(&messages[i - 1].content, MessageContent::Blocks(blocks) if blocks.iter().any(|b| b.is_tool_use()));
        let curr_has_tools = matches!(&messages[i].content, MessageContent::Blocks(blocks) if blocks.iter().any(|b| b.is_tool_use()));

        if prev_has_tools || curr_has_tools {
            i += 1;
            continue;
        }

        if messages[i].role == messages[i - 1].role
            && !matches!(messages[i].role, MessageRole::System)
        {
            let merged_content = match (&messages[i - 1].content, &messages[i].content) {
                (MessageContent::Simple(a), MessageContent::Simple(b)) => {
                    MessageContent::Simple(format!("{}\n{}", a, b))
                }
                _ => messages[i].content.clone(),
            };
            messages[i - 1].content = merged_content;
            messages.remove(i);
        } else {
            i += 1;
        }
    }

    if messages.is_empty() {
        messages.push(ChatMessage::user("(conversation continued)".to_string()));
    }
}

#[test]
fn test_removes_leading_assistant_messages() {
    let mut msgs = vec![ChatMessage::assistant("orphan"), ChatMessage::user("hello")];
    fix_conversation_messages(&mut msgs);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content.as_text(), "hello");
}

#[test]
fn test_removes_leading_orphaned_tool_results() {
    let mut msgs = vec![
        ChatMessage::user(r#"{"type":"tool_result","content":"orphan"}"#),
        ChatMessage::user("actual message"),
    ];
    fix_conversation_messages(&mut msgs);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content.as_text(), "actual message");
}

#[test]
fn test_removes_trailing_assistant_without_tool_use() {
    let mut msgs = vec![
        ChatMessage::user("hello"),
        ChatMessage::assistant("trailing 1"),
        ChatMessage::assistant("trailing 2"),
    ];
    fix_conversation_messages(&mut msgs);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content.as_text(), "hello");
}

#[test]
fn test_keeps_trailing_assistant_with_tool_use() {
    let tool_use_block = ContentBlock::ToolUse {
        id: "call-1".to_string(),
        name: "bash".to_string(),
        input: serde_json::json!({"command": "ls"}),
    };
    let mut msgs = vec![
        ChatMessage::user("hello"),
        ChatMessage::assistant(MessageContent::Blocks(vec![tool_use_block])),
    ];
    fix_conversation_messages(&mut msgs);
    assert_eq!(msgs.len(), 2);
}

#[test]
fn test_merges_consecutive_same_role_messages() {
    let mut msgs = vec![
        ChatMessage::user("msg1"),
        ChatMessage::user("msg2"),
        ChatMessage::assistant("resp"),
        ChatMessage::assistant("resp2"),
        ChatMessage::user("reply"),
    ];
    fix_conversation_messages(&mut msgs);
    // merged user + merged assistant + user = 3
    assert_eq!(msgs.len(), 3);
    assert!(msgs[0].content.as_text().contains("msg1"));
    assert!(msgs[0].content.as_text().contains("msg2"));
    assert!(msgs[1].content.as_text().contains("resp"));
    assert!(msgs[1].content.as_text().contains("resp2"));
}

#[test]
fn test_does_not_merge_tool_messages() {
    let tool_use_block = ContentBlock::ToolUse {
        id: "call-1".to_string(),
        name: "bash".to_string(),
        input: serde_json::json!({"command": "ls"}),
    };
    let tool_result_block = ContentBlock::ToolResult {
        tool_use_id: "call-1".to_string(),
        content: "output".to_string(),
        is_error: false,
    };
    let mut msgs = vec![
        ChatMessage::user("hello"),
        ChatMessage::assistant(MessageContent::Blocks(vec![tool_use_block])),
        ChatMessage::user(MessageContent::Blocks(vec![tool_result_block])),
    ];
    fix_conversation_messages(&mut msgs);
    assert_eq!(msgs.len(), 3, "tool-use messages must not be merged");
}

#[test]
fn test_preserves_system_messages() {
    let mut msgs = vec![
        ChatMessage::system("sys1"),
        ChatMessage::system("sys2"),
        ChatMessage::user("hello"),
    ];
    fix_conversation_messages(&mut msgs);
    // System messages should not be merged with each other
    assert_eq!(msgs.len(), 3);
}

#[test]
fn test_empty_messages_gets_default() {
    let mut msgs: Vec<ChatMessage> = vec![];
    fix_conversation_messages(&mut msgs);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content.as_text(), "(conversation continued)");
}

#[test]
fn test_complex_reordering_scenario() {
    let tool_use_block = ContentBlock::ToolUse {
        id: "call-1".to_string(),
        name: "read".to_string(),
        input: serde_json::json!({"path": "/tmp/test"}),
    };
    let mut msgs = vec![
        // Leading assistant should be removed
        ChatMessage::assistant("orphan leading"),
        // Orphaned tool result should be removed
        ChatMessage::user(r#"{"type":"tool_result","content":"old"}"#),
        // This user message stays as new first
        ChatMessage::user("hello"),
        // Two consecutive user messages get merged
        ChatMessage::user("world"),
        // Assistant with tool_use stays (trailing with tool_use kept)
        ChatMessage::assistant(MessageContent::Blocks(vec![tool_use_block])),
    ];
    fix_conversation_messages(&mut msgs);
    assert_eq!(
        msgs.len(),
        2,
        "should have merged user + tool-use assistant"
    );
    assert!(msgs[0].content.as_text().contains("hello"));
    assert!(msgs[0].content.as_text().contains("world"));
}
