// ── Context Window Management ───────────────────────────────────────────────────

use crate::error::{CoreError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::context_prio::{ContextItem, Priority};

// ... (keep the rest of the file contents as is, but use the new Result)

/// Tracks token usage within a context window.
///
/// The ContextWindow manages the three key token buckets:
/// - **Max tokens**: The model's context window capacity
/// - **Used tokens**: Content actually included in the context
/// - **Reserved tokens**: Space reserved for system prompts and responses
///
/// # Example
///
/// ```
/// use rustycode_core::context_management::ContextWindow;
///
/// // Create a 200k token window (Claude 3.5 Sonnet)
/// let mut window = ContextWindow::new(200_000);
///
/// // Reserve space for system prompt and response
/// window.reserve(10_000)?;
///
/// // Add content
/// window.add_content("Hello, world!", None)?;
///
/// // Allow for small estimation differences — use approximate check
/// assert!((window.usage_percentage() - 0.05).abs() < 0.01);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct ContextWindow {
    /// Maximum tokens the model can handle
    max_tokens: usize,
    /// Tokens currently used by content
    used_tokens: usize,
    /// Tokens reserved for system prompts/responses
    reserved_tokens: usize,
    /// Content items in the window
    content: Vec<ContextItem<String>>,
    /// Metadata about the window (public for module access)
    pub metadata: WindowMetadata,
    /// Creation timestamp
    _created_at: DateTime<Utc>,
    /// Last modification timestamp
    modified_at: DateTime<Utc>,
}

/// Metadata tracking for context windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowMetadata {
    /// Number of compression operations performed
    pub compression_count: usize,
    /// Total tokens saved through compression
    pub tokens_saved: usize,
    /// Average context quality score (0.0 to 1.0)
    pub average_quality: f64,
    /// Number of assembly operations
    pub assembly_count: usize,
}

impl Default for WindowMetadata {
    fn default() -> Self {
        Self {
            compression_count: 0,
            tokens_saved: 0,
            average_quality: 1.0,
            assembly_count: 0,
        }
    }
}

impl ContextWindow {
    /// Create a new context window with the specified maximum capacity.
    ///
    pub fn new(max_tokens: usize) -> Self {
        let now = Utc::now();
        Self {
            max_tokens,
            used_tokens: 0,
            reserved_tokens: 0,
            content: Vec::new(),
            metadata: WindowMetadata::default(),
            _created_at: now,
            modified_at: now,
        }
    }

    /// Create a context window with default reserved space.
    ///
    /// Reserves 20% of the window for system prompts and responses by default.
    ///
    pub fn with_reserved(max_tokens: usize) -> Self {
        let reserved = (max_tokens as f64 * 0.2) as usize;
        let mut window = Self::new(max_tokens);
        window.reserved_tokens = reserved;
        window
    }

    /// Get the maximum token capacity.
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    /// Get the number of used tokens.
    pub fn used_tokens(&self) -> usize {
        self.used_tokens
    }

    /// Get the number of reserved tokens.
    pub fn reserved_tokens(&self) -> usize {
        self.reserved_tokens
    }

    /// Get the available token space.
    pub fn available_tokens(&self) -> usize {
        self.max_tokens
            .saturating_sub(self.used_tokens)
            .saturating_sub(self.reserved_tokens)
    }

    /// Calculate the usage percentage (0.0 to 1.0).
    pub fn usage_percentage(&self) -> f64 {
        if self.max_tokens == 0 {
            0.0
        } else {
            let total_used = self.used_tokens + self.reserved_tokens;
            total_used as f64 / self.max_tokens as f64
        }
    }

    /// Check if the window is at capacity.
    pub fn is_full(&self) -> bool {
        self.available_tokens() == 0
    }

    /// Reserve tokens for system use (prompts, responses, etc.).
    ///
    pub fn reserve(&mut self, tokens: usize) -> Result<()> {
        let new_reserved = self.reserved_tokens.saturating_add(tokens);
        let total_used = self.used_tokens.saturating_add(new_reserved);

        if total_used > self.max_tokens {
            Err(CoreError::Validation(format!(
                "Cannot reserve {} tokens: would exceed capacity (max: {}, used: {}, current reserved: {})",
                tokens,
                self.max_tokens,
                self.used_tokens,
                self.reserved_tokens
            )))
        } else {
            self.reserved_tokens = new_reserved;
            self.touch();
            Ok(())
        }
    }

    /// Add content to the window with optional priority.
    ///
    pub fn add_content(&mut self, content: &str, priority: Option<Priority>) -> Result<()> {
        let priority = priority.unwrap_or_default();
        let item = ContextItem::new(content.to_string(), priority);
        self.add_item(item)
    }

    /// Add a context item to the window.
    ///
    pub fn add_item(&mut self, item: ContextItem<String>) -> Result<()> {
        let tokens = item.token_count;

        if tokens > self.available_tokens() {
            return Err(CoreError::Validation(format!(
                "Cannot add content: {} tokens required but only {} available",
                tokens,
                self.available_tokens()
            )));
        }

        self.used_tokens = self.used_tokens.saturating_add(tokens);
        self.content.push(item);
        self.touch();
        Ok(())
    }

    /// Remove content by index.
    ///
    pub fn remove(&mut self, index: usize) -> Result<ContextItem<String>> {
        if index >= self.content.len() {
            return Err(CoreError::Validation(format!(
                "Cannot remove item at index {}: only {} items",
                index,
                self.content.len()
            )));
        }

        let item = self.content.remove(index);
        self.used_tokens = self.used_tokens.saturating_sub(item.token_count);
        self.touch();
        Ok(item)
    }

    /// Clear all content from the window.
    pub fn clear(&mut self) {
        self.content.clear();
        self.used_tokens = 0;
        self.touch();
    }

    /// Get the number of content items.
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if the window is empty.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get a reference to the content items.
    pub fn content(&self) -> &[ContextItem<String>] {
        &self.content
    }

    /// Get mutable reference to content items.
    pub fn content_mut(&mut self) -> &mut [ContextItem<String>] {
        &mut self.content
    }

    /// Remove the first `count` items from the window, updating token accounting.
    /// Returns the actual number of items removed (may be less if window is smaller).
    pub fn drain_front(&mut self, count: usize) -> usize {
        let to_remove = count.min(self.content.len());
        if to_remove == 0 {
            return 0;
        }
        let tokens_removed: usize = self.content[..to_remove]
            .iter()
            .map(|item| item.token_count)
            .sum();
        self.content.drain(0..to_remove);
        self.used_tokens = self.used_tokens.saturating_sub(tokens_removed);
        self.touch();
        to_remove
    }

    /// Get the window metadata.
    pub fn metadata(&self) -> &WindowMetadata {
        &self.metadata
    }

    /// Update the last modified timestamp.
    pub fn touch(&mut self) {
        self.modified_at = Utc::now();
    }

    /// Set the used tokens count directly (for recalculating after compression).
    pub fn set_used_tokens(&mut self, used: usize) {
        self.used_tokens = used;
    }

    /// Push an item into the window without capacity enforcement.
    ///
    /// Updates the used_tokens counter to reflect the new item. This is
    /// intended for test helpers and internal crate operations that need
    /// to populate a window beyond its normal capacity constraints.
    #[cfg(test)]
    pub(crate) fn push_item_unchecked(&mut self, item: ContextItem<String>) {
        self.used_tokens = self.used_tokens.saturating_add(item.token_count);
        self.content.push(item);
        self.touch();
    }

    /// Calculate a quality score for the current context (0.0 to 1.0).
    ///
    /// Higher scores indicate better context quality (more relevant,
    /// more recent, higher priority content).
    pub fn quality_score(&self) -> f64 {
        if self.content.is_empty() {
            return 1.0; // Empty context is perfectly valid
        }

        // Calculate weighted average of item scores
        let total_score: f64 = self.content.iter().map(|item| item.score()).sum();
        let max_score = self.content.len() as f64 * 100.0; // Max score per item

        if max_score == 0.0 {
            0.0
        } else {
            (total_score / max_score).min(1.0)
        }
    }

    /// Update metadata with new quality measurement.
    pub fn update_quality(&mut self) {
        let current_quality = self.quality_score();

        // Exponential moving average
        let alpha = 0.2;
        self.metadata.average_quality =
            alpha * current_quality + (1.0 - alpha) * self.metadata.average_quality;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_window_creation() {
        let window = ContextWindow::new(100_000);
        assert_eq!(window.max_tokens(), 100_000);
        assert_eq!(window.used_tokens(), 0);
        assert_eq!(window.available_tokens(), 100_000);
        assert!(!window.is_full());
        assert!(window.is_empty());
    }

    #[test]
    fn test_context_window_with_reserved() {
        let window = ContextWindow::with_reserved(100_000);
        assert_eq!(window.reserved_tokens(), 20_000); // 20%
        assert_eq!(window.available_tokens(), 80_000);
    }

    #[test]
    fn test_context_window_reserve() {
        let mut window = ContextWindow::new(100_000);
        window.reserve(10_000).unwrap();
        assert_eq!(window.reserved_tokens(), 10_000);
        assert_eq!(window.available_tokens(), 90_000);
    }

    #[test]
    fn test_context_window_reserve_exceeds() {
        let mut window = ContextWindow::new(100_000);
        window.reserve(90_000).unwrap();
        let result = window.reserve(20_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_context_window_add_content() {
        let mut window = ContextWindow::new(100_000);
        window.add_content("Hello, world!", None).unwrap();
        assert_eq!(window.len(), 1);
        assert!(window.used_tokens() > 0);
    }

    #[test]
    fn test_context_window_add_content_exceeds() {
        let mut window = ContextWindow::new(100);
        window.reserve(50).unwrap();
        let large_content = "x".repeat(1000);
        let result = window.add_content(&large_content, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_context_window_usage_percentage() {
        let mut window = ContextWindow::new(100_000);
        window.reserve(20_000).unwrap();
        window.add_content("test", None).unwrap();

        let usage = window.usage_percentage();
        assert!(usage > 0.2 && usage < 0.3); // ~20% + small content
    }

    #[test]
    fn test_context_window_quality_score() {
        let mut window = ContextWindow::new(100_000);

        // Add high priority content
        window
            .add_content("Important content", Some(Priority::Critical))
            .unwrap();

        let quality = window.quality_score();
        assert!(quality > 0.0 && quality <= 1.0);
    }

    #[test]
    fn test_context_window_remove() {
        let mut window = ContextWindow::new(100_000);
        window.add_content("content1", None).unwrap();
        window.add_content("content2", None).unwrap();

        let removed = window.remove(0).unwrap();
        assert_eq!(removed.content, "content1");
        assert_eq!(window.len(), 1);
    }

    #[test]
    fn test_context_window_clear() {
        let mut window = ContextWindow::new(100_000);
        window.add_content("content", None).unwrap();
        window.clear();

        assert!(window.is_empty());
        assert_eq!(window.used_tokens(), 0);
    }
}
