//! Message tagging system
//!
//! Provides tag management for messages:
//! - Predefined tags: "important", "idea", "bug", "solution"
//! - Custom tags: arbitrary user-created tags
//! - Tag operations: add, remove, list tags
//! - Tag filtering: show only messages with specific tag
//! - Persistence: tags survive session recovery

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ============================================================================
// TAG TYPES
// ============================================================================

/// Predefined tag types
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[non_exhaustive]
pub enum TagType {
    /// Important message
    Important,
    /// Idea or suggestion
    Idea,
    /// Bug report or issue
    Bug,
    /// Solution or fix
    Solution,
    /// Clarification question from AI
    Clarification,
    /// Custom user-defined tag
    Custom(String),
}

impl TagType {
    /// Get the display name of this tag
    pub fn display_name(&self) -> String {
        match self {
            TagType::Important => "important".to_string(),
            TagType::Idea => "idea".to_string(),
            TagType::Bug => "bug".to_string(),
            TagType::Solution => "solution".to_string(),
            TagType::Clarification => "clarification".to_string(),
            TagType::Custom(name) => name.clone(),
        }
    }

    /// Get the color for this tag
    pub fn color(&self) -> Color {
        match self {
            TagType::Important => Color::Rgb(255, 80, 80),  // Red
            TagType::Idea => Color::Rgb(100, 200, 255),     // Light blue
            TagType::Bug => Color::Rgb(255, 100, 100),      // Light red
            TagType::Solution => Color::Rgb(100, 255, 100), // Light green
            TagType::Clarification => Color::Rgb(255, 165, 0), // Orange
            TagType::Custom(_) => Color::Rgb(200, 150, 255), // Purple
        }
    }

    /// Get the icon for this tag
    pub fn icon(&self) -> &str {
        match self {
            TagType::Important => "⭐",
            TagType::Idea => "💡",
            TagType::Bug => "🐛",
            TagType::Solution => "✓",
            TagType::Clarification => "❓",
            TagType::Custom(_) => "•",
        }
    }

    /// Parse a tag from a string
    pub fn from_string(s: &str) -> Self {
        match s {
            "important" => TagType::Important,
            "idea" => TagType::Idea,
            "bug" => TagType::Bug,
            "solution" => TagType::Solution,
            "clarification" => TagType::Clarification,
            custom => TagType::Custom(custom.to_string()),
        }
    }

    /// Check if this is a predefined tag
    pub fn is_predefined(&self) -> bool {
        matches!(
            self,
            TagType::Important
                | TagType::Idea
                | TagType::Bug
                | TagType::Solution
                | TagType::Clarification
        )
    }
}

/// A tag applied to a message
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag {
    /// Tag type
    pub tag_type: TagType,
    /// Optional note attached to the tag
    pub note: Option<String>,
}

impl Tag {
    /// Create a new tag
    pub fn new(tag_type: TagType) -> Self {
        Self {
            tag_type,
            note: None,
        }
    }

    /// Create a new tag with a note
    pub fn with_note(tag_type: TagType, note: String) -> Self {
        Self {
            tag_type,
            note: Some(note),
        }
    }

    /// Get the display name
    pub fn display_name(&self) -> String {
        self.tag_type.display_name()
    }
}

// ============================================================================
// TAG FILTER
// ============================================================================

/// Filter for displaying messages by tag
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TagFilter {
    /// Active filter (None = show all)
    pub active_tag: Option<TagType>,
}

impl TagFilter {
    /// Create a new tag filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the active tag filter
    pub fn set_active(&mut self, tag: Option<TagType>) {
        self.active_tag = tag;
    }

    /// Clear the filter (show all messages)
    pub fn clear(&mut self) {
        self.active_tag = None;
    }

    /// Check if a filter is active
    pub fn is_active(&self) -> bool {
        self.active_tag.is_some()
    }

    /// Check if a set of tags matches the filter
    pub fn matches(&self, tags: &[Tag]) -> bool {
        match &self.active_tag {
            None => true, // No filter, show all
            Some(filter_tag) => tags.iter().any(|tag| &tag.tag_type == filter_tag),
        }
    }
}

// ============================================================================
// TAG REGISTRY
// ============================================================================

/// Registry of tags for all messages
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TagRegistry {
    /// Map of message_id -> tags
    tags: HashMap<String, Vec<Tag>>,
}

impl TagRegistry {
    /// Create a new tag registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a tag to a message
    pub fn add_tag(&mut self, message_id: String, tag: Tag) -> bool {
        let tags = self.tags.entry(message_id).or_default();
        // Prevent duplicates
        if !tags.contains(&tag) {
            tags.push(tag);
            tags.sort();
            true
        } else {
            false
        }
    }

    /// Remove a specific tag from a message
    pub fn remove_tag(&mut self, message_id: &str, tag: &Tag) -> bool {
        if let Some(tags) = self.tags.get_mut(message_id) {
            let original_len = tags.len();
            tags.retain(|t| t != tag);
            let removed = original_len > 0 && tags.len() < original_len;
            // If all tags removed, delete the entry
            if tags.is_empty() {
                let _ = tags; // Release the mutable ref before calling remove
                self.tags.remove(message_id);
            }
            removed
        } else {
            false
        }
    }

    /// Remove all tags of a specific type from a message
    pub fn remove_tag_type(&mut self, message_id: &str, tag_type: &TagType) -> bool {
        if let Some(tags) = self.tags.get_mut(message_id) {
            let original_len = tags.len();
            tags.retain(|t| &t.tag_type != tag_type);
            tags.len() < original_len
        } else {
            false
        }
    }

    /// Get tags for a message
    pub fn get_tags(&self, message_id: &str) -> Option<Vec<Tag>> {
        self.tags.get(message_id).cloned()
    }

    /// Check if a message has a specific tag type
    pub fn has_tag(&self, message_id: &str, tag_type: &TagType) -> bool {
        self.tags
            .get(message_id)
            .map(|tags| tags.iter().any(|t| &t.tag_type == tag_type))
            .unwrap_or(false)
    }

    /// Get all messages with a specific tag
    pub fn get_messages_with_tag(&self, tag_type: &TagType) -> Vec<String> {
        self.tags
            .iter()
            .filter(|(_, tags)| tags.iter().any(|t| &t.tag_type == tag_type))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Clear all tags for a message
    pub fn clear_message_tags(&mut self, message_id: &str) -> bool {
        self.tags.remove(message_id).is_some()
    }

    /// Get all unique tag types in use
    pub fn get_all_tag_types(&self) -> Vec<TagType> {
        let mut tag_types = HashSet::new();
        for tags in self.tags.values() {
            for tag in tags {
                tag_types.insert(tag.tag_type.clone());
            }
        }
        let mut result: Vec<_> = tag_types.into_iter().collect();
        result.sort();
        result
    }

    /// Get count of messages with specific tag
    pub fn count_with_tag(&self, tag_type: &TagType) -> usize {
        self.tags
            .values()
            .filter(|tags| tags.iter().any(|t| &t.tag_type == tag_type))
            .count()
    }

    /// Get total number of messages with any tags
    pub fn count_tagged_messages(&self) -> usize {
        self.tags
            .iter()
            .filter(|(_, tags)| !tags.is_empty())
            .count()
    }

    /// Merge another tag registry into this one
    pub fn merge(&mut self, other: TagRegistry) {
        for (message_id, tags) in other.tags {
            for tag in tags {
                self.add_tag(message_id.clone(), tag);
            }
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // TAG TYPE TESTS
    // ========================================================================

    #[test]
    fn test_tag_type_display_names() {
        assert_eq!(TagType::Important.display_name(), "important");
        assert_eq!(TagType::Idea.display_name(), "idea");
        assert_eq!(TagType::Bug.display_name(), "bug");
        assert_eq!(TagType::Solution.display_name(), "solution");
        assert_eq!(
            TagType::Custom("my-tag".to_string()).display_name(),
            "my-tag"
        );
    }

    #[test]
    fn test_tag_type_from_string() {
        assert_eq!(TagType::from_string("important"), TagType::Important);
        assert_eq!(TagType::from_string("idea"), TagType::Idea);
        assert_eq!(TagType::from_string("bug"), TagType::Bug);
        assert_eq!(TagType::from_string("solution"), TagType::Solution);
        assert_eq!(
            TagType::from_string("custom"),
            TagType::Custom("custom".to_string())
        );
    }

    #[test]
    fn test_tag_type_icons() {
        assert_eq!(TagType::Important.icon(), "⭐");
        assert_eq!(TagType::Idea.icon(), "💡");
        assert_eq!(TagType::Bug.icon(), "🐛");
        assert_eq!(TagType::Solution.icon(), "✓");
        assert_eq!(TagType::Custom("test".to_string()).icon(), "•");
    }

    #[test]
    fn test_tag_type_is_predefined() {
        assert!(TagType::Important.is_predefined());
        assert!(TagType::Idea.is_predefined());
        assert!(TagType::Bug.is_predefined());
        assert!(TagType::Solution.is_predefined());
        assert!(!TagType::Custom("test".to_string()).is_predefined());
    }

    #[test]
    fn test_tag_type_colors() {
        let important_color = TagType::Important.color();
        assert!(matches!(important_color, Color::Rgb(_, _, _)));

        let custom_color = TagType::Custom("test".to_string()).color();
        assert!(matches!(custom_color, Color::Rgb(_, _, _)));
    }

    // ========================================================================
    // TAG STRUCT TESTS
    // ========================================================================

    #[test]
    fn test_tag_creation() {
        let tag = Tag::new(TagType::Important);
        assert_eq!(tag.tag_type, TagType::Important);
        assert_eq!(tag.note, None);
    }

    #[test]
    fn test_tag_with_note() {
        let tag = Tag::with_note(TagType::Important, "Critical issue".to_string());
        assert_eq!(tag.tag_type, TagType::Important);
        assert_eq!(tag.note, Some("Critical issue".to_string()));
    }

    #[test]
    fn test_tag_display_name() {
        let tag = Tag::new(TagType::Idea);
        assert_eq!(tag.display_name(), "idea");
    }

    // ========================================================================
    // TAG FILTER TESTS
    // ========================================================================

    #[test]
    fn test_tag_filter_creation() {
        let filter = TagFilter::new();
        assert_eq!(filter.active_tag, None);
        assert!(!filter.is_active());
    }

    #[test]
    fn test_tag_filter_set_active() {
        let mut filter = TagFilter::new();
        filter.set_active(Some(TagType::Important));
        assert_eq!(filter.active_tag, Some(TagType::Important));
        assert!(filter.is_active());
    }

    #[test]
    fn test_tag_filter_clear() {
        let mut filter = TagFilter::new();
        filter.set_active(Some(TagType::Important));
        filter.clear();
        assert_eq!(filter.active_tag, None);
        assert!(!filter.is_active());
    }

    #[test]
    fn test_tag_filter_matches_no_filter() {
        let filter = TagFilter::new();
        let tag = Tag::new(TagType::Important);
        assert!(filter.matches(&[tag]));
    }

    #[test]
    fn test_tag_filter_matches_with_filter() {
        let mut filter = TagFilter::new();
        filter.set_active(Some(TagType::Important));

        let important_tag = Tag::new(TagType::Important);
        let idea_tag = Tag::new(TagType::Idea);

        assert!(filter.matches(std::slice::from_ref(&important_tag)));
        assert!(!filter.matches(std::slice::from_ref(&idea_tag)));
        assert!(filter.matches(&[idea_tag, important_tag]));
    }

    #[test]
    fn test_tag_filter_matches_empty_tags() {
        let mut filter = TagFilter::new();
        filter.set_active(Some(TagType::Important));
        assert!(!filter.matches(&[]));
    }

    // ========================================================================
    // TAG REGISTRY TESTS
    // ========================================================================

    #[test]
    fn test_registry_add_tag() {
        let mut registry = TagRegistry::new();
        let tag = Tag::new(TagType::Important);

        assert!(registry.add_tag("msg1".to_string(), tag.clone()));
        assert_eq!(registry.get_tags("msg1"), Some(vec![tag]));
    }

    #[test]
    fn test_registry_add_duplicate_tag() {
        let mut registry = TagRegistry::new();
        let tag = Tag::new(TagType::Important);

        assert!(registry.add_tag("msg1".to_string(), tag.clone()));
        assert!(!registry.add_tag("msg1".to_string(), tag)); // Duplicate should fail
        assert_eq!(registry.get_tags("msg1").unwrap().len(), 1);
    }

    #[test]
    fn test_registry_add_multiple_tags() {
        let mut registry = TagRegistry::new();
        let important = Tag::new(TagType::Important);
        let idea = Tag::new(TagType::Idea);

        registry.add_tag("msg1".to_string(), important);
        registry.add_tag("msg1".to_string(), idea);

        assert_eq!(registry.get_tags("msg1").unwrap().len(), 2);
    }

    #[test]
    fn test_registry_remove_tag() {
        let mut registry = TagRegistry::new();
        let tag = Tag::new(TagType::Important);

        registry.add_tag("msg1".to_string(), tag.clone());
        assert!(registry.remove_tag("msg1", &tag));
        assert_eq!(registry.get_tags("msg1"), None);
    }

    #[test]
    fn test_registry_remove_nonexistent_tag() {
        let mut registry = TagRegistry::new();
        let tag = Tag::new(TagType::Important);
        assert!(!registry.remove_tag("msg1", &tag));
    }

    #[test]
    fn test_registry_remove_tag_type() {
        let mut registry = TagRegistry::new();
        let important1 = Tag::new(TagType::Important);
        let important2 = Tag::with_note(TagType::Important, "Note".to_string());
        let idea = Tag::new(TagType::Idea);

        registry.add_tag("msg1".to_string(), important1);
        registry.add_tag("msg1".to_string(), important2);
        registry.add_tag("msg1".to_string(), idea.clone());

        assert!(registry.remove_tag_type("msg1", &TagType::Important));
        let tags = registry.get_tags("msg1").unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0], idea);
    }

    #[test]
    fn test_registry_has_tag() {
        let mut registry = TagRegistry::new();
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Important));

        assert!(registry.has_tag("msg1", &TagType::Important));
        assert!(!registry.has_tag("msg1", &TagType::Idea));
        assert!(!registry.has_tag("msg2", &TagType::Important));
    }

    #[test]
    fn test_registry_get_messages_with_tag() {
        let mut registry = TagRegistry::new();
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Important));
        registry.add_tag("msg2".to_string(), Tag::new(TagType::Idea));
        registry.add_tag("msg3".to_string(), Tag::new(TagType::Important));

        let important_messages = registry.get_messages_with_tag(&TagType::Important);
        assert_eq!(important_messages.len(), 2);
        assert!(important_messages.contains(&"msg1".to_string()));
        assert!(important_messages.contains(&"msg3".to_string()));
    }

    #[test]
    fn test_registry_clear_message_tags() {
        let mut registry = TagRegistry::new();
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Important));
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Idea));

        assert!(registry.clear_message_tags("msg1"));
        assert_eq!(registry.get_tags("msg1"), None);
    }

    #[test]
    fn test_registry_clear_nonexistent_message() {
        let mut registry = TagRegistry::new();
        assert!(!registry.clear_message_tags("nonexistent"));
    }

    #[test]
    fn test_registry_get_all_tag_types() {
        let mut registry = TagRegistry::new();
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Important));
        registry.add_tag("msg2".to_string(), Tag::new(TagType::Idea));
        registry.add_tag("msg3".to_string(), Tag::new(TagType::Important));
        registry.add_tag(
            "msg4".to_string(),
            Tag::new(TagType::Custom("custom".to_string())),
        );

        let tag_types = registry.get_all_tag_types();
        assert!(tag_types.contains(&TagType::Important));
        assert!(tag_types.contains(&TagType::Idea));
        assert!(tag_types.contains(&TagType::Custom("custom".to_string())));
        assert_eq!(tag_types.len(), 3);
    }

    #[test]
    fn test_registry_count_with_tag() {
        let mut registry = TagRegistry::new();
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Important));
        registry.add_tag("msg2".to_string(), Tag::new(TagType::Important));
        registry.add_tag("msg3".to_string(), Tag::new(TagType::Idea));

        assert_eq!(registry.count_with_tag(&TagType::Important), 2);
        assert_eq!(registry.count_with_tag(&TagType::Idea), 1);
        assert_eq!(registry.count_with_tag(&TagType::Bug), 0);
    }

    #[test]
    fn test_registry_count_tagged_messages() {
        let mut registry = TagRegistry::new();
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Important));
        registry.add_tag("msg2".to_string(), Tag::new(TagType::Idea));

        assert_eq!(registry.count_tagged_messages(), 2);
    }

    #[test]
    fn test_registry_merge() {
        let mut registry1 = TagRegistry::new();
        registry1.add_tag("msg1".to_string(), Tag::new(TagType::Important));

        let mut registry2 = TagRegistry::new();
        registry2.add_tag("msg2".to_string(), Tag::new(TagType::Idea));

        registry1.merge(registry2);

        assert_eq!(registry1.count_tagged_messages(), 2);
        assert!(registry1.has_tag("msg1", &TagType::Important));
        assert!(registry1.has_tag("msg2", &TagType::Idea));
    }

    #[test]
    fn test_registry_tags_sorted() {
        let mut registry = TagRegistry::new();
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Solution));
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Bug));
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Important));

        let tags = registry.get_tags("msg1").unwrap();
        // Tags should be in sorted order
        assert!(tags[0].tag_type <= tags[1].tag_type);
    }

    // ========================================================================
    // SERIALIZATION TESTS
    // ========================================================================

    #[test]
    fn test_tag_type_serialization() {
        let tag = TagType::Important;
        let json = serde_json::to_string(&tag).unwrap();
        let deserialized: TagType = serde_json::from_str(&json).unwrap();
        assert_eq!(tag, deserialized);
    }

    #[test]
    fn test_custom_tag_type_serialization() {
        let tag = TagType::Custom("my-tag".to_string());
        let json = serde_json::to_string(&tag).unwrap();
        let deserialized: TagType = serde_json::from_str(&json).unwrap();
        assert_eq!(tag, deserialized);
    }

    #[test]
    fn test_tag_serialization() {
        let tag = Tag::with_note(TagType::Important, "Note".to_string());
        let json = serde_json::to_string(&tag).unwrap();
        let deserialized: Tag = serde_json::from_str(&json).unwrap();
        assert_eq!(tag, deserialized);
    }

    #[test]
    fn test_registry_serialization() {
        let mut registry = TagRegistry::new();
        registry.add_tag("msg1".to_string(), Tag::new(TagType::Important));
        registry.add_tag("msg2".to_string(), Tag::new(TagType::Idea));

        let json = serde_json::to_string(&registry).unwrap();
        let deserialized: TagRegistry = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.count_tagged_messages(), 2);
        assert!(deserialized.has_tag("msg1", &TagType::Important));
        assert!(deserialized.has_tag("msg2", &TagType::Idea));
    }

    #[test]
    fn test_tag_filter_serialization() {
        let mut filter = TagFilter::new();
        filter.set_active(Some(TagType::Important));

        let json = serde_json::to_string(&filter).unwrap();
        let deserialized: TagFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(filter, deserialized);
    }
}
