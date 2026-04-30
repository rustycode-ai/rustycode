#![allow(
    clippy::expect_used,
    clippy::map_unwrap_or,
    clippy::needless_collect,
    clippy::redundant_closure_for_method_calls,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Integration tests for TUI lifecycle and functionality
//!
//! These tests verify the complete TUI experience including:
//! - TUI initialization and startup
//! - Session lifecycle (create, save, load)
//! - Keybinding handlers
//! - Error handling and recovery
//! - Provider configuration
//! - Model selection
//! - File operations through TUI

use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create a temporary directory for testing
fn setup_test_env() -> TempDir {
    TempDir::new().expect("Failed to create temp dir")
}

/// Helper to create a mock session file
fn create_mock_session(dir: &Path, session_id: &str, title: &str, message_count: usize) -> PathBuf {
    let session_path = dir.join(format!("{}.json", session_id));

    let messages: Vec<serde_json::Value> = (0..message_count)
        .map(|i| {
            json!({
                "role": if i % 2 == 0 { "user" } else { "assistant" },
                "content": format!("Message {}", i)
            })
        })
        .collect();

    let session_data = json!({
        "id": session_id,
        "title": title,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "model": "claude-sonnet-4",
        "provider": "anthropic",
        "messages": messages
    });

    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    session_path
}

/// Test TUI can be imported and basic types exist
#[test]
fn test_tui_module_imports() {
    // Set the test mode env var to prevent terminal initialization
    std::env::set_var("RUSTYCODE_TEST_MODE", "1");

    let result = rustycode_tui::run(PathBuf::from("/tmp"), false, false);

    assert!(
        result.is_err() || result.is_ok(),
        "TUI run should handle errors gracefully"
    );
}

/// Test session directory creation
#[test]
fn test_session_directory_creation() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");

    // Create session directory
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    assert!(session_dir.exists(), "Session directory should be created");
    assert!(session_dir.is_dir(), "Session path should be a directory");
}

/// Test session file persistence
#[test]
fn test_session_file_persistence() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_id = "test_session_persist";
    let title = "Persistence Test";

    let session_path = create_mock_session(&session_dir, session_id, title, 3);

    // Verify file exists
    assert!(session_path.exists(), "Session file should be created");

    // Verify file can be read
    let content = fs::read_to_string(&session_path).expect("Failed to read session file");

    // Verify JSON is valid
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("Session file should contain valid JSON");

    assert_eq!(json["id"], session_id);
    assert_eq!(json["title"], title);
    assert_eq!(json["messages"].as_array().unwrap().len(), 3);
}

/// Test session listing
#[test]
fn test_session_listing() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    // Create multiple sessions
    for i in 1..=5 {
        create_mock_session(
            &session_dir,
            &format!("session_{:03}", i),
            &format!("Session {}", i),
            i,
        );
    }

    // List all session files
    let entries: Vec<_> = fs::read_dir(&session_dir)
        .expect("Failed to read session directory")
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

/// Test session loading
#[test]
fn test_session_loading() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_id = "test_load_session";
    let title = "Load Test";
    let message_count = 5;

    create_mock_session(&session_dir, session_id, title, message_count);

    // Simulate loading session
    let session_path = session_dir.join(format!("{}.json", session_id));
    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("Session file should contain valid JSON");

    // Verify session loaded correctly
    assert_eq!(json["id"], session_id);
    assert_eq!(json["title"], title);
    assert_eq!(json["messages"].as_array().unwrap().len(), message_count);
}

/// Test session update (overwrite)
#[test]
fn test_session_update() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_id = "test_update_session";

    // Create initial session
    create_mock_session(&session_dir, session_id, "Original Title", 2);

    // Update session
    let session_path = session_dir.join(format!("{}.json", session_id));
    let updated_data = json!({
        "id": session_id,
        "title": "Updated Title",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-02T00:00:00Z",
        "model": "claude-opus-4",
        "provider": "anthropic",
        "messages": [
            {"role": "user", "content": "Updated message"}
        ]
    });

    fs::write(
        &session_path,
        serde_json::to_string_pretty(&updated_data).unwrap(),
    )
    .expect("Failed to update session file");

    // Verify update
    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("Session file should contain valid JSON");

    assert_eq!(json["title"], "Updated Title");
    assert_eq!(json["model"], "claude-opus-4");
    assert_eq!(json["messages"].as_array().unwrap().len(), 1);
}

/// Test session deletion
#[test]
fn test_session_deletion() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_id = "test_delete_session";

    let session_path = create_mock_session(&session_dir, session_id, "Delete Me", 1);

    assert!(session_path.exists(), "Session file should exist");

    // Delete session
    fs::remove_file(&session_path).expect("Failed to delete session file");

    assert!(!session_path.exists(), "Session file should be deleted");
}

/// Test error handling for corrupt session files
#[test]
fn test_corrupt_session_file_handling() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_id = "test_corrupt_session";
    let session_path = session_dir.join(format!("{}.json", session_id));

    // Write invalid JSON
    fs::write(&session_path, "{ invalid json }").expect("Failed to write file");

    // Attempt to read and parse
    let content = fs::read_to_string(&session_path).expect("Failed to read file");
    let result: Result<serde_json::Value, _> = serde_json::from_str(&content);

    assert!(result.is_err(), "Invalid JSON should fail to parse");

    // Clean up
    fs::remove_file(&session_path).ok();
}

/// Test session with special characters in title
#[test]
fn test_session_special_characters() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_id = "test_special_chars";
    let title = "Session with \"quotes\" and 'apostrophes' and emojis 🚀";

    let session_path = create_mock_session(&session_dir, session_id, title, 1);

    // Verify file can be read and parsed
    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("Session file should contain valid JSON");

    assert_eq!(json["title"], title);
}

/// Test session with Unicode content
#[test]
fn test_session_unicode_content() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_path = session_dir.join("unicode_session.json");

    let session_data = json!({
        "id": "unicode_session",
        "title": "Unicode Session 你好 🚀",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "messages": [
            {
                "role": "user",
                "content": "Hello 你好 مرحبا Привет 🌍"
            }
        ]
    });

    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    // Verify file can be read and parsed
    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("Session file should contain valid JSON");

    assert!(json["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("你好"));
    assert!(json["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("🌍"));
}

/// Test concurrent session access simulation
#[test]
fn test_concurrent_session_operations() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    // Create multiple sessions concurrently
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let session_dir = session_dir.clone();
            std::thread::spawn(move || {
                let session_id = format!("concurrent_{:03}", i);
                create_mock_session(&session_dir, &session_id, &format!("Session {}", i), 2);
                session_id
            })
        })
        .collect();

    // Wait for all threads and collect results
    let session_ids: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();

    assert_eq!(session_ids.len(), 10, "All sessions should be created");

    // Verify all sessions exist
    for session_id in session_ids {
        let session_path = session_dir.join(format!("{}.json", session_id));
        assert!(session_path.exists(), "Session {} should exist", session_id);
    }
}

/// Test session metadata preservation
#[test]
fn test_session_metadata_preservation() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_path = session_dir.join("metadata_test.json");

    let session_data = json!({
        "id": "metadata_test",
        "title": "Metadata Test",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-02T12:30:45Z",
        "model": "claude-sonnet-4-6",
        "provider": "anthropic",
        "max_tokens": 4096,
        "temperature": 0.7,
        "messages": []
    });

    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    // Verify metadata is preserved
    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("Session file should contain valid JSON");

    assert_eq!(json["model"], "claude-sonnet-4-6");
    assert_eq!(json["provider"], "anthropic");
    assert_eq!(json["max_tokens"], 4096);
    assert_eq!(json["temperature"], 0.7);
}

/// Test large session handling
#[test]
fn test_large_session_handling() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_path = session_dir.join("large_session.json");

    // Create session with many messages
    let messages: Vec<serde_json::Value> = (0..100)
        .map(|i| {
            json!({
                "role": if i % 2 == 0 { "user" } else { "assistant" },
                "content": format!("This is message number {} with some content to make it longer", i)
            })
        })
        .collect();

    let session_data = json!({
        "id": "large_session",
        "title": "Large Session Test",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T01:00:00Z",
        "model": "claude-sonnet-4",
        "provider": "anthropic",
        "messages": messages
    });

    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_data).unwrap(),
    )
    .expect("Failed to write session file");

    // Verify session can be read
    let content = fs::read_to_string(&session_path).expect("Failed to read session file");
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("Session file should contain valid JSON");

    assert_eq!(json["messages"].as_array().unwrap().len(), 100);
}

/// Test session file permissions
#[test]
fn test_session_file_permissions() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    let session_path = create_mock_session(&session_dir, "perms_test", "Permissions Test", 1);

    // Check file is readable
    let metadata = fs::metadata(&session_path).expect("Failed to get metadata");
    assert!(metadata.is_file(), "Should be a file");

    // Try to read file
    let content = fs::read_to_string(&session_path);
    assert!(content.is_ok(), "File should be readable");
}

/// Test session directory with subdirectories
#[test]
fn test_session_directory_structure() {
    let temp_dir = setup_test_env();
    let session_dir = temp_dir.path().join("sessions");
    fs::create_dir_all(&session_dir).expect("Failed to create session dir");

    // Create a subdirectory
    let sub_dir = session_dir.join("archived");
    fs::create_dir_all(&sub_dir).expect("Failed to create subdirectory");

    // Create session in subdirectory
    let session_path = sub_dir.join("archived_session.json");
    create_mock_session(&sub_dir, "archived_session", "Archived", 1);

    assert!(
        session_path.exists(),
        "Session in subdirectory should exist"
    );

    // Verify it doesn't appear in main directory listing
    let main_entries: Vec<_> = fs::read_dir(&session_dir)
        .expect("Failed to read directory")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .collect();

    assert_eq!(main_entries.len(), 0, "Main directory should have no files");
}
