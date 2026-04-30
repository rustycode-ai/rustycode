//! Bookmark System for Messages
//!
//! This module provides a bookmark system for marking important messages
//! in the conversation.
//!
//! ## Features
//!
//! - **Add bookmarks**: Mark important messages with descriptions
//! - **Remove bookmarks**: Unmark messages
//! - **List bookmarks**: View all bookmarks
//! - **Navigate**: Quick jump to bookmarked messages
//!
//! ## Usage
//!
//! ```rust,no_run
//! use rustycode_tui::bookmarks::BookmarkManager;
//!
//! let mut bookmarks = BookmarkManager::new();
//!
//! // Add a bookmark
//! bookmarks.add(5, "Important code example".to_string());
//!
//! // List all bookmarks
//! for (index, description) in bookmarks.list() {
//!     println!("Message {}: {}", index, description);
//! }
//!
//! // Remove a bookmark
//! bookmarks.remove(5);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// BOOKMARK MANAGER
// ============================================================================

/// Manager for message bookmarks
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BookmarkManager {
    /// Map of message index to description
    bookmarks: HashMap<usize, String>,
}

impl BookmarkManager {
    /// Create a new bookmark manager
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a bookmark
    ///
    /// # Arguments
    ///
    /// * `index` - Message index to bookmark
    /// * `description` - Description for the bookmark
    pub fn add(&mut self, index: usize, description: String) {
        self.bookmarks.insert(index, description);
    }

    /// Remove a bookmark
    ///
    /// # Arguments
    ///
    /// * `index` - Message index to unbookmark
    ///
    /// # Returns
    ///
    /// * `Some(description)` - If the bookmark existed
    /// * `None` - If there was no bookmark at this index
    pub fn remove(&mut self, index: usize) -> Option<String> {
        self.bookmarks.remove(&index)
    }

    /// Check if a message is bookmarked
    ///
    /// # Arguments
    ///
    /// * `index` - Message index to check
    pub fn is_bookmarked(&self, index: usize) -> bool {
        self.bookmarks.contains_key(&index)
    }

    /// Get bookmark description
    ///
    /// # Arguments
    ///
    /// * `index` - Message index
    ///
    /// # Returns
    ///
    /// * `Some(description)` - If the message is bookmarked
    /// * `None` - If the message is not bookmarked
    pub fn get(&self, index: usize) -> Option<&String> {
        self.bookmarks.get(&index)
    }

    /// List all bookmarks sorted by index
    ///
    /// # Returns
    ///
    /// A vector of (index, description) pairs sorted by index
    pub fn list(&self) -> Vec<(usize, String)> {
        let mut bookmarks: Vec<_> = self
            .bookmarks
            .iter()
            .map(|(index, desc)| (*index, desc.clone()))
            .collect();

        bookmarks.sort_by_key(|(index, _)| *index);

        bookmarks
    }

    /// Get the number of bookmarks
    pub fn count(&self) -> usize {
        self.bookmarks.len()
    }

    /// Clear all bookmarks
    pub fn clear(&mut self) {
        self.bookmarks.clear();
    }

    /// Get the next bookmark after a given index
    ///
    /// # Arguments
    ///
    /// * `index` - Current message index
    ///
    /// # Returns
    ///
    /// * `Some(next_index)` - If there is a bookmark after the given index
    /// * `None` - If there is no next bookmark (wraps to first)
    pub fn next_bookmark(&self, index: usize) -> Option<usize> {
        let sorted = self.list();

        if sorted.is_empty() {
            return None;
        }

        // Find first bookmark greater than current index
        for (bookmark_index, _) in &sorted {
            if *bookmark_index > index {
                return Some(*bookmark_index);
            }
        }

        // Wrap to first bookmark
        sorted.first().map(|(index, _)| *index)
    }

    /// Get the previous bookmark before a given index
    ///
    /// # Arguments
    ///
    /// * `index` - Current message index
    ///
    /// # Returns
    ///
    /// * `Some(prev_index)` - If there is a bookmark before the given index
    /// * `None` - If there is no previous bookmark (wraps to last)
    pub fn prev_bookmark(&self, index: usize) -> Option<usize> {
        let sorted = self.list();

        if sorted.is_empty() {
            return None;
        }

        // Find last bookmark less than current index
        for (_i, (bookmark_index, _)) in sorted.iter().enumerate().rev() {
            if *bookmark_index < index {
                return Some(*bookmark_index);
            }
        }

        // Wrap to last bookmark
        sorted.last().map(|(index, _)| *index)
    }
}

// ============================================================================
// BOOKMARK UI STATE
// ============================================================================

/// UI state for the bookmarks list popup
#[derive(Clone, Debug, Default)]
pub struct BookmarkListState {
    /// Whether the bookmark list is visible
    pub visible: bool,

    /// Currently selected bookmark index in the list
    pub selected_index: usize,

    /// Cached list of bookmarks for display
    pub cached_bookmarks: Vec<(usize, String)>,
}

impl BookmarkListState {
    /// Create a new bookmark list state
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Show the bookmark list
    pub fn show(&mut self, bookmarks: &[(usize, String)]) {
        self.visible = true;
        self.cached_bookmarks = bookmarks.to_vec();
        self.selected_index = 0;
    }

    /// Hide the bookmark list
    pub fn hide(&mut self) {
        self.visible = false;
        self.cached_bookmarks.clear();
        self.selected_index = 0;
    }

    /// Toggle visibility
    pub fn toggle(&mut self, bookmarks: &[(usize, String)]) {
        if self.visible {
            self.hide();
        } else {
            self.show(bookmarks);
        }
    }

    /// Check if the list is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the currently selected bookmark
    ///
    /// # Returns
    ///
    /// * `Some((index, description))` - If there is a selection
    /// * `None` - If the list is empty or not visible
    pub fn selected(&self) -> Option<(usize, String)> {
        if !self.visible || self.cached_bookmarks.is_empty() {
            return None;
        }

        self.cached_bookmarks.get(self.selected_index).cloned()
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if !self.cached_bookmarks.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        if self.selected_index + 1 < self.cached_bookmarks.len() {
            self.selected_index += 1;
        }
    }

    /// Get the bookmark count
    pub fn count(&self) -> usize {
        self.cached_bookmarks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bookmark_manager_new() {
        let manager = BookmarkManager::new();
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_bookmark_manager_add() {
        let mut manager = BookmarkManager::new();
        manager.add(5, "Test bookmark".to_string());

        assert_eq!(manager.count(), 1);
        assert!(manager.is_bookmarked(5));
        assert_eq!(manager.get(5), Some(&"Test bookmark".to_string()));
    }

    #[test]
    fn test_bookmark_manager_remove() {
        let mut manager = BookmarkManager::new();
        manager.add(5, "Test bookmark".to_string());

        let removed = manager.remove(5);

        assert_eq!(removed, Some("Test bookmark".to_string()));
        assert_eq!(manager.count(), 0);
        assert!(!manager.is_bookmarked(5));
    }

    #[test]
    fn test_bookmark_manager_list() {
        let mut manager = BookmarkManager::new();
        manager.add(3, "Third".to_string());
        manager.add(1, "First".to_string());
        manager.add(2, "Second".to_string());

        let list = manager.list();

        assert_eq!(list.len(), 3);
        assert_eq!(list[0], (1, "First".to_string()));
        assert_eq!(list[1], (2, "Second".to_string()));
        assert_eq!(list[2], (3, "Third".to_string()));
    }

    #[test]
    fn test_bookmark_manager_next_bookmark() {
        let mut manager = BookmarkManager::new();
        manager.add(5, "Five".to_string());
        manager.add(10, "Ten".to_string());
        manager.add(15, "Fifteen".to_string());

        // Next after index 0 should be 5
        assert_eq!(manager.next_bookmark(0), Some(5));

        // Next after index 5 should be 10
        assert_eq!(manager.next_bookmark(5), Some(10));

        // Next after index 15 should wrap to 5
        assert_eq!(manager.next_bookmark(15), Some(5));
    }

    #[test]
    fn test_bookmark_manager_prev_bookmark() {
        let mut manager = BookmarkManager::new();
        manager.add(5, "Five".to_string());
        manager.add(10, "Ten".to_string());
        manager.add(15, "Fifteen".to_string());

        // Prev before index 20 should be 15
        assert_eq!(manager.prev_bookmark(20), Some(15));

        // Prev before index 15 should be 10
        assert_eq!(manager.prev_bookmark(15), Some(10));

        // Prev before index 5 should wrap to 15
        assert_eq!(manager.prev_bookmark(5), Some(15));
    }

    #[test]
    fn test_bookmark_manager_clear() {
        let mut manager = BookmarkManager::new();
        manager.add(1, "One".to_string());
        manager.add(2, "Two".to_string());

        manager.clear();

        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_bookmark_list_state_new() {
        let state = BookmarkListState::new();
        assert!(!state.is_visible());
        assert_eq!(state.count(), 0);
    }

    #[test]
    fn test_bookmark_list_state_show_hide() {
        let mut state = BookmarkListState::new();
        let bookmarks = vec![(1, "One".to_string()), (2, "Two".to_string())];

        state.show(&bookmarks);

        assert!(state.is_visible());
        assert_eq!(state.count(), 2);

        state.hide();

        assert!(!state.is_visible());
        assert_eq!(state.count(), 0);
    }

    #[test]
    fn test_bookmark_list_state_selected() {
        let mut state = BookmarkListState::new();
        let bookmarks = vec![(1, "One".to_string()), (2, "Two".to_string())];

        state.show(&bookmarks);

        assert_eq!(state.selected(), Some((1, "One".to_string())));

        state.move_down();

        assert_eq!(state.selected(), Some((2, "Two".to_string())));
    }

    #[test]
    fn test_bookmark_list_state_navigation() {
        let mut state = BookmarkListState::new();
        let bookmarks = vec![
            (1, "One".to_string()),
            (2, "Two".to_string()),
            (3, "Three".to_string()),
        ];

        state.show(&bookmarks);

        assert_eq!(state.selected_index, 0);

        state.move_down();
        assert_eq!(state.selected_index, 1);

        state.move_down();
        assert_eq!(state.selected_index, 2);

        // Can't go past end
        state.move_down();
        assert_eq!(state.selected_index, 2);

        state.move_up();
        assert_eq!(state.selected_index, 1);

        state.move_up();
        assert_eq!(state.selected_index, 0);

        // Can't go before start
        state.move_up();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_bookmark_list_state_toggle() {
        let mut state = BookmarkListState::new();
        let bookmarks = vec![(1, "One".to_string())];

        state.toggle(&bookmarks);
        assert!(state.is_visible());

        state.toggle(&bookmarks);
        assert!(!state.is_visible());
    }
}
