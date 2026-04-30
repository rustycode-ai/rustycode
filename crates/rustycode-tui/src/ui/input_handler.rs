//! Input handling logic with keyboard events and paste support.
//!
//! This module provides:
//! - InputAction for key event results
//! - InputHandler for keyboard input and state management
//! - Coordinates paste and history management via sub-modules

use crossterm::event::{KeyCode, KeyModifiers};

// Re-export from sibling modules
pub use super::input_paste::{PasteHandler, PasteResult};
pub use super::input_state::{InputMode, InputState};

// Private imports for the coordinator
use super::input_history::HistoryManager;

// ── Input Actions ───────────────────────────────────────────────────────────

/// Result of handling a key event
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum InputAction {
    /// Send message with lines
    SendMessage(Vec<String>),
    /// Open the command palette for slash commands
    OpenCommandPalette,
    /// Open the skill palette for skill mentions
    OpenSkillPalette,
    /// Event was consumed (input changed)
    Consumed,
    /// Event was ignored (pass through)
    Ignored,
    /// Browse history backward
    HistoryPrevious,
    /// Browse history forward
    HistoryNext,
    /// Reverse search in history (Ctrl+R)
    SearchReverse,
    /// Remove image
    RemoveImage(String),
}

// ── Input Handler ───────────────────────────────────────────────────────────

/// Handles keyboard input and state management
#[derive(Debug)]
pub struct InputHandler {
    /// Current input state
    pub state: InputState,
    /// Paste handler
    paste_handler: Option<PasteHandler>,
    /// History manager
    history: HistoryManager,
    /// Last killed text (Ctrl+W / Ctrl+K) for Ctrl+Y yank
    last_kill: Option<String>,
}

impl Drop for InputHandler {
    fn drop(&mut self) {
        // Cleanup temp files on drop
        self.state.cleanup();
    }
}

impl InputHandler {
    /// Create new input handler
    pub fn new() -> Self {
        Self {
            state: InputState::new(),
            paste_handler: PasteHandler::new().ok(),
            history: HistoryManager::new(),
            last_kill: None,
        }
    }

    /// Set command history (load from disk)
    pub fn set_history(&mut self, history: Vec<String>) {
        self.history.set_history(history);
    }

    /// Add command to history
    pub fn add_to_history(&mut self, command: String) {
        self.history.add_to_history(command);
    }

    /// Get command history for persistence
    pub fn get_history(&self) -> &[String] {
        self.history.get_history()
    }

    /// Check if currently browsing history
    pub fn is_browsing_history(&self) -> bool {
        self.history.is_browsing_history()
    }

    /// Get current history position (1-indexed for display)
    pub fn history_position(&self) -> (usize, usize) {
        self.history.history_position()
    }

    /// Check if currently in reverse search mode
    pub fn is_in_reverse_search(&self) -> bool {
        self.history.is_in_reverse_search()
    }

    /// Get reverse search query and match info
    pub fn reverse_search_info(&self) -> (String, usize, usize) {
        self.history.reverse_search_info()
    }

    /// Handle key event
    pub fn handle_key_event(&mut self, key_code: KeyCode, modifiers: KeyModifiers) -> InputAction {
        tracing::debug!("Key event: code={:?}, modifiers={:?}", key_code, modifiers);
        match (key_code, modifiers) {
            // === Multi-line handling ===
            (KeyCode::Enter, KeyModifiers::ALT) => {
                // Option+Enter: Toggle input mode
                match self.state.mode {
                    InputMode::SingleLine => {
                        self.state.mode = InputMode::MultiLine;
                        self.state.insert_newline();
                        InputAction::Consumed
                    }
                    InputMode::MultiLine => {
                        // Send message - avoid clone by collecting into Vec
                        let lines: Vec<String> = self.state.lines.to_vec();
                        InputAction::SendMessage(lines)
                    }
                }
            }

            (KeyCode::Enter, KeyModifiers::SHIFT) => {
                // Shift+Enter: insert newline in both modes (matches hint text)
                if self.state.mode == InputMode::SingleLine {
                    self.state.mode = InputMode::MultiLine;
                }
                self.state.insert_newline();
                InputAction::Consumed
            }

            (KeyCode::Enter, KeyModifiers::CONTROL) => {
                // Ctrl+Enter: Send message in both modes (standard multi-line submit)
                let lines: Vec<String> = self.state.lines.to_vec();
                InputAction::SendMessage(lines)
            }

            (KeyCode::Enter, KeyModifiers::NONE) => {
                // Handle reverse search mode
                if self.history.is_in_reverse_search() {
                    if self.history.has_reverse_search_match() {
                        // Accept current match and send
                        let text = self.state.all_text();
                        self.history.exit_reverse_search(&mut self.state);
                        InputAction::SendMessage(vec![text])
                    } else {
                        // No matches: exit search mode, restoring the original
                        // pending_input. The "no matches" display text must never
                        // be submitted as a message.
                        self.history.exit_reverse_search(&mut self.state);
                        InputAction::Consumed
                    }
                } else {
                    // Plain Enter: behavior depends on mode
                    match self.state.mode {
                        InputMode::SingleLine => {
                            // Send immediately
                            InputAction::SendMessage(vec![self.state.current_line()])
                        }
                        InputMode::MultiLine => {
                            // Insert newline
                            self.state.insert_newline();
                            InputAction::Consumed
                        }
                    }
                }
            }

            // === Paste handling ===
            (KeyCode::Char('v'), KeyModifiers::CONTROL) => {
                // Ctrl+V: Paste from clipboard
                if let Some(ref mut handler) = self.paste_handler {
                    if let Ok(result) = handler.handle_paste(&mut self.state) {
                        return match result {
                            PasteResult::Text => InputAction::Consumed,
                            PasteResult::Image => InputAction::Consumed,
                            PasteResult::None => InputAction::Ignored,
                        };
                    }
                }
                InputAction::Ignored
            }

            // === Reverse search (Ctrl+R) ===
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                if self.history.is_in_reverse_search() {
                    // Ctrl+R again: cycle to next match
                    self.history.cycle_reverse_search_match(&mut self.state);
                    InputAction::Consumed
                } else {
                    // Start reverse search
                    self.history.start_reverse_search(&mut self.state);
                    InputAction::SearchReverse
                }
            }

            // === Normal typing ===
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                // Handle reverse search mode
                if self.history.is_in_reverse_search() {
                    self.history.add_reverse_search_char(&mut self.state, c);
                    InputAction::Consumed
                } else {
                    // Exit history mode when typing
                    self.history.exit_history_mode();

                    // Note: '/' and '@' are inserted as normal characters so users can type
                    // slash commands like /help, /quit, etc. directly. Command/skill palettes
                    // are opened via keyboard shortcuts (Ctrl+Shift+P, Ctrl+Shift+S) or by
                    // submitting a bare "/" on Enter.

                    if c == 'x' && !self.state.images.is_empty() {
                        if let Some(img) = self.state.images.last() {
                            let id = img.id.clone();
                            self.state.remove_image(&id);
                            InputAction::Consumed
                        } else {
                            InputAction::Ignored
                        }
                    } else {
                        self.state.insert_char(c);
                        InputAction::Consumed
                    }
                }
            }

            // === Navigation ===
            (KeyCode::Up, KeyModifiers::NONE) => {
                if self.state.mode == InputMode::MultiLine {
                    self.state.move_cursor_up();
                    InputAction::Consumed
                } else {
                    self.history.navigate_previous(&mut self.state);
                    InputAction::Consumed
                }
            }

            (KeyCode::Down, KeyModifiers::NONE) => {
                if self.state.mode == InputMode::MultiLine {
                    self.state.move_cursor_down();
                    InputAction::Consumed
                } else {
                    self.history.navigate_next(&mut self.state);
                    InputAction::Consumed
                }
            }

            (KeyCode::Left, KeyModifiers::NONE) => {
                self.state.move_cursor_left();
                InputAction::Consumed
            }

            (KeyCode::Right, KeyModifiers::NONE) => {
                self.state.move_cursor_right();
                InputAction::Consumed
            }

            // === Backspace/Delete ===
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                if self.history.is_in_reverse_search() {
                    self.history.remove_reverse_search_char(&mut self.state);
                    InputAction::Consumed
                } else {
                    self.history.exit_history_mode();
                    self.state.backspace();
                    InputAction::Consumed
                }
            }

            (KeyCode::Delete, KeyModifiers::NONE) => {
                self.history.exit_history_mode();
                self.state.delete();
                InputAction::Consumed
            }

            (KeyCode::Backspace, KeyModifiers::CONTROL) => {
                // Ctrl+Backspace: Delete word backward
                self.history.exit_history_mode();
                self.state.delete_word_backward();
                InputAction::Consumed
            }

            (KeyCode::Delete, KeyModifiers::CONTROL) => {
                // Ctrl+Delete: Delete word forward
                self.history.exit_history_mode();
                self.state.delete_word_forward();
                InputAction::Consumed
            }

            // === Escape to exit multi-line or reverse search ===
            (KeyCode::Esc, KeyModifiers::NONE) => {
                if self.history.is_in_reverse_search() {
                    // Exit reverse search mode
                    self.history.exit_reverse_search(&mut self.state);
                    InputAction::Consumed
                } else if self.state.mode == InputMode::MultiLine {
                    self.state.mode = InputMode::SingleLine;
                    self.state.flatten_to_single_line();
                    InputAction::Consumed
                } else {
                    InputAction::Ignored
                }
            }

            // === Home/End ===
            (KeyCode::Home, KeyModifiers::NONE) => {
                self.state.cursor_col = 0;
                InputAction::Consumed
            }

            (KeyCode::End, KeyModifiers::NONE) => {
                if let Some(line) = self.state.lines.get(self.state.cursor_row) {
                    self.state.cursor_col = line.len();
                }
                InputAction::Consumed
            }

            // === Readline-style keybindings ===
            (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                // Ctrl+A: Go to beginning of line
                self.state.cursor_col = 0;
                InputAction::Consumed
            }

            (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                // Ctrl+E: Go to end of line
                if let Some(line) = self.state.lines.get(self.state.cursor_row) {
                    self.state.cursor_col = line.len();
                }
                InputAction::Consumed
            }

            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                // Ctrl+U: Clear line (kill entire line)
                if let Some(line) = self.state.lines.get_mut(self.state.cursor_row) {
                    line.clear();
                    self.state.cursor_col = 0;
                }
                InputAction::Consumed
            }

            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                self.history.exit_history_mode();
                let deleted = self.state.delete_word_backward();
                if !deleted.is_empty() {
                    self.last_kill = Some(deleted);
                }
                InputAction::Consumed
            }

            (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                if let Some(line) = self.state.lines.get_mut(self.state.cursor_row) {
                    let col = line.floor_char_boundary(self.state.cursor_col);
                    let killed = line[col..].to_string();
                    line.truncate(col);
                    if !killed.is_empty() {
                        self.last_kill = Some(killed);
                    }
                }
                InputAction::Consumed
            }

            (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                // Ctrl+L: Clear entire input
                self.state.clear();
                InputAction::Consumed
            }

            _ => InputAction::Ignored,
        }
    }

    /// Clear input and reset to initial state
    pub fn clear(&mut self) {
        self.state.clear();
    }

    /// Clear input and add to history (call before sending a message)
    pub fn clear_and_save_to_history(&mut self) {
        let text = self.state.all_text();
        if !text.trim().is_empty() {
            self.add_to_history(text);
        }
        self.state.clear();
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_plain_enter_single_line() {
        let mut handler = InputHandler::new();
        handler.state.lines[0] = "Hello".to_string();

        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

        assert!(matches!(action, InputAction::SendMessage(_)));
        if let InputAction::SendMessage(lines) = action {
            assert_eq!(lines, vec!["Hello"]);
        }
    }

    #[test]
    fn test_handler_plain_enter_multi_line() {
        let mut handler = InputHandler::new();
        handler.state.mode = InputMode::MultiLine;
        handler.state.lines[0] = "Hello".to_string();

        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.lines.len(), 2); // Newline added
    }

    #[test]
    fn test_handler_option_enter_single_line() {
        let mut handler = InputHandler::new();
        handler.state.lines[0] = "Hi".to_string();

        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::ALT);

        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.mode, InputMode::MultiLine);
        assert_eq!(handler.state.lines.len(), 2); // Newline added
    }

    #[test]
    fn test_handler_option_enter_multi_line() {
        let mut handler = InputHandler::new();
        handler.state.mode = InputMode::MultiLine;
        handler.state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];

        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::ALT);

        assert!(matches!(action, InputAction::SendMessage(_)));
    }

    #[test]
    fn test_handler_shift_enter_inserts_newline() {
        // Shift+Enter should insert newline (not send), matching hint text
        let mut handler = InputHandler::new();
        handler.state.lines[0] = "Hello".to_string();
        handler.state.cursor_col = 5; // cursor at end of line

        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::SHIFT);
        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.mode, InputMode::MultiLine);
        assert_eq!(handler.state.lines.len(), 2);
        assert_eq!(handler.state.lines[0], "Hello");
        assert_eq!(handler.state.lines[1], "");
    }

    #[test]
    fn test_handler_shift_enter_in_multiline() {
        // Shift+Enter in multiline mode should also insert newline
        let mut handler = InputHandler::new();
        handler.state.mode = InputMode::MultiLine;
        handler.state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];

        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::SHIFT);
        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.lines.len(), 3);
    }

    #[test]
    fn test_handler_up_single_line() {
        let mut handler = InputHandler::new();
        let action = handler.handle_key_event(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(action, InputAction::Consumed);
    }

    #[test]
    fn test_handler_slash_inserts_as_normal_char() {
        let mut handler = InputHandler::new();

        let action = handler.handle_key_event(KeyCode::Char('/'), KeyModifiers::NONE);

        // '/' is inserted as a normal character so users can type commands like /help directly
        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.all_text(), "/");
    }

    #[test]
    fn test_handler_at_inserts_as_normal_char() {
        let mut handler = InputHandler::new();

        let action = handler.handle_key_event(KeyCode::Char('@'), KeyModifiers::NONE);

        // '@' is inserted as a normal character
        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.all_text(), "@");
    }

    #[test]
    fn test_handler_up_multi_line() {
        let mut handler = InputHandler::new();
        handler.state.mode = InputMode::MultiLine;
        handler.state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];
        handler.state.cursor_row = 1;

        let action = handler.handle_key_event(KeyCode::Up, KeyModifiers::NONE);

        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.cursor_row, 0);
    }

    #[test]
    fn test_handler_esc_exits_multiline() {
        let mut handler = InputHandler::new();
        handler.state.mode = InputMode::MultiLine;
        handler.state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];

        let action = handler.handle_key_event(KeyCode::Esc, KeyModifiers::NONE);

        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.mode, InputMode::SingleLine);
        assert_eq!(handler.state.lines.len(), 1); // Collapsed
        assert_eq!(handler.state.lines[0], "Line 1 Line 2"); // Joined
    }

    #[test]
    fn test_home_end() {
        let mut handler = InputHandler::new();
        handler.state.lines[0] = "Hello".to_string();
        handler.state.cursor_col = 2;

        let action = handler.handle_key_event(KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.cursor_col, 0);

        let action = handler.handle_key_event(KeyCode::End, KeyModifiers::NONE);
        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.cursor_col, 5);
    }

    // === Reverse Search Tests ===

    #[test]
    fn test_reverse_search_basic() {
        let mut handler = InputHandler::new();
        handler.set_history(vec![
            "command one".to_string(),
            "test command".to_string(),
            "another test".to_string(),
        ]);

        // Start reverse search
        handler.history.start_reverse_search(&mut handler.state);
        assert!(handler.is_in_reverse_search());
        assert_eq!(handler.history.reverse_search_info().0, "");

        // Should show most recent command (last in history)
        assert_eq!(handler.state.all_text(), "another test");
    }

    #[test]
    fn test_reverse_search_filter() {
        let mut handler = InputHandler::new();
        handler.set_history(vec![
            "command one".to_string(),
            "test command".to_string(),
            "another test".to_string(),
        ]);

        handler.history.start_reverse_search(&mut handler.state);

        // Type "test" to filter
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'e');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 's');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');

        // Should find 2 matches containing "test" (most recent first)
        assert_eq!(handler.history.reverse_search_matches.len(), 2);
    }

    #[test]
    fn test_reverse_search_cycle_matches() {
        let mut handler = InputHandler::new();
        handler.set_history(vec![
            "command one".to_string(),
            "test command".to_string(),
            "another test".to_string(),
        ]);

        handler.history.start_reverse_search(&mut handler.state);
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'e');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 's');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');

        // Initially at first match
        assert_eq!(handler.history.reverse_search_index, 0);
        assert_eq!(handler.state.all_text(), "another test");

        // Cycle to next match
        handler
            .history
            .cycle_reverse_search_match(&mut handler.state);
        assert_eq!(handler.history.reverse_search_index, 1);
        assert_eq!(handler.state.all_text(), "test command");

        // Cycle back to first match
        handler
            .history
            .cycle_reverse_search_match(&mut handler.state);
        assert_eq!(handler.history.reverse_search_index, 0);
    }

    #[test]
    fn test_reverse_search_exit() {
        let mut handler = InputHandler::new();
        handler.set_history(vec!["command one".to_string(), "test command".to_string()]);

        // Set some pending input
        handler.state.lines = vec!["pending".to_string()];
        handler.state.cursor_col = 7;

        // Exit reverse search
        handler.history.exit_reverse_search(&mut handler.state);
        assert!(!handler.is_in_reverse_search());

        // Should restore pending input
        assert_eq!(handler.state.all_text(), "pending");
    }

    #[test]
    fn test_reverse_search_backspace() {
        let mut handler = InputHandler::new();
        handler.set_history(vec![
            "command one".to_string(),
            "test command".to_string(),
            "another test".to_string(),
        ]);

        handler.history.start_reverse_search(&mut handler.state);

        // Type "test"
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'e');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 's');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');

        // Backspace to "tes"
        handler
            .history
            .remove_reverse_search_char(&mut handler.state);

        // Should still find matches (assuming "tes" still matches)
        assert_eq!(handler.history.reverse_search_info().0, "tes");
    }

    #[test]
    fn test_reverse_search_no_matches() {
        let mut handler = InputHandler::new();
        handler.set_history(vec!["command one".to_string(), "command two".to_string()]);

        handler.history.start_reverse_search(&mut handler.state);

        // Type something that doesn't match
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'x');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'y');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'z');

        // Should have no matches
        assert_eq!(handler.history.reverse_search_matches.len(), 0);
    }

    #[test]
    fn test_reverse_search_enter_no_matches_does_not_send() {
        let mut handler = InputHandler::new();
        handler.set_history(vec!["command one".to_string(), "command two".to_string()]);

        // Set up pending input that should be restored
        handler.state.lines = vec!["my pending work".to_string()];
        handler.state.cursor_col = 15;

        // Start reverse search
        handler.history.start_reverse_search(&mut handler.state);

        // Type something that matches nothing
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'x');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'y');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'z');

        // Confirm no matches
        assert_eq!(handler.history.reverse_search_matches.len(), 0);

        // Press Enter — should NOT send a message
        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(action, InputAction::Consumed);

        // Should have exited reverse search and restored pending input
        assert!(!handler.is_in_reverse_search());
        assert_eq!(handler.state.all_text(), "my pending work");
    }

    #[test]
    fn test_reverse_search_enter_with_match_sends() {
        let mut handler = InputHandler::new();
        handler.set_history(vec!["command one".to_string(), "test command".to_string()]);

        handler.state.lines = vec!["my pending work".to_string()];
        handler.state.cursor_col = 15;

        // Start reverse search
        handler.history.start_reverse_search(&mut handler.state);

        // Type "test" — should match
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'e');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 's');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');

        assert!(!handler.history.reverse_search_matches.is_empty());

        // Press Enter — should send the matched command
        let action = handler.handle_key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert!(matches!(action, InputAction::SendMessage(_)));
        if let InputAction::SendMessage(lines) = action {
            // Should send the matched history entry, not the "no matches" text
            assert!(
                !lines[0].contains("no matches"),
                "Should not contain display text, got: {}",
                lines[0]
            );
            assert!(
                lines[0].contains("test"),
                "Should contain the matched command, got: {}",
                lines[0]
            );
        }
    }

    #[test]
    fn test_large_history_performance() {
        let mut handler = InputHandler::new();
        let history: Vec<String> = (0..150).map(|i| format!("command {}", i)).collect();
        handler.set_history(history);

        // Start reverse search
        let start = std::time::Instant::now();
        handler.history.start_reverse_search(&mut handler.state);
        let duration = start.elapsed();

        // Should be fast (< 200ms, generous for CI runners under load)
        assert!(
            duration.as_millis() < 200,
            "Reverse search too slow: {:?}",
            duration
        );

        // Test filtering performance
        let start = std::time::Instant::now();
        handler
            .history
            .add_reverse_search_char(&mut handler.state, '1');
        let duration = start.elapsed();

        // Should also be fast
        assert!(
            duration.as_millis() < 200,
            "Reverse search filter too slow: {:?}",
            duration
        );
    }

    #[test]
    fn test_reverse_search_empty_history() {
        let mut handler = InputHandler::new();
        handler.set_history(vec![]);

        // Starting reverse search with empty history should do nothing
        handler.history.start_reverse_search(&mut handler.state);
        assert!(!handler.is_in_reverse_search());
    }

    #[test]
    fn test_reverse_search_info() {
        let mut handler = InputHandler::new();
        handler.set_history(vec![
            "command one".to_string(),
            "test command".to_string(),
            "another test".to_string(),
        ]);

        handler.history.start_reverse_search(&mut handler.state);
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 'e');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 's');
        handler
            .history
            .add_reverse_search_char(&mut handler.state, 't');

        let (query, current, total) = handler.reverse_search_info();
        assert_eq!(query, "test");
        assert_eq!(current, 1); // 1-indexed
        assert_eq!(total, 2);
    }

    // === Readline-style keybinding tests ===

    #[test]
    fn test_ctrl_a_beginning_of_line() {
        let mut handler = InputHandler::new();
        handler.state.lines[0] = "Hello World".to_string();
        handler.state.cursor_col = 8;

        let action = handler.handle_key_event(KeyCode::Char('a'), KeyModifiers::CONTROL);

        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.cursor_col, 0);
    }

    #[test]
    fn test_ctrl_e_end_of_line() {
        let mut handler = InputHandler::new();
        handler.state.lines[0] = "Hello".to_string();
        handler.state.cursor_col = 0;

        let action = handler.handle_key_event(KeyCode::Char('e'), KeyModifiers::CONTROL);

        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.cursor_col, 5);
    }

    #[test]
    fn test_ctrl_u_clear_line() {
        let mut handler = InputHandler::new();
        handler.state.lines[0] = "Hello World".to_string();
        handler.state.cursor_col = 5;

        let action = handler.handle_key_event(KeyCode::Char('u'), KeyModifiers::CONTROL);

        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.lines[0], "");
        assert_eq!(handler.state.cursor_col, 0);
    }

    #[test]
    fn test_ctrl_w_delete_word_backward() {
        let mut handler = InputHandler::new();
        handler.state.lines[0] = "hello world".to_string();
        handler.state.cursor_col = 11; // At end

        let action = handler.handle_key_event(KeyCode::Char('w'), KeyModifiers::CONTROL);

        assert_eq!(action, InputAction::Consumed);
        // delete_word_backward deletes the word "world" and the space before it
        assert_eq!(handler.state.lines[0], "hello");
    }

    #[test]
    fn test_ctrl_k_kill_to_end_of_line() {
        let mut handler = InputHandler::new();
        handler.state.lines[0] = "hello world".to_string();
        handler.state.cursor_col = 6; // After "hello "

        let action = handler.handle_key_event(KeyCode::Char('k'), KeyModifiers::CONTROL);

        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.lines[0], "hello ");
    }

    #[test]
    fn test_ctrl_l_clear_entire_input() {
        let mut handler = InputHandler::new();
        handler.state.lines = vec!["Line 1".to_string(), "Line 2".to_string()];

        let action = handler.handle_key_event(KeyCode::Char('l'), KeyModifiers::CONTROL);

        assert_eq!(action, InputAction::Consumed);
        assert_eq!(handler.state.lines.len(), 1);
        assert_eq!(handler.state.lines[0], "");
        assert_eq!(handler.state.cursor_col, 0);
    }

    // === UTF-8 boundary safety tests ===

    #[test]
    fn test_ctrl_k_mid_byte_utf8_does_not_panic() {
        let mut handler = InputHandler::new();
        // "abc" + 你 (3 bytes, bytes 3..6) + "xyz"
        handler.state.lines[0] = "abc你好xyz".to_string();
        // Set cursor to byte 4 -- inside 你 (bytes 3..6)
        handler.state.cursor_col = 4;

        let action = handler.handle_key_event(KeyCode::Char('k'), KeyModifiers::CONTROL);

        assert_eq!(action, InputAction::Consumed);
        // cursor_col is floored to byte 3, so "abc" is kept and "你好xyz" is killed
        assert_eq!(handler.state.lines[0], "abc");
    }

    #[test]
    fn test_ctrl_k_mid_byte_emoji_does_not_panic() {
        let mut handler = InputHandler::new();
        // 😀 is 4 bytes (bytes 0..4), 👍 is 4 bytes (bytes 4..8)
        handler.state.lines[0] = "😀👍".to_string();
        // Set cursor to byte 6 -- inside 👍 (bytes 4..8)
        handler.state.cursor_col = 6;

        let action = handler.handle_key_event(KeyCode::Char('k'), KeyModifiers::CONTROL);

        assert_eq!(action, InputAction::Consumed);
        // cursor_col is floored to byte 4, so 😀 is kept
        assert_eq!(handler.state.lines[0], "😀");
    }
}
