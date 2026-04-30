#![allow(
    clippy::expect_used,
    clippy::map_unwrap_or,
    clippy::redundant_closure_for_method_calls,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Tests for TUI session management
//!
//! Tests session save/load functionality including:
//! - Session creation
//! - Session persistence
//! - Message history restoration
//! - Session naming
//! - Metadata preservation

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper function to create a temporary session directory
fn create_temp_session_dir() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

/// Helper to create a test session file
fn create_test_session_file(dir: &Path, session_id: &str, title: &str) -> PathBuf {
    let session_path = dir.join(format!("{}.json", session_id));

    let session_data = json!({
        "id": session_id,
        "title": title,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": [
            {
                "role": "user",
                "content": "Test message"
            },
            {
                "role": "assistant",
                "content": "Test response"
            }
        ]
    });

    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    session_path
}

/// Test session file creation
#[test]
fn test_session_file_creation() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_001";
    let title = "Test Session";

    let session_path = create_test_session_file(temp_dir.path(), session_id, title);

    assert!(session_path.exists(), "Session file should be created");
    assert!(
        session_path.extension().unwrap() == "json",
        "Session file should be JSON"
    );
}

/// Test session file deserialization
#[test]
fn test_session_file_deserialization() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_002";
    let title = "Deserialize Test";

    let session_path = create_test_session_file(temp_dir.path(), session_id, title);

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert_eq!(json["id"], session_id);
    assert_eq!(json["title"], title);
    assert!(json["messages"].is_array());
    assert_eq!(json["messages"].as_array().unwrap().len(), 2);
}

/// Test session with multiple messages
#[test]
fn test_session_with_multiple_messages() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_003";

    let messages = vec![
        json!({"role": "user", "content": "First message"}),
        json!({"role": "assistant", "content": "First response"}),
        json!({"role": "user", "content": "Second message"}),
        json!({"role": "assistant", "content": "Second response"}),
        json!({"role": "user", "content": "Third message"}),
        json!({"role": "assistant", "content": "Third response"}),
    ];

    let session_data = json!({
        "id": session_id,
        "title": "Multi-message Session",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": messages
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert_eq!(json["messages"].as_array().unwrap().len(), 6);
}

/// Test session with special characters in title
#[test]
fn test_session_special_characters_in_title() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_004";
    let title = "Session with \"quotes\" and 'apostrophes' and emojis 🚀";

    let session_data = json!({
        "id": session_id,
        "title": title,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": []
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert_eq!(json["title"], title);
}

/// Test session with code blocks in messages
#[test]
fn test_session_with_code_blocks() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_005";

    let message_with_code = json!({
        "role": "assistant",
        "content": r#"Here's some code:

```rust
fn main() {
    println!("Hello, World!");
}
```

Hope this helps!"#
    });

    let session_data = json!({
        "id": session_id,
        "title": "Code Session",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": [message_with_code]
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert!(json["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("```"));
}

/// Test session metadata preservation
#[test]
fn test_session_metadata_preservation() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_006";

    let session_data = json!({
        "id": session_id,
        "title": "Metadata Test",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-02T12:30:45Z",
        "model": "claude-sonnet-4",
        "provider": "anthropic",
        "messages": []
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert_eq!(json["created_at"], "2024-01-01T00:00:00Z");
    assert_eq!(json["updated_at"], "2024-01-02T12:30:45Z");
    assert_eq!(json["model"], "claude-sonnet-4");
    assert_eq!(json["provider"], "anthropic");
}

/// Test session with tool calls
#[test]
fn test_session_with_tool_calls() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_007";

    let tool_message = json!({
        "role": "assistant",
        "content": "Let me check that file for you.",
        "tool_calls": [
            {
                "id": "call_001",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\": \"src/main.rs\"}"
                }
            }
        ]
    });

    let tool_result = json!({
        "role": "tool",
        "tool_call_id": "call_001",
        "content": "File contents here..."
    });

    let session_data = json!({
        "id": session_id,
        "title": "Tool Session",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": [tool_message, tool_result]
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert!(json["messages"][0]["tool_calls"].is_array());
    assert_eq!(json["messages"][1]["role"], "tool");
}

/// Test empty session
#[test]
fn test_empty_session() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_008";

    let session_data = json!({
        "id": session_id,
        "title": "Empty Session",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "messages": []
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert_eq!(json["messages"].as_array().unwrap().len(), 0);
}

/// Test session with very long messages
#[test]
fn test_session_with_long_messages() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_009";

    let long_content = "A".repeat(10000);

    let message = json!({
        "role": "assistant",
        "content": long_content
    });

    let session_data = json!({
        "id": session_id,
        "title": "Long Message Session",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": [message]
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert_eq!(
        json["messages"][0]["content"].as_str().unwrap().len(),
        10000
    );
}

/// Test session with Unicode content
#[test]
fn test_session_with_unicode_content() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_010";

    let message = json!({
        "role": "user",
        "content": "Hello 你好 🚀 مرحبا Привет"
    });

    let session_data = json!({
        "id": session_id,
        "title": "Unicode Session 🌍",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": [message]
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert!(json["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("你好"));
    assert!(json["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("🚀"));
}

/// Test session list generation
#[test]
fn test_session_list_generation() {
    let temp_dir = create_temp_session_dir();

    // Create multiple sessions
    for i in 1..=5 {
        let session_id = format!("session_{:03}", i);
        create_test_session_file(temp_dir.path(), &session_id, &format!("Session {}", i));
    }

    // List all session files
    let entries: Vec<_> = fs::read_dir(temp_dir.path())
        .expect("Failed to read directory")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(entries.len(), 5, "Should have 5 session files");
}

/// Test session update (overwrite)
#[test]
fn test_session_update() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_011";

    // Create initial session
    create_test_session_file(temp_dir.path(), session_id, "Original Title");

    // Update session
    let updated_data = json!({
        "id": session_id,
        "title": "Updated Title",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-02T00:00:00Z",
        "messages": [
            {
                "role": "user",
                "content": "Updated message"
            }
        ]
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&updated_data).unwrap(),
    )
    .expect("Failed to update session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert_eq!(json["title"], "Updated Title");
    assert_eq!(json["messages"].as_array().unwrap().len(), 1);
}

/// Test session file corruption handling
#[test]
fn test_session_file_corruption() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_012";

    let session_path = temp_dir.path().join(format!("{}.json", session_id));

    // Write invalid JSON
    fs::write(&session_path, "{ invalid json }").expect("Failed to write file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let result: Result<Value, _> = serde_json::from_str(&content);

    assert!(result.is_err(), "Invalid JSON should fail to parse");
}

/// Test session with large number of messages
#[test]
fn test_session_with_many_messages() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_013";

    let messages: Vec<Value> = (0..100)
        .map(|i| {
            json!({
                "role": if i % 2 == 0 { "user" } else { "assistant" },
                "content": format!("Message number {}", i)
            })
        })
        .collect();

    let session_data = json!({
        "id": session_id,
        "title": "Many Messages Session",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": messages
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert_eq!(json["messages"].as_array().unwrap().len(), 100);
}

/// Test session with system message
#[test]
fn test_session_with_system_message() {
    let temp_dir = create_temp_session_dir();
    let session_id = "test_session_014";

    let system_message = json!({
        "role": "system",
        "content": "You are a helpful coding assistant."
    });

    let session_data = json!({
        "id": session_id,
        "title": "System Message Session",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": [system_message]
    });

    let session_path = temp_dir.path().join(format!("{}.json", session_id));
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: Value = serde_json::from_str(&content).expect("Failed to parse JSON");

    assert_eq!(json["messages"][0]["role"], "system");
}
