//! Command Palette Integration Test
//!
//! This test verifies the command palette component works correctly
//! and can be integrated into a TUI application.

#[cfg(test)]
mod integration_tests {
    use super::super::super::ui::command_palette::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_full_command_palette_workflow() {
        // Create palette with custom commands
        let commands = vec![
            Command::new("test1", "First test command", || CommandResult::Success),
            Command::new("test2", "Second test command", || {
                CommandResult::SuccessWithMessage("Test 2 executed".to_string())
            }),
            Command::new("exit", "Exit test", || CommandResult::Close),
        ];

        let mut palette = CommandPalette::with_commands(commands);

        // Test 1: Initial state
        assert!(!palette.is_visible());
        assert_eq!(palette.state().query, "");
        assert_eq!(palette.state().filtered_count(), 3);

        // Test 2: Show palette
        palette.show();
        assert!(palette.is_visible());
        assert_eq!(palette.state().selected_index, 0);

        // Test 3: Type to filter
        palette.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        palette.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(palette.state().query, "te");
        assert_eq!(palette.state().filtered_count(), 2); // test1, test2

        // Test 4: Navigate
        palette.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(palette.state().selected_index, 1);

        palette.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(palette.state().selected_index, 0);

        // Test 5: Execute command
        let cmd = palette.take_selected();
        assert!(cmd.is_some());
        assert_eq!(cmd.as_ref().unwrap().name, "test1");

        let result = cmd.unwrap().execute();
        assert!(matches!(result, CommandResult::Success));

        // Test 6: Palette should close after execution
        assert!(!palette.is_visible());

        // Test 7: Backspace
        palette.show();
        palette.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        palette.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert_eq!(palette.state().query, "xy");

        palette.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(palette.state().query, "x");

        // Test 8: Clear with Ctrl+U
        palette.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(palette.state().query, "");

        // Test 9: Escape closes
        palette.show();
        palette.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!palette.is_visible());
    }

    #[test]
    fn test_fuzzy_matching_ranking() {
        let matcher = FuzzyMatcher::new();

        let commands = vec![
            Command::new("help", "Show help dialog", || CommandResult::Success),
            Command::new("hello", "Say hello", || CommandResult::Success),
            Command::new("helper", "Helper function", || CommandResult::Success),
            Command::new("exit", "Exit application", || CommandResult::Success),
        ];

        // Test exact match
        let score = matcher.match_score("help", &commands[0]);
        assert_eq!(score, MatchScore::Exact);

        // Test prefix match
        let score = matcher.match_score("hel", &commands[0]);
        assert_eq!(score, MatchScore::Prefix);

        // Test substring match
        let score = matcher.match_score("elp", &commands[0]);
        assert_eq!(score, MatchScore::Substring);

        // Test description match
        let score = matcher.match_score("dialog", &commands[0]);
        assert_eq!(score, MatchScore::Substring);

        // Test no match
        let score = matcher.match_score("xyz", &commands[0]);
        assert_eq!(score, MatchScore::None);

        // Test ranking
        let matches = matcher.filter_commands("hel", &commands);
        assert_eq!(matches.len(), 3); // help, hello, helper

        // help should be ranked highest (exact match)
        assert_eq!(matches[0].0, 0);

        // helper should be ranked second (prefix)
        assert_eq!(matches[1].0, 2);

        // hello should be ranked third (prefix)
        assert_eq!(matches[2].0, 1);
    }

    #[test]
    fn test_match_highlighting() {
        let matcher = FuzzyMatcher::new();

        // Test exact match highlighting
        let line = matcher.highlight_matches("help", "help");
        // Should have highlighted span
        assert!(line.spans.len() > 0);

        // Test substring highlighting
        let line = matcher.highlight_matches("el", "help");
        assert!(line.spans.len() > 0);

        // Test no match (should return original text)
        let line = matcher.highlight_matches("xyz", "help");
        assert_eq!(line.spans.len(), 1); // Single span with original text
    }

    #[test]
    fn test_command_results() {
        let cmd1 = Command::new("success", "Success command", || CommandResult::Success);
        let cmd2 = Command::new("msg", "Message command", || {
            CommandResult::SuccessWithMessage("Test message".to_string())
        });
        let cmd3 = Command::new("error", "Error command", || {
            CommandResult::Error("Test error".to_string())
        });
        let cmd4 = Command::new("close", "Close command", || CommandResult::Close);

        // Test execution
        let result1 = cmd1.execute();
        assert!(matches!(result1, CommandResult::Success));

        let result2 = cmd2.execute();
        assert!(matches!(result2, CommandResult::SuccessWithMessage(_)));

        let result3 = cmd3.execute();
        assert!(matches!(result3, CommandResult::Error(_)));

        let result4 = cmd4.execute();
        assert!(matches!(result4, CommandResult::Close));

        // Test should_close
        assert!(!result1.should_close());
        assert!(!result2.should_close());
        assert!(!result3.should_close());
        assert!(result4.should_close());
    }

    #[test]
    fn test_vim_key_bindings() {
        let mut palette = CommandPalette::new();
        palette.show();

        // Test 'j' for down
        palette.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(palette.state().selected_index, 1);

        // Test 'k' for up
        palette.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(palette.state().selected_index, 0);
    }

    #[test]
    fn test_empty_query_shows_all() {
        let mut palette = CommandPalette::new();
        palette.show();

        // Empty query should show all commands
        assert_eq!(palette.state().query, "");
        assert!(palette.state().filtered_count() > 0);

        // Type something
        palette.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(palette.state().query, "x");

        // Clear query
        palette.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(palette.state().query, "");

        // Should show all commands again
        assert!(palette.state().filtered_count() > 0);
    }

    #[test]
    fn test_selection_bounds() {
        let mut palette = CommandPalette::new();
        palette.show();

        let count = palette.state().filtered_count();

        // Try to go beyond bounds
        for _ in 0..count + 10 {
            palette.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }

        // Should be clamped to last item
        assert_eq!(palette.state().selected_index, count.saturating_sub(1));

        // Try to go below 0
        for _ in 0..count + 10 {
            palette.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        }

        // Should be clamped to 0
        assert_eq!(palette.state().selected_index, 0);
    }
}
