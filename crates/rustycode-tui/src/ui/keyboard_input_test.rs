//! Integration tests for keyboard input processing and slash commands.
//!
//! This module tests the complete keyboard input flow:
//! - crossterm event reception
//! - InputHandler key event processing
//! - Slash command detection and execution
//! - Enter key submission behavior

use crate::ui::input::{InputHandler, InputAction, InputMode};
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn test_input_handler_single_line_enter_submits() {
    let mut handler = InputHandler::new();

    // Type some text
    handler.handle_key_event(KeyCode::Char('h'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('e'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('p'), KeyModifiers::NONE);

    // Press Enter in single-line mode - should submit
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

    match action {
        InputAction::SendMessage(lines) => {
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0], "help");
        }
        _ => panic!("Expected SendMessage action, got {:?}", action),
    }
}

#[test]
fn test_input_handler_slash_command_detection() {
    let mut handler = InputHandler::new();

    // Type "/help"
    handler.handle_key_event(KeyCode::Char('/'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('h'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('e'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('p'), KeyModifiers::NONE);

    // Press Enter - should submit with slash command
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

    match action {
        InputAction::SendMessage(lines) => {
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0], "/help");
            assert!(lines[0].starts_with('/'));
        }
        _ => panic!("Expected SendMessage action for slash command, got {:?}", action),
    }
}

#[test]
fn test_input_handler_multiline_toggle() {
    let mut handler = InputHandler::new();

    // Initially in single-line mode
    assert_eq!(handler.state.mode, InputMode::SingleLine);

    // Type some text
    handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('i'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('n'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('e'), KeyModifiers::NONE);

    // Press Option+Enter - should switch to multi-line and insert newline
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::ALT);

    match action {
        InputAction::Consumed => {
            // Should now be in multi-line mode
            assert_eq!(handler.state.mode, InputMode::MultiLine);
            assert_eq!(handler.state.lines.len(), 2);
        }
        _ => panic!("Expected Consumed action for mode toggle, got {:?}", action),
    }
}

#[test]
fn test_input_handler_multiline_enter_inserts_newline() {
    let mut handler = InputHandler::new();

    // Switch to multi-line mode
    handler.handle_key_event(KeyCode::Enter, KeyModifiers::ALT);
    assert_eq!(handler.state.mode, InputMode::MultiLine);

    // Type some text
    handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('i'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('n'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('e'), KeyModifiers::NONE);

    // Press Enter - should insert newline, not submit
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

    match action {
        InputAction::Consumed => {
            // Should still be in multi-line mode with more lines
            assert_eq!(handler.state.mode, InputMode::MultiLine);
            assert_eq!(handler.state.lines.len(), 2);
        }
        _ => panic!("Expected Consumed action for newline insert, got {:?}", action),
    }
}

#[test]
fn test_input_handler_multiline_option_enter_submits() {
    let mut handler = InputHandler::new();

    // Switch to multi-line mode
    handler.handle_key_event(KeyCode::Enter, KeyModifiers::ALT);
    assert_eq!(handler.state.mode, InputMode::MultiLine);

    // Type two lines
    handler.handle_key_event(KeyCode::Char('f'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('i'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('r'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('s'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('t'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

    handler.handle_key_event(KeyCode::Char('s'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('e'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('c'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('o'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('n'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('d'), KeyModifiers::NONE);

    // Press Option+Enter - should submit both lines
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::ALT);

    match action {
        InputAction::SendMessage(lines) => {
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0], "first");
            assert_eq!(lines[1], "second");
        }
        _ => panic!("Expected SendMessage action, got {:?}", action),
    }
}

#[test]
fn test_input_handler_backspace_deletes() {
    let mut handler = InputHandler::new();

    // Type "hello"
    handler.handle_key_event(KeyCode::Char('h'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('e'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('o'), KeyModifiers::NONE);

    // Press backspace twice
    handler.handle_key_event(KeyCode::Backspace, KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Backspace, KeyModifiers::NONE);

    // Should have "hel"
    assert_eq!(handler.state.lines[0], "hel");
}

#[test]
fn test_input_handler_empty_input_rejected() {
    let mut handler = InputHandler::new();

    // Press Enter without typing anything
    let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

    match action {
        InputAction::SendMessage(lines) => {
            // Should have empty content
            assert!(lines.is_empty() || lines.iter().all(|l| l.trim().is_empty()));
        }
        _ => panic!("Expected SendMessage action, got {:?}", action),
    }
}

#[test]
fn test_input_handler_various_slash_commands() {
    let commands = vec![
        "/help",
        "/review",
        "/task list",
        "/todo add test",
        "/agent analyze",
    ];

    for cmd in commands {
        let mut handler = InputHandler::new();

        // Type the command character by character
        for ch in cmd.chars() {
            handler.handle_key_event(KeyCode::Char(ch), KeyModifiers::NONE);
        }

        // Press Enter
        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

        match action {
            InputAction::SendMessage(lines) => {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0], cmd);
                assert!(lines[0].starts_with('/'), "Command should start with /");
            }
            _ => panic!("Expected SendMessage for command '{}', got {:?}", cmd, action),
        }
    }
}

#[test]
fn test_input_handler_ctrl_c ignored() {
    let mut handler = InputHandler::new();

    // Type some text
    handler.handle_key_event(KeyCode::Char('t'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('e'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('x'), KeyModifiers::NONE);
    handler.handle_key_event(KeyCode::Char('t'), KeyModifiers::NONE);

    // Press Ctrl+C - should be ignored (passed to global shortcuts)
    let action = handler.handle_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);

    match action {
        InputAction::Ignored => {
            // Expected - Ctrl+C is handled by global shortcuts
        }
        _ => panic!("Expected Ignored action for Ctrl+C, got {:?}", action),
    }
}
