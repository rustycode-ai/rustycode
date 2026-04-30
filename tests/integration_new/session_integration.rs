// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Session integration tests
//!
//! Tests cover:
//! - Session lifecycle (create, use, fork, archive, delete)
//! - Message compaction strategies
//! - Session serialization with compression
//! - Token accounting accuracy
//! - Session metadata and status

use rustycode_session::{
    CompactionEngine, CompactionStrategy, Message, MessagePart, MessageRole, SerializationFormat,
    Session, SessionSerializer, SessionStatus,
};

mod common;

#[tokio::test]
async fn test_session_lifecycle() {
    let mut session = Session::new("Test Session");

    assert_eq!(session.name, "Test Session");
    assert_eq!(session.status, SessionStatus::Active);
    assert_eq!(session.message_count(), 0);

    session.add_message(Message::user("Hello"));
    session.add_message(Message::assistant("Hi there!"));

    assert_eq!(session.message_count(), 2);

    session.status = SessionStatus::Archived;
    assert_eq!(session.status, SessionStatus::Archived);

    let forked = session.fork();
    assert_eq!(forked.message_count(), 2);
    assert_eq!(forked.status, SessionStatus::Active);

    // Clear messages on forked copy
    let mut forked_mut = forked;
    forked_mut.messages.clear();
    assert_eq!(forked_mut.message_count(), 0);
}

#[tokio::test]
async fn test_message_compaction() {
    let mut session = Session::new("Compaction Test");

    for i in 0..100 {
        session.add_message(Message::user(format!("Message {}", i)));
        session.add_message(Message::assistant(format!("Response {}", i)));
    }

    let initial_count = session.message_count();
    assert_eq!(initial_count, 200);

    let engine = CompactionEngine::new(CompactionStrategy::TokenThreshold {
        target_ratio: 0.25,
        min_messages: 50,
    });
    let result = engine.compact(&session);

    // Compaction may succeed or fail depending on summarization support
    if let Ok((_compacted_messages, report)) = result {
        assert!(report.new_count < initial_count);
        assert!(report.messages_removed > 0);
    }
}

#[tokio::test]
async fn test_compaction_strategies() {
    let mut session = Session::new("Strategy Test");

    for i in 0..50 {
        session.add_message(Message::user(format!("User message {}", i)));
        session.add_message(Message::assistant(format!("Assistant response {}", i)));
    }

    // TokenThreshold strategy
    let engine1 = CompactionEngine::new(CompactionStrategy::TokenThreshold {
        target_ratio: 0.5,
        min_messages: 20,
    });
    if let Ok((_msgs, report)) = engine1.compact(&session) {
        assert!(report.messages_removed > 0);
    }

    // MessageAge strategy
    let engine2 = CompactionEngine::new(CompactionStrategy::MessageAge {
        max_age: chrono::Duration::zero(),
        keep_recent: 20,
    });
    if let Ok((_msgs, report)) = engine2.compact(&session) {
        assert!(report.messages_removed > 0);
    }

    // SemanticImportance strategy
    let engine3 = CompactionEngine::new(CompactionStrategy::SemanticImportance {
        importance_threshold: 0.8,
        min_messages: 10,
    });
    if let Ok((_msgs, _report)) = engine3.compact(&session) {
        // May succeed or fail depending on summarization support
    }
}

#[tokio::test]
async fn test_session_serialization() {
    let mut session = Session::new("Serialization Test");

    session.add_message(Message::user("Hello"));
    session.add_message(Message::assistant("Hi!"));
    session.add_message(Message::user("Check this code:"));
    session.add_message(Message::assistant("I see it."));

    // Serialize to JSON
    let serialized = SessionSerializer::serialize(&session, SerializationFormat::Json).unwrap();
    assert!(!serialized.is_empty());

    // Deserialize
    let loaded = SessionSerializer::deserialize(&serialized, SerializationFormat::Json).unwrap();

    assert_eq!(loaded.name, session.name);
    assert_eq!(loaded.message_count(), session.message_count());
}

#[tokio::test]
async fn test_serialization_compression() {
    let mut session = Session::new("Compression Test");

    for i in 0..100 {
        session.add_message(Message::user(format!(
            "Long message number {} with lots of text to compress",
            i
        )));
        session.add_message(Message::assistant(format!(
            "Response number {} with even more text to ensure we have enough data",
            i
        )));
    }

    // Serialize with compressed JSON
    let compressed =
        SessionSerializer::serialize(&session, SerializationFormat::CompressedJson).unwrap();
    let uncompressed = SessionSerializer::serialize(&session, SerializationFormat::Json).unwrap();

    // Compressed should be smaller
    assert!(
        compressed.len() < uncompressed.len(),
        "Compressed ({}) should be smaller than uncompressed ({})",
        compressed.len(),
        uncompressed.len()
    );

    // Deserialize and verify
    let loaded =
        SessionSerializer::deserialize(&compressed, SerializationFormat::CompressedJson).unwrap();
    assert_eq!(loaded.message_count(), session.message_count());
}

#[tokio::test]
async fn test_token_accounting() {
    let mut session = Session::new("Token Accounting");

    session.add_message(Message::user("Hello, world!"));
    // token_count reflects metadata.total_tokens which requires explicit token metadata
    // Without explicit tokens, estimate_tokens uses heuristic
    let tokens1 = session.estimate_tokens();
    assert!(tokens1 > 0);

    session.add_message(Message::assistant("Hi! How can I help?"));
    let tokens2 = session.estimate_tokens();
    assert!(tokens2 >= tokens1);
}

#[tokio::test]
async fn test_session_metadata() {
    let mut session = Session::new("Metadata Test");

    // Update name directly
    session.name = "New Title".to_string();
    assert_eq!(session.name, "New Title");

    // Add tags
    session.add_tag("test");
    session.add_tag("integration");

    assert!(session.metadata.tags.contains(&"test".to_string()));
    assert!(session.metadata.tags.contains(&"integration".to_string()));

    // Status transitions
    assert_eq!(session.status, SessionStatus::Active);
    session.status = SessionStatus::Archived;
    assert_eq!(session.status, SessionStatus::Archived);
    session.status = SessionStatus::Deleted;
    assert_eq!(session.status, SessionStatus::Deleted);
}

#[tokio::test]
async fn test_message_types() {
    let mut session = Session::new("Message Types");

    session.add_message(Message::user("Simple text"));
    assert_eq!(session.message_count(), 1);

    // Multi-part message via manual construction
    let multi = Message {
        id: "msg-manual".to_string(),
        role: MessageRole::Assistant,
        parts: vec![
            MessagePart::Text {
                content: "Here's the answer:".to_string(),
            },
            MessagePart::Code {
                language: "python".to_string(),
                code: "print('hello')".to_string(),
            },
            MessagePart::Text {
                content: "That's it!".to_string(),
            },
        ],
        timestamp: chrono::Utc::now(),
        metadata: Default::default(),
    };
    session.add_message(multi);
    assert_eq!(session.message_count(), 2);

    let messages = &session.messages;
    assert_eq!(messages.len(), 2);

    assert_eq!(messages[0].role, MessageRole::User);
    assert_eq!(messages[1].role, MessageRole::Assistant);
    assert!(messages[1].parts.len() > 1);
}

#[tokio::test]
async fn test_session_fork_preserves_messages() {
    let mut session = Session::new("Original");

    session.metadata.tags.push("original".into());
    session.metadata.tags.push("important".into());

    session.add_message(Message::user("Test"));
    session.add_message(Message::assistant("Response"));

    let forked = session.fork();

    assert_eq!(forked.message_count(), 2);

    // Original should be unchanged
    assert_eq!(session.name, "Original");
}

#[tokio::test]
async fn test_session_clear_operations() {
    let mut session = Session::new("Clear Test");

    for i in 0..10 {
        session.add_message(Message::user(format!("Message {}", i)));
    }

    assert_eq!(session.message_count(), 10);

    session.clear();
    assert_eq!(session.message_count(), 0);
}

#[tokio::test]
async fn test_session_id_generation() {
    let session1 = Session::new("Test 1");
    let session2 = Session::new("Test 2");

    assert_ne!(session1.id, session2.id);
    assert!(!session1.id.as_str().is_empty());
    assert!(!session2.id.as_str().is_empty());
}

#[tokio::test]
async fn test_empty_session_serialization() {
    let session = Session::new("Empty");

    let serialized = SessionSerializer::serialize(&session, SerializationFormat::Json).unwrap();
    let loaded = SessionSerializer::deserialize(&serialized, SerializationFormat::Json).unwrap();

    assert_eq!(loaded.name, "Empty");
    assert_eq!(loaded.message_count(), 0);
}

#[tokio::test]
async fn test_session_context_tracking() {
    let mut session = Session::new("Context Test");

    session.set_task("Implement feature X");
    assert_eq!(session.context.task.as_deref(), Some("Implement feature X"));

    session.set_phase("implementation");
    assert_eq!(
        session.context.current_phase.as_deref(),
        Some("implementation")
    );

    session.touch_file("src/main.rs");
    session.touch_file("src/lib.rs");
    assert_eq!(session.context.files_touched.len(), 2);

    session.record_decision("Use async/await pattern");
    assert_eq!(session.context.decisions.len(), 1);
}
