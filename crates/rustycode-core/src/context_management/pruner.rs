// ── Context Pruner ─────────────────────────────────────────────────────────────

use crate::context_management::window::ContextWindow;
use crate::context_prio::Priority;
use std::cmp::Ordering;

/// Removes low-value content from context windows.
///
/// The ContextPruner analyzes context items and removes those that
/// contribute minimal value to stay within token constraints.
#[derive(Debug, Clone)]
pub struct ContextPruner {
    /// Minimum priority level to keep
    min_priority: Priority,
    /// Maximum age of items to keep (None = no limit)
    max_age: Option<chrono::Duration>,
    /// Minimum score threshold (0.0 to 1.0)
    min_score: f64,
}

impl Default for ContextPruner {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextPruner {
    /// Create a new context pruner with default settings.
    pub fn new() -> Self {
        Self {
            min_priority: Priority::Low,
            max_age: None,
            min_score: 0.3,
        }
    }

    /// Set the minimum priority level.
    pub fn with_min_priority(mut self, priority: Priority) -> Self {
        self.min_priority = priority;
        self
    }

    /// Set the maximum age for items.
    pub fn with_max_age(mut self, duration: chrono::Duration) -> Self {
        self.max_age = Some(duration);
        self
    }

    /// Set the minimum score threshold.
    pub fn with_min_score(mut self, score: f64) -> Self {
        self.min_score = score.clamp(0.0, 1.0);
        self
    }

    /// Prune context items that don't meet criteria.
    ///
    /// # Arguments
    ///
    /// * `window` - Context window to prune
    ///
    /// # Returns
    ///
    /// Number of items pruned
    pub fn prune(&self, window: &mut ContextWindow) -> usize {
        let original_len = window.content().len();
        let now = chrono::Utc::now();

        // Collect indices to remove (in reverse order for safe removal)
        let indices_to_remove: Vec<usize> = window
            .content()
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                // Check priority
                if item.priority < self.min_priority {
                    return true;
                }

                // Check age
                if let Some(max_age) = self.max_age {
                    if let Some(timestamp) = item.metadata.timestamp {
                        if now - timestamp > max_age {
                            return true;
                        }
                    }
                }

                // Check score
                let normalized_score = (item.score() / 100.0).min(1.0);
                if normalized_score < self.min_score {
                    return true;
                }

                false
            })
            .map(|(idx, _)| idx)
            .collect();

        // Remove in reverse order to avoid index shifting
        for idx in indices_to_remove.into_iter().rev() {
            window.remove(idx).ok();
        }

        original_len - window.content().len()
    }

    /// Prune to fit within a token budget.
    ///
    /// # Arguments
    ///
    /// * `window` - Context window to prune
    /// * `target_tokens` - Target token count
    ///
    /// # Returns
    ///
    /// Number of items pruned
    pub fn prune_to_fit(&self, window: &mut ContextWindow, target_tokens: usize) -> usize {
        while window.used_tokens() > target_tokens && !window.content().is_empty() {
            // Find and remove the lowest-value item
            let min_idx = window
                .content()
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    a.1.score()
                        .partial_cmp(&b.1.score())
                        .unwrap_or(Ordering::Equal)
                })
                .map(|(idx, _)| idx);

            if let Some(idx) = min_idx {
                window.remove(idx).ok();
            } else {
                break;
            }
        }

        window.content().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_prio::Priority;

    fn create_test_window(max_tokens: usize) -> ContextWindow {
        let mut window = ContextWindow::new(max_tokens);
        let chunk = "x".repeat(500);
        let _ = window.add_content(&format!("critical {}", chunk), Some(Priority::Critical));
        let _ = window.add_content(&format!("high {}", chunk), Some(Priority::High));
        let _ = window.add_content(&format!("medium {}", chunk), Some(Priority::Medium));
        let _ = window.add_content(&format!("low {}", chunk), Some(Priority::Low));
        let _ = window.add_content(&format!("minimal {}", chunk), Some(Priority::Minimal));
        window
    }

    #[test]
    fn test_pruner_creation() {
        let pruner = ContextPruner::new();
        assert_eq!(pruner.min_priority, Priority::Low);
        assert_eq!(pruner.min_score, 0.3);
    }

    #[test]
    fn test_pruner_with_min_priority() {
        let pruner = ContextPruner::new().with_min_priority(Priority::Medium);
        assert_eq!(pruner.min_priority, Priority::Medium);
    }

    #[test]
    fn test_pruner_with_min_score() {
        let pruner = ContextPruner::new().with_min_score(0.7);
        assert_eq!(pruner.min_score, 0.7);
    }

    #[test]
    fn test_pruner_prune() {
        let pruner = ContextPruner::new().with_min_priority(Priority::High);
        let mut window = create_test_window(1000);

        let original_len = window.content().len();
        let pruned = pruner.prune(&mut window);

        assert!(pruned > 0);
        assert!(window.content().len() < original_len);
        // All remaining items should be high priority or higher
        assert!(window
            .content()
            .iter()
            .all(|item| item.priority >= Priority::High));
    }

    #[test]
    fn test_pruner_prune_to_fit() {
        let pruner = ContextPruner::new();
        let mut window = create_test_window(1000);

        let _pruned = pruner.prune_to_fit(&mut window, 500);

        assert!(window.used_tokens() <= 500);
    }
}
