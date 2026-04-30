// ── Context Prioritization Types ───────────────────────────────────────────────

use crate::context::TokenCounter;

/// Priority level for context items.
///
/// Items are prioritized in this order (highest to lowest):
/// 1. Critical
/// 2. High
/// 3. Medium
/// 4. Low
/// 5. Minimal
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Priority {
    /// Critical items (always included if possible)
    Critical = 100,
    /// High importance items
    High = 75,
    /// Medium importance items
    #[default]
    Medium = 50,
    /// Low importance items
    Low = 25,
    /// Minimal importance items
    Minimal = 10,
}

/// Metadata for context items that influences prioritization.
#[derive(Debug, Clone)]
pub struct Metadata {
    /// Optional identifier (e.g., file path, memory ID)
    pub id: Option<String>,
    /// Optional timestamp for recency scoring
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// Optional tags for custom categorization
    pub tags: Vec<String>,
    /// Optional custom score multiplier (0.0 to 10.0, default 1.0)
    pub score_multiplier: f64,
    /// Usage frequency (higher = more important)
    pub usage_count: usize,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            id: None,
            timestamp: None,
            tags: Vec::new(),
            score_multiplier: 1.0,
            usage_count: 0,
        }
    }
}

/// A context item that can be prioritized and scored.
///
/// # Type Parameters
///
/// * `T` - The underlying data type
///
/// # Example
///
/// ```
/// use rustycode_core::context_prio::{ContextItem, Priority};
///
/// let item = ContextItem::new("file content", Priority::High)
///     .with_metadata("path", "src/main.rs");
/// ```
#[derive(Debug, Clone)]
pub struct ContextItem<T> {
    /// The actual content/data
    pub content: T,
    /// Priority level of this item
    pub priority: Priority,
    /// Estimated token count
    pub token_count: usize,
    /// Additional metadata for scoring
    pub metadata: Metadata,
}

impl<T> ContextItem<T>
where
    T: AsRef<str>,
{
    /// Create a new context item with the given priority.
    ///
    /// # Arguments
    ///
    /// * `content` - The content/data
    /// * `priority` - Priority level
    pub fn new(content: T, priority: Priority) -> Self {
        let token_count = TokenCounter::estimate_tokens(content.as_ref());

        Self {
            content,
            priority,
            token_count,
            metadata: Metadata::default(),
        }
    }

    /// Set the ID metadata field.
    ///
    /// # Arguments
    ///
    /// * `id` - Identifier for this item
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.metadata.id = Some(id.into());
        self
    }

    /// Set metadata by key.
    ///
    /// # Arguments
    ///
    /// * `key` - Metadata key
    /// * `value` - Metadata value
    pub fn with_metadata(mut self, key: &str, value: impl Into<String>) -> Self {
        match key {
            "id" => self.metadata.id = Some(value.into()),
            "timestamp" => {
                // Parse ISO 8601 timestamp
                if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&value.into()) {
                    self.metadata.timestamp = Some(ts.with_timezone(&chrono::Utc));
                }
            }
            "tag" => self.metadata.tags.push(value.into()),
            "multiplier" => {
                if let Ok(m) = value.into().parse::<f64>() {
                    self.metadata.score_multiplier = m.clamp(0.0, 10.0);
                }
            }
            "usage_count" => {
                if let Ok(c) = value.into().parse::<usize>() {
                    self.metadata.usage_count = c;
                }
            }
            _ => {}
        }
        self
    }

    /// Set the timestamp metadata.
    ///
    /// # Arguments
    ///
    /// * `timestamp` - Timestamp for recency scoring
    pub fn with_timestamp(mut self, timestamp: chrono::DateTime<chrono::Utc>) -> Self {
        self.metadata.timestamp = Some(timestamp);
        self
    }

    /// Add a tag to the metadata.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag to add
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.metadata.tags.push(tag.into());
        self
    }

    /// Set the score multiplier.
    ///
    /// # Arguments
    ///
    /// * `multiplier` - Score multiplier (0.0 to 10.0)
    pub fn with_score_multiplier(mut self, multiplier: f64) -> Self {
        self.metadata.score_multiplier = multiplier.clamp(0.0, 10.0);
        self
    }

    /// Set the usage count.
    ///
    /// # Arguments
    ///
    /// * `count` - Usage frequency
    pub fn with_usage_count(mut self, count: usize) -> Self {
        self.metadata.usage_count = count;
        self
    }

    /// Calculate the composite score for this item.
    ///
    /// Higher scores indicate higher priority. The score is calculated as:
    ///
    /// ```text
    /// score = priority_score * multiplier * (1 + usage_bonus + recency_bonus)
    /// ```
    ///
    /// # Returns
    ///
    /// Composite score (higher is better)
    pub fn score(&self) -> f64 {
        let base = self.priority as usize as f64;

        // Apply custom multiplier
        let mut score = base * self.metadata.score_multiplier;

        // Add usage bonus (logarithmic scaling)
        if self.metadata.usage_count > 0 {
            let usage_bonus = (self.metadata.usage_count as f64).ln() * 2.0;
            score *= 1.0 + usage_bonus / 100.0;
        }

        // Add recency bonus (if timestamp available)
        if let Some(ts) = self.metadata.timestamp {
            let age_hours = (chrono::Utc::now() - ts).num_hours().max(0) as f64;
            let recency_bonus = 100.0 / (1.0 + age_hours / 24.0); // Decay over days
            score *= 1.0 + recency_bonus / 100.0;
        }

        score
    }

    /// Calculate score per token (efficiency metric).
    ///
    /// Higher values indicate more priority per token cost.
    ///
    /// # Returns
    ///
    /// Score per token (higher is better)
    pub fn score_per_token(&self) -> f64 {
        if self.token_count == 0 {
            0.0
        } else {
            self.score() / (self.token_count as f64)
        }
    }
}

impl<T> PartialEq for ContextItem<T> {
    fn eq(&self, other: &Self) -> bool {
        // Compare by ID if available, otherwise by content
        match (&self.metadata.id, &other.metadata.id) {
            (Some(a), Some(b)) => a == b,
            _ => std::ptr::eq(&self.content, &other.content),
        }
    }
}

impl<T> Eq for ContextItem<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_context_item_creation() {
        let item = ContextItem::new("test content", Priority::High);

        assert_eq!(item.content, "test content");
        assert_eq!(item.priority, Priority::High);
        assert!(item.token_count > 0);
    }

    #[test]
    fn test_context_item_with_id() {
        let item = ContextItem::new("content", Priority::Medium).with_id("test-id");

        assert_eq!(item.metadata.id, Some("test-id".to_string()));
    }

    #[test]
    fn test_context_item_with_tags() {
        let item = ContextItem::new("content", Priority::Medium)
            .with_tag("important")
            .with_tag("recent");

        assert_eq!(item.metadata.tags.len(), 2);
        assert!(item.metadata.tags.contains(&"important".to_string()));
    }

    #[test]
    fn test_context_item_score_multiplier() {
        let item1 = ContextItem::new("content", Priority::High).with_score_multiplier(2.0);

        let item2 = ContextItem::new("content", Priority::High).with_score_multiplier(0.5);

        assert!(item1.score() > item2.score());
    }

    #[test]
    fn test_context_item_timestamp() {
        let now = Utc::now();
        let old = now - chrono::Duration::days(30);

        let item_recent = ContextItem::new("content", Priority::Medium).with_timestamp(now);

        let item_old = ContextItem::new("content", Priority::Medium).with_timestamp(old);

        assert!(item_recent.score() >= item_old.score());
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Medium);
        assert!(Priority::Medium > Priority::Low);
        assert!(Priority::Low > Priority::Minimal);
    }
}
