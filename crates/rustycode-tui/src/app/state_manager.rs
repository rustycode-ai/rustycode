//! State management for the TUI
//!
//! This module handles all application state including:
//! - Message expansion/collapse states
//! - Scrolling operations
//! - Viewport management
//! - Selection state

use crate::ui::{Message, MessageRole};
use std::ops::Range;

/// Manager for TUI state
pub struct StateManager {
    /// Current scroll offset (0 = top)
    scroll_offset: usize,
    /// Viewport height (number of visible messages)
    viewport_height: usize,
    /// Total number of messages
    message_count: usize,
}

impl StateManager {
    /// Create a new state manager
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            viewport_height: 10,
            message_count: 0,
        }
    }

    /// Update the viewport height
    pub fn update_viewport_height(&mut self, height: usize) {
        self.viewport_height = height.max(1);
        // Ensure scroll offset is valid
        self.ensure_scroll_offset_valid();
    }

    /// Update the message count
    pub fn update_message_count(&mut self, count: usize) {
        self.message_count = count;
        // Adjust scroll offset if needed
        self.ensure_scroll_offset_valid();
    }

    /// Get the current scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Get the viewport height
    pub fn viewport_height(&self) -> usize {
        self.viewport_height
    }

    /// Get the total number of messages
    pub fn message_count(&self) -> usize {
        self.message_count
    }

    /// Calculate the visible range of message indices
    pub fn visible_range(&self) -> Range<usize> {
        let start = self.scroll_offset;
        let end = (self.scroll_offset + self.viewport_height).min(self.message_count);
        start..end
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self) {
        let max_offset = self.max_scroll_offset();
        if self.scroll_offset < max_offset {
            self.scroll_offset += 1;
        }
    }

    /// Scroll up by one page
    pub fn page_up(&mut self) {
        let page_size = self.viewport_height.saturating_sub(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    /// Scroll down by one page
    pub fn page_down(&mut self) {
        let page_size = self.viewport_height.saturating_sub(1);
        let max_offset = self.max_scroll_offset();
        self.scroll_offset = (self.scroll_offset + page_size).min(max_offset);
    }

    /// Scroll to the top
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    /// Scroll to the bottom
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.max_scroll_offset();
    }

    /// Calculate the maximum scroll offset
    fn max_scroll_offset(&self) -> usize {
        self.message_count.saturating_sub(self.viewport_height)
    }

    /// Ensure the scroll offset is valid
    fn ensure_scroll_offset_valid(&mut self) {
        let max_offset = self.max_scroll_offset();
        if self.scroll_offset > max_offset {
            self.scroll_offset = max_offset;
        }
    }

    /// Toggle collapse state for a message
    ///
    /// Returns the new collapse state
    pub fn toggle_message_collapse(message: &mut Message) -> bool {
        message.collapsed = !message.collapsed;
        message.collapsed
    }

    /// Expand all messages
    pub fn expand_all_messages(messages: &mut [Message]) {
        for msg in messages {
            msg.collapsed = false;
        }
    }

    /// Collapse all messages except user messages
    pub fn collapse_all_except_user(messages: &mut [Message]) {
        for msg in messages {
            if !matches!(msg.role, MessageRole::User) {
                msg.collapsed = true;
            }
        }
    }

    /// Expand all tool executions
    pub fn expand_all_tools(messages: &mut [Message]) {
        use crate::ui::message::ExpansionLevel;

        for msg in messages {
            msg.tools_expansion = ExpansionLevel::Expanded;
            // Note: ToolExecution doesn't have individual expansion, it's controlled at message level
        }
    }

    /// Collapse all tool executions
    pub fn collapse_all_tools(messages: &mut [Message]) {
        use crate::ui::message::ExpansionLevel;

        for msg in messages {
            msg.tools_expansion = ExpansionLevel::Collapsed;
        }
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_manager_creation() {
        let manager = StateManager::new();
        assert_eq!(manager.scroll_offset(), 0);
        assert_eq!(manager.viewport_height(), 10);
        assert_eq!(manager.message_count(), 0);
    }

    #[test]
    fn test_update_viewport_height() {
        let mut manager = StateManager::new();
        manager.update_viewport_height(20);
        assert_eq!(manager.viewport_height(), 20);
    }

    #[test]
    fn test_update_message_count() {
        let mut manager = StateManager::new();
        manager.update_message_count(100);
        assert_eq!(manager.message_count(), 100);
    }

    #[test]
    fn test_visible_range() {
        let mut manager = StateManager::new();
        manager.update_viewport_height(10);
        manager.update_message_count(100);

        let range = manager.visible_range();
        assert_eq!(range, 0..10);
    }

    #[test]
    fn test_scroll_up() {
        let mut manager = StateManager::new();
        manager.update_viewport_height(10);
        manager.update_message_count(100);
        manager.scroll_to_bottom();

        let initial_offset = manager.scroll_offset();
        manager.scroll_up();
        assert_eq!(manager.scroll_offset(), initial_offset - 1);
    }

    #[test]
    fn test_scroll_down() {
        let mut manager = StateManager::new();
        manager.update_viewport_height(10);
        manager.update_message_count(100);

        manager.scroll_down();
        assert_eq!(manager.scroll_offset(), 1);
    }

    #[test]
    fn test_page_up() {
        let mut manager = StateManager::new();
        manager.update_viewport_height(10);
        manager.update_message_count(100);
        manager.scroll_to_bottom();

        let initial_offset = manager.scroll_offset();
        manager.page_up();
        assert_eq!(manager.scroll_offset(), initial_offset - 9);
    }

    #[test]
    fn test_page_down() {
        let mut manager = StateManager::new();
        manager.update_viewport_height(10);
        manager.update_message_count(100);

        manager.page_down();
        assert_eq!(manager.scroll_offset(), 9);
    }

    #[test]
    fn test_scroll_to_top() {
        let mut manager = StateManager::new();
        manager.update_viewport_height(10);
        manager.update_message_count(100);
        manager.scroll_to_bottom();

        manager.scroll_to_top();
        assert_eq!(manager.scroll_offset(), 0);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut manager = StateManager::new();
        manager.update_viewport_height(10);
        manager.update_message_count(100);

        manager.scroll_to_bottom();
        assert_eq!(manager.scroll_offset(), 90);
    }

    #[test]
    fn test_toggle_message_collapse() {
        use crate::ui::{Message, MessageRole};

        let mut message = Message::new(MessageRole::Assistant, "Test".to_string());
        assert!(!message.collapsed);

        let new_state = StateManager::toggle_message_collapse(&mut message);
        assert!(new_state);
        assert!(message.collapsed);

        let new_state = StateManager::toggle_message_collapse(&mut message);
        assert!(!new_state);
        assert!(!message.collapsed);
    }
}
