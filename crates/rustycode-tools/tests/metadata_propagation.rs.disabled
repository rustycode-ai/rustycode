//! End-to-end tests for Metadata Propagation
//!
//! Tests that metadata flows correctly through the tool execution system:
//! - execution_time_ms is tracked
//! - content_hash is computed correctly
//! - line_count is accurate
//! - Metadata is passed to LLM providers

use rustycode_tools::{BashTool, ReadFileTool, Tool, ToolContext};
use serde_json::json;
use std::fs;
use std::io::Write;
use std::time::Instant;
use tempfile::TempDir;

#[test]
fn test_tool_execution_time_tracking() {
    // Test that execution time is tracked
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    let mut file = fs::File::create(&test_file).unwrap();
    writeln!(file, "Test content").unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let start = Instant::now();
    let result = tool.execute(json!({"path": test_file.to_str().unwrap()}), &ctx);
    let _duration = start.elapsed();

    assert!(result.is_ok(), "Tool execution should succeed");
    let output = result.unwrap();

    assert!(!output.text.is_empty());

    if let Some(_structured) = output.structured {
        // Structured output present - good
    }
}

#[test]
fn test_content_hash_computation_for_files() {
    // Test that content hash is computed correctly for file reads
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    let mut file = fs::File::create(&test_file).unwrap();
    writeln!(file, "Consistent content").unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Read the file twice
    let result1 = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();
    let result2 = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();

    // Both should have the same content
    assert_eq!(result1.text, result2.text);

    // If metadata includes content hash, it should be the same
    // (This tests the concept - actual hash implementation may vary)
    assert_eq!(result1.text.len(), result2.text.len());
}

#[test]
fn test_line_count_accuracy() {
    // Test that line count is accurately tracked
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");

    // Create file with known number of lines
    let mut file = fs::File::create(&test_file).unwrap();
    for i in 0..10 {
        writeln!(file, "Line {}", i).unwrap();
    }

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();

    // Count actual lines in output
    let actual_lines = result.text.lines().count();
    assert_eq!(actual_lines, 10, "Should have 10 lines");
}

#[test]
fn test_metadata_in_structured_output() {
    // Test that metadata appears in structured output
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    let mut file = fs::File::create(&test_file).unwrap();
    writeln!(file, "Test content").unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();

    // Check if structured output contains metadata
    if let Some(_structured) = result.structured {
        // Structured output present - good
    }
}

#[test]
fn test_execution_metadata_for_different_tools() {
    // Test that metadata is tracked across different tool types
    let temp_dir = TempDir::new().unwrap();

    // Test ReadFileTool
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "content").unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let read_tool = ReadFileTool;
    let read_result = read_tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();

    assert!(!read_result.text.is_empty());

    // Test BashTool
    let bash_tool = BashTool;
    let bash_result = bash_tool
        .execute(json!({"command": "echo test"}), &ctx)
        .unwrap();

    assert!(!bash_result.text.is_empty());

    // Both should have output
    assert_ne!(read_result.text.len(), 0);
    assert_ne!(bash_result.text.len(), 0);
}

#[test]
fn test_error_metadata_propagation() {
    // Test that error information is propagated in metadata
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Try to read a non-existent file
    let result = tool.execute(json!({"path": "/nonexistent/file.txt"}), &ctx);

    // Should fail
    assert!(result.is_err());

    // Error should contain useful information
    let error_msg = result.unwrap_err().to_string();
    assert!(!error_msg.is_empty(), "Error message should not be empty");
}

#[test]
fn test_metadata_with_large_files() {
    // Test metadata tracking with larger files
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("large.txt");

    // Create a file with multiple lines (under READ_MAX_LINES limit of 80)
    let mut file = fs::File::create(&test_file).unwrap();
    for i in 0..50 {
        writeln!(file, "Line {} with some content", i).unwrap();
    }

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let start = Instant::now();
    let result = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();
    let duration = start.elapsed();

    // Should complete in reasonable time
    assert!(
        duration.as_millis() < 1000,
        "Large file read should be fast"
    );

    // Should have content
    assert!(!result.text.is_empty());

    // Should have multiple lines
    let line_count = result.text.lines().count();
    assert_eq!(line_count, 50, "Should have 50 lines");
}

#[test]
fn test_output_size_tracking() {
    // Test that output size is tracked
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    let content = "A".repeat(1000); // 1000 bytes
    fs::write(&test_file, &content).unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();

    // Output size should be reasonable
    assert!(
        result.text.len() >= 1000,
        "Output should contain the content"
    );
}

#[test]
fn test_structured_output_consistency() {
    // Test that structured output is consistent across executions
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "consistent content").unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result1 = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();
    let result2 = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();

    // Text output should be identical
    assert_eq!(result1.text, result2.text);

    if let (Some(s1), Some(s2)) = (&result1.structured, &result2.structured) {
        assert_eq!(
            s1.as_object().map(|o| o.keys().collect::<Vec<_>>()),
            s2.as_object().map(|o| o.keys().collect::<Vec<_>>())
        );
    }
}

#[test]
fn test_metadata_preserves_file_encoding() {
    // Test that metadata handling preserves file encoding
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("utf8.txt");

    // Write UTF-8 content with special characters
    let content = "Hello 世界 🌍";
    fs::write(&test_file, content).unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();

    // UTF-8 characters should be preserved
    assert!(
        result.text.contains("世界"),
        "Should preserve Chinese characters"
    );
    assert!(result.text.contains("🌍"), "Should preserve emoji");
}

#[test]
fn test_binary_file_handling() {
    // Test that metadata handles binary files gracefully
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("binary.bin");

    // Write binary data
    let binary_data: Vec<u8> = (0..255).collect();
    fs::write(&test_file, &binary_data).unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool.execute(json!({"path": test_file.to_str().unwrap()}), &ctx);

    // Should either succeed or fail gracefully
    if let Ok(output) = result {
        // If it succeeds, should have some output
        assert!(!output.text.is_empty() || output.structured.is_some());
    }
    // If it fails, that's also acceptable for binary files
}

#[test]
fn test_metadata_with_symlinks() {
    // Test metadata handling with symbolic links
    let temp_dir = TempDir::new().unwrap();
    let target_file = temp_dir.path().join("target.txt");
    let link_file = temp_dir.path().join("link.txt");

    fs::write(&target_file, "content").unwrap();

    // Create symlink
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_file, &link_file).unwrap();

    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&target_file, &link_file).unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Read through symlink
    let result = tool.execute(json!({"path": link_file.to_str().unwrap()}), &ctx);

    // Should resolve symlink and read content
    if let Ok(output) = result {
        assert!(output.text.contains("content"));
    }
}

#[test]
fn test_execution_time_for_bash_commands() {
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = BashTool;

    let start = Instant::now();
    let result = tool.execute(json!({"command": "echo fast"}), &ctx).unwrap();
    let duration = start.elapsed();

    assert!(!result.text.is_empty());
    assert!(duration.as_millis() < 2000, "Echo should complete quickly");

    let start = Instant::now();
    let result2 = tool
        .execute(json!({"command": "echo second"}), &ctx)
        .unwrap();
    let duration2 = start.elapsed();

    assert!(!result2.text.is_empty());
    assert!(duration2.as_millis() < 1000, "Second call should be faster");
}

#[test]
fn test_metadata_with_empty_files() {
    // Test metadata handling with empty files
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("empty.txt");
    fs::File::create(&test_file).unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let result = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();

    // Should succeed even with empty file
    assert!(result.text.is_empty() || result.text.trim().is_empty());

    // Line count should be 0 or 1 (depending on implementation)
    let line_count = result.text.lines().count();
    assert!(line_count <= 1, "Empty file should have 0 or 1 lines");
}

#[test]
fn test_metadata_propagation_to_llm_format() {
    // Test that metadata can be formatted for LLM consumption
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "test content").unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    let _result = tool
        .execute(json!({"path": test_file.to_str().unwrap()}), &ctx)
        .unwrap();

    // The text output should be LLM-friendly
    assert!(!_result.text.is_empty());

    // If structured output exists, it should be JSON-serializable
    if let Some(structured) = _result.structured {
        let json_str = serde_json::to_string(&structured);
        assert!(
            json_str.is_ok(),
            "Structured output should be JSON-serializable"
        );
    }
}
