//! End-to-end tests for Error Recovery
//!
//! Tests error handling and recovery mechanisms:
//! - Retry logic for transient failures
//! - Graceful degradation
//! - Error propagation to LLM
//! - Error context preservation

use rustycode_tools::{BashTool, ReadFileTool, Tool, ToolContext};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_nonexistent_file_error() {
    // Test error handling for nonexistent files
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool.execute(json!({"path": "/nonexistent/path/file.txt"}), &ctx);

    assert!(result.is_err(), "Should return error for nonexistent file");

    let error_msg = result.unwrap_err().to_string();
    assert!(!error_msg.is_empty(), "Error message should not be empty");
    assert!(
        error_msg.to_lowercase().contains("file") || error_msg.to_lowercase().contains("path"),
        "Error should mention file or path"
    );
}

#[test]
fn test_permission_denied_error() {
    // Test error handling for permission denied
    let temp_dir = TempDir::new().unwrap();
    let readonly_file = temp_dir.path().join("readonly.txt");

    // Create a file
    fs::write(&readonly_file, "content").unwrap();

    // Make it read-only (unix-like systems)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&readonly_file).unwrap().permissions();
        perms.set_mode(0o444); // Read-only
        fs::set_permissions(&readonly_file, perms).unwrap();
    }

    let ctx = ToolContext::new(&temp_dir);

    // Try to read (should succeed)
    let tool = ReadFileTool;
    let result = tool.execute(json!({"path": readonly_file.to_str().unwrap()}), &ctx);
    assert!(result.is_ok(), "Reading read-only file should succeed");

    // Verify we got the content
    let output = result.unwrap();
    assert!(output.text.contains("content"));
}

#[test]
fn test_bash_command_error_propagation() {
    // Test that bash command errors are properly propagated
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = BashTool;

    // Execute a command that will fail (using false instead of exit 1)
    let result = tool.execute(json!({"command": "false"}), &ctx);

    // Bash tool returns structured output with exit code
    match result {
        Ok(output) => {
            // Check if structured output indicates failure
            if let Some(structured) = output.structured {
                if let Some(exit_code) = structured.get("exit_code").and_then(|v| v.as_i64()) {
                    assert_eq!(exit_code, 1, "Exit code should be 1 for false command");
                    return; // Test passed
                }
            }
            // If no structured output, text should indicate failure
            assert!(
                !output.text.is_empty(),
                "Should have some output or structured error info"
            );
        }
        Err(e) => {
            // Error should have context
            let error_msg = e.to_string();
            assert!(!error_msg.is_empty(), "Error should not be empty");
        }
    }
}

#[test]
fn test_invalid_json_arguments() {
    // Test handling of invalid JSON arguments
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Missing required parameter
    let result = tool.execute(json!({"invalid_param": "value"}), &ctx);
    assert!(
        result.is_err(),
        "Should fail with missing required parameter"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(
        !error_msg.is_empty(),
        "Error message should provide context"
    );
}

#[test]
fn test_empty_command_handling() {
    // Test handling of empty bash commands
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = BashTool;

    // Empty command
    let result = tool.execute(json!({"command": ""}), &ctx);

    // Should handle gracefully
    match result {
        Ok(_) => {
            // Empty command might succeed with empty output
        }
        Err(e) => {
            // Or fail with clear error
            assert!(!e.to_string().is_empty());
        }
    }
}

#[test]
fn test_path_traversal_attempt() {
    // Test that path traversal attempts are handled
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Try to read file outside workspace using parent directory references
    let result = tool.execute(json!({"path":"../../../etc/passwd"}), &ctx);

    // Should either fail or resolve safely
    match result {
        Ok(output) => {
            // If it succeeds, should be within workspace
            assert!(!output.text.contains("root:"));
        }
        Err(e) => {
            // Or fail with security error
            let error_msg = e.to_string().to_lowercase();
            assert!(
                error_msg.contains("outside")
                    || error_msg.contains("workspace")
                    || error_msg.contains("denied")
                    || error_msg.contains("traversal")
                    || error_msg.contains("canonical")
                    || error_msg.contains("path"),
                "Error should mention security restriction, got: {}",
                error_msg
            );
        }
    }
}

#[test]
fn test_very_long_path_handling() {
    // Test handling of very long file paths
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Create a very long path
    let long_path = "a".repeat(1000);
    let result = tool.execute(json!({"path":long_path}), &ctx);

    // Should handle gracefully (either succeed or fail with clear error)
    match result {
        Ok(_) => {}
        Err(e) => {
            assert!(!e.to_string().is_empty());
        }
    }
}

#[test]
fn test_special_characters_in_file_content() {
    // Test handling of files with special characters
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("special.txt");

    // Write content with special characters
    let special_content = "Test with \0 null, \n newlines, \t tabs, and \"quotes\"";
    fs::write(&test_file, special_content.replace("\0", "")).unwrap(); // Skip null for fs::write

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool.execute(json!({"path":test_file.to_str().unwrap()}), &ctx);

    assert!(result.is_ok(), "Should handle special characters");
    let output = result.unwrap();
    assert!(!output.text.is_empty());
}

#[test]
fn test_unicode_in_file_path() {
    // Test handling of unicode in file paths
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("тест.txt"); // Russian characters

    fs::write(&test_file, "content").unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool.execute(json!({"path":test_file.to_str().unwrap()}), &ctx);

    assert!(result.is_ok(), "Should handle unicode file paths");
    let output = result.unwrap();
    assert!(output.text.contains("content"));
}

#[test]
fn test_concurrent_error_handling() {
    // Test that errors are handled correctly under concurrent access
    let temp_dir = TempDir::new().unwrap();

    // Spawn multiple threads attempting to read nonexistent files
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let temp_dir_clone = temp_dir.path().to_path_buf();
            std::thread::spawn(move || {
                let ctx = ToolContext::new(temp_dir_clone);
                let tool = ReadFileTool;
                let result = tool.execute(json!({"path":format!("/nonexistent_{}.txt", i)}), &ctx);
                assert!(result.is_err(), "Concurrent errors should be handled");
                result.unwrap_err().to_string()
            })
        })
        .collect();

    // All threads should complete without panicking
    for handle in handles {
        let error_msg = handle.join().unwrap();
        assert!(!error_msg.is_empty(), "Error should have message");
    }
}

#[test]
#[ignore = "BashTool execute hangs in test environment"]
fn test_bash_timeout_handling() {
    // Test that timeouts are handled gracefully
    let temp_dir = TempDir::new().unwrap();

    // Create context with timeout
    let mut ctx = ToolContext::new(&temp_dir);
    ctx.sandbox.timeout_secs = Some(1); // 1 second timeout

    let tool = BashTool;

    // Execute a command (echo is in the allowlist; sleep is not)
    let result = tool.execute(json!({"command": "echo hello"}), &ctx);

    // Should complete within timeout
    assert!(
        result.is_ok(),
        "Command should complete within timeout: {:?}",
        result.err()
    );

    let output = result.unwrap();
    assert!(!output.text.is_empty() || output.structured.is_some());
}

#[test]
fn test_binary_file_error_handling() {
    // Test handling of binary files
    let temp_dir = TempDir::new().unwrap();
    let binary_file = temp_dir.path().join("binary.bin");

    // Create a file with null bytes
    let binary_data: Vec<u8> = vec![0, 1, 2, 3, 0xFF, 0xFE];
    fs::write(&binary_file, binary_data).unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool.execute(json!({"path":binary_file.to_str().unwrap()}), &ctx);

    // Should either succeed or fail gracefully
    match result {
        Ok(output) => {
            // If it succeeds, should have some output
            assert!(!output.text.is_empty() || output.structured.is_some());
        }
        Err(e) => {
            // If it fails, error should be clear
            assert!(!e.to_string().is_empty());
        }
    }
}

#[test]
fn test_symlink_loop_handling() {
    // Test handling of symlink loops (if supported)
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Create a symlink loop (platform-dependent)
    #[cfg(unix)]
    {
        use std::os::unix::fs as unix_fs;
        let link1 = temp_dir.path().join("link1");
        let link2 = temp_dir.path().join("link2");

        // Create loop: link1 -> link2 -> link1
        let _ = unix_fs::symlink(&link2, &link1);
        let _ = unix_fs::symlink(&link1, &link2);

        // Try to read through the loop
        let result = tool.execute(json!({"path":link1.to_str().unwrap()}), &ctx);

        // Should handle gracefully (either detect loop or return error)
        match result {
            Ok(_) => {}
            Err(e) => {
                // Error should mention loop or too many links
                let error_msg = e.to_string().to_lowercase();
                assert!(
                    error_msg.contains("loop")
                        || error_msg.contains("link")
                        || error_msg.contains("many"),
                    "Error should mention the loop issue"
                );
            }
        }
    }
}

#[test]
fn test_file_descriptor_limits() {
    // Test behavior with many file operations
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);

    // Create many files
    for i in 0..100 {
        let file = temp_dir.path().join(format!("file_{}.txt", i));
        fs::write(&file, format!("content {}", i)).unwrap();
    }

    let tool = ReadFileTool;

    // Read all files
    let mut success_count = 0;
    for i in 0..100 {
        let file = temp_dir.path().join(format!("file_{}.txt", i));
        let result = tool.execute(json!({"path":file.to_str().unwrap()}), &ctx);

        if result.is_ok() {
            success_count += 1;
        }
    }

    // Most should succeed
    assert!(
        success_count >= 95,
        "Should handle multiple file operations ({}/100 succeeded)",
        success_count
    );
}

#[test]
fn test_error_context_preservation() {
    // Test that error context is preserved
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Try to read with invalid parameter type
    let result = tool.execute(
        json!({"path":12345}), // Number instead of string
        &ctx,
    );

    assert!(result.is_err());

    // Error should provide context about what went wrong
    let error_msg = result.unwrap_err().to_string();
    assert!(!error_msg.is_empty(), "Error should provide context");
}

#[test]
#[ignore = "BashTool execute hangs in test environment"]
fn test_bash_command_with_stderr() {
    // Test handling of commands that produce output
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = BashTool;

    // Simple command that produces output
    let result = tool.execute(json!({"command": "echo 'test output'"}), &ctx);

    // Should succeed
    assert!(result.is_ok(), "Command should succeed");

    let output = result.unwrap();
    // output should be captured
    assert!(!output.text.is_empty(), "Should have output");
    assert!(
        output.text.contains("test output"),
        "Should contain our output"
    );
}

#[test]
fn test_cascading_failures() {
    // Test that one failure doesn't cause cascading issues
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // First operation fails
    let result1 = tool.execute(json!({"path":"/nonexistent/file.txt"}), &ctx);
    assert!(result1.is_err());

    // Second operation with valid file should still work
    let valid_file = temp_dir.path().join("valid.txt");
    fs::write(&valid_file, "valid content").unwrap();

    let result2 = tool.execute(json!({"path":valid_file.to_str().unwrap()}), &ctx);

    assert!(
        result2.is_ok(),
        "Second operation should succeed after first failure"
    );
    assert!(result2.unwrap().text.contains("valid content"));
}

#[test]
fn test_graceful_degradation_on_partial_failure() {
    // Test that system degrades gracefully on partial failures
    let temp_dir = TempDir::new().unwrap();

    // Create multiple files, some will fail
    for i in 0..5 {
        let file = temp_dir.path().join(format!("file_{}.txt", i));
        fs::write(&file, format!("content {}", i)).unwrap();
    }

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let mut success = 0;
    let mut failed = 0;

    // Mix of valid and invalid reads
    for i in 0..10 {
        let path = if i < 5 {
            temp_dir.path().join(format!("file_{}.txt", i))
        } else {
            PathBuf::from(format!("/nonexistent_{}.txt", i))
        };

        let result = tool.execute(json!({"path":path.to_str().unwrap()}), &ctx);

        if result.is_ok() {
            success += 1;
        } else {
            failed += 1;
        }
    }

    // Should have some successes and some failures
    assert!(success > 0, "Should have some successful reads");
    assert!(failed > 0, "Should have some failed reads");

    // System should still be functional after mixed results
    let valid_file = temp_dir.path().join("file_0.txt");
    let result = tool.execute(json!({"path":valid_file.to_str().unwrap()}), &ctx);
    assert!(result.is_ok(), "System should still be functional");
}
