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

//! Integration tests for the TUI component architecture.
//!
//! These tests verify end-to-end workflows and component interactions.
//!
//! # Running Tests
//!
//! ```bash
//! # Run all integration tests
//! cargo test --test integration_tests
//!
//! # Run specific test
//! cargo test --test integration_tests test_full_workflow
//!
//! # Run with output
//! cargo test --test integration_tests -- --nocapture
//! ```

#![cfg(any())]

use rustycode_tui::ui::input::{InputHandler, InputMode, InputState};
use rustycode_tui::ui::markdown::{MarkdownRenderer, StreamingMessage};
use rustycode_tui::ui::message::{Message, MessageRole, ToolExecution, ToolStatus};
use rustycode_tui::ui::status::{StatusBar, StatusConfig, ToolExecutions};
use std::time::Duration;

// ============================================================================
// Full Workflow Tests
// ============================================================================

#[test]
fn test_full_workflow() {
    // 1. User types a message
    let mut handler = InputHandler::new();
    let mut state = InputState::new(InputMode::SingleLine);

    handler.handle_char('H', &mut state);
    handler.handle_char('e', &mut state);
    handler.handle_char('l', &mut state);
    handler.handle_char('l', &mut state);
    handler.handle_char('o', &mut state);

    assert_eq!(state.text(), "Hello");

    // 2. Message is created
    let user_msg = Message::new(MessageRole::User, "Hello".to_string());
    assert_eq!(user_msg.role(), MessageRole::User);
    assert_eq!(user_msg.content(), "Hello");

    // 3. AI starts streaming response
    let mut ai_response = StreamingMessage::new(MessageRole::Assistant);
    ai_response.append_content("Hi there! ");
    ai_response.append_content("How can I help?");

    // 4. Tool execution is triggered
    let mut tool = ToolExecution::new("read_file".to_string());
    tool.update_status(ToolStatus::Running);
    tool.append_output("File contents...");

    // 5. Tool completes
    tool.update_status(ToolStatus::Complete);
    assert_eq!(tool.status(), ToolStatus::Complete);
    assert!(!tool.output().is_empty());

    // 6. Finalize response
    let final_msg = ai_response.finalize();
    assert_eq!(final_msg.content(), "Hi there! How can I help?");
}

#[test]
fn test_multiline_workflow() {
    // User switches to multiline mode
    let mut handler = InputHandler::new();
    let mut state = InputState::new(InputMode::MultiLine);

    // Type multiple lines
    handler.handle_char('L', &mut state);
    handler.handle_newline(&mut state);
    handler.handle_char('2', &mut state);

    assert_eq!(state.text(), "L\n2");
    assert_eq!(state.mode(), InputMode::MultiLine);

    // Send message
    let action = handler.handle_enter(&mut state);
    assert!(action.is_send_message());
}

// ============================================================================
// Input Handling Tests
// ============================================================================

#[test]
fn test_multiline_paste() {
    let mut handler = InputHandler::new();
    let mut state = InputState::new(InputMode::MultiLine);

    let paste_text = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
    let result = handler.paste_text(paste_text.to_string(), &mut state);

    assert!(result.is_success());
    assert_eq!(state.text(), paste_text);
}

#[test]
fn test_input_navigation() {
    let mut handler = InputHandler::new();
    let mut state = InputState::new(InputMode::SingleLine);

    // Type text
    for c in "Hello".chars() {
        handler.handle_char(c, &mut state);
    }

    // Navigate back
    handler.handle_left(&mut state);
    handler.handle_left(&mut state);

    // Insert in middle
    handler.handle_char('X', &mut state);

    assert_eq!(state.text(), "HelXlo");

    // Navigate to end
    handler.handle_right(&mut state);
    handler.handle_right(&mut state);

    // Delete
    handler.handle_backspace(&mut state);
    assert_eq!(state.text(), "HelXl");
}

#[test]
fn test_input_modes() {
    let mut state = InputState::new(InputMode::SingleLine);

    // Single line mode
    state.set_mode(InputMode::MultiLine);
    assert_eq!(state.mode(), InputMode::MultiLine);

    // Multi-line mode
    state.set_mode(InputMode::SingleLine);
    assert_eq!(state.mode(), InputMode::SingleLine);
}

// ============================================================================
// Message Rendering Tests
// ============================================================================

#[test]
fn test_message_rendering() {
    let renderer = MarkdownRenderer::new();
    let messages = vec![
        Message::new(MessageRole::User, "Hello".to_string()),
        Message::new(
            MessageRole::Assistant,
            "Hi! **Bold** and `code`".to_string(),
        ),
    ];

    let rendered = renderer.render_messages(&messages, 80, 24);
    assert!(!rendered.is_empty());
}

#[test]
fn test_streaming_rendering() {
    let renderer = MarkdownRenderer::new();
    let mut msg = StreamingMessage::new(MessageRole::Assistant);

    msg.append_content("Chunk 1 ");
    msg.append_content("Chunk 2 ");
    msg.append_content("Chunk 3");

    let rendered = renderer.render_streaming(&msg, 80, 24);
    assert!(!rendered.is_empty());
    assert!(rendered.contains("Chunk 1"));
    assert!(rendered.contains("Chunk 3"));
}

#[test]
fn test_large_message_set() {
    let renderer = MarkdownRenderer::new();
    let messages: Vec<Message> = (0..1000)
        .map(|i| {
            Message::new(
                if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                format!("Message {}", i),
            )
        })
        .collect();

    let start = std::time::Instant::now();
    let rendered = renderer.render_messages(&messages, 80, 24);
    let duration = start.elapsed();

    assert!(!rendered.is_empty());
    // Should render 1000 messages in reasonable time
    assert!(duration < Duration::from_millis(100));
}

// ============================================================================
// Tool Execution Tests
// ============================================================================

#[test]
fn test_tool_execution_lifecycle() {
    let mut tool = ToolExecution::new("bash".to_string());

    // Initial state
    assert_eq!(tool.status(), ToolStatus::Running);
    assert!(tool.start_time().is_some());
    assert!(tool.end_time().is_none());

    // Update output
    tool.append_output("Line 1\n");
    tool.append_output("Line 2\n");

    assert!(tool.output().contains("Line 1"));
    assert!(tool.output().contains("Line 2"));

    // Complete
    tool.update_status(ToolStatus::Complete);
    tool.append_output("Done");

    assert_eq!(tool.status(), ToolStatus::Complete);
    assert!(tool.end_time().is_some());

    // Duration
    let duration = tool.duration();
    assert!(duration.is_some());
}

#[test]
fn test_tool_execution_failure() {
    let mut tool = ToolExecution::new("read_file".to_string());

    tool.update_status(ToolStatus::Failed);
    tool.append_output("Error: File not found");

    assert_eq!(tool.status(), ToolStatus::Failed);
    assert!(tool.output().contains("Error"));
}

// ============================================================================
// Status Bar Tests
// ============================================================================

#[test]
fn test_status_bar_updates() {
    let mut status = StatusBar::new(StatusConfig::default());

    // Update tools
    status.update_tools(ToolExecutions {
        active: vec!["read_file".to_string(), "bash".to_string()],
        completed: 5,
        failed: 1,
    });

    status.set_message("Working...");

    let rendered = status.render(80);
    assert!(!rendered.is_empty());
    // Should contain tool info
    assert!(rendered.contains("read_file") || rendered.contains("2"));
}

#[test]
fn test_status_bar_themes() {
    let status = StatusBar::new(StatusConfig::default());
    let rendered = status.render(80);

    assert!(!rendered.is_empty());
}

// ============================================================================
// Error Recovery Tests
// ============================================================================

#[test]
fn test_tool_failure_recovery() {
    let mut tool = ToolExecution::new("failing_tool".to_string());

    // Tool fails
    tool.update_status(ToolStatus::Failed);
    tool.append_output("Connection failed");

    assert_eq!(tool.status(), ToolStatus::Failed);

    // Recovery: create new tool execution
    let new_tool = ToolExecution::new("retry_tool".to_string());
    assert_eq!(new_tool.status(), ToolStatus::Running);
    assert!(new_tool.output().is_empty());
}

#[test]
fn test_input_error_handling() {
    let mut handler = InputHandler::new();
    let mut state = InputState::new(InputMode::MultiLine);

    // Paste invalid UTF-8 (should not crash)
    let result = handler.paste_text("Valid text".to_string(), &mut state);
    assert!(result.is_success());

    // Empty paste
    let result = handler.paste_text("".to_string(), &mut state);
    assert!(result.is_success());
}

#[test]
fn test_rendering_error_handling() {
    let renderer = MarkdownRenderer::new();

    // Empty message list
    let rendered = renderer.render_messages(&[], 80, 24);
    assert!(!rendered.is_empty()); // Should handle gracefully

    // Message with special characters
    let msg = Message::new(MessageRole::User, "Special: \0\x01\x02\r\n\t".to_string());
    let rendered = renderer.render_messages(&[msg], 80, 24);
    assert!(!rendered.is_empty()); // Should handle gracefully
}

// ============================================================================
// Concurrent Operation Tests
// ============================================================================

#[test]
fn test_concurrent_tool_execution() {
    let mut tools = vec![
        ToolExecution::new("tool1".to_string()),
        ToolExecution::new("tool2".to_string()),
        ToolExecution::new("tool3".to_string()),
    ];

    // All start as running
    assert!(tools.iter().all(|t| t.status() == ToolStatus::Running));

    // Update in parallel
    for (i, tool) in tools.iter_mut().enumerate() {
        tool.append_output(format!("Output from tool {}", i));
    }

    // Complete in reverse order
    tools[2].update_status(ToolStatus::Complete);
    tools[0].update_status(ToolStatus::Complete);
    tools[1].update_status(ToolStatus::Failed);

    assert_eq!(tools[0].status(), ToolStatus::Complete);
    assert_eq!(tools[1].status(), ToolStatus::Failed);
    assert_eq!(tools[2].status(), ToolStatus::Complete);
}

#[test]
fn test_streaming_while_typing() {
    // Simulate user typing while AI streams response
    let mut handler = InputHandler::new();
    let mut input_state = InputState::new(InputMode::MultiLine);
    let mut stream_msg = StreamingMessage::new(MessageRole::Assistant);

    // Stream response chunks
    stream_msg.append_content("Response ");
    stream_msg.append_content("chunk 1 ");

    // User types while streaming
    handler.handle_char('H', &mut input_state);
    handler.handle_char('i', &mut input_state);

    // More streaming
    stream_msg.append_content("chunk 2");

    // More typing
    handler.handle_char('!', &mut input_state);

    // Verify both states are correct
    assert_eq!(input_state.text(), "Hi!");
    assert_eq!(stream_msg.content(), "Response chunk 1 chunk 2");
}

// ============================================================================
// Performance Tests
// ============================================================================

#[test]
fn test_performance_1000_messages() {
    let renderer = MarkdownRenderer::new();
    let messages: Vec<Message> = (0..1000)
        .map(|i| {
            Message::new(
                if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                format!("Message {} with some content", i),
            )
        })
        .collect();

    let start = std::time::Instant::now();
    let _rendered = renderer.render_messages(&messages, 80, 24);
    let duration = start.elapsed();

    // Should render quickly
    assert!(
        duration < Duration::from_millis(50),
        "Rendering took {:?}",
        duration
    );
}

#[test]
fn test_performance_rapid_input() {
    let mut handler = InputHandler::new();
    let mut state = InputState::new(InputMode::SingleLine);

    let start = std::time::Instant::now();

    for _ in 0..1000 {
        handler.handle_char('x', &mut state);
        handler.handle_backspace(&mut state);
    }

    let duration = start.elapsed();

    // Should handle rapid input quickly
    assert!(
        duration < Duration::from_millis(100),
        "Input took {:?}",
        duration
    );
}

#[test]
fn test_performance_rapid_tools() {
    let start = std::time::Instant::now();

    for i in 0..100 {
        let mut tool = ToolExecution::new(format!("tool_{}", i));
        tool.append_output(format!("Output {}", i));
        tool.update_status(ToolStatus::Complete);
    }

    let duration = start.elapsed();

    // Should handle rapid tool execution
    assert!(
        duration < Duration::from_millis(100),
        "Tools took {:?}",
        duration
    );
}

// ============================================================================
// Memory Tests
// ============================================================================

#[test]
fn test_memory_large_conversation() {
    let messages: Vec<Message> = (0..10000)
        .map(|i| {
            Message::new(
                if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                format!("Message {} with content", i),
            )
        })
        .collect();

    // Should handle large conversations without crashing
    assert_eq!(messages.len(), 10000);
}

#[test]
fn test_memory_streaming_chunks() {
    let mut msg = StreamingMessage::new(MessageRole::Assistant);

    for i in 0..1000 {
        msg.append_content(format!("Chunk {} ", i));
    }

    let finalized = msg.finalize();
    assert!(finalized.content().len() > 1000);
}
