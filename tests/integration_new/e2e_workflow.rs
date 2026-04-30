// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! End-to-end workflow integration tests
//!
//! Tests cover:
//! - Complete coding workflow with multi-part messages
//! - Debugging workflow with error analysis
//! - Tool calling integration using session MessagePart types
//! - Session persistence across workflows

use rustycode_providers::{CostTracker, ModelRegistry, ProviderMetadata};
use rustycode_session::{
    Message, MessagePart, MessageRole, SerializationFormat, Session, SessionSerializer,
    SessionStatus,
};

mod common;
use common::TestConfig;

/// Helper to build a multi-part message
fn build_message(role: MessageRole, parts: Vec<MessagePart>) -> Message {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    Message {
        id: format!("msg-{}", COUNTER.fetch_add(1, Ordering::Relaxed)),
        role,
        parts,
        timestamp: chrono::Utc::now(),
        metadata: Default::default(),
    }
}

#[tokio::test]
async fn test_complete_coding_workflow() {
    // Create session
    let mut session = Session::new("Coding Task");
    session.add_message(Message::user("Help me implement a binary search function"));

    // Agent responds with code
    session.add_message(build_message(
        MessageRole::Assistant,
        vec![
            MessagePart::Text {
                content: "I'll help you implement binary search in Rust:".to_string(),
            },
            MessagePart::Code {
                language: "rust".to_string(),
                code: r#"fn binary_search(arr: &[i32], target: i32) -> Option<usize> {
    let mut left = 0;
    let mut right = arr.len();
    while left < right {
        let mid = left + (right - left) / 2;
        match arr[mid].cmp(&target) {
            std::cmp::Ordering::Equal => return Some(mid),
            std::cmp::Ordering::Less => left = mid + 1,
            std::cmp::Ordering::Greater => right = mid,
        }
    }
    None
}"#
                .to_string(),
            },
            MessagePart::Text {
                content: "This implementation has O(log n) time complexity.".to_string(),
            },
        ],
    ));

    assert_eq!(session.message_count(), 2);
    assert!(session.estimate_tokens() > 0);
    assert_eq!(session.status, SessionStatus::Active);

    // Follow-up question
    session.add_message(Message::user("Can you add tests?"));
    assert_eq!(session.message_count(), 3);

    // Track costs
    let cost_tracker = CostTracker::new();
    cost_tracker
        .track("anthropic/claude-3-5-sonnet", 500, 300, 0.006)
        .await;
    let summary = cost_tracker.summary().await;
    assert!(summary.total_cost > 0.0);
}

#[tokio::test]
async fn test_debugging_workflow() {
    let mut session = Session::new("Debugging Task");

    // User reports error with code
    session.add_message(build_message(
        MessageRole::User,
        vec![
            MessagePart::Text {
                content: "I'm getting a panic in my code:".to_string(),
            },
            MessagePart::Code {
                language: "rust".to_string(),
                code: "thread 'main' panicked at 'index out of bounds'".to_string(),
            },
            MessagePart::Text {
                content: "Here's the code:".to_string(),
            },
            MessagePart::Code {
                language: "rust".to_string(),
                code: r#"fn main() {
    let arr = [1, 2, 3];
    println!("{}", arr[5]);
}"#
                .to_string(),
            },
        ],
    ));

    // Agent analyzes error
    session.add_message(build_message(
        MessageRole::Assistant,
        vec![
            MessagePart::Text {
                content: "The panic occurs because you're accessing an index out of bounds."
                    .to_string(),
            },
            MessagePart::Code {
                language: "rust".to_string(),
                code: r#"fn main() {
    let arr = [1, 2, 3];
    match arr.get(5) {
        Some(&value) => println!("{}", value),
        None => eprintln!("Index out of bounds!"),
    }
}"#
                .to_string(),
            },
            MessagePart::Text {
                content: "Use `get()` which returns an Option for safe access.".to_string(),
            },
        ],
    ));

    assert_eq!(session.message_count(), 2);

    let assistant_msg = &session.messages[1];
    assert!(assistant_msg.parts.len() > 2);

    let has_code = assistant_msg
        .parts
        .iter()
        .any(|p| matches!(p, MessagePart::Code { .. }));
    assert!(has_code, "Assistant should provide code examples");
}

#[tokio::test]
async fn test_tool_calling_integration() {
    let mut session = Session::new("Tool Usage");

    // User asks to read a file
    session.add_message(Message::user("Read the file README.md and summarize it"));

    // Agent calls Read tool
    session.add_message(build_message(
        MessageRole::Assistant,
        vec![
            MessagePart::Text {
                content: "I'll read the README.md file for you.".to_string(),
            },
            MessagePart::ToolCall {
                id: "call_1".to_string(),
                name: "Read".to_string(),
                input: serde_json::json!({ "file_path": "README.md" }),
            },
        ],
    ));

    // Tool response
    session.add_message(build_message(
        MessageRole::User,
        vec![MessagePart::ToolResult {
            tool_call_id: "call_1".to_string(),
            content: "# My Project\n\nThis is a sample project.".to_string(),
            is_error: false,
        }],
    ));

    // Agent summarizes
    session.add_message(Message::assistant(
        "The project is a sample project with basic features.",
    ));

    assert_eq!(session.message_count(), 4);

    // Verify tool use
    assert!(session.messages[1]
        .parts
        .iter()
        .any(|p| matches!(p, MessagePart::ToolCall { .. })));

    // Verify tool result
    assert!(session.messages[2]
        .parts
        .iter()
        .any(|p| matches!(p, MessagePart::ToolResult { .. })));
}

#[tokio::test]
async fn test_session_persistence_across_workflows() {
    let _test_config = TestConfig::new();

    // Workflow 1: Initial conversation
    let mut session1 = Session::new("Multi-Workflow Session");
    session1.add_message(Message::user("Help me understand Rust ownership"));
    session1.add_message(Message::assistant(
        "Ownership is Rust's key feature for memory safety...",
    ));

    // Serialize
    let data = SessionSerializer::serialize(&session1, SerializationFormat::Json).unwrap();

    // Workflow 2: Load and continue
    let mut session2 = SessionSerializer::deserialize(&data, SerializationFormat::Json).unwrap();
    assert_eq!(session2.message_count(), 2);

    // Continue conversation
    session2.add_message(Message::user("Can you give me an example?"));
    session2.add_message(build_message(
        MessageRole::Assistant,
        vec![
            MessagePart::Text {
                content: "Here's an example:".to_string(),
            },
            MessagePart::Code {
                language: "rust".to_string(),
                code: r#"fn main() {
    let s1 = String::from("hello");
    let s2 = s1; // s1 is moved to s2
    println!("{}", s2);
}"#
                .to_string(),
            },
        ],
    ));

    assert_eq!(session2.message_count(), 4);

    // Save again
    let data2 = SessionSerializer::serialize(&session2, SerializationFormat::Json).unwrap();

    // Workflow 3: Load and fork
    let session3 = SessionSerializer::deserialize(&data2, SerializationFormat::Json).unwrap();
    let mut session3_fork = session3.fork();

    assert_eq!(session3_fork.message_count(), 4);

    session3_fork.add_message(Message::user("What about borrowing?"));
    assert_eq!(session3_fork.message_count(), 5);

    // Original should be unchanged
    assert_eq!(session3.message_count(), 4);
}

#[tokio::test]
async fn test_error_recovery_workflow() {
    let mut session = Session::new("Error Recovery");

    // Initial request that fails
    session.add_message(Message::user("Connect to the database"));

    // Error response
    session.add_message(build_message(
        MessageRole::Assistant,
        vec![
            MessagePart::Text {
                content: "I attempted to connect but encountered an error:".to_string(),
            },
            MessagePart::Code {
                language: "text".to_string(),
                code: "Error: Connection refused (os error 61)".to_string(),
            },
            MessagePart::ToolCall {
                id: "call_1".to_string(),
                name: "Bash".to_string(),
                input: serde_json::json!({ "command": "pg_isready" }),
            },
        ],
    ));

    // Tool result - database not running
    session.add_message(build_message(
        MessageRole::User,
        vec![MessagePart::ToolResult {
            tool_call_id: "call_1".to_string(),
            content: "pg_isready: server not running".to_string(),
            is_error: true,
        }],
    ));

    // Recovery
    session.add_message(build_message(
        MessageRole::Assistant,
        vec![
            MessagePart::Text {
                content: "The database is not running. Let me start it:".to_string(),
            },
            MessagePart::ToolCall {
                id: "call_2".to_string(),
                name: "Bash".to_string(),
                input: serde_json::json!({ "command": "brew services start postgresql" }),
            },
        ],
    ));

    assert_eq!(session.message_count(), 4);

    // Verify error and recovery messages
    let has_error = session.messages.iter().any(|m| {
        m.parts.iter().any(|p| {
            matches!(p, MessagePart::Text { content } if content.to_lowercase().contains("error"))
        })
    });
    assert!(has_error);

    let has_recovery = session.messages.iter().any(|m| {
        m.parts
            .iter()
            .any(|p| matches!(p, MessagePart::Text { content } if content.contains("start")))
    });
    assert!(has_recovery);
}

#[tokio::test]
async fn test_multi_agent_workflow() {
    let mut session = Session::new("Multi-Agent Task");

    // User request
    session.add_message(Message::user("I need to add authentication to my API"));

    // Agent 1: Planner
    session.add_message(Message::assistant(
        "I'll create a plan:\n1. Design Phase\n2. Implementation Phase\n3. Testing Phase",
    ));

    // Agent 2: Developer
    session.add_message(build_message(
        MessageRole::Assistant,
        vec![
            MessagePart::Text {
                content: "I'll implement the JWT authentication system:".to_string(),
            },
            MessagePart::Code {
                language: "rust".to_string(),
                code: "fn generate_token(user_id: &str) -> Result<String, Error> { /* ... */ }"
                    .to_string(),
            },
        ],
    ));

    // Agent 3: Reviewer
    session.add_message(Message::assistant(
        "Security review:\n1. Secret key is hardcoded - use env var\n2. No token refresh mechanism",
    ));

    assert_eq!(session.message_count(), 4);

    // Verify different content types
    let has_code = session.messages.iter().any(|m| {
        m.parts
            .iter()
            .any(|p| matches!(p, MessagePart::Code { .. }))
    });
    assert!(has_code);
}

#[tokio::test]
async fn test_provider_and_session_integration() {
    // Test that provider registry and session work together
    let registry = ModelRegistry::new();
    registry
        .register_provider(ProviderMetadata {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            auth_method: rustycode_providers::AuthMethod::ApiKey,
            capabilities: rustycode_providers::ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 8192,
                max_context_window: 200_000,
            },
            pricing: rustycode_providers::PricingInfo {
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
                currency: rustycode_providers::Currency::Usd,
            },
        })
        .await;

    assert_eq!(registry.count().await, 1);
    assert!(registry.has_provider("anthropic").await);

    // Create a session tied to a specific model
    let mut session = Session::new("Provider Integration");
    session.metadata.model_used = Some("claude-3-5-sonnet".to_string());

    session.add_message(Message::user("Hello"));

    assert_eq!(
        session.metadata.model_used.as_deref(),
        Some("claude-3-5-sonnet")
    );
}

#[tokio::test]
async fn test_error_recovery_tool_failure() {
    let mut session = Session::new("Tool Error Recovery");

    // User requests a file read
    session.add_message(Message::user("Read the contents of config.json"));

    // Agent attempts to call Read tool but it fails
    session.add_message(build_message(
        MessageRole::Assistant,
        vec![
            MessagePart::Text {
                content: "I'll read the config.json file for you.".to_string(),
            },
            MessagePart::ToolCall {
                id: "call_1".to_string(),
                name: "Read".to_string(),
                input: serde_json::json!({ "file_path": "config.json" }),
            },
        ],
    ));

    // Tool result - file not found
    session.add_message(build_message(
        MessageRole::User,
        vec![MessagePart::ToolResult {
            tool_call_id: "call_1".to_string(),
            content: "File not found: config.json".to_string(),
            is_error: true,
        }],
    ));

    // Agent recovers by suggesting alternative
    session.add_message(build_message(
        MessageRole::Assistant,
        vec![
            MessagePart::Text {
                content:
                    "The config.json file doesn't exist. Let me check what files are available:"
                        .to_string(),
            },
            MessagePart::ToolCall {
                id: "call_2".to_string(),
                name: "Bash".to_string(),
                input: serde_json::json!({ "command": "ls -la" }),
            },
        ],
    ));

    assert_eq!(session.message_count(), 4);

    // Verify error was recorded
    let tool_result = &session.messages[2];
    assert!(tool_result
        .parts
        .iter()
        .any(|p| { matches!(p, MessagePart::ToolResult { is_error: true, .. }) }));

    // Verify recovery attempt
    let recovery_msg = &session.messages[3];
    assert!(recovery_msg
        .parts
        .iter()
        .any(|p| { matches!(p, MessagePart::ToolCall { name, .. } if name == "Bash") }));
}

#[tokio::test]
async fn test_empty_session_operations() {
    let session = Session::new("Empty Session");

    // Test operations on empty session
    assert_eq!(session.message_count(), 0);
    assert_eq!(session.estimate_tokens(), 0);

    // Should not panic
    let last_msg = session.last_message();
    assert!(last_msg.is_none());

    // Session should remain valid
    assert_eq!(session.status, SessionStatus::Active);
}
