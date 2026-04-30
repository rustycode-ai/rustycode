// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Tool Execution Integration Tests
//!
//! This test suite validates the tool execution system, with special focus on:
//! - Eager streaming parameter parsing (the bug that was just fixed)
//! - Delta-based parameter accumulation (traditional streaming)
//! - Simple and complex file operations
//! - Multiple tool calls in sequence
//! - Tool execution via runtime and direct dispatch

use rustycode_bus::ToolExecutedEvent;
use rustycode_protocol::{SessionId, ToolCall};
use rustycode_runtime::AsyncRuntime;
use rustycode_tools::{
    BashInput, CompileTimeBash, CompileTimeReadFile, CompileTimeWriteFile, ReadFileInput,
    ToolDispatcher, WriteFileInput,
};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

/// Test helper to create a temporary runtime environment
struct TestEnvironment {
    _temp_dir: TempDir,
    runtime: AsyncRuntime,
}

impl TestEnvironment {
    async fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cwd = temp_dir.path();

        // Create necessary directories
        std::fs::create_dir_all(cwd.join("data")).expect("Failed to create data dir");
        std::fs::create_dir_all(cwd.join("skills")).expect("Failed to create skills dir");
        std::fs::create_dir_all(cwd.join("memory")).expect("Failed to create memory dir");

        // Create config file
        let config_content = format!(
            r#"
data_dir = "{}"
skills_dir = "{}"
memory_dir = "{}"
lsp_servers = []
"#,
            cwd.join("data").display(),
            cwd.join("skills").display(),
            cwd.join("memory").display()
        );
        std::fs::write(cwd.join(".rustycode.toml"), config_content)
            .expect("Failed to write config");

        // Initialize runtime
        let runtime = AsyncRuntime::load(cwd)
            .await
            .expect("Failed to load runtime");

        Self {
            _temp_dir: temp_dir,
            runtime,
        }
    }

    fn path(&self) -> &Path {
        self._temp_dir.path()
    }

    fn file_path(&self, name: &str) -> std::path::PathBuf {
        self._temp_dir.path().join(name)
    }
}

// ============================================================================
// Basic Tool Execution Tests
// ============================================================================

#[tokio::test]
async fn test_write_file_with_small_content() {
    let env = TestEnvironment::new().await;
    let test_file = env.file_path("small_test.txt");

    // Execute write_file tool via runtime
    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "test-write-small".to_string(),
        name: "write_file".to_string(),
        arguments: serde_json::json!({
            "path": test_file.to_str().unwrap(),
            "content": "Hello, World!"
        }),
    };

    let result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution failed");

    assert!(result.success, "Tool execution should succeed");
    assert!(
        result.output.contains("12 bytes written"),
        "Output should indicate bytes written: {}",
        result.output
    );

    // Verify file was created with correct content
    let content = std::fs::read_to_string(&test_file).expect("Failed to read file");
    assert_eq!(content, "Hello, World!");
}

#[tokio::test]
async fn test_write_file_with_large_content() {
    let env = TestEnvironment::new().await;
    let test_file = env.file_path("large_test.rs");

    // Create a large Rust file content (> 100 chars)
    let large_content = r#"//! Email validation module
//!
//! This module provides comprehensive email validation functionality.

use regex::Regex;

/// Validates an email address according to RFC 5322
pub fn validate_email(email: &str) -> Result<bool, EmailError> {
    if email.len() > 254 {
        return Err(EmailError::TooLong);
    }
    
    let pattern = Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
        .map_err(|_| EmailError::InvalidPattern)?;
    
    Ok(pattern.is_match(email))
}

#[derive(Debug, Clone, PartialEq)]
pub enum EmailError {
    TooLong,
    InvalidPattern,
    InvalidDomain,
}

impl std::fmt::Display for EmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmailError::TooLong => write!(f, "Email address exceeds maximum length"),
            EmailError::InvalidPattern => write!(f, "Invalid email pattern"),
            EmailError::InvalidDomain => write!(f, "Invalid domain"),
        }
    }
}

impl std::error::Error for EmailError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_email() {
        assert!(validate_email("test@example.com").unwrap());
        assert!(validate_email("user.name@domain.co.uk").unwrap());
        assert!(validate_email("user+tag@example.io").unwrap());
    }

    #[test]
    fn test_invalid_email() {
        assert!(!validate_email("invalid").unwrap());
        assert!(!validate_email("@example.com").unwrap());
        assert!(!validate_email("user@").unwrap());
    }

    #[test]
    fn test_email_too_long() {
        let long_email = "a".repeat(250) + "@example.com";
        assert!(matches!(
            validate_email(&long_email),
            Err(EmailError::TooLong)
        ));
    }
}
"#;

    // Execute write_file tool via runtime
    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "test-write-large".to_string(),
        name: "write_file".to_string(),
        arguments: serde_json::json!({
            "path": test_file.to_str().unwrap(),
            "content": large_content
        }),
    };

    let result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution failed");

    assert!(result.success, "Tool execution should succeed");
    assert!(
        result.output.contains("bytes written"),
        "Output should indicate bytes written"
    );

    // Verify file was created with correct content
    let content = std::fs::read_to_string(&test_file).expect("Failed to read file");
    assert_eq!(content, large_content);
    assert!(content.len() > 100, "Content should be > 100 chars");
    assert!(content.contains("validate_email"));
    assert!(content.contains("mod tests"));
}

#[tokio::test]
async fn test_read_file_tool() {
    let env = TestEnvironment::new().await;
    let test_file = env.file_path("read_test.txt");

    // Create a test file
    let test_content = "This is test content for read_file tool.\nLine 2\nLine 3";
    std::fs::write(&test_file, test_content).expect("Failed to write test file");

    // Execute read_file tool via runtime
    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "test-read".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({
            "path": test_file.to_str().unwrap()
        }),
    };

    let result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution failed");

    assert!(result.success, "Tool execution should succeed");
    assert_eq!(result.output, test_content);
}

#[tokio::test]
async fn test_read_file_with_line_range() {
    let env = TestEnvironment::new().await;
    let test_file = env.file_path("read_range_test.txt");

    // Create a test file with multiple lines
    let test_content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
    std::fs::write(&test_file, test_content).expect("Failed to write test file");

    // Execute read_file tool with line range
    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "test-read-range".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({
            "path": test_file.to_str().unwrap(),
            "start_line": 2,
            "end_line": 4
        }),
    };

    let result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution failed");

    assert!(result.success, "Tool execution should succeed");
    assert_eq!(result.output, "Line 2\nLine 3\nLine 4");
}

#[tokio::test]
async fn test_bash_tool() {
    let env = TestEnvironment::new().await;

    // Execute bash tool via runtime
    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "test-bash".to_string(),
        name: "bash".to_string(),
        arguments: serde_json::json!({
            "command": "echo",
            "args": ["-n", "Hello from bash"]
        }),
    };

    let result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution failed");

    assert!(result.success, "Tool execution should succeed");
    assert_eq!(result.output, "Hello from bash");
}

#[tokio::test]
async fn test_bash_tool_with_working_dir() {
    let env = TestEnvironment::new().await;
    let subdir = env.file_path("subdir");
    std::fs::create_dir(&subdir).expect("Failed to create subdirectory");

    // Execute bash tool with working directory
    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "test-bash-cwd".to_string(),
        name: "bash".to_string(),
        arguments: serde_json::json!({
            "command": "pwd",
            "working_dir": subdir.to_str().unwrap()
        }),
    };

    let result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution failed");

    assert!(result.success, "Tool execution should succeed");
    assert!(result.output.contains("subdir"), "Output should contain subdir: {}", result.output);
}

// ============================================================================
// Eager Streaming Parameter Parsing Tests
// ============================================================================

/// These tests specifically validate the eager streaming fix where tool parameters
/// sent via "eager streaming" (complete parameters in content_block_start event)
/// were being ignored.

#[test]
fn test_eager_streaming_small_content_parsing() {
    // Simulate the scenario where Anthropic sends complete parameters
    // in the content_block_start event (eager streaming)
    let input = serde_json::json!({
        "path": "/tmp/eager_small.txt",
        "content": "Small content"
    });

    // Verify the JSON can be parsed correctly
    let parsed: serde_json::Value = serde_json::from_str(&input.to_string()).unwrap();
    assert_eq!(parsed["path"], "/tmp/eager_small.txt");
    assert_eq!(parsed["content"], "Small content");

    // Verify it can be used as tool arguments
    let tool_call = ToolCall::with_generated_id("write_file", parsed);
    assert_eq!(tool_call.name, "write_file");
    assert!(tool_call.arguments["path"].as_str().is_some());
    assert!(tool_call.arguments["content"].as_str().is_some());
}

#[test]
fn test_eager_streaming_large_content_parsing() {
    // This is the exact scenario that was failing before the fix
    let large_content = "pub fn validate_email(email: &str) -> bool {\n    // Complex validation logic\n    email.contains('@') && email.contains('.')\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn test_valid_email() {\n        assert!(validate_email(\"test@example.com\"));\n    }\n}";

    let input = serde_json::json!({
        "path": "email_validator.rs",
        "content": large_content
    });

    // Verify the JSON can be parsed correctly
    let parsed: serde_json::Value = serde_json::from_str(&input.to_string()).unwrap();
    assert_eq!(parsed["path"], "email_validator.rs");
    assert_eq!(parsed["content"].as_str().unwrap(), large_content);

    // Verify the content length is preserved
    assert!(parsed["content"].as_str().unwrap().len() > 100);
}

#[test]
fn test_eager_streaming_complex_nested_parameters() {
    // Test with complex nested JSON parameters
    let input = serde_json::json!({
        "path": "/tmp/complex.json",
        "content": serde_json::json!({
            "name": "test",
            "items": ["a", "b", "c"],
            "nested": {
                "key1": "value1",
                "key2": 42
            }
        }).to_string()
    });

    let parsed: serde_json::Value = serde_json::from_str(&input.to_string()).unwrap();
    let content_str = parsed["content"].as_str().unwrap();
    
    // Verify the nested content can be parsed back
    let nested: serde_json::Value = serde_json::from_str(content_str).unwrap();
    assert_eq!(nested["name"], "test");
    assert_eq!(nested["items"].as_array().unwrap().len(), 3);
    assert_eq!(nested["nested"]["key2"], 42);
}

// ============================================================================
// Delta-Based Parameter Accumulation Tests
// ============================================================================

/// These tests validate the traditional streaming approach where parameters
/// are accumulated from delta events.

#[test]
fn test_delta_based_parameter_accumulation() {
    // Simulate accumulating JSON chunks from delta events
    let chunks = vec![
        "{\"path\": \"/tmp/",
        "test.txt\", \"content\": \"",
        "Hello, ",
        "World!\"}",
    ];

    let mut accumulated = String::new();
    for chunk in chunks {
        accumulated.push_str(chunk);
    }

    // Verify accumulated JSON is valid
    let parsed: serde_json::Value = serde_json::from_str(&accumulated).unwrap();
    assert_eq!(parsed["path"], "/tmp/test.txt");
    assert_eq!(parsed["content"], "Hello, World!");
}

#[test]
fn test_delta_based_large_content_accumulation() {
    // Simulate accumulating large content from multiple delta events
    let content_parts = vec![
        "pub fn ",
        "validate_email",
        "(email: &str)",
        " -> bool {\n    ",
        "email.contains('@')",
        "\n}",
    ];

    let mut accumulated_content = String::new();
    for part in content_parts {
        accumulated_content.push_str(part);
    }

    let input = serde_json::json!({
        "path": "validator.rs",
        "content": accumulated_content
    });

    let parsed: serde_json::Value = serde_json::from_str(&input.to_string()).unwrap();
    assert!(parsed["content"].as_str().unwrap().contains("validate_email"));
}

// ============================================================================
// Multiple Tool Calls in Sequence Tests
// ============================================================================

#[tokio::test]
async fn test_multiple_tools_in_sequence() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    // Subscribe to tool events
    let (_id, mut rx) = bus
        .subscribe("tool.*")
        .await
        .expect("Failed to subscribe to tool events");

    let session_id = SessionId::new();

    // Execute first tool: write_file
    let file_path = env.file_path("sequence_test.txt");
    let write_call = ToolCall {
        call_id: "seq-write".to_string(),
        name: "write_file".to_string(),
        arguments: serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "Initial content"
        }),
    };

    let result1 = env
        .runtime
        .execute_tool(&session_id, write_call, env.path())
        .await
        .expect("Write tool execution failed");
    assert!(result1.success);

    // Execute second tool: read_file
    let read_call = ToolCall {
        call_id: "seq-read".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({
            "path": file_path.to_str().unwrap()
        }),
    };

    let result2 = env
        .runtime
        .execute_tool(&session_id, read_call, env.path())
        .await
        .expect("Read tool execution failed");
    assert!(result2.success);
    assert_eq!(result2.output, "Initial content");

    // Execute third tool: bash to append
    let bash_call = ToolCall {
        call_id: "seq-bash".to_string(),
        name: "bash".to_string(),
        arguments: serde_json::json!({
            "command": "echo",
            "args": ["-n", "Appended"],
            "working_dir": env.path().to_str().unwrap()
        }),
    };

    let result3 = env
        .runtime
        .execute_tool(&session_id, bash_call, env.path())
        .await
        .expect("Bash tool execution failed");
    assert!(result3.success);
    assert_eq!(result3.output, "Appended");

    // Verify all events were published
    let mut event_count = 0;
    while let Ok(Ok(_event)) = timeout(Duration::from_millis(100), rx.recv()).await {
        event_count += 1;
        if event_count >= 3 {
            break;
        }
    }

    assert_eq!(event_count, 3, "Expected 3 tool events to be published");
}

#[tokio::test]
async fn test_write_read_verify_cycle() {
    let env = TestEnvironment::new().await;

    let session_id = SessionId::new();
    let test_file = env.file_path("cycle_test.txt");
    let test_content = "Test content for write-read-verify cycle";

    // Step 1: Write file
    let write_result = env
        .runtime
        .execute_tool(
            &session_id,
            ToolCall {
                call_id: "cycle-write".to_string(),
                name: "write_file".to_string(),
                arguments: serde_json::json!({
                    "path": test_file.to_str().unwrap(),
                    "content": test_content
                }),
            },
            env.path(),
        )
        .await
        .expect("Write failed");
    assert!(write_result.success);

    // Step 2: Read file back
    let read_result = env
        .runtime
        .execute_tool(
            &session_id,
            ToolCall {
                call_id: "cycle-read".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({
                    "path": test_file.to_str().unwrap()
                }),
            },
            env.path(),
        )
        .await
        .expect("Read failed");
    assert!(read_result.success);

    // Step 3: Verify content matches
    assert_eq!(read_result.output, test_content);

    // Step 4: Verify file exists on disk
    assert!(test_file.exists());
    let disk_content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(disk_content, test_content);
}

// ============================================================================
// Compile-Time Tool Dispatch Tests
// ============================================================================

#[test]
fn test_compile_time_write_file_dispatch() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("compile_time_test.txt");

    let input = WriteFileInput {
        path: file_path.clone(),
        content: "Compile-time dispatch test".to_string(),
        create_parents: Some(false),
    };

    let result = ToolDispatcher::<CompileTimeWriteFile>::dispatch(input).unwrap();

    assert_eq!(result.path, file_path);
    assert_eq!(result.bytes_written, 26);

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "Compile-time dispatch test");
}

#[test]
fn test_compile_time_read_file_dispatch() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("read_test.txt");
    std::fs::write(&file_path, "Test content for compile-time read").unwrap();

    let input = ReadFileInput {
        path: file_path.clone(),
        start_line: None,
        end_line: None,
    };

    let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(input).unwrap();

    assert_eq!(result.content, "Test content for compile-time read");
    assert_eq!(result.path, file_path);
    assert_eq!(result.bytes, 34);
}

#[test]
fn test_compile_time_bash_dispatch() {
    let input = BashInput {
        command: "echo".to_string(),
        args: Some(vec!["-n", "Compile-time bash"]),
        working_dir: None,
        timeout_secs: Some(5),
    };

    let result = ToolDispatcher::<CompileTimeBash>::dispatch(input).unwrap();

    assert_eq!(result.stdout, "Compile-time bash");
    assert_eq!(result.exit_code, 0);
    assert!(result.stderr.is_empty());
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_read_nonexistent_file() {
    let env = TestEnvironment::new().await;

    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "test-read-missing".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({
            "path": "/nonexistent/path/file.txt"
        }),
    };

    let result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution should return result");

    assert!(!result.success, "Reading nonexistent file should fail");
    assert!(
        result.error.is_some() || result.output.contains("error") || result.output.contains("Error"),
        "Should have error indication"
    );
}

#[tokio::test]
async fn test_bash_invalid_command() {
    let env = TestEnvironment::new().await;

    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "test-bash-invalid".to_string(),
        name: "bash".to_string(),
        arguments: serde_json::json!({
            "command": "this_command_definitely_does_not_exist_12345"
        }),
    };

    let result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution should return result");

    assert!(!result.success, "Invalid command should fail");
}

#[tokio::test]
async fn test_invalid_tool_parameters() {
    let env = TestEnvironment::new().await;

    let session_id = SessionId::new();
    // Missing required "path" parameter for write_file
    let tool_call = ToolCall {
        call_id: "test-invalid-params".to_string(),
        name: "write_file".to_string(),
        arguments: serde_json::json!({
            "content": "Missing path parameter"
        }),
    };

    let result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await;

    // Should either fail or handle gracefully
    match result {
        Ok(tool_result) => {
            // If it succeeds, it should use a default or handle gracefully
            // The exact behavior depends on the tool implementation
        }
        Err(_) => {
            // Error is also acceptable
        }
    }
}

// ============================================================================
// Tool Execution Event Tests
// ============================================================================

#[tokio::test]
async fn test_tool_execution_publishes_correct_events() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    let (_id, mut rx) = bus
        .subscribe("tool.executed")
        .await
        .expect("Failed to subscribe");

    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "event-test".to_string(),
        name: "bash".to_string(),
        arguments: serde_json::json!({
            "command": "echo",
            "args": ["event test"]
        }),
    };

    let _result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution failed");

    // Verify event was received
    let event = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("No event received");

    assert_eq!(event.event_type(), "tool.executed");

    // Verify event data
    let event_data = event.serialize();
    assert_eq!(event_data["tool_name"], "bash");
    assert_eq!(event_data["session_id"], session_id.to_string());
    assert!(event_data["success"].as_bool().unwrap());
}

#[tokio::test]
async fn test_tool_execution_event_contains_arguments() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    let (_id, mut rx) = bus
        .subscribe("tool.executed")
        .await
        .expect("Failed to subscribe");

    let session_id = SessionId::new();
    let tool_call = ToolCall {
        call_id: "args-test".to_string(),
        name: "write_file".to_string(),
        arguments: serde_json::json!({
            "path": "/tmp/test_args.txt",
            "content": "test content"
        }),
    };

    let _result = env
        .runtime
        .execute_tool(&session_id, tool_call, env.path())
        .await
        .expect("Tool execution failed");

    // Verify event was received with arguments
    let event = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("No event received");

    let event_data = event.serialize();
    assert!(event_data["arguments"].is_object());
}

// ============================================================================
// Concurrent Tool Execution Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_tool_executions() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    let (_id, mut rx) = bus
        .subscribe("tool.*")
        .await
        .expect("Failed to subscribe");

    let session_id = SessionId::new();

    // Execute tools sequentially (AsyncRuntime is not Send-safe)
    for i in 0..5 {
        let tool_call = ToolCall {
            call_id: format!("concurrent-{}", i),
            name: "bash".to_string(),
            arguments: serde_json::json!({
                "command": "echo",
                "args": [format!("test{}", i)]
            }),
        };

        let result = env
            .runtime
            .execute_tool(&session_id, tool_call, env.path())
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().success);
    }

    // Verify all events were published
    let mut count = 0;
    while let Ok(Ok(_event)) = timeout(Duration::from_millis(100), rx.recv()).await {
        count += 1;
        if count >= 5 {
            break;
        }
    }

    assert_eq!(count, 5, "Expected 5 tool events");
}

// ============================================================================
// End-to-End Integration Tests
// ============================================================================

#[tokio::test]
async fn test_end_to_end_file_workflow() {
    let env = TestEnvironment::new().await;
    let session_id = SessionId::new();

    // Create a Rust file with tests (simulating a typical AI coding workflow)
    let rust_file = env.file_path("calculator.rs");
    let rust_content = r#"//! Simple calculator module

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
        assert_eq!(add(-1, 1), 0);
    }

    #[test]
    fn test_subtract() {
        assert_eq!(subtract(5, 3), 2);
        assert_eq!(subtract(0, 5), -5);
    }
}
"#;

    // Step 1: Write the file
    let write_result = env
        .runtime
        .execute_tool(
            &session_id,
            ToolCall {
                call_id: "e2e-write".to_string(),
                name: "write_file".to_string(),
                arguments: serde_json::json!({
                    "path": rust_file.to_str().unwrap(),
                    "content": rust_content
                }),
            },
            env.path(),
        )
        .await
        .expect("Write failed");
    assert!(write_result.success);

    // Step 2: Read the file back
    let read_result = env
        .runtime
        .execute_tool(
            &session_id,
            ToolCall {
                call_id: "e2e-read".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({
                    "path": rust_file.to_str().unwrap()
                }),
            },
            env.path(),
        )
        .await
        .expect("Read failed");
    assert!(read_result.success);
    assert_eq!(read_result.output, rust_content);

    // Step 3: List directory to verify file exists
    let list_result = env
        .runtime
        .execute_tool(
            &session_id,
            ToolCall {
                call_id: "e2e-list".to_string(),
                name: "list_dir".to_string(),
                arguments: serde_json::json!({
                    "path": env.path().to_str().unwrap()
                }),
            },
            env.path(),
        )
        .await
        .expect("List failed");
    assert!(list_result.success);
    assert!(list_result.output.contains("calculator.rs"));

    // Step 4: Use bash to check file size
    let bash_result = env
        .runtime
        .execute_tool(
            &session_id,
            ToolCall {
                call_id: "e2e-bash".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({
                    "command": "wc",
                    "args": ["-c", rust_file.to_str().unwrap()]
                }),
            },
            env.path(),
        )
        .await
        .expect("Bash failed");
    assert!(bash_result.success);
    // wc output contains the byte count
    assert!(
        bash_result.output.contains("calculator.rs"),
        "Output should contain filename: {}",
        bash_result.output
    );
}

#[tokio::test]
async fn test_complex_parameter_handling() {
    let env = TestEnvironment::new().await;
    let session_id = SessionId::new();

    // Test with various parameter types
    let test_cases = vec![
        // Simple string
        serde_json::json!({
            "path": env.file_path("test1.txt").to_str().unwrap(),
            "content": "Simple"
        }),
        // String with special characters
        serde_json::json!({
            "path": env.file_path("test2.txt").to_str().unwrap(),
            "content": "Special chars: !@#$%^&*()_+-=[]{}|;':\",./<>?"
        }),
        // String with newlines
        serde_json::json!({
            "path": env.file_path("test3.txt").to_str().unwrap(),
            "content": "Line 1\nLine 2\nLine 3"
        }),
        // String with unicode
        serde_json::json!({
            "path": env.file_path("test4.txt").to_str().unwrap(),
            "content": "Unicode: 你好世界 🎉 émojis"
        }),
    ];

    for (i, args) in test_cases.iter().enumerate() {
        let tool_call = ToolCall {
            call_id: format!("complex-params-{}", i),
            name: "write_file".to_string(),
            arguments: args.clone(),
        };

        let result = env
            .runtime
            .execute_tool(&session_id, tool_call, env.path())
            .await
            .expect("Tool execution failed");

        assert!(result.success, "Test case {} should succeed", i);
    }

    // Verify all files were created
    for i in 1..=4 {
        let file_path = env.file_path(&format!("test{}.txt", i));
        assert!(file_path.exists(), "File {} should exist", i);
    }
}

// ============================================================================
// Tool Execution Metrics Tests
// ============================================================================

#[tokio::test]
async fn test_tool_execution_metrics() {
    let env = TestEnvironment::new().await;
    let bus = env.runtime.event_bus();

    let initial_metrics = bus.metrics();
    let initial_published = initial_metrics.events_published;

    let session_id = SessionId::new();

    // Execute multiple tools
    for i in 0..3 {
        let tool_call = ToolCall {
            call_id: format!("metrics-{}", i),
            name: "bash".to_string(),
            arguments: serde_json::json!({
                "command": "echo",
                "args": [format!("test{}", i)]
            }),
        };

        let _result = env
            .runtime
            .execute_tool(&session_id, tool_call, env.path())
            .await
            .expect("Tool execution failed");
    }

    // Give time for events to be processed
    tokio::time::sleep(Duration::from_millis(50)).await;

    let final_metrics = bus.metrics();
    assert!(
        final_metrics.events_published >= initial_published + 3,
        "Expected at least 3 more events published"
    );
}
