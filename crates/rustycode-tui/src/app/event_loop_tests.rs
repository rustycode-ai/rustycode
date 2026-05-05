//! Tests for message handling and tool execution
//!
//! These tests ensure that message structures and tool operations work correctly
//! and handle None values gracefully.

use crate::services::agent_mode::AiMode;
use crate::app::event_loop::TUI;
use crate::ui::input::InputMode;
use crate::ui::message_types::{ExpansionLevel, Message, MessageRole, ToolExecution, ToolStatus};
use std::path::PathBuf;

#[test]
fn test_tui_creation() {
    let (tx, _) = tokio::sync::broadcast::channel(1);
    let tui = TUI::new(PathBuf::from("."), AiMode::default(), false, tx.subscribe());
    assert!(tui.is_ok());
}

#[test]
fn test_default_tui() {
    let tui = TUI::default();
    assert_eq!(tui.messages.len(), 0);
    assert_eq!(tui.input_mode, InputMode::SingleLine);
}

#[test]
fn test_render_message_without_tools() {
    // Create a message without tool_executions (None)
    let message = Message::new(MessageRole::User, "Hello, world!".to_string());

    // Verify tool_executions is None
    assert!(message.tool_executions.is_none());

    // Verify we can check has_thinking without panicking
    assert!(!message.has_thinking());
}

#[test]
fn test_render_message_without_thinking() {
    // Create a message without thinking (None)
    let message = Message::new(MessageRole::Assistant, "Hi there!".to_string());

    // Verify thinking is None
    assert!(message.thinking.is_none());

    // Verify has_thinking returns false
    assert!(!message.has_thinking());
}

#[test]
fn test_render_message_with_empty_tools() {
    // Create a message with empty tool_executions
    let mut message = Message::new(MessageRole::Assistant, "Done!".to_string());

    // Set tools to empty array
    message.tool_executions = Some(vec![]);

    // Verify tool_executions is Some but empty
    assert!(message.tool_executions.is_some());
    assert_eq!(message.tool_executions.as_ref().unwrap().len(), 0);
}

#[test]
fn test_render_message_with_thinking() {
    // Create a message with thinking
    let mut message = Message::new(MessageRole::Assistant, "Answer!".to_string());

    // Set thinking
    message.thinking = Some("Let me think...".to_string());

    // Verify thinking is Some
    assert!(message.thinking.is_some());
    assert!(message.has_thinking());
}

#[test]
fn test_render_all_message_states() {
    // Test message without tools or thinking
    let msg1 = Message::new(MessageRole::User, "Test".to_string());
    assert!(msg1.tool_executions.is_none());
    assert!(msg1.thinking.is_none());
    assert!(!msg1.has_thinking());

    // Test message with tools only
    let tool = ToolExecution::new(
        "tool_1".to_string(),
        "test_tool".to_string(),
        "test_tool: success (5b)".to_string(),
    );
    let msg2 = Message::new(MessageRole::Assistant, "Test".to_string()).with_tools(vec![tool]);
    assert!(msg2.tool_executions.is_some());
    assert!(msg2.thinking.is_none());
    assert!(!msg2.has_thinking());

    // Test message with thinking only
    let msg3 = Message::new(MessageRole::Assistant, "Test".to_string())
        .with_thinking("thinking...".to_string());
    assert!(msg3.tool_executions.is_none());
    assert!(msg3.thinking.is_some());
    assert!(msg3.has_thinking());

    // Test message with both
    let tool = ToolExecution::new(
        "tool_1".to_string(),
        "tool".to_string(),
        "tool: result".to_string(),
    );
    let msg4 = Message::new(MessageRole::Assistant, "Test".to_string())
        .with_tools(vec![tool])
        .with_thinking("thinking...".to_string());
    assert!(msg4.tool_executions.is_some());
    assert!(msg4.thinking.is_some());
    assert!(msg4.has_thinking());
}

#[test]
fn test_message_defaults() {
    // Test that Message::new() creates valid defaults
    let message = Message::new(MessageRole::User, "Test".to_string());

    // Verify all Option fields are None by default
    assert!(message.tool_executions.is_none());
    assert!(message.thinking.is_none());
    assert_eq!(message.tools_expansion, ExpansionLevel::Collapsed);
    assert_eq!(message.thinking_expansion, ExpansionLevel::Collapsed);
    assert!(message.focused_tool_index.is_none());
    assert!(!message.collapsed);
}

#[test]
fn test_message_with_tools_but_none_thinking() {
    // Test a common case: assistant message with tools but no thinking
    let mut tool = ToolExecution::new(
        "tool_1".to_string(),
        "read_file".to_string(),
        "read_file: src/main.rs (145b)".to_string(),
    );
    tool.complete(Some("file contents".to_string()));

    let message = Message::new(
        MessageRole::Assistant,
        "I'll help you with that.".to_string(),
    )
    .with_tools(vec![tool]);

    // This should not panic
    assert!(message.tool_executions.is_some());
    assert!(message.thinking.is_none());
    assert!(!message.has_thinking());
}

#[test]
fn test_message_expansion_levels() {
    let message = Message::new(MessageRole::User, "Test".to_string());

    // Test that expansion levels work correctly
    assert_eq!(message.tools_expansion, ExpansionLevel::Collapsed);
    assert_eq!(message.thinking_expansion, ExpansionLevel::Collapsed);
}

#[test]
fn test_tool_status_enum() {
    // Test that ToolStatus enum values exist and have correct icons
    assert_eq!(ToolStatus::Running.icon(), "◐");
    assert_eq!(ToolStatus::Complete.icon(), "●");
    assert_eq!(ToolStatus::Failed.icon(), "✗");
}

#[test]
fn test_tool_execution_lifecycle() {
    let mut tool = ToolExecution::new(
        "tool_1".to_string(),
        "test_tool".to_string(),
        "test_tool: starting".to_string(),
    );

    // Initially should be Running
    assert_eq!(tool.status, ToolStatus::Running);
    assert!(tool.end_time.is_none());
    assert!(tool.duration_ms.is_none());

    // Complete the tool
    tool.complete(Some("Test output".to_string()));
    assert_eq!(tool.status, ToolStatus::Complete);
    assert!(tool.end_time.is_some());
    assert!(tool.duration_ms.is_some());
    assert_eq!(tool.detailed_output, Some("Test output".to_string()));
}

#[test]
fn test_tool_execution_failure() {
    let mut tool = ToolExecution::new(
        "tool_1".to_string(),
        "failing_tool".to_string(),
        "failing_tool: starting".to_string(),
    );

    // Initially should be Running
    assert_eq!(tool.status, ToolStatus::Running);

    // Fail the tool
    tool.fail("Tool execution failed".to_string());
    assert_eq!(tool.status, ToolStatus::Failed);
    assert!(tool.end_time.is_some());
    assert!(tool.duration_ms.is_some());
}

#[test]
fn test_message_toggle_tools_expansion() {
    let mut message = Message::new(MessageRole::Assistant, "Test".to_string());

    // Start collapsed
    assert_eq!(message.tools_expansion, ExpansionLevel::Collapsed);

    // Toggle to expanded
    message.toggle_tools_expansion();
    assert_eq!(message.tools_expansion, ExpansionLevel::Expanded);

    // Toggle back to collapsed
    message.toggle_tools_expansion();
    assert_eq!(message.tools_expansion, ExpansionLevel::Collapsed);
}

#[test]
fn test_message_toggle_thinking_expansion() {
    let mut message = Message::new(MessageRole::Assistant, "Test".to_string())
        .with_thinking("Let me think...".to_string());

    // Start collapsed
    assert_eq!(message.thinking_expansion, ExpansionLevel::Collapsed);

    // Toggle to expanded
    message.toggle_thinking_expansion();
    assert_eq!(message.thinking_expansion, ExpansionLevel::Expanded);

    // Toggle back to collapsed
    message.toggle_thinking_expansion();
    assert_eq!(message.thinking_expansion, ExpansionLevel::Collapsed);
}
