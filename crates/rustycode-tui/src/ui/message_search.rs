//! Message search and filter functionality
//!
//! Provides full-text search across conversation history with:
//! - Case-sensitive/insensitive search
//! - Role-based filtering (User, Assistant, System)
//! - Match highlighting and navigation
//! - Real-time match counting

use crate::ui::message_types::{Message, MessageRole};

/// Search filter by message role
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RoleFilter {
    /// Show all messages
    All,
    /// Show only User messages
    User,
    /// Show only Assistant messages
    Assistant,
    /// Show only System messages
    System,
}

impl RoleFilter {
    /// Check if a role matches this filter
    pub fn matches(&self, role: &MessageRole) -> bool {
        match self {
            RoleFilter::All => true,
            RoleFilter::User => role == &MessageRole::User,
            RoleFilter::Assistant => role == &MessageRole::Assistant,
            RoleFilter::System => role == &MessageRole::System,
        }
    }
}

/// Position of a match in a message
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchPosition {
    /// Index of the message containing this match
    pub message_index: usize,
    /// Start byte position in the message content
    pub start: usize,
    /// End byte position in the message content
    pub end: usize,
}

/// Search configuration and state
#[derive(Clone, Debug)]
pub struct SearchState {
    /// Current search query
    pub query: String,
    /// Whether search is case-sensitive
    pub case_sensitive: bool,
    /// Current role filter
    pub role_filter: RoleFilter,
    /// All match positions (immutable after search)
    pub matches: Vec<MatchPosition>,
    /// Index of currently selected match
    pub current_match_index: usize,
    /// Whether the search box is visible
    pub visible: bool,
}

impl SearchState {
    /// Create a new search state
    pub fn new() -> Self {
        Self {
            query: String::new(),
            case_sensitive: false,
            role_filter: RoleFilter::All,
            matches: Vec::new(),
            current_match_index: 0,
            visible: false,
        }
    }

    /// Get total number of matches
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Get current match position in the list (1-indexed for display)
    pub fn current_match_number(&self) -> usize {
        if self.matches.is_empty() {
            0
        } else {
            self.current_match_index + 1
        }
    }

    /// Get the current match, if any
    pub fn current_match(&self) -> Option<&MatchPosition> {
        self.matches.get(self.current_match_index)
    }

    /// Navigate to next match
    pub fn next_match(&mut self) {
        if !self.matches.is_empty() {
            self.current_match_index = (self.current_match_index + 1) % self.matches.len();
        }
    }

    /// Navigate to previous match
    pub fn prev_match(&mut self) {
        if !self.matches.is_empty() {
            self.current_match_index = if self.current_match_index == 0 {
                self.matches.len() - 1
            } else {
                self.current_match_index - 1
            };
        }
    }

    /// Clear search state
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current_match_index = 0;
        self.visible = false;
    }

    /// Toggle case sensitivity
    pub fn toggle_case_sensitive(&mut self) {
        self.case_sensitive = !self.case_sensitive;
    }

    /// Set role filter
    pub fn set_role_filter(&mut self, filter: RoleFilter) {
        self.role_filter = filter;
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

/// Core search engine for messages
pub struct SearchEngine;

impl SearchEngine {
    /// Find all matches in a query string within a subject string
    fn find_matches(query: &str, subject: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
        if query.is_empty() {
            return Vec::new();
        }

        let search_query = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        let search_subject = if case_sensitive {
            subject.to_string()
        } else {
            subject.to_lowercase()
        };

        let mut matches = Vec::new();
        let query_len = search_query.len();

        // Use byte-level search to preserve UTF-8 boundaries
        let search_bytes = search_query.as_bytes();
        let subject_bytes = search_subject.as_bytes();

        for i in 0..subject_bytes.len().saturating_sub(query_len - 1) {
            if subject_bytes[i..i + query_len] == *search_bytes {
                // Verify we have valid UTF-8 boundaries by reconstructing from bytes
                if std::str::from_utf8(&subject_bytes[i..i + query_len]).is_ok() {
                    matches.push((i, i + query_len));
                }
            }
        }

        matches
    }

    /// Search messages and update search state
    ///
    /// Returns up to `MAX_TOTAL_MATCHES` matches across all messages.
    /// This prevents span explosion when searching for short strings
    /// (e.g., "a" in a large code block) that would otherwise produce
    /// thousands of match positions.
    pub fn search(
        query: &str,
        messages: &[Message],
        case_sensitive: bool,
        role_filter: &RoleFilter,
    ) -> Vec<MatchPosition> {
        if query.is_empty() || messages.is_empty() {
            return Vec::new();
        }

        /// Maximum total matches across all messages.
        /// Prevents unbounded span creation during highlighting.
        const MAX_TOTAL_MATCHES: usize = 200;

        let mut all_matches = Vec::new();

        for (msg_idx, message) in messages.iter().enumerate() {
            // Apply role filter
            if !role_filter.matches(&message.role) {
                continue;
            }

            // Find matches in content
            let matches = Self::find_matches(query, &message.content, case_sensitive);

            // Convert to MatchPosition (respecting per-message cap)
            let max_per_msg = MAX_TOTAL_MATCHES / 4; // Distribute across messages
            for (start, end) in matches.into_iter().take(max_per_msg) {
                all_matches.push(MatchPosition {
                    message_index: msg_idx,
                    start,
                    end,
                });
                if all_matches.len() >= MAX_TOTAL_MATCHES {
                    return all_matches;
                }
            }
        }

        all_matches
    }

    /// Add character to search query and return whether search should be updated
    pub fn add_char(state: &mut SearchState, c: char) {
        state.query.push(c);
    }

    /// Remove last character from search query
    pub fn backspace(state: &mut SearchState) {
        state.query.pop();
    }

    /// Reset to first match
    pub fn reset_match_position(state: &mut SearchState) {
        state.current_match_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // SearchState Tests
    // ============================================================================

    #[test]
    fn test_search_state_new() {
        let state = SearchState::new();
        assert_eq!(state.query, "");
        assert!(!state.case_sensitive);
        assert_eq!(state.role_filter, RoleFilter::All);
        assert!(state.matches.is_empty());
        assert_eq!(state.current_match_index, 0);
        assert!(!state.visible);
    }

    #[test]
    fn test_search_state_match_count() {
        let mut state = SearchState::new();
        assert_eq!(state.match_count(), 0);

        state.matches.push(MatchPosition {
            message_index: 0,
            start: 0,
            end: 5,
        });
        assert_eq!(state.match_count(), 1);

        state.matches.push(MatchPosition {
            message_index: 1,
            start: 10,
            end: 15,
        });
        assert_eq!(state.match_count(), 2);
    }

    #[test]
    fn test_search_state_current_match_number() {
        let mut state = SearchState::new();
        assert_eq!(state.current_match_number(), 0); // No matches

        state.matches.push(MatchPosition {
            message_index: 0,
            start: 0,
            end: 5,
        });
        state.matches.push(MatchPosition {
            message_index: 1,
            start: 10,
            end: 15,
        });

        assert_eq!(state.current_match_number(), 1); // First match (1-indexed)
        state.current_match_index = 1;
        assert_eq!(state.current_match_number(), 2); // Second match
    }

    #[test]
    fn test_search_state_current_match() {
        let mut state = SearchState::new();
        assert!(state.current_match().is_none());

        let match1 = MatchPosition {
            message_index: 0,
            start: 0,
            end: 5,
        };
        state.matches.push(match1.clone());

        assert_eq!(state.current_match(), Some(&match1));
    }

    #[test]
    fn test_search_state_next_match() {
        let mut state = SearchState::new();
        state.matches.push(MatchPosition {
            message_index: 0,
            start: 0,
            end: 5,
        });
        state.matches.push(MatchPosition {
            message_index: 1,
            start: 10,
            end: 15,
        });
        state.matches.push(MatchPosition {
            message_index: 2,
            start: 20,
            end: 25,
        });

        assert_eq!(state.current_match_index, 0);
        state.next_match();
        assert_eq!(state.current_match_index, 1);
        state.next_match();
        assert_eq!(state.current_match_index, 2);
        // Wrap around
        state.next_match();
        assert_eq!(state.current_match_index, 0);
    }

    #[test]
    fn test_search_state_prev_match() {
        let mut state = SearchState::new();
        state.matches.push(MatchPosition {
            message_index: 0,
            start: 0,
            end: 5,
        });
        state.matches.push(MatchPosition {
            message_index: 1,
            start: 10,
            end: 15,
        });
        state.matches.push(MatchPosition {
            message_index: 2,
            start: 20,
            end: 25,
        });

        assert_eq!(state.current_match_index, 0);
        state.prev_match();
        assert_eq!(state.current_match_index, 2); // Wrap to last
        state.prev_match();
        assert_eq!(state.current_match_index, 1);
        state.prev_match();
        assert_eq!(state.current_match_index, 0);
    }

    #[test]
    fn test_search_state_clear() {
        let mut state = SearchState::new();
        state.query = "test".to_string();
        state.visible = true;
        state.matches.push(MatchPosition {
            message_index: 0,
            start: 0,
            end: 5,
        });
        state.current_match_index = 1;

        state.clear();

        assert_eq!(state.query, "");
        assert!(state.matches.is_empty());
        assert_eq!(state.current_match_index, 0);
        assert!(!state.visible);
    }

    #[test]
    fn test_search_state_toggle_case_sensitive() {
        let mut state = SearchState::new();
        assert!(!state.case_sensitive);

        state.toggle_case_sensitive();
        assert!(state.case_sensitive);

        state.toggle_case_sensitive();
        assert!(!state.case_sensitive);
    }

    #[test]
    fn test_search_state_set_role_filter() {
        let mut state = SearchState::new();
        assert_eq!(state.role_filter, RoleFilter::All);

        state.set_role_filter(RoleFilter::User);
        assert_eq!(state.role_filter, RoleFilter::User);

        state.set_role_filter(RoleFilter::Assistant);
        assert_eq!(state.role_filter, RoleFilter::Assistant);
    }

    // ============================================================================
    // RoleFilter Tests
    // ============================================================================

    #[test]
    fn test_role_filter_all() {
        let filter = RoleFilter::All;
        assert!(filter.matches(&MessageRole::User));
        assert!(filter.matches(&MessageRole::Assistant));
        assert!(filter.matches(&MessageRole::System));
    }

    #[test]
    fn test_role_filter_user() {
        let filter = RoleFilter::User;
        assert!(filter.matches(&MessageRole::User));
        assert!(!filter.matches(&MessageRole::Assistant));
        assert!(!filter.matches(&MessageRole::System));
    }

    #[test]
    fn test_role_filter_assistant() {
        let filter = RoleFilter::Assistant;
        assert!(!filter.matches(&MessageRole::User));
        assert!(filter.matches(&MessageRole::Assistant));
        assert!(!filter.matches(&MessageRole::System));
    }

    #[test]
    fn test_role_filter_system() {
        let filter = RoleFilter::System;
        assert!(!filter.matches(&MessageRole::User));
        assert!(!filter.matches(&MessageRole::Assistant));
        assert!(filter.matches(&MessageRole::System));
    }

    // ============================================================================
    // SearchEngine Tests - Basic Matching
    // ============================================================================

    #[test]
    fn test_find_matches_simple() {
        let matches = SearchEngine::find_matches("hello", "hello world", false);
        assert_eq!(matches, vec![(0, 5)]);
    }

    #[test]
    fn test_find_matches_multiple() {
        let matches = SearchEngine::find_matches("a", "banana", false);
        assert_eq!(matches, vec![(1, 2), (3, 4), (5, 6)]);
    }

    #[test]
    fn test_find_matches_none() {
        let matches = SearchEngine::find_matches("xyz", "hello world", false);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_matches_empty_query() {
        let matches = SearchEngine::find_matches("", "hello world", false);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_matches_case_insensitive() {
        let matches = SearchEngine::find_matches("Hello", "hello world", false);
        assert_eq!(matches, vec![(0, 5)]);
    }

    #[test]
    fn test_find_matches_case_sensitive() {
        let matches = SearchEngine::find_matches("Hello", "hello world", true);
        assert!(matches.is_empty());

        let matches = SearchEngine::find_matches("hello", "hello world", true);
        assert_eq!(matches, vec![(0, 5)]);
    }

    #[test]
    fn test_find_matches_overlapping() {
        // Should find overlapping matches
        let matches = SearchEngine::find_matches("aa", "aaa", false);
        assert_eq!(matches, vec![(0, 2), (1, 3)]);
    }

    #[test]
    fn test_find_matches_at_boundaries() {
        let matches = SearchEngine::find_matches("hello", "hello", false);
        assert_eq!(matches, vec![(0, 5)]);

        let matches = SearchEngine::find_matches("world", "hello world", false);
        assert_eq!(matches, vec![(6, 11)]);
    }

    #[test]
    fn test_find_matches_with_special_chars() {
        let matches = SearchEngine::find_matches("a.b", "a.b.c", false);
        assert_eq!(matches, vec![(0, 3)]);
    }

    // ============================================================================
    // SearchEngine Tests - Full Search
    // ============================================================================

    #[test]
    fn test_search_empty_query() {
        let messages = vec![
            Message::user("hello".to_string()),
            Message::assistant("world".to_string()),
        ];

        let matches = SearchEngine::search("", &messages, false, &RoleFilter::All);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_search_empty_messages() {
        let matches = SearchEngine::search("hello", &[], false, &RoleFilter::All);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_search_single_message() {
        let messages = vec![Message::user("hello world".to_string())];

        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message_index, 0);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[0].end, 5);
    }

    #[test]
    fn test_search_multiple_messages() {
        let messages = vec![
            Message::user("hello".to_string()),
            Message::assistant("hello world".to_string()),
            Message::user("world".to_string()),
        ];

        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].message_index, 0);
        assert_eq!(matches[1].message_index, 1);
    }

    #[test]
    fn test_search_multiple_matches_in_one_message() {
        let messages = vec![Message::user("hello hello hello".to_string())];

        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].message_index, 0);
        assert_eq!(matches[1].message_index, 0);
        assert_eq!(matches[2].message_index, 0);
    }

    #[test]
    fn test_search_with_role_filter_user() {
        let messages = vec![
            Message::user("hello".to_string()),
            Message::assistant("hello".to_string()),
            Message::system("hello".to_string()),
        ];

        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::User);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message_index, 0);
    }

    #[test]
    fn test_search_with_role_filter_assistant() {
        let messages = vec![
            Message::user("hello".to_string()),
            Message::assistant("hello".to_string()),
            Message::system("hello".to_string()),
        ];

        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::Assistant);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message_index, 1);
    }

    #[test]
    fn test_search_with_role_filter_system() {
        let messages = vec![
            Message::user("hello".to_string()),
            Message::assistant("hello".to_string()),
            Message::system("hello".to_string()),
        ];

        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::System);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message_index, 2);
    }

    #[test]
    fn test_search_with_role_filter_all() {
        let messages = vec![
            Message::user("hello".to_string()),
            Message::assistant("hello".to_string()),
            Message::system("hello".to_string()),
        ];

        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_search_case_sensitive() {
        let messages = vec![
            Message::user("Hello".to_string()),
            Message::assistant("hello".to_string()),
        ];

        let matches = SearchEngine::search("Hello", &messages, true, &RoleFilter::All);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message_index, 0);
    }

    #[test]
    fn test_search_case_insensitive() {
        let messages = vec![
            Message::user("Hello".to_string()),
            Message::assistant("hello".to_string()),
        ];

        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_search_complex_scenario() {
        let messages = vec![
            Message::user("I need help with Python".to_string()),
            Message::assistant("Python is great! Let me help with Python basics.".to_string()),
            Message::system("Tool executed".to_string()),
            Message::user("Thanks for the help with Python!".to_string()),
        ];

        // Search for "Python" in all roles
        let matches = SearchEngine::search("Python", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 4);

        // Search only in User messages
        let matches = SearchEngine::search("Python", &messages, false, &RoleFilter::User);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].message_index, 0);
        assert_eq!(matches[1].message_index, 3);

        // Case-sensitive search
        let matches = SearchEngine::search("Python", &messages, true, &RoleFilter::All);
        assert_eq!(matches.len(), 4);

        let matches = SearchEngine::search("python", &messages, true, &RoleFilter::All);
        assert_eq!(matches.len(), 0);
    }

    // ============================================================================
    // SearchEngine Tests - Character Operations
    // ============================================================================

    #[test]
    fn test_add_char() {
        let mut state = SearchState::new();
        SearchEngine::add_char(&mut state, 'h');
        assert_eq!(state.query, "h");

        SearchEngine::add_char(&mut state, 'i');
        assert_eq!(state.query, "hi");
    }

    #[test]
    fn test_backspace() {
        let mut state = SearchState::new();
        state.query = "hello".to_string();

        SearchEngine::backspace(&mut state);
        assert_eq!(state.query, "hell");

        SearchEngine::backspace(&mut state);
        assert_eq!(state.query, "hel");
    }

    #[test]
    fn test_backspace_empty() {
        let mut state = SearchState::new();
        SearchEngine::backspace(&mut state);
        assert_eq!(state.query, "");
    }

    #[test]
    fn test_reset_match_position() {
        let mut state = SearchState::new();
        state.current_match_index = 5;

        SearchEngine::reset_match_position(&mut state);
        assert_eq!(state.current_match_index, 0);
    }

    // ============================================================================
    // Edge Cases and Special Characters
    // ============================================================================

    #[test]
    fn test_search_with_unicode() {
        let messages = vec![Message::user("Hello 世界".to_string())];

        let matches = SearchEngine::search("世界", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_search_with_numbers() {
        let messages = vec![Message::user("Error code 404".to_string())];

        let matches = SearchEngine::search("404", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_search_with_punctuation() {
        let messages = vec![Message::user("Hello, world!".to_string())];

        let matches = SearchEngine::search("world!", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_search_multiline_content() {
        let messages = vec![Message::user("Line 1\nLine 2\nLine 3".to_string())];

        let matches = SearchEngine::search("Line", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_match_position_equality() {
        let m1 = MatchPosition {
            message_index: 0,
            start: 5,
            end: 10,
        };
        let m2 = MatchPosition {
            message_index: 0,
            start: 5,
            end: 10,
        };
        assert_eq!(m1, m2);
    }

    // ============================================================================
    // Rendering Integration Tests
    // ============================================================================

    #[test]
    fn test_search_box_visible_when_enabled() {
        let mut state = SearchState::new();
        assert!(!state.visible);

        state.visible = true;
        assert!(state.visible);
    }

    #[test]
    fn test_search_box_match_count_display() {
        let mut state = SearchState::new();
        state.query = "test".to_string();
        state.matches.push(MatchPosition {
            message_index: 0,
            start: 0,
            end: 4,
        });
        state.matches.push(MatchPosition {
            message_index: 1,
            start: 5,
            end: 9,
        });

        // Verify match count is correct
        assert_eq!(state.match_count(), 2);
        assert_eq!(state.current_match_number(), 1); // First match is current

        // Navigate to second match
        state.next_match();
        assert_eq!(state.current_match_number(), 2); // Now showing second match
    }

    #[test]
    fn test_search_box_no_matches_state() {
        let state = SearchState::new();
        assert_eq!(state.match_count(), 0);
        assert_eq!(state.current_match_number(), 0); // Should be 0 when no matches
    }

    #[test]
    fn test_match_positions_for_same_message() {
        let messages = vec![Message::user("hello hello hello".to_string())];

        // Search should find all 3 occurrences
        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 3);

        // All matches should be in message 0
        for m in &matches {
            assert_eq!(m.message_index, 0);
        }

        // Verify positions are different
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[1].start, 6);
        assert_eq!(matches[2].start, 12);
    }

    #[test]
    fn test_search_visible_and_query_integration() {
        let mut state = SearchState::new();

        // Initially not visible
        assert!(!state.visible);
        assert_eq!(state.query, "");

        // Simulate user opening search
        state.visible = true;
        SearchEngine::add_char(&mut state, 't');
        SearchEngine::add_char(&mut state, 'e');
        SearchEngine::add_char(&mut state, 's');
        SearchEngine::add_char(&mut state, 't');

        assert!(state.visible);
        assert_eq!(state.query, "test");
    }

    #[test]
    fn test_search_input_and_clear() {
        let mut state = SearchState::new();
        state.visible = true;

        // Add characters
        SearchEngine::add_char(&mut state, 'h');
        SearchEngine::add_char(&mut state, 'i');
        assert_eq!(state.query, "hi");

        // Backspace
        SearchEngine::backspace(&mut state);
        assert_eq!(state.query, "h");

        // Clear everything
        state.clear();
        assert!(!state.visible);
        assert_eq!(state.query, "");
    }

    #[test]
    fn test_multiple_matches_in_different_messages() {
        let messages = vec![
            Message::user("Python is great".to_string()),
            Message::assistant("Python makes coding easy".to_string()),
            Message::system("Python executed successfully".to_string()),
        ];

        let matches = SearchEngine::search("Python", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 3);

        // Verify each message has exactly one match
        assert_eq!(matches[0].message_index, 0);
        assert_eq!(matches[1].message_index, 1);
        assert_eq!(matches[2].message_index, 2);
    }

    #[test]
    fn test_search_highlighting_preserves_match_positions() {
        let messages = vec![Message::user("hello world hello".to_string())];

        let matches = SearchEngine::search("hello", &messages, false, &RoleFilter::All);
        assert_eq!(matches.len(), 2);

        // First match
        assert_eq!(matches[0].message_index, 0);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[0].end, 5);

        // Second match
        assert_eq!(matches[1].message_index, 0);
        assert_eq!(matches[1].start, 12);
        assert_eq!(matches[1].end, 17);
    }
}
