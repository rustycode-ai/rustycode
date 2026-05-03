//! UI State Management
//!
//! Manages the core UI state for message display, scrolling, and interaction.

use ratatui::layout::Rect;
use std::cell::RefCell;
use std::time::Instant;

/// Placeholder message type - will be replaced with actual Message from widgets
#[derive(Debug, Clone)]
pub struct Message {
    pub content: String,
    pub role: MessageRole,
}

#[derive(Debug, Clone)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Core UI state for message display and navigation
#[derive(Debug)]
pub struct UiState {
    /// Message list
    pub messages: Vec<Message>,

    /// Message display state
    pub scroll_offset_line: usize,
    pub selected_message: usize,
    pub viewport_height: usize,
    pub last_total_lines: std::cell::Cell<usize>,
    pub messages_area: std::cell::Cell<Rect>,

    /// User scroll tracking
    pub user_scrolled: bool,
    pub last_user_scroll_time: Instant,

    /// Message interaction areas
    pub message_areas: RefCell<Vec<(usize, Rect)>>,
    pub message_line_offsets: RefCell<Vec<usize>>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset_line: 0,
            selected_message: 0,
            viewport_height: 0,
            last_total_lines: std::cell::Cell::new(0),
            messages_area: std::cell::Cell::new(Rect::default()),
            user_scrolled: false,
            last_user_scroll_time: Instant::now(),
            message_areas: RefCell::new(Vec::new()),
            message_line_offsets: RefCell::new(Vec::new()),
        }
    }
}

impl UiState {
    /// Create new UI state
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a message to the list
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Get message count
    pub const fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Clear all messages
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.scroll_offset_line = 0;
        self.selected_message = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_msg(content: &str) -> Message {
        Message {
            content: content.to_string(),
            role: MessageRole::User,
        }
    }

    fn assistant_msg(content: &str) -> Message {
        Message {
            content: content.to_string(),
            role: MessageRole::Assistant,
        }
    }

    #[test]
    fn test_ui_state_default() {
        let state = UiState::default();
        assert!(state.messages.is_empty());
        assert_eq!(state.scroll_offset_line, 0);
        assert_eq!(state.selected_message, 0);
        assert_eq!(state.viewport_height, 0);
        assert!(!state.user_scrolled);
    }

    #[test]
    fn test_ui_state_new() {
        let state = UiState::new();
        assert!(state.messages.is_empty());
        assert_eq!(state.message_count(), 0);
    }

    #[test]
    fn test_add_message() {
        let mut state = UiState::new();
        state.add_message(user_msg("Hello"));
        assert_eq!(state.message_count(), 1);
        assert_eq!(state.messages[0].content, "Hello");
    }

    #[test]
    fn test_add_multiple_messages() {
        let mut state = UiState::new();
        state.add_message(user_msg("Hi"));
        state.add_message(assistant_msg("Hello!"));
        state.add_message(user_msg("How are you?"));
        assert_eq!(state.message_count(), 3);
        assert!(matches!(state.messages[0].role, MessageRole::User));
        assert!(matches!(state.messages[1].role, MessageRole::Assistant));
        assert!(matches!(state.messages[2].role, MessageRole::User));
    }

    #[test]
    fn test_clear_messages() {
        let mut state = UiState::new();
        state.add_message(user_msg("Hello"));
        state.add_message(assistant_msg("Hi!"));
        state.scroll_offset_line = 5;
        state.selected_message = 2;
        state.clear_messages();
        assert_eq!(state.message_count(), 0);
        assert_eq!(state.scroll_offset_line, 0);
        assert_eq!(state.selected_message, 0);
    }

    #[test]
    fn test_clear_empty_messages() {
        let mut state = UiState::new();
        state.clear_messages();
        assert_eq!(state.message_count(), 0);
    }

    #[test]
    fn test_message_roles() {
        let user = Message {
            content: "u".to_string(),
            role: MessageRole::User,
        };
        let assistant = Message {
            content: "a".to_string(),
            role: MessageRole::Assistant,
        };
        let system = Message {
            content: "s".to_string(),
            role: MessageRole::System,
        };
        assert!(matches!(user.role, MessageRole::User));
        assert!(matches!(assistant.role, MessageRole::Assistant));
        assert!(matches!(system.role, MessageRole::System));
    }

    #[test]
    fn test_message_count_after_removal() {
        let mut state = UiState::new();
        state.add_message(user_msg("a"));
        state.add_message(user_msg("b"));
        state.messages.remove(0);
        assert_eq!(state.message_count(), 1);
        assert_eq!(state.messages[0].content, "b");
    }
}
