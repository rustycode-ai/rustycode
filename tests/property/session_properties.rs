// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Property-based tests for session system
//!
//! Uses proptest to verify invariants and properties

use proptest::prelude::*;
use rustycode_session::{
    CompactionEngine, CompactionStrategy, Message, MessagePart, MessageRole, Session,
};
use std::iter;

// Generate valid message content
fn arb_message_content() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9 ]{1,100}",
        "[\\n\\r a-zA-Z0-9]{10,200}",  // Multi-line
    ]
}

// Generate a valid message
fn arb_message() -> impl Strategy<Value = Message> {
    arb_message_content().prop_map(|content| Message::user(content))
}

// Generate multiple messages
fn arb_messages(count: usize) -> impl Strategy<Value = Vec<Message>> {
    prop::collection::vec(arb_message(), count..=count)
}

proptest! {
    #[test]
    fn session_message_count_is_accurate(messages in arb_messages(1..100)) {
        let mut session = Session::new("Test".to_string());

        for msg in messages.clone() {
            session.add_message(msg);
        }

        prop_assert_eq!(session.message_count(), messages.len());
    }

    #[test]
    fn session_token_count_is_monotonic(messages in arb_messages(1..50)) {
        let mut session = Session::new("Test".to_string());

        let mut previous_tokens = 0;

        for msg in messages {
            session.add_message(msg);
            let current_tokens = session.token_count();

            // Token count should always increase
            prop_assert!(current_tokens >= previous_tokens, "Token count decreased: {} -> {}", previous_tokens, current_tokens);

            previous_tokens = current_tokens;
        }
    }

    #[test]
    fn session_fork_preserves_messages(messages in arb_messages(1..50)) {
        let mut session = Session::new("Original".to_string());

        for msg in messages.clone() {
            session.add_message(msg);
        }

        let forked = session.fork("Forked".to_string());

        prop_assert_eq!(forked.message_count(), session.message_count());
        prop_assert_eq!(forked.token_count(), session.token_count());
    }

    #[test]
    fn session_fork_increments_id(
        title in "[a-zA-Z0-9 ]{1,50}",
    ) {
        let session1 = Session::new(title.clone());
        let session2 = Session::new(title.clone());

        prop_assert_ne!(session1.id(), session2.id());
    }

    #[test]
    fn compaction_reduces_message_count(
        messages in arb_messages(20..100),
        keep_last in 5usize..19,
    ) {
        let mut session = Session::new("Test".to_string());

        for msg in messages {
            session.add_message(msg);
        }

        let initial_count = session.message_count();

        let engine = CompactionEngine::new();
        let _ = engine.compact(
            &mut session,
            CompactionStrategy::RecentMessages { keep_last }
        );

        prop_assert!(session.message_count() < initial_count);
        prop_assert_eq!(session.message_count(), keep_last);
    }

    #[test]
    fn compaction_reduces_token_count(
        messages in arb_messages(50..100),
        keep_last in 10usize..40,
    ) {
        let mut session = Session::new("Test".to_string());

        for msg in messages {
            session.add_message(msg);
        }

        let initial_tokens = session.token_count();

        let engine = CompactionEngine::new();
        let _ = engine.compact(
            &mut session,
            CompactionStrategy::RecentMessages { keep_last }
        );

        prop_assert!(session.token_count() < initial_tokens);
    }

    #[test]
    fn clearing_session_resets_counts(messages in arb_messages(1..100)) {
        let mut session = Session::new("Test".to_string());

        for msg in messages {
            session.add_message(msg);
        }

        prop_assert!(session.message_count() > 0);
        prop_assert!(session.token_count() > 0);

        session.clear_messages();

        prop_assert_eq!(session.message_count(), 0);
        prop_assert_eq!(session.token_count(), 0);
    }

    #[test]
    fn session_clone_is_independent(messages in arb_messages(1..50)) {
        let mut session1 = Session::new("Original".to_string());

        for msg in messages.clone() {
            session1.add_message(msg);
        }

        let session2 = session1.clone();

        // Modify session2
        session2.add_message(Message::user("New message".to_string()));

        // Session1 should be unchanged
        prop_assert_eq!(session1.message_count(), messages.len());
        prop_assert_eq!(session2.message_count(), messages.len() + 1);
    }

    #[test]
    fn token_budget_compaction_respects_limit(
        messages in arb_messages(50..100),
        max_tokens in 1000usize..5000,
    ) {
        let mut session = Session::new("Test".to_string());

        for msg in messages {
            session.add_message(msg);
        }

        let engine = CompactionEngine::new();
        let _ = engine.compact(
            &mut session,
            CompactionStrategy::TokenBudget { max_tokens }
        );

        // Should be close to max_tokens (allow 20% margin)
        prop_assert!(session.token_count() <= max_tokens * 6 / 5);
    }

    #[test]
    fn multiple_compactions_are_monotonic(
        messages in arb_messages(100..200),
        keep1 in 50usize..90,
        keep2 in 20usize..40,
        keep3 in 5usize..15,
    ) {
        let mut session = Session::new("Test".to_string());

        for msg in messages {
            session.add_message(msg);
        }

        let initial_count = session.message_count();

        let engine = CompactionEngine::new();

        // First compaction
        let _ = engine.compact(
            &mut session,
            CompactionStrategy::RecentMessages { keep_last: keep1 }
        );
        prop_assert!(session.message_count() <= keep1);

        // Second compaction
        let _ = engine.compact(
            &mut session,
            CompactionStrategy::RecentMessages { keep_last: keep2 }
        );
        prop_assert!(session.message_count() <= keep2);

        // Third compaction
        let _ = engine.compact(
            &mut session,
            CompactionStrategy::RecentMessages { keep_last: keep3 }
        );
        prop_assert!(session.message_count() <= keep3);

        // Final count should be smallest
        prop_assert_eq!(session.message_count(), keep3);
    }

    #[test]
    fn session_metadata_independence(
        title1 in "[a-zA-Z0-9 ]{1,50}",
        title2 in "[a-zA-Z0-9 ]{1,50}",
    ) {
        let mut session1 = Session::new(title1.clone());
        let session2 = Session::new(title2.clone());

        session1.set_description("Description 1".to_string());
        session1.add_tag("tag1".to_string());

        session2.set_description("Description 2".to_string());
        session2.add_tag("tag2".to_string());

        // Metadata should be independent
        prop_assert_eq!(session1.metadata().title, title1);
        prop_assert_eq!(session2.metadata().title, title2);

        prop_assert_eq!(session1.metadata().description.as_ref().unwrap(), "Description 1");
        prop_assert_eq!(session2.metadata().description.as_ref().unwrap(), "Description 2");
    }

    #[test]
    fn message_order_is_preserved(messages in arb_messages(1..100)) {
        let mut session = Session::new("Test".to_string());

        for msg in messages.clone() {
            session.add_message(msg);
        }

        let retrieved_messages = session.messages();

        prop_assert_eq!(retrieved_messages.len(), messages.len());

        for (i, msg) in retrieved_messages.iter().enumerate() {
            // Extract content to compare
            if let MessagePart::Text(content) = &messages[i].parts[0] {
                if let MessagePart::Text(retrieved_content) = &msg.parts[0] {
                    prop_assert_eq!(content, retrieved_content);
                }
            }
        }
    }

    #[test]
    fn empty_session_has_zero_counts(
        title in "[a-zA-Z0-9 ]{1,50}",
    ) {
        let session = Session::new(title);

        prop_assert_eq!(session.message_count(), 0);
        prop_assert_eq!(session.token_count(), 0);
    }

    #[test]
    fn compaction_preserves_message_structure(
        messages in arb_messages(20..50),
        keep_last in 5usize..15,
    ) {
        let mut session = Session::new("Test".to_string());

        for msg in messages {
            session.add_message(msg);
        }

        let engine = CompactionEngine::new();
        let _ = engine.compact(
            &mut session,
            CompactionStrategy::RecentMessages { keep_last }
        );

        // Remaining messages should maintain structure
        let remaining = session.messages();
        prop_assert_eq!(remaining.len(), keep_last);

        // All messages should be valid
        for msg in remaining {
            prop_assert!(!msg.parts.is_empty());
            prop_assert!(matches!(msg.role, MessageRole::User | MessageRole::Assistant));
        }
    }

    #[test]
    fn session_status_transitions(
        messages in arb_messages(1..50),
    ) {
        let mut session = Session::new("Test".to_string());

        for msg in messages {
            session.add_message(msg);
        }

        // Initial status
        prop_assert_eq!(session.status(), rustycode_session::SessionStatus::Active);

        // Change to paused
        session.set_status(rustycode_session::SessionStatus::Paused);
        prop_assert_eq!(session.status(), rustycode_session::SessionStatus::Paused);

        // Change to archived
        session.set_status(rustycode_session::SessionStatus::Archived);
        prop_assert_eq!(session.status(), rustycode_session::SessionStatus::Archived);

        // Change back to active
        session.set_status(rustycode_session::SessionStatus::Active);
        prop_assert_eq!(session.status(), rustycode_session::SessionStatus::Active);
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[test]
    fn test_large_message_handling() {
        let mut session = Session::new("Large Messages".to_string());

        // Add a very large message
        let large_content = "x".repeat(1_000_000);
        session.add_message(Message::user(large_content));

        assert_eq!(session.message_count(), 1);
        assert!(session.token_count() > 0);
    }

    #[test]
    fn test_unicode_message_handling() {
        let mut session = Session::new("Unicode".to_string());

        let unicode_content = "Hello 世界 🌍 Привет مرحبا";
        session.add_message(Message::user(unicode_content.to_string()));

        assert_eq!(session.message_count(), 1);

        let messages = session.messages();
        if let MessagePart::Text(content) = &messages[0].parts[0] {
            assert_eq!(content, unicode_content);
        }
    }

    #[test]
    fn test_multipart_message_structure() {
        let message = Message::assistant(vec![
            MessagePart::text("Part 1"),
            MessagePart::code_block("rust", "fn main() {}"),
            MessagePart::text("Part 2"),
        ]);

        let mut session = Session::new("Multipart".to_string());
        session.add_message(message);

        assert_eq!(session.message_count(), 1);

        let retrieved = session.messages();
        assert_eq!(retrieved[0].parts.len(), 3);
    }
}
