//! History management for command input.
//!
//! This module provides:
//! - Command history browsing (up/down arrows)
//! - Reverse search (Ctrl+R) with substring matching

use super::input_state::InputState;

// ── History Manager ─────────────────────────────────────────────────────────────

/// Manages command history navigation and reverse search
#[derive(Debug, Default)]
pub struct HistoryManager {
    /// Command history
    history: Vec<String>,
    /// Current position in history (0 = not browsing, 1 = most recent, etc.)
    history_pos: usize,
    browsing_history: bool,
    /// Temporary storage for unsaved input when browsing history
    pending_input: String,
    /// Reverse search mode
    in_reverse_search: bool,
    reverse_search_query: String,
    pub reverse_search_matches: Vec<String>,
    /// Current position in reverse search matches
    pub reverse_search_index: usize,
}

impl HistoryManager {
    /// Create new history manager
    pub fn new() -> Self {
        Self::default()
    }

    /// Set command history (load from disk)
    pub fn set_history(&mut self, history: Vec<String>) {
        self.history = history;
        self.history_pos = 0;
        self.browsing_history = false;
    }

    /// Add command to history
    pub fn add_to_history(&mut self, command: String) {
        // Don't add empty commands or duplicates
        if command.trim().is_empty() {
            return;
        }

        // Don't add if it's the same as the last command
        if let Some(last) = self.history.last() {
            if last == &command {
                return;
            }
        }

        self.history.push(command);

        // Limit to 1000 commands
        if self.history.len() > 1000 {
            self.history.remove(0);
        }

        // Reset browsing state
        self.history_pos = 0;
        self.browsing_history = false;
    }

    /// Get command history for persistence
    pub fn history(&self) -> &[String] {
        &self.history
    }

    pub fn is_browsing_history(&self) -> bool {
        self.browsing_history
    }

    /// Get current history position (1-indexed for display)
    pub fn history_position(&self) -> (usize, usize) {
        if self.browsing_history && self.history_pos > 0 {
            (self.history_pos, self.history.len())
        } else {
            (0, 0)
        }
    }

    /// Navigate to previous command in history
    pub fn navigate_previous(&mut self, input_state: &mut InputState) {
        if self.history.is_empty() {
            return;
        }

        // If not yet browsing, save current input and start from most recent
        if !self.browsing_history {
            self.pending_input = input_state.all_text();
            self.browsing_history = true;
            self.history_pos = 0;
        }

        // Move to previous command if available
        if self.history_pos < self.history.len() {
            self.history_pos += 1;
            let cmd = self.history[self.history.len() - self.history_pos].clone();
            // Preserve multi-line entries: split on \n so cursor navigation works
            // Defensive: ensure we replace the entire input with the recalled history
            // instead of appending to the existing content. Some event dispatch
            // paths could lead to multiple mutations within a single Up press,
            // which would otherwise cause an append-like effect. Always push the
            // recalled command as a fresh lines vector.
            let recalled_lines = if cmd.contains('\n') {
                cmd.lines().map(|l| l.to_string()).collect::<Vec<_>>()
            } else {
                vec![cmd]
            };
            input_state.lines.clear();
            input_state.lines.extend(recalled_lines);
            if input_state.lines.is_empty() {
                input_state.lines = vec![String::new()];
            }
            input_state.cursor_row = input_state.lines.len() - 1;
            input_state.cursor_col = input_state.lines.last().map_or(0, |l| l.len());
            if self.history[self.history.len() - self.history_pos].contains('\n') {
                input_state.mode = super::input_state::InputMode::MultiLine;
            }
        }
    }

    /// Navigate to next command in history
    pub fn navigate_next(&mut self, input_state: &mut InputState) {
        if !self.browsing_history {
            return;
        }

        // Move to next command
        if self.history_pos > 1 {
            self.history_pos -= 1;
            let cmd = self.history[self.history.len() - self.history_pos].clone();
            let is_multiline = cmd.contains('\n');
            // Preserve multi-line entries: split on \n so cursor navigation works
            input_state.lines = if is_multiline {
                cmd.lines().map(|l| l.to_string()).collect::<Vec<_>>()
            } else {
                vec![cmd]
            };
            if input_state.lines.is_empty() {
                input_state.lines = vec![String::new()];
            }
            input_state.cursor_row = input_state.lines.len() - 1;
            input_state.cursor_col = input_state.lines.last().map_or(0, |l| l.len());
            input_state.mode = if is_multiline {
                super::input_state::InputMode::MultiLine
            } else {
                super::input_state::InputMode::SingleLine
            };
        } else {
            // Exit history mode, restore pending input
            self.browsing_history = false;
            self.history_pos = 0;
            let pending = self.pending_input.clone();
            let has_newlines = pending.contains('\n');
            input_state.lines = if has_newlines {
                pending.lines().map(|l| l.to_string()).collect::<Vec<_>>()
            } else {
                vec![pending]
            };
            if input_state.lines.is_empty() {
                input_state.lines = vec![String::new()];
            }
            input_state.cursor_row = input_state.lines.len() - 1;
            input_state.cursor_col = input_state.lines.last().map_or(0, |l| l.len());
            if has_newlines {
                input_state.mode = super::input_state::InputMode::MultiLine;
            }
        }
    }

    /// Exit history mode (call when user starts typing)
    pub fn exit_history_mode(&mut self) {
        if self.browsing_history {
            self.browsing_history = false;
            self.history_pos = 0;
            self.pending_input.clear();
        }
    }

    pub fn is_in_reverse_search(&self) -> bool {
        self.in_reverse_search
    }

    /// Check if the current reverse search has at least one match.
    ///
    /// Used by the Enter key handler to decide whether to submit or cancel.
    pub fn has_reverse_search_match(&self) -> bool {
        self.in_reverse_search && !self.reverse_search_matches.is_empty()
    }

    /// Get reverse search query and match info
    pub fn reverse_search_info(&self) -> (String, usize, usize) {
        if self.in_reverse_search {
            let match_count = self.reverse_search_matches.len();
            let position = if match_count == 0 {
                0
            } else {
                self.reverse_search_index + 1
            };
            (self.reverse_search_query.clone(), position, match_count)
        } else {
            (String::new(), 0, 0)
        }
    }

    /// Start reverse search through command history
    pub fn start_reverse_search(&mut self, input_state: &mut InputState) {
        if self.history.is_empty() {
            return;
        }

        // Save current input
        self.pending_input = input_state.all_text();

        // Enter reverse search mode with empty query
        self.in_reverse_search = true;
        self.reverse_search_query = String::new();
        self.reverse_search_matches = self.history.iter().rev().cloned().collect(); // Most recent first
        self.reverse_search_index = 0;

        // Show most recent command
        if let Some(match_cmd) = self.reverse_search_matches.first() {
            set_input_from_text(input_state, match_cmd);
        }
    }

    /// Cycle to next reverse search match
    pub fn cycle_reverse_search_match(&mut self, input_state: &mut InputState) {
        if !self.reverse_search_matches.is_empty() {
            self.reverse_search_index =
                (self.reverse_search_index + 1) % self.reverse_search_matches.len();
            let match_cmd = &self.reverse_search_matches[self.reverse_search_index];
            set_input_from_text(input_state, match_cmd);
        }
    }

    /// Add character to reverse search query
    pub fn add_reverse_search_char(&mut self, input_state: &mut InputState, c: char) {
        self.reverse_search_query.push(c);
        self.update_reverse_search_matches();

        // Show first match, or keep query in input area
        // (renderer handles "no matches" display in info bar)
        if let Some(match_cmd) = self.reverse_search_matches.first() {
            set_input_from_text(input_state, match_cmd);
        } else {
            input_state.lines = vec![self.reverse_search_query.clone()];
            input_state.cursor_row = 0;
            input_state.cursor_col = input_state.lines[0].len();
        }
    }

    /// Remove character from reverse search query (backspace)
    pub fn remove_reverse_search_char(&mut self, input_state: &mut InputState) {
        if !self.reverse_search_query.is_empty() {
            self.reverse_search_query.pop();
            self.update_reverse_search_matches();

            // Update display with new matches
            // (renderer handles "no matches" display in info bar)
            if let Some(match_cmd) = self.reverse_search_matches.first() {
                set_input_from_text(input_state, match_cmd);
            } else {
                input_state.lines = vec![self.reverse_search_query.clone()];
                input_state.cursor_col = input_state.lines[0].len();
            }
        }
    }

    /// Update reverse search matches based on current query
    fn update_reverse_search_matches(&mut self) {
        if self.reverse_search_query.is_empty() {
            self.reverse_search_matches = self.history.iter().rev().cloned().collect();
        // Most recent first
        } else {
            self.reverse_search_matches = self
                .history
                .iter()
                .filter(|cmd| cmd.contains(&self.reverse_search_query))
                .rev() // Most recent first
                .cloned()
                .collect();
        }
        self.reverse_search_index = 0;
    }

    /// Exit reverse search mode
    pub fn exit_reverse_search(&mut self, input_state: &mut InputState) {
        if self.in_reverse_search {
            self.in_reverse_search = false;
            self.reverse_search_query.clear();
            self.reverse_search_matches.clear();
            self.reverse_search_index = 0;

            // Restore pending input or clear
            if self.pending_input.is_empty() {
                input_state.clear();
            } else {
                set_input_from_text(input_state, &self.pending_input.clone());
            }
        }
    }
}

/// Set input state from a text string, preserving multi-line content.
///
/// Splits on `\n` so multi-line history entries render correctly
/// and cursor navigation works as expected.
fn set_input_from_text(input_state: &mut InputState, text: &str) {
    let has_newlines = text.contains('\n');
    if has_newlines {
        input_state.lines = text.lines().map(|l| l.to_string()).collect::<Vec<_>>();
    } else {
        input_state.lines = vec![text.to_string()];
    }
    if input_state.lines.is_empty() {
        input_state.lines = vec![String::new()];
    }
    input_state.cursor_row = input_state.lines.len() - 1;
    input_state.cursor_col = input_state.lines.last().map_or(0, |l| l.len());
    input_state.mode = if has_newlines {
        super::input_state::InputMode::MultiLine
    } else {
        super::input_state::InputMode::SingleLine
    };
}

// ── Tests ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_input_state(text: &str) -> InputState {
        let mut state = InputState::new();
        state.lines = vec![text.to_string()];
        state.cursor_col = text.len();
        state
    }

    #[test]
    fn test_history_add_and_get() {
        let mut manager = HistoryManager::new();
        manager.add_to_history("command one".to_string());
        manager.add_to_history("command two".to_string());

        assert_eq!(manager.history().len(), 2);
        assert_eq!(manager.history()[0], "command one");
        assert_eq!(manager.history()[1], "command two");
    }

    #[test]
    fn test_history_no_duplicates() {
        let mut manager = HistoryManager::new();
        manager.add_to_history("same command".to_string());
        manager.add_to_history("same command".to_string());

        assert_eq!(manager.history().len(), 1);
    }

    #[test]
    fn test_history_no_empty_commands() {
        let mut manager = HistoryManager::new();
        manager.add_to_history("".to_string());
        manager.add_to_history("   ".to_string());
        manager.add_to_history("valid command".to_string());

        assert_eq!(manager.history().len(), 1);
    }

    #[test]
    fn test_history_limit() {
        let mut manager = HistoryManager::new();
        for i in 0..2000 {
            manager.add_to_history(format!("command {}", i));
        }

        assert_eq!(manager.history().len(), 1000);
    }

    #[test]
    fn test_history_navigate_previous() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec![
            "command one".to_string(),
            "command two".to_string(),
            "command three".to_string(),
        ]);

        let mut state = create_test_input_state("current input");
        manager.navigate_previous(&mut state);

        assert!(manager.is_browsing_history());
        assert_eq!(state.all_text(), "command three");
        assert_eq!(manager.history_position().0, 1);

        manager.navigate_previous(&mut state);
        assert_eq!(state.all_text(), "command two");
    }

    #[test]
    fn test_history_navigate_next() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec!["command one".to_string(), "command two".to_string()]);

        let mut state = create_test_input_state("current input");
        manager.navigate_previous(&mut state);
        manager.navigate_previous(&mut state);

        manager.navigate_next(&mut state);
        assert_eq!(state.all_text(), "command two");

        manager.navigate_next(&mut state);
        // Should restore pending input
        assert_eq!(state.all_text(), "current input");
        assert!(!manager.is_browsing_history());
    }

    #[test]
    fn test_history_exit_mode() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec!["command one".to_string()]);

        let mut state = create_test_input_state("current input");
        manager.navigate_previous(&mut state);

        assert!(manager.is_browsing_history());

        manager.exit_history_mode();
        assert!(!manager.is_browsing_history());
        assert_eq!(manager.history_position(), (0, 0));
    }

    #[test]
    fn test_reverse_search_basic() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec![
            "command one".to_string(),
            "test command".to_string(),
            "another test".to_string(),
        ]);

        let mut state = create_test_input_state("current");
        manager.start_reverse_search(&mut state);

        assert!(manager.is_in_reverse_search());
        assert_eq!(manager.reverse_search_info().0, "");
        assert_eq!(state.all_text(), "another test");
    }

    #[test]
    fn test_reverse_search_filter() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec![
            "command one".to_string(),
            "test command".to_string(),
            "another test".to_string(),
        ]);

        let mut state = create_test_input_state("current");
        manager.start_reverse_search(&mut state);

        manager.add_reverse_search_char(&mut state, 't');
        manager.add_reverse_search_char(&mut state, 'e');
        manager.add_reverse_search_char(&mut state, 's');
        manager.add_reverse_search_char(&mut state, 't');

        let (query, current, total) = manager.reverse_search_info();
        assert_eq!(query, "test");
        assert_eq!(current, 1);
        assert_eq!(total, 2);
        assert_eq!(state.all_text(), "another test");
    }

    #[test]
    fn test_reverse_search_cycle() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec![
            "command one".to_string(),
            "test command".to_string(),
            "another test".to_string(),
        ]);

        let mut state = create_test_input_state("current");
        manager.start_reverse_search(&mut state);

        manager.add_reverse_search_char(&mut state, 't');
        manager.add_reverse_search_char(&mut state, 'e');
        manager.add_reverse_search_char(&mut state, 's');
        manager.add_reverse_search_char(&mut state, 't');

        manager.cycle_reverse_search_match(&mut state);
        let (_query, current, _total) = manager.reverse_search_info();
        assert_eq!(current, 2);
        assert_eq!(state.all_text(), "test command");
    }

    #[test]
    fn test_reverse_search_backspace() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec![
            "command one".to_string(),
            "test command".to_string(),
            "another test".to_string(),
        ]);

        let mut state = create_test_input_state("current");
        manager.start_reverse_search(&mut state);

        manager.add_reverse_search_char(&mut state, 't');
        manager.add_reverse_search_char(&mut state, 'e');
        manager.remove_reverse_search_char(&mut state);

        // After adding 't', 'e' (query="te"), then backspace, query should be "t"
        assert_eq!(manager.reverse_search_info().0, "t");
    }

    #[test]
    fn test_reverse_search_exit() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec!["command one".to_string()]);

        let mut state = create_test_input_state("pending");
        manager.start_reverse_search(&mut state);

        manager.exit_reverse_search(&mut state);
        assert!(!manager.is_in_reverse_search());
        assert_eq!(state.all_text(), "pending");
    }

    #[test]
    fn test_has_reverse_search_match() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec!["command one".to_string(), "command two".to_string()]);

        // Not in reverse search — false
        assert!(!manager.has_reverse_search_match());

        let mut state = create_test_input_state("current");
        manager.start_reverse_search(&mut state);

        // In reverse search with matches — true
        assert!(manager.has_reverse_search_match());

        // Type something that matches nothing
        manager.add_reverse_search_char(&mut state, 'z');
        manager.add_reverse_search_char(&mut state, 'z');
        manager.add_reverse_search_char(&mut state, 'z');

        // In reverse search but no matches — false
        assert!(!manager.has_reverse_search_match());
    }

    #[test]
    fn test_reverse_search_empty_history() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec![]);

        let mut state = create_test_input_state("current");
        manager.start_reverse_search(&mut state);

        assert!(!manager.is_in_reverse_search());
    }

    #[test]
    fn test_reverse_search_no_matches() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec!["command one".to_string(), "command two".to_string()]);

        let mut state = create_test_input_state("current");
        manager.start_reverse_search(&mut state);

        manager.add_reverse_search_char(&mut state, 'x');
        manager.add_reverse_search_char(&mut state, 'y');
        manager.add_reverse_search_char(&mut state, 'z');

        let (query, _current, total) = manager.reverse_search_info();
        assert_eq!(query, "xyz");
        assert_eq!(total, 0);
        // Query text should be preserved in input state (not the display string)
        assert_eq!(state.all_text(), "xyz");
        assert!(!manager.has_reverse_search_match());
    }

    #[test]
    fn test_history_position() {
        let mut manager = HistoryManager::new();
        manager.set_history(vec!["command one".to_string(), "command two".to_string()]);

        let mut state = create_test_input_state("current");
        assert_eq!(manager.history_position(), (0, 0));

        manager.navigate_previous(&mut state);
        assert_eq!(manager.history_position(), (1, 2));

        manager.navigate_previous(&mut state);
        assert_eq!(manager.history_position(), (2, 2));
    }
}
