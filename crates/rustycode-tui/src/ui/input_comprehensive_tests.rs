//! Comprehensive tests for input handling.
//!
//! This module provides extensive testing for:
//! - Rapid typing performance
//! - Unicode and grapheme cluster handling
//! - Multi-line mode switching
//! - Command history management
//! - Special characters and edge cases

#[cfg(test)]
mod comprehensive_tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use std::time::Instant;

    #[test]
    fn test_rapid_typing_performance() {
        let mut handler = InputHandler::new();
        let start = Instant::now();

        // Simulate rapid typing (1000 chars)
        for c in 'a'..='z' {
            for _ in 0..40 {
                handler.handle_key_event(KeyCode::Char(c), KeyModifiers::NONE);
            }
        }

        let duration = start.elapsed();
        assert!(
            duration.as_millis() < 100,
            "Rapid typing too slow: {:?}",
            duration
        );

        // Verify all characters were inserted
        assert_eq!(handler.state.all_text().len(), 1000);
    }

    #[test]
    fn test_unicode_cursor_movement() {
        let mut handler = InputHandler::new();
        handler.state.insert_str("สวัสดีชาวโลก"); // Thai: "Hello world"

        // Test cursor moves by grapheme, not byte
        let start_col = handler.state.cursor_col;
        handler.handle_key_event(KeyCode::Right, KeyModifiers::NONE);

        // Should move by 1 grapheme cluster, not 1 byte
        // Thai characters are multi-byte, so movement should be more than 1 byte
        assert_ne!(handler.state.cursor_col, start_col + 1);

        // Verify we can move back and forth
        let moved_col = handler.state.cursor_col;
        handler.handle_key_event(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(handler.state.cursor_col, start_col);

        // Move right again to verify consistency
        handler.handle_key_event(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(handler.state.cursor_col, moved_col);
    }

    #[test]
    fn test_emoji_cursor_movement() {
        let mut handler = InputHandler::new();
        handler.state.insert_str("Hello 👋 World 🌍");

        // Move through emoji
        handler.handle_key_event(KeyCode::Left, KeyModifiers::NONE); // Move back from end
        let before_emoji = handler.state.cursor_col;

        // Move past the emoji (should jump entire grapheme cluster)
        handler.handle_key_event(KeyCode::Left, KeyModifiers::NONE);
        let after_emoji = handler.state.cursor_col;

        // The difference should be more than 1 byte (emoji are multi-byte)
        assert!(after_emoji < before_emoji);
    }

    #[test]
    fn test_command_history_with_duplicates() {
        let mut handler = InputHandler::new();

        // Add same command twice
        handler.add_to_history("test command".to_string());
        handler.add_to_history("test command".to_string());

        // Should only appear once in history
        assert_eq!(handler.history.len(), 1);

        // Add different command
        handler.add_to_history("another command".to_string());
        assert_eq!(handler.history.len(), 2);

        // Add first command again - should not duplicate
        handler.add_to_history("test command".to_string());
        assert_eq!(handler.history.len(), 2);
    }

    #[test]
    fn test_multiline_mode_switching() {
        let mut handler = InputHandler::new();

        // Start in single-line mode
        assert_eq!(handler.state.mode, InputMode::SingleLine);

        // Enter multiline mode with Alt+Enter
        handler.handle_key_event(KeyCode::Enter, KeyModifiers::ALT);
        assert_eq!(handler.state.mode, InputMode::MultiLine);

        // Exit with Esc
        handler.handle_key_event(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(handler.state.mode, InputMode::SingleLine);
    }

    #[test]
    fn test_multiline_navigation() {
        let mut handler = InputHandler::new();

        // Enter multiline mode
        handler.state.mode = InputMode::MultiLine;
        handler.state.insert_str("Line 1");
        handler.state.lines.push(String::new());
        handler.state.cursor_row = 1;
        handler.state.cursor_col = 0;
        handler.state.insert_str("Line 2");

        // Should have 2 lines
        assert_eq!(handler.state.lines.len(), 2);
        assert_eq!(handler.state.all_text(), "Line 1\nLine 2");

        // Move up
        handler.handle_key_event(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(handler.state.cursor_row, 0);

        // Move down
        handler.handle_key_event(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(handler.state.cursor_row, 1);
    }

    #[test]
    fn test_backspace_deletes_grapheme() {
        let mut handler = InputHandler::new();
        handler.state.insert_str("Hello 👋");

        // Move to end
        handler.state.cursor_col = handler.state.current_line().len();

        // Backspace should delete entire emoji
        handler.handle_key_event(KeyCode::Backspace, KeyModifiers::NONE);

        // Should have deleted the emoji
        assert_eq!(handler.state.all_text(), "Hello ");
    }

    #[test]
    fn test_delete_key_deletes_grapheme() {
        let mut handler = InputHandler::new();
        handler.state.insert_str("👋 World");

        // Move to start
        handler.state.cursor_col = 0;

        // Delete should remove entire emoji
        handler.handle_key_event(KeyCode::Delete, KeyModifiers::NONE);

        // Should have deleted the emoji
        assert_eq!(handler.state.all_text(), " World");
    }

    #[test]
    fn test_empty_input_does_not_send() {
        let mut handler = InputHandler::new();

        // Try to send empty message
        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

        // Should not send empty message
        match action {
            InputAction::SendMessage(text) => {
                panic!("Should not send empty message, got: {:?}", text);
            }
            _ => {}
        }
    }

    #[test]
    fn test_very_long_line() {
        let mut handler = InputHandler::new();
        let long_text = "A".repeat(10000);
        handler.state.insert_str(&long_text);

        // Should handle gracefully
        assert_eq!(handler.state.all_text().len(), 10000);

        // Cursor movement should still work
        handler.state.cursor_col = 5000;
        handler.handle_key_event(KeyCode::Left, KeyModifiers::NONE);
        assert!(handler.state.cursor_col < 5000);
    }

    #[test]
    fn test_special_characters() {
        let mut handler = InputHandler::new();

        let special = "!@#$%^&*()_+-=[]{}|;':\",./<>?`~";
        handler.state.insert_str(special);

        assert_eq!(handler.state.all_text(), special);
    }

    #[test]
    fn test_null_bytes_handling() {
        let mut handler = InputHandler::new();

        // Insert text with null byte
        handler.state.insert_str("Hello\0World");

        // Should handle or strip null bytes
        let text = handler.state.all_text();
        assert!(!text.contains('\0'), "Null bytes should be handled");
    }

    #[test]
    fn test_mixed_scripts() {
        let mut handler = InputHandler::new();

        // Mix of Latin, Thai, Emoji
        handler.state.insert_str("Hello สวัสดี 👋");

        // Cursor movement should work correctly
        handler.state.cursor_col = handler.state.current_line().len();
        handler.handle_key_event(KeyCode::Left, KeyModifiers::NONE);
        handler.handle_key_event(KeyCode::Left, KeyModifiers::NONE);

        // Should have moved correctly through mixed scripts
        assert!(handler.state.cursor_col > 0);
    }

    #[test]
    fn test_arabic_right_to_left() {
        let mut handler = InputHandler::new();

        // Arabic text (right-to-left)
        handler.state.insert_str("مرحبا بالعالم");

        // Should handle RTL text
        assert_eq!(handler.state.all_text(), "مرحبا بالعالم");

        // Cursor movement should work
        handler.state.cursor_col = handler.state.current_line().len();
        handler.handle_key_event(KeyCode::Left, KeyModifiers::NONE);
        assert!(handler.state.cursor_col < handler.state.current_line().len());
    }

    #[test]
    fn test_history_navigation() {
        let mut handler = InputHandler::new();

        handler.add_to_history("first command".to_string());
        handler.add_to_history("second command".to_string());
        handler.add_to_history("third command".to_string());

        // Start with empty input
        assert_eq!(handler.state.all_text(), "");

        // Navigate back through history
        handler.handle_key_event(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(handler.state.all_text(), "third command");

        handler.handle_key_event(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(handler.state.all_text(), "second command");

        // Navigate forward
        handler.handle_key_event(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(handler.state.all_text(), "third command");
    }

    #[test]
    fn test_insert_at_cursor() {
        let mut handler = InputHandler::new();
        handler.state.insert_str("Hello");

        // Move cursor to middle
        handler.state.cursor_col = 2;

        // Insert at cursor
        handler.state.insert_char('X');

        assert_eq!(handler.state.all_text(), "HXello");
    }

    #[test]
    fn test_enter_behavior_in_modes() {
        let mut handler = InputHandler::new();
        handler.state.insert_str("test");

        // In single-line mode, Enter should send
        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert!(matches!(action, InputAction::SendMessage(_)));

        // In multi-line mode, Enter should insert newline
        handler.state.mode = InputMode::MultiLine;
        handler.state.insert_str("test");
        handler.state.cursor_col = handler.state.current_line().len();

        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);
        // Should not send in multiline mode
        assert!(!matches!(action, InputAction::SendMessage(_)));
    }

    #[test]
    fn test_word_navigation() {
        let mut handler = InputHandler::new();
        handler.state.insert_str("Hello world test");

        // Move to end
        handler.state.cursor_col = handler.state.current_line().len();

        // Ctrl+Left should move back by word
        handler.handle_key_event(KeyCode::Left, KeyModifiers::CONTROL);

        // Should have moved back past "test"
        assert!(handler.state.cursor_col < handler.state.current_line().len());
    }

    #[test]
    fn test_zwj_sequences() {
        let mut handler = InputHandler::new();

        // Zero-width joiner sequences (e.g., skin tone modifiers)
        handler.state.insert_str("👨‍👩‍👧‍👦"); // Family emoji (ZWJ sequence)

        // Should handle as single grapheme
        let text = handler.state.all_text();
        assert_eq!(text, "👨‍👩‍👧‍👦");

        // Cursor should move by entire sequence
        handler.state.cursor_col = text.len();
        handler.handle_key_event(KeyCode::Left, KeyModifiers::NONE);

        // Should be at start now (moved past entire ZWJ sequence)
        assert_eq!(handler.state.cursor_col, 0);
    }
}
