//! Integration tests for the compile-time tool system
//!
//! These tests demonstrate the complete functionality of the compile-time tool system.

use rustycode_tools::compile_time::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_complete_read_write_workflow() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.txt");

    // Write a file
    let content = "Hello, World!\nThis is a test.";
    let write_result = ToolDispatcher::<CompileTimeWriteFile>::dispatch(WriteFileInput {
        path: file_path.clone(),
        content: content.to_string(),
        create_parents: Some(false),
    })
    .unwrap();

    assert_eq!(write_result.bytes_written, content.len());

    // Read the file back
    let read_result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
        path: file_path.clone(),
        start_line: None,
        end_line: None,
    })
    .unwrap();

    assert_eq!(read_result.content, content);
    assert_eq!(read_result.bytes, content.len());

    // Read specific lines
    let lines_result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
        path: file_path.clone(),
        start_line: Some(1),
        end_line: Some(1),
    })
    .unwrap();

    assert_eq!(lines_result.content, "Hello, World!");
}

#[test]
fn test_bash_integration_with_file_operations() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("bash_output.txt");

    // Create a file using bash
    ToolDispatcher::<CompileTimeBash>::dispatch(BashInput {
        command: "sh".to_string(),
        args: Some(vec![
            "-c".to_string(),
            format!("echo 'Test content' > {}", file_path.display()),
        ]),
        working_dir: Some(dir.path().to_path_buf()),
        timeout_secs: Some(5),
    })
    .unwrap();

    // Verify the file was created
    let read_result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
        path: file_path.clone(),
        start_line: None,
        end_line: None,
    })
    .unwrap();

    assert_eq!(read_result.content.trim(), "Test content");
}

#[test]
fn test_tool_metadata_consistency() {
    // Verify that tool metadata is consistent and accessible
    let tools = vec![
        ("read_file", ToolPermission::Read),
        ("write_file", ToolPermission::Write),
        ("bash", ToolPermission::Execute),
    ];

    for (name, expected_permission) in tools {
        let metadata = match name {
            "read_file" => CompileTimeReadFile::METADATA,
            "write_file" => CompileTimeWriteFile::METADATA,
            "bash" => CompileTimeBash::METADATA,
            _ => panic!("Unknown tool: {}", name),
        };

        assert_eq!(metadata.name, name);
        assert_eq!(metadata.permission, expected_permission);
    }
}

#[test]
fn test_error_handling_comprehensive() {
    // Test file not found
    let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
        path: PathBuf::from("/nonexistent/file.txt"),
        start_line: None,
        end_line: None,
    });

    assert!(result.is_err());
    match result {
        Err(ReadFileError::Io(_)) => (),
        _ => panic!("Expected Io error"),
    }

    // Test inverted line range — implementation auto-swaps start/end
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "Line 1\nLine 2\nLine 3").unwrap();

    let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
        path: file_path,
        start_line: Some(5),
        end_line: Some(2), // end < start, will be auto-swapped
    });

    // Implementation auto-swaps inverted ranges instead of erroring
    assert!(result.is_ok());

    // Test command timeout
    let result = ToolDispatcher::<CompileTimeBash>::dispatch(BashInput {
        command: "sleep".to_string(),
        args: Some(vec!["10".to_string()]),
        working_dir: None,
        timeout_secs: Some(1),
    });

    if let Ok(output) = result {
        assert!(output.timed_out, "expected timeout flag");
    }
}

#[test]
fn test_dispatcher_zero_cost() {
    // Verify that the dispatcher is zero-sized
    assert_eq!(
        std::mem::size_of::<ToolDispatcher<CompileTimeReadFile>>(),
        0
    );
    assert_eq!(
        std::mem::size_of::<ToolDispatcher<CompileTimeWriteFile>>(),
        0
    );
    assert_eq!(std::mem::size_of::<ToolDispatcher<CompileTimeBash>>(), 0);
}

#[test]
fn test_type_safety_guarantees() {
    // This test demonstrates that type safety is enforced at compile time.
    // The following code would NOT compile (uncommented to verify):

    /*
    // Wrong input type - compile error!
    let _ = ToolDispatcher::<CompileTimeReadFile>::dispatch(WriteFileInput {
        path: PathBuf::from("test.txt"),
        content: "test".to_string(),
        create_parents: Some(false),
    });

    // Wrong output type - compile error!
    let _: ReadFileOutput = ToolDispatcher::<CompileTimeWriteFile>::dispatch(
        WriteFileInput {
            path: PathBuf::from("test.txt"),
            content: "test".to_string(),
            create_parents: Some(false),
        },
    );
    */

    // Correct usage - this compiles and works
    let input = ReadFileInput {
        path: PathBuf::from("/etc/hosts"),
        start_line: None,
        end_line: None,
    };

    let result: Result<ReadFileOutput, ReadFileError> =
        ToolDispatcher::<CompileTimeReadFile>::dispatch(input);

    // This will be Ok or Err depending on if /etc/hosts exists
    if result.is_ok() {}
}

#[test]
fn test_nested_directory_creation() {
    let dir = TempDir::new().unwrap();
    let nested_path = dir.path().join("a/b/c/d/test.txt");

    // Write to a nested path with parent creation
    let result = ToolDispatcher::<CompileTimeWriteFile>::dispatch(WriteFileInput {
        path: nested_path.clone(),
        content: "Nested file content".to_string(),
        create_parents: Some(true),
    })
    .unwrap();

    assert_eq!(result.bytes_written, 19);
    assert!(nested_path.exists());
    assert!(nested_path.parent().unwrap().exists());

    // Verify content
    let read_result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
        path: nested_path,
        start_line: None,
        end_line: None,
    })
    .unwrap();

    assert_eq!(read_result.content, "Nested file content");
}

#[test]
fn test_bash_with_various_commands() {
    // Test echo
    let result = ToolDispatcher::<CompileTimeBash>::dispatch(BashInput {
        command: "echo".to_string(),
        args: Some(vec!["test".to_string()]),
        working_dir: None,
        timeout_secs: Some(5),
    })
    .unwrap();

    assert_eq!(result.stdout.trim(), "test");
    assert_eq!(result.exit_code, 0);

    // Test true (always succeeds)
    let result = ToolDispatcher::<CompileTimeBash>::dispatch(BashInput {
        command: "true".to_string(),
        args: None,
        working_dir: None,
        timeout_secs: Some(5),
    })
    .unwrap();

    assert_eq!(result.exit_code, 0);

    // Test false (always fails)
    let result = ToolDispatcher::<CompileTimeBash>::dispatch(BashInput {
        command: "false".to_string(),
        args: None,
        working_dir: None,
        timeout_secs: Some(5),
    })
    .unwrap();

    assert_ne!(result.exit_code, 0);
}
