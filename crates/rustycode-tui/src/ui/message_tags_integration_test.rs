//! Integration tests for message tagging system
//!
//! Tests the full tagging workflow including filtering and persistence

#[cfg(test)]
mod tests {
    use crate::ui::message_tags::{Tag, TagFilter, TagRegistry, TagType};
    #[allow(unused_imports)]
    use crate::ui::message_types::Message;

    // ========================================================================
    // MESSAGE + TAG INTEGRATION TESTS
    // ========================================================================

    #[test]
    fn test_message_with_tags_serialization() {
        let mut msg = Message::user("Test message".to_string());
        msg.add_tag(Tag::new(TagType::Important));
        msg.add_tag(Tag::with_note(TagType::Bug, "Critical".to_string()));

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.tags.len(), 2);
        assert!(deserialized.has_tag(&TagType::Important));
        assert!(deserialized.has_tag(&TagType::Bug));
    }

    #[test]
    fn test_tag_registry_with_multiple_messages() {
        let mut registry = TagRegistry::new();

        // Add messages with different tags
        let msg1_id = "msg1".to_string();
        let msg2_id = "msg2".to_string();
        let msg3_id = "msg3".to_string();

        registry.add_tag(msg1_id.clone(), Tag::new(TagType::Important));
        registry.add_tag(msg2_id.clone(), Tag::new(TagType::Idea));
        registry.add_tag(msg3_id.clone(), Tag::new(TagType::Important));
        registry.add_tag(msg3_id.clone(), Tag::new(TagType::Bug));

        // Test filtering
        let important_msgs = registry.get_messages_with_tag(&TagType::Important);
        assert_eq!(important_msgs.len(), 2);

        let idea_msgs = registry.get_messages_with_tag(&TagType::Idea);
        assert_eq!(idea_msgs.len(), 1);

        let bug_msgs = registry.get_messages_with_tag(&TagType::Bug);
        assert_eq!(bug_msgs.len(), 1);
    }

    // ========================================================================
    // TAG FILTER + MESSAGE FILTERING INTEGRATION TESTS
    // ========================================================================

    #[test]
    fn test_filter_messages_by_tag() {
        let mut messages = [
            Message::user("First message".to_string()),
            Message::assistant("Second message".to_string()),
            Message::user("Third message".to_string()),
        ];

        messages[0].add_tag(Tag::new(TagType::Important));
        messages[2].add_tag(Tag::new(TagType::Important));

        let mut filter = TagFilter::new();
        filter.set_active(Some(TagType::Important));

        let filtered: Vec<_> = messages
            .iter()
            .filter(|m| filter.matches(m.get_tags()))
            .collect();

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].content, "First message");
        assert_eq!(filtered[1].content, "Third message");
    }

    #[test]
    fn test_no_filter_shows_all() {
        let mut messages = [
            Message::user("First".to_string()),
            Message::assistant("Second".to_string()),
            Message::user("Third".to_string()),
        ];

        messages[0].add_tag(Tag::new(TagType::Important));
        // Others have no tags

        let filter = TagFilter::new();
        let filtered: Vec<_> = messages
            .iter()
            .filter(|m| filter.matches(m.get_tags()))
            .collect();

        assert_eq!(filtered.len(), 3); // All shown when no filter
    }

    #[test]
    fn test_empty_messages_with_filter() {
        let messages: Vec<Message> = vec![];

        let mut filter = TagFilter::new();
        filter.set_active(Some(TagType::Important));

        let filtered: Vec<_> = messages
            .iter()
            .filter(|m| filter.matches(m.get_tags()))
            .collect();

        assert_eq!(filtered.len(), 0);
    }

    // ========================================================================
    // COMPLEX TAGGING SCENARIOS
    // ========================================================================

    #[test]
    fn test_message_multiple_tags_and_filter() {
        let mut msg = Message::assistant("Complex message".to_string());
        msg.add_tag(Tag::new(TagType::Important));
        msg.add_tag(Tag::new(TagType::Solution));
        msg.add_tag(Tag::new(TagType::Idea));

        let mut filter_important = TagFilter::new();
        filter_important.set_active(Some(TagType::Important));

        let mut filter_idea = TagFilter::new();
        filter_idea.set_active(Some(TagType::Idea));

        let mut filter_bug = TagFilter::new();
        filter_bug.set_active(Some(TagType::Bug));

        assert!(filter_important.matches(msg.get_tags()));
        assert!(filter_idea.matches(msg.get_tags()));
        assert!(!filter_bug.matches(msg.get_tags()));
    }

    #[test]
    fn test_tag_lifecycle() {
        let mut msg = Message::user("Test".to_string());

        // Add tag
        assert!(msg.add_tag(Tag::new(TagType::Important)));
        assert!(msg.has_tag(&TagType::Important));

        // Try to add duplicate
        assert!(!msg.add_tag(Tag::new(TagType::Important)));

        // Remove tag
        assert!(msg.remove_tag_type(&TagType::Important));
        assert!(!msg.has_tag(&TagType::Important));

        // Try to remove non-existent tag
        assert!(!msg.remove_tag_type(&TagType::Important));
    }

    #[test]
    fn test_custom_tag_workflow() {
        let mut registry = TagRegistry::new();

        let msg_id = "msg1".to_string();
        registry.add_tag(
            msg_id.clone(),
            Tag::new(TagType::Custom("review".to_string())),
        );
        registry.add_tag(
            msg_id.clone(),
            Tag::new(TagType::Custom("urgent".to_string())),
        );

        let tags = registry.get_tags(&msg_id).unwrap();
        assert_eq!(tags.len(), 2);

        let tag_types = registry.get_all_tag_types();
        assert!(tag_types.contains(&TagType::Custom("review".to_string())));
        assert!(tag_types.contains(&TagType::Custom("urgent".to_string())));
    }

    // ========================================================================
    // PERSISTENCE TESTS
    // ========================================================================

    #[test]
    fn test_registry_persistence_round_trip() {
        let mut registry1 = TagRegistry::new();
        registry1.add_tag("msg1".to_string(), Tag::new(TagType::Important));
        registry1.add_tag("msg2".to_string(), Tag::new(TagType::Idea));
        registry1.add_tag(
            "msg3".to_string(),
            Tag::with_note(TagType::Bug, "Crash on startup".to_string()),
        );

        // Serialize
        let json = serde_json::to_string(&registry1).unwrap();

        // Deserialize
        let registry2: TagRegistry = serde_json::from_str(&json).unwrap();

        // Verify
        assert_eq!(registry2.count_tagged_messages(), 3);
        assert!(registry2.has_tag("msg1", &TagType::Important));
        assert!(registry2.has_tag("msg2", &TagType::Idea));
        assert!(registry2.has_tag("msg3", &TagType::Bug));
    }

    #[test]
    fn test_message_persistence_with_tags() {
        let mut msg1 = Message::assistant("Solution".to_string());
        msg1.add_tag(Tag::new(TagType::Solution));
        msg1.add_tag(Tag::with_note(
            TagType::Important,
            "Key insight".to_string(),
        ));

        let json = serde_json::to_string(&msg1).unwrap();
        let msg2: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(msg2.content, msg1.content);
        assert_eq!(msg2.tags.len(), 2);
        assert!(msg2.has_tag(&TagType::Solution));
        assert!(msg2.has_tag(&TagType::Important));
    }

    // ========================================================================
    // EDGE CASE TESTS
    // ========================================================================

    #[test]
    fn test_tag_note_preservation() {
        let note = "This is a critical issue that needs attention".to_string();
        let tag = Tag::with_note(TagType::Bug, note.clone());

        assert_eq!(tag.note, Some(note.clone()));

        let json = serde_json::to_string(&tag).unwrap();
        let deserialized: Tag = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.note, Some(note));
    }

    #[test]
    fn test_tag_sorting() {
        let mut msg = Message::user("Test".to_string());

        // Add tags in random order
        msg.add_tag(Tag::new(TagType::Solution));
        msg.add_tag(Tag::new(TagType::Important));
        msg.add_tag(Tag::new(TagType::Idea));
        msg.add_tag(Tag::new(TagType::Bug));

        // Tags should be sorted
        let tags = msg.get_tags();
        assert!(tags[0].tag_type <= tags[1].tag_type);
        assert!(tags[1].tag_type <= tags[2].tag_type);
        assert!(tags[2].tag_type <= tags[3].tag_type);
    }

    #[test]
    fn test_clear_and_reuse_message() {
        let mut msg = Message::assistant("Test".to_string());

        // Add tags
        msg.add_tag(Tag::new(TagType::Important));
        msg.add_tag(Tag::new(TagType::Idea));
        assert_eq!(msg.get_tags().len(), 2);

        // Clear tags
        msg.clear_tags();
        assert_eq!(msg.get_tags().len(), 0);
        assert!(!msg.has_any_tags());

        // Add new tags
        msg.add_tag(Tag::new(TagType::Bug));
        assert_eq!(msg.get_tags().len(), 1);
        assert!(msg.has_tag(&TagType::Bug));
    }

    #[test]
    fn test_large_tag_count() {
        let mut registry = TagRegistry::new();

        // Add 100 messages with different tags
        for i in 0..100 {
            let msg_id = format!("msg{}", i);
            let tag_type = match i % 4 {
                0 => TagType::Important,
                1 => TagType::Idea,
                2 => TagType::Bug,
                _ => TagType::Solution,
            };
            registry.add_tag(msg_id, Tag::new(tag_type));
        }

        assert_eq!(registry.count_tagged_messages(), 100);
        assert_eq!(registry.count_with_tag(&TagType::Important), 25);
        assert_eq!(registry.count_with_tag(&TagType::Idea), 25);
        assert_eq!(registry.count_with_tag(&TagType::Bug), 25);
        assert_eq!(registry.count_with_tag(&TagType::Solution), 25);
    }
}
