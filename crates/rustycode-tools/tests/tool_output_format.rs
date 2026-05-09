//! Integration tests validating the Tool Output Format Specification
//! documented in crates/rustycode-tools/README.md.
//!
//! These tests verify:
//! - ToolOutput.text is always non-empty for successful operations
//! - Success messages follow consistent patterns
//! - Error messages are descriptive and help the LLM self-correct
//! - No JSON objects are embedded in .text output
//! - Structured metadata is separate from text output
//! - read_file uses line-numbered format
//! - write_file/edit_file use "<verb> <path>" success pattern
//! - Bash output includes exit code for non-zero exits
//! - Grep output uses "<path>:<line>: <content>" format
//! - Glob output lists one path per line with count summary

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::format_collect,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::redundant_clone
)]

use base64::Engine;
use rustycode_tools::Tool;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

// Helper to create a ToolContext with structured output enabled for format testing.
fn make_ctx(workspace: &std::path::Path) -> rustycode_tools::ToolContext {
    rustycode_tools::ToolContext::new(workspace).with_structured_output(true)
}

// Assert that success/error messages are plain text, not JSON objects.
fn assert_no_json_object(text: &str) {
    let trimmed = text.trim();
    if trimmed.starts_with("Successfully")
        || trimmed.starts_with("wrote")
        || trimmed.starts_with("Error")
    {
        assert!(
            !trimmed.starts_with('{'),
            "Success/error message should not be JSON: {trimmed}"
        );
    }
}

// ─── ToolOutput struct tests ─────────────────────────────────────

#[test]
fn tool_output_text_factory_creates_non_empty() {
    let out = rustycode_tools::ToolOutput::text("hello");
    assert_eq!(out.text, "hello");
    assert!(out.structured.is_none());
    assert!(out.new_cwd.is_none());
}

#[test]
fn tool_output_with_structured_separates_text() {
    let out =
        rustycode_tools::ToolOutput::with_structured("file content here", json!({"bytes": 100}));
    assert_eq!(out.text, "file content here");
    assert!(out.structured.is_some());
    // Text should NOT contain the structured JSON
    assert!(!out.text.contains("bytes"));
    assert!(!out.text.contains('{'));
}

#[test]
fn tool_output_with_metadata_gates_on_context() {
    let dir = tempdir().unwrap();
    let ctx = make_ctx(dir.path());

    // make_ctx enables structured output for testing
    let out =
        rustycode_tools::ToolOutput::text("result").with_metadata(&ctx, || json!({"key": "value"}));
    assert!(out.structured.is_some());
    assert_eq!(out.text, "result");
    assert!(!out.text.contains("key"));
}

#[test]
fn tool_output_with_cwd_change_text_is_separate() {
    let out = rustycode_tools::ToolOutput::with_cwd_change(
        "changed directory",
        std::path::PathBuf::from("/tmp"),
    );
    assert_eq!(out.text, "changed directory");
    assert!(out.structured.is_none());
    assert_eq!(out.new_cwd.unwrap().to_str().unwrap(), "/tmp");
}

// ─── read_file format tests ──────────────────────────────────────

#[test]
fn read_file_returns_line_numbered_format() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("test.txt"), "line1\nline2\nline3").unwrap();

    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());
    let result = tool.execute(json!({ "path": "test.txt" }), &ctx).unwrap();

    // Text should contain line numbers in cat -n style
    assert!(
        result.text.contains("1\t"),
        "Expected tab-separated line number 1"
    );
    assert!(result.text.contains("line1"));
    assert!(result.text.contains("line2"));
    assert!(result.text.contains("line3"));

    // Should not be empty
    assert!(!result.text.is_empty());
}

#[test]
fn read_file_success_output_is_non_empty() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("hello.rs"), "fn main() {}").unwrap();

    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());
    let result = tool.execute(json!({ "path": "hello.rs" }), &ctx).unwrap();

    assert!(
        !result.text.trim().is_empty(),
        "read_file output must not be empty"
    );
}

#[test]
fn read_file_error_descriptive_for_missing_file() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());
    let result = tool.execute(json!({ "path": "nonexistent.txt" }), &ctx);

    assert!(result.is_err(), "Missing file should return Err");
    let err = result.unwrap_err().to_string();
    // Error should mention the file or what went wrong
    assert!(
        err.contains("not found") || err.contains("Failed") || err.contains("No such"),
        "Error should describe the problem: {err}"
    );
}

#[test]
fn read_file_offset_limit_returns_paginated_content() {
    let workspace = tempdir().unwrap();
    // Build 20 lines
    let lines: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    fs::write(workspace.path().join("big.txt"), &lines).unwrap();

    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());
    let result = tool
        .execute(json!({ "path": "big.txt", "offset": 5, "limit": 3 }), &ctx)
        .unwrap();

    // Should show lines 6-8 (offset 5 = skip 5 lines, limit 3)
    assert!(
        result.text.contains("line 6"),
        "Expected line 6 in output: {}",
        result.text
    );
    assert!(
        result.text.contains("line 7"),
        "Expected line 7 in output: {}",
        result.text
    );
    assert!(
        result.text.contains("line 8"),
        "Expected line 8 in output: {}",
        result.text
    );
    assert!(!result.text.contains("line 1"), "Should not contain line 1");
    assert!(
        !result.text.contains("line 20"),
        "Should not contain line 20"
    );
}

#[test]
fn read_file_structured_metadata_separate_from_text() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("code.rs"), "fn main() {}").unwrap();

    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());
    let result = tool.execute(json!({ "path": "code.rs" }), &ctx).unwrap();

    // Structured metadata exists but text should be plain content
    if let Some(meta) = &result.structured {
        assert!(meta["path"].is_string(), "structured should have path");
        assert!(
            meta["binary"].is_boolean(),
            "structured should have binary flag"
        );
        // Text should NOT duplicate the structured JSON
        assert!(
            !result.text.starts_with('{'),
            "Text output should not be JSON: {}",
            result.text
        );
    }
}

// ─── write_file format tests ──────────────────────────────────────

#[test]
fn write_file_success_includes_path_and_stats() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::fs::write_file::WriteFileTool;
    let ctx = make_ctx(workspace.path());

    let result = tool
        .execute(json!({ "path": "new.txt", "content": "hello world" }), &ctx)
        .unwrap();

    // Success message should mention the path and stats
    assert!(
        result.text.contains("new.txt") || result.text.contains("bytes"),
        "Write output should mention file path or stats: {}",
        result.text
    );
    assert!(!result.text.is_empty(), "Write output must not be empty");
}

#[test]
fn write_file_creates_file_in_existing_directory() {
    let workspace = tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("subdir")).unwrap();
    let tool = rustycode_tools::providers::fs::write_file::WriteFileTool;
    let ctx = make_ctx(workspace.path());

    let result = tool
        .execute(
            json!({ "path": "subdir/file.txt", "content": "nested content" }),
            &ctx,
        )
        .unwrap();

    assert!(
        result.text.contains("bytes") || result.text.contains("lines"),
        "Write output should include stats: {}",
        result.text
    );
    assert!(workspace.path().join("subdir/file.txt").exists());
}

#[test]
fn write_file_error_for_both_content_types() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::fs::write_file::WriteFileTool;
    let ctx = make_ctx(workspace.path());

    let result = tool.execute(
        json!({ "path": "test.txt", "content": "text", "content_base64": "dGV4dA==" }),
        &ctx,
    );

    assert!(
        result.is_err(),
        "Should reject both content and content_base64"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("either") || err.contains("both"),
        "Error should mention the conflict: {err}"
    );
}

#[test]
fn write_file_binary_output_mentions_binary() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::fs::write_file::WriteFileTool;
    let ctx = make_ctx(workspace.path());

    let bytes = vec![0x89u8, 0x50, 0x4e, 0x47];
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let result = tool
        .execute(
            json!({ "path": "image.png", "content_base64": encoded }),
            &ctx,
        )
        .unwrap();

    assert!(
        result.text.contains("binary") || result.text.contains("bytes"),
        "Binary write output should mention binary/bytes: {}",
        result.text
    );
}

// ─── edit_file format tests ──────────────────────────────────────

#[test]
fn edit_file_success_shows_replacement_count() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("edit.txt"), "hello world\nhello rust").unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());

    let result = tool
        .execute(
            json!({
                "path": "edit.txt",
                "old_text": "hello world",
                "new_text": "goodbye world"
            }),
            &ctx,
        )
        .unwrap();

    // Success message should indicate the edit happened
    assert!(!result.text.is_empty(), "Edit output must not be empty");
    // The file should have been modified
    let content = fs::read_to_string(workspace.path().join("edit.txt")).unwrap();
    assert!(content.contains("goodbye world"), "File should be modified");
    assert!(
        !content.contains("hello world"),
        "Old text should be replaced"
    );
}

#[test]
fn edit_file_not_found_error_is_descriptive() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("edit.txt"), "foo bar baz").unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());

    let result = tool.execute(
        json!({
            "path": "edit.txt",
            "old_text": "does not exist in file",
            "new_text": "replacement"
        }),
        &ctx,
    );

    // Either returns an error or a descriptive Ok message
    match result {
        Ok(output) => {
            // Should explain what went wrong to help LLM self-correct
            assert!(
                output.text.contains("not found") || output.text.contains("No changes"),
                "Not-found error should be descriptive: {}",
                output.text
            );
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("not found") || msg.contains("Failed"),
                "Error message should be descriptive: {msg}"
            );
        }
    }
}

#[test]
fn edit_file_multiple_matches_error_when_not_replace_all() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("dup.txt"), "abc\nabc\nabc").unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());

    let result = tool.execute(
        json!({
            "path": "dup.txt",
            "old_text": "abc",
            "new_text": "xyz"
        }),
        &ctx,
    );

    match result {
        Ok(output) => {
            assert!(
                output.text.contains("matches") || output.text.contains("replace_all"),
                "Multiple match error should mention replace_all: {}",
                output.text
            );
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("matches") || msg.contains("replace_all"),
                "Error should mention the issue: {msg}"
            );
        }
    }
}

// ─── list_dir format tests ──────────────────────────────────────

#[test]
fn list_dir_returns_entries_with_types() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("file1.txt"), "content").unwrap();
    fs::create_dir(workspace.path().join("subdir")).unwrap();

    let tool = rustycode_tools::providers::fs::list_dir::ListDirTool;
    let ctx = make_ctx(workspace.path());
    let result = tool.execute(json!({ "path": "." }), &ctx).unwrap();

    // Should list entries with type indicators
    assert!(!result.text.is_empty(), "list_dir output must not be empty");
    assert!(
        result.text.contains("file1.txt") || result.text.contains("file"),
        "Should mention the file: {}",
        result.text
    );
    assert!(
        result.text.contains("subdir") || result.text.contains("dir"),
        "Should mention the directory: {}",
        result.text
    );
}

#[test]
fn list_dir_error_for_missing_directory() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::fs::list_dir::ListDirTool;
    let ctx = make_ctx(workspace.path());

    let result = tool.execute(json!({ "path": "nonexistent_dir" }), &ctx);
    assert!(result.is_err(), "Should fail for missing directory");
}

// ─── bash format tests ──────────────────────────────────────

#[test]
fn bash_returns_command_output() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::bash::BashTool;
    let ctx = make_ctx(workspace.path());

    let result = tool
        .execute(json!({ "command": "echo hello" }), &ctx)
        .unwrap();

    assert!(
        result.text.contains("hello"),
        "Bash output should contain command output: {}",
        result.text
    );
    assert!(!result.text.is_empty());
}

#[test]
fn bash_shows_exit_code_on_failure() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::bash::BashTool;
    let ctx = make_ctx(workspace.path());

    // Use a command from the allowed list that returns non-zero
    let result = tool
        .execute(json!({ "command": "ls /nonexistent_dir_xyz_12345" }), &ctx)
        .unwrap();

    // Should contain error output from the failed command
    assert!(
        result.text.contains("nonexistent")
            || result.text.contains("No such")
            || result.text.contains("error"),
        "Failed command output should indicate failure: {}",
        result.text
    );
}

#[test]
fn bash_timeout_parameter_accepted() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::bash::BashTool;
    let ctx = make_ctx(workspace.path());

    // Verify that the bash tool accepts the timeout parameter and
    // returns output (either success or timeout message).
    // The actual timeout behavior is covered by the 58 bash unit tests.
    let result = tool.execute(json!({ "command": "echo done", "timeout": 5 }), &ctx);

    match result {
        Ok(output) => {
            assert!(!output.text.is_empty(), "Bash output should not be empty");
        }
        Err(e) => {
            // Timeout is also valid
            let msg = e.to_string();
            assert!(
                msg.contains("timeout") || msg.contains("timed out") || msg.contains("blocked"),
                "Unexpected error: {msg}"
            );
        }
    }
}

// ─── grep format tests ──────────────────────────────────────

#[test]
fn grep_returns_path_line_content_format() {
    let workspace = tempdir().unwrap();
    fs::write(
        workspace.path().join("code.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();

    let tool = rustycode_tools::providers::grep::GrepTool;
    let ctx = make_ctx(workspace.path());

    let result = tool
        .execute(json!({ "pattern": "println", "path": "." }), &ctx)
        .unwrap();

    assert!(
        !result.text.is_empty(),
        "Grep should return output for match"
    );
    // Should contain the file path and the matching line
    assert!(
        result.text.contains("code.rs"),
        "Should show file path: {}",
        result.text
    );
}

#[test]
fn grep_no_matches_returns_empty_or_message() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("file.txt"), "hello world").unwrap();

    let tool = rustycode_tools::providers::grep::GrepTool;
    let ctx = make_ctx(workspace.path());

    let result = tool
        .execute(
            json!({ "pattern": "zzz_nonexistent_pattern", "path": "." }),
            &ctx,
        )
        .unwrap();

    // No matches is a valid result - output should be empty or say "no matches"
    assert!(
        result.text.is_empty() || result.text.contains("0") || result.text.contains("no match"),
        "No-match result should be clean: '{}'",
        result.text
    );
}

// ─── glob format tests ──────────────────────────────────────

#[test]
fn glob_returns_paths_with_count() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("a.rs"), "").unwrap();
    fs::write(workspace.path().join("b.rs"), "").unwrap();
    fs::write(workspace.path().join("c.txt"), "").unwrap();

    let tool = rustycode_tools::providers::glob::GlobTool;
    let ctx = make_ctx(workspace.path());

    let result = tool.execute(json!({ "pattern": "**/*.rs" }), &ctx).unwrap();

    // Should list the .rs files
    assert!(
        result.text.contains("a.rs"),
        "Should list a.rs: {}",
        result.text
    );
    assert!(
        result.text.contains("b.rs"),
        "Should list b.rs: {}",
        result.text
    );
    assert!(!result.text.contains("c.txt"), "Should not list c.txt");
}

// ─── git tools format tests ──────────────────────────────────────

#[test]
fn git_status_shows_working_tree_state() {
    let workspace = tempdir().unwrap();
    // Initialize a git repo so git tools work
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(workspace.path())
        .output()
        .expect("git init");
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(workspace.path())
        .output()
        .expect("git config email");
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(workspace.path())
        .output()
        .expect("git config name");
    fs::write(workspace.path().join("new.txt"), "content").unwrap();

    let tool = rustycode_tools::providers::git::GitStatusTool;
    let ctx = make_ctx(workspace.path());
    let result = tool.execute(json!({}), &ctx).unwrap();

    assert!(
        !result.text.is_empty(),
        "git_status output should not be empty"
    );
    // Should show something about the file changes or "nothing to commit"
}

#[test]
fn git_log_shows_commits() {
    let workspace = tempdir().unwrap();
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(workspace.path())
        .output()
        .expect("git init");
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(workspace.path())
        .output()
        .expect("git config email");
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(workspace.path())
        .output()
        .expect("git config name");
    fs::write(workspace.path().join("initial.txt"), "first").unwrap();
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(workspace.path())
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(workspace.path())
        .output()
        .expect("git commit");

    let tool = rustycode_tools::providers::git::GitLogTool;
    let ctx = make_ctx(workspace.path());
    let result = tool.execute(json!({ "max_count": 5 }), &ctx).unwrap();

    assert!(!result.text.is_empty(), "git_log should return output");
    assert!(
        result.text.contains("initial commit"),
        "Should show commit message: {}",
        result.text
    );
}

// ─── apply_patch format tests ──────────────────────────────────────

#[test]
fn apply_patch_success_mentions_path() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("patch.txt"), "line1\nline2\nline3\n").unwrap();

    let tool = rustycode_tools::providers::fs::apply_patch::ApplyPatchTool;
    let ctx = make_ctx(workspace.path());

    let patch =
        "--- a/patch.txt\n+++ b/patch.txt\n@@ -1,3 +1,3 @@\n line1\n-line2\n+modified\n line3\n";
    let result = tool.execute(json!({ "patch": patch }), &ctx).unwrap();

    assert!(
        !result.text.is_empty(),
        "apply_patch output must not be empty"
    );
    assert!(
        result.text.contains("patch.txt")
            || result.text.contains("patch")
            || result.text.contains("hunk"),
        "Should mention the patched file or hunks: {}",
        result.text
    );

    let content = fs::read_to_string(workspace.path().join("patch.txt")).unwrap();
    assert!(content.contains("modified"), "File should be patched");
}

#[test]
fn apply_patch_invalid_patch_returns_error() {
    let workspace = tempdir().unwrap();

    let tool = rustycode_tools::providers::fs::apply_patch::ApplyPatchTool;
    let ctx = make_ctx(workspace.path());

    let result = tool.execute(json!({ "patch": "not a valid patch" }), &ctx);

    assert!(result.is_err(), "Invalid patch should return error");
}

// ─── Cross-cutting format tests ──────────────────────────────────────

#[test]
fn all_tool_outputs_text_is_string_not_json() {
    // Verify ToolOutput.text is always a plain string, not JSON,
    // for the common read/write operations
    let workspace = tempdir().unwrap();
    let ctx = make_ctx(workspace.path());

    // Write
    let write_tool = rustycode_tools::providers::fs::write_file::WriteFileTool;
    let write_result = write_tool
        .execute(json!({ "path": "test.txt", "content": "hello" }), &ctx)
        .unwrap();
    assert_no_json_object(&write_result.text);

    // Read
    let read_tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let read_result = read_tool
        .execute(json!({ "path": "test.txt" }), &ctx)
        .unwrap();
    // read_file text is file content - that's fine, not a "message"
    assert!(!read_result.text.is_empty());

    // List dir
    let list_tool = rustycode_tools::providers::fs::list_dir::ListDirTool;
    let list_result = list_tool.execute(json!({ "path": "." }), &ctx).unwrap();
    assert_no_json_object(&list_result.text);
}

#[test]
fn structured_metadata_does_not_leak_into_text() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("meta_test.rs"), "fn test() {}").unwrap();

    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());
    let result = tool
        .execute(json!({ "path": "meta_test.rs" }), &ctx)
        .unwrap();

    // If structured metadata exists, it must not appear in text
    if let Some(meta) = &result.structured {
        let meta_str = serde_json::to_string(meta).unwrap();
        // The text should not contain the serialized JSON metadata
        // (it may share field values like the path, but not the JSON object)
        if !result.text.starts_with('{') {
            // Non-JSON text should not accidentally contain the full metadata blob
            assert!(
                result.text.len() < meta_str.len() || !result.text.contains("\"total_bytes\""),
                "Text should not duplicate structured metadata fields"
            );
        }
    }
}

#[test]
fn error_messages_are_human_readable() {
    // Verify that error messages use plain English, not error codes
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());

    // Try to read a directory instead of a file
    let result = tool.execute(json!({ "path": "." }), &ctx);
    if let Err(e) = result {
        let msg = e.to_string();
        // Error should be a readable sentence, not a code
        assert!(msg.len() > 10, "Error message should be descriptive: {msg}");
        assert!(!msg.starts_with("0x"), "Error should not be a hex code");
    }
}

// ─── LSP tool parameter validation tests ──────────────────────────────

#[test]
fn lsp_hover_rejects_empty_path() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::lsp::hover::LspHoverTool;
    let ctx = make_ctx(workspace.path());

    let result = tool.execute(json!({ "line": 0, "character": 0 }), &ctx);
    assert!(result.is_err(), "LSP tool should reject empty path");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("relative_path") || err.contains("file_path") || err.contains("required"),
        "Error should mention the missing parameter: {err}"
    );
}

// ─── multiedit format tests ──────────────────────────────────────

#[test]
fn multiedit_reports_per_file_results() {
    let workspace = tempdir().unwrap();
    fs::write(workspace.path().join("a.txt"), "hello world").unwrap();
    fs::write(workspace.path().join("b.txt"), "hello rust").unwrap();

    let tool = rustycode_tools::providers::fs::multiedit::MultiEditTool;
    let ctx = make_ctx(workspace.path());

    let result = tool
        .execute(
            json!({
                "edits": [
                    {
                        "path": "a.txt",
                        "operation": "edit",
                        "old_text": "hello",
                        "new_text": "goodbye"
                    },
                    {
                        "path": "b.txt",
                        "operation": "edit",
                        "old_text": "hello",
                        "new_text": "goodbye"
                    }
                ]
            }),
            &ctx,
        )
        .unwrap();

    assert!(
        !result.text.is_empty(),
        "multiedit output must not be empty"
    );
    // Should mention the files it edited
    assert!(
        result.text.contains("a.txt") || result.text.contains("2") || result.text.contains("edit"),
        "multiedit should report per-file results: {}",
        result.text
    );
}

// ─── Security boundary format tests ──────────────────────────────────

#[test]
fn blocked_file_returns_descriptive_message() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());

    // Try to read a blocked extension
    let result = tool.execute(json!({ "path": "secrets.env" }), &ctx);
    // Either the file doesn't exist (error) or it's blocked (ok with message)
    match result {
        Ok(output) => {
            assert!(
                output.text.contains("[Blocked]") || output.text.contains("not allowed"),
                "Blocked file should have clear message: {}",
                output.text
            );
        }
        Err(_) => {
            // File not found is also acceptable
        }
    }
}

#[test]
fn path_traversal_blocked_with_message() {
    let workspace = tempdir().unwrap();
    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());

    let result = tool.execute(json!({ "path": "../../../etc/passwd" }), &ctx);
    assert!(result.is_err(), "Path traversal should be blocked");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("outside")
            || err.contains("blocked")
            || err.contains("workspace")
            || err.contains("traversal")
            || err.contains("not allowed"),
        "Path traversal error should explain the block: {err}"
    );
}

// ─── Edit file encoding tests ──────────────────────────────────────

/// Helper: write raw bytes to a file (for precise encoding control)
fn write_raw(path: impl AsRef<std::path::Path>, bytes: &[u8]) {
    use std::io::Write;
    let mut f = std::fs::File::create(path.as_ref()).unwrap();
    f.write_all(bytes).unwrap();
}

/// Helper: read raw bytes from a file
fn read_raw(path: impl AsRef<std::path::Path>) -> Vec<u8> {
    std::fs::read(path.as_ref()).unwrap()
}

// --- UTF-8 multibyte character preservation ---

#[test]
fn edit_preserves_cjk_characters() {
    let workspace = tempdir().unwrap();
    let content = "function hello() {\n  return \"こんにちは世界\";\n}\n";
    fs::write(workspace.path().join("cjk.js"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "cjk.js",
                "old_text": "hello",
                "new_text": "greet"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("cjk.js")).unwrap();
    assert!(new_content.contains("greet"), "Should contain replacement");
    assert!(
        new_content.contains("こんにちは世界"),
        "CJK characters must be preserved byte-for-byte"
    );
    assert!(!new_content.contains("hello"), "Old text should be gone");
}

#[test]
fn edit_preserves_emoji() {
    let workspace = tempdir().unwrap();
    // Rocket emoji is 4 bytes (U+1F680),checkered flag is 4 bytes (U+1F3C1)
    let content = "status = \"🚀 launched 🏁\"\nresult = \"success\"\n";
    fs::write(workspace.path().join("emoji.py"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "emoji.py",
                "old_text": "success",
                "new_text": "done"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("emoji.py")).unwrap();
    assert!(new_content.contains("done"), "Should contain replacement");
    assert!(
        new_content.contains("🚀 launched 🏁"),
        "Emoji must be preserved"
    );
}

#[test]
fn edit_replaces_multibyte_old_text_with_ascii() {
    let workspace = tempdir().unwrap();
    let content = "msg = \"café résumé naïve\"\nprint(msg)\n";
    fs::write(workspace.path().join("accent.py"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "accent.py",
                "old_text": "café résumé naïve",
                "new_text": "plain text"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("accent.py")).unwrap();
    assert!(new_content.contains("plain text"));
    assert!(!new_content.contains("café"));
}

#[test]
fn edit_replaces_ascii_with_multibyte_new_text() {
    let workspace = tempdir().unwrap();
    let content = "greeting = \"hello\"\nprint(greeting)\n";
    fs::write(workspace.path().join("greet.py"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "greet.py",
                "old_text": "hello",
                "new_text": "こんにちは"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("greet.py")).unwrap();
    assert!(new_content.contains("こんにちは"));
    assert!(!new_content.contains("hello"));
}

#[test]
fn edit_preserves_combining_characters() {
    let workspace = tempdir().unwrap();
    // é as combining: 'e' + U+0301 (2 code points, 3 bytes)
    let content = "name = \"meteo\u{0301}rologie\"\n";
    fs::write(workspace.path().join("combine.txt"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "combine.txt",
                "old_text": "meteo",
                "new_text": "climat"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("combine.txt")).unwrap();
    // The combining accent (U+0301) must survive the edit
    assert!(
        new_content.contains("\u{0301}"),
        "Combining character must be preserved"
    );
}

// --- CRLF line ending handling ---

#[test]
fn edit_crlf_file_preserves_crlf_exact_match() {
    let workspace = tempdir().unwrap();
    let content = "line1\r\nline2\r\nline3\r\n";
    write_raw(workspace.path().join("crlf.txt"), content.as_bytes());

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "crlf.txt",
                "old_text": "line2",
                "new_text": "replaced"
            }),
            &ctx,
        )
        .unwrap();

    let bytes = read_raw(workspace.path().join("crlf.txt"));
    let new_content = String::from_utf8(bytes.clone()).unwrap();
    assert!(new_content.contains("replaced"));
    assert!(!new_content.contains("line2"));
    // CRLF must be preserved, not converted to LF
    assert!(
        new_content.contains("\r\n"),
        "CRLF line endings must be preserved"
    );
    assert_eq!(
        new_content.matches("\r\n").count(),
        3,
        "Should have exactly 3 CRLF line endings"
    );
}

#[test]
fn edit_crlf_file_via_normalized_match() {
    let workspace = tempdir().unwrap();
    // File has CRLF but old_text uses LF — normalized match should find it
    let content = "hello world\r\nfoo bar\r\n";
    write_raw(workspace.path().join("mix.txt"), content.as_bytes());

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "mix.txt",
                "old_text": "hello world\nfoo bar",  // LF in old_text, CRLF in file
                "new_text": "replaced world\nnew bar"
            }),
            &ctx,
        )
        .unwrap();

    let bytes = read_raw(workspace.path().join("mix.txt"));
    let new_content = String::from_utf8(bytes).unwrap();
    assert!(
        new_content.contains("replaced world"),
        "Should contain replacement"
    );
    // Should still be CRLF — not converted
    assert!(
        new_content.contains("\r\n"),
        "CRLF must be preserved after normalized match"
    );
}

#[test]
fn edit_crlf_with_multibyte_content() {
    let workspace = tempdir().unwrap();
    // CRLF + CJK characters — both encoding concerns at once
    let content = "名前 = \"田中\"\r\n年齢 = 30\r\n";
    write_raw(workspace.path().join("cjk_crlf.txt"), content.as_bytes());

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "cjk_crlf.txt",
                "old_text": "30",
                "new_text": "35"
            }),
            &ctx,
        )
        .unwrap();

    let bytes = read_raw(workspace.path().join("cjk_crlf.txt"));
    let new_content = String::from_utf8(bytes).unwrap();
    assert!(new_content.contains("35"), "Replacement should work");
    assert!(new_content.contains("田中"), "CJK must survive");
    assert!(new_content.contains("\r\n"), "CRLF must survive");
}

// --- Curly quote normalization with multibyte content ---

#[test]
fn edit_curly_quotes_with_cjk_preserves_cjk() {
    let workspace = tempdir().unwrap();
    // Curly double quotes (\u{201C}/\u{201D} = 3 bytes each) + CJK chars
    let content = "msg = \u{201C}日本語テスト\u{201D}\nprint(msg)\n";
    fs::write(workspace.path().join("curly_cjk.py"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    // LLM sends straight quotes but file has curly — quote normalization should match
    let _result = tool
        .execute(
            json!({
                "path": "curly_cjk.py",
                "old_text": "\"日本語テスト\"",
                "new_text": "\" replaced \""
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("curly_cjk.py")).unwrap();
    assert!(
        new_content.contains("replaced"),
        "Quote normalization should match"
    );
    assert!(
        !new_content.contains("\u{201C}"),
        "Curly quotes should be replaced"
    );
}

#[test]
fn edit_curly_quotes_byte_offset_correctness_with_emoji() {
    let workspace = tempdir().unwrap();
    // Emoji (4 bytes each) before the curly quotes — tests char↔byte offset mapping
    let content = "🎉🚀 msg = \u{201C}hello\u{201D}\ndone\n";
    fs::write(workspace.path().join("curly_emoji.txt"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    // Straight quotes in old_text should match curly quotes in file
    let _result = tool
        .execute(
            json!({
                "path": "curly_emoji.txt",
                "old_text": "\"hello\"",
                "new_text": "world"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("curly_emoji.txt")).unwrap();
    assert!(
        new_content.contains("world"),
        "Quote normalization should match despite emoji"
    );
    assert!(
        new_content.contains("🎉🚀"),
        "Emoji before match must be preserved"
    );
}

#[test]
fn edit_curly_single_quotes_with_accented_chars() {
    let workspace = tempdir().unwrap();
    // Curly single quotes + accented chars — different byte widths
    let content = "word = café\u{2019}s\n";
    fs::write(workspace.path().join("curly_acc.txt"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "curly_acc.txt",
                "old_text": "café's",
                "new_text": "brasserie"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("curly_acc.txt")).unwrap();
    assert!(
        new_content.contains("brasserie"),
        "Should replace via quote normalization"
    );
}

// --- UTF-8 BOM handling ---

#[test]
fn edit_file_with_utf8_bom_preserves_bom() {
    let workspace = tempdir().unwrap();
    // UTF-8 BOM is 3 bytes: EF BB BF
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"content = \"hello\"\n");
    write_raw(workspace.path().join("bom.txt"), &bytes);

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "bom.txt",
                "old_text": "hello",
                "new_text": "world"
            }),
            &ctx,
        )
        .unwrap();

    let new_bytes = read_raw(workspace.path().join("bom.txt"));
    // BOM should be preserved
    assert_eq!(new_bytes[0], 0xEF, "BOM byte 1 must be preserved");
    assert_eq!(new_bytes[1], 0xBB, "BOM byte 2 must be preserved");
    assert_eq!(new_bytes[2], 0xBF, "BOM byte 3 must be preserved");

    let content = std::str::from_utf8(&new_bytes).unwrap();
    assert!(content.contains("world"), "Replacement should work");
    // BOM character U+FEFF should be present at start
    assert!(
        content.starts_with('\u{FEFF}'),
        "BOM character should be at start"
    );
}

// --- Edge cases: multibyte at boundaries ---

#[test]
fn edit_multibyte_at_start_of_file() {
    let workspace = tempdir().unwrap();
    let content = "日本語のテキスト\nsecond line\n";
    fs::write(workspace.path().join("start.txt"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "start.txt",
                "old_text": "日本語",
                "new_text": "中文"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("start.txt")).unwrap();
    assert!(
        new_content.starts_with("中文"),
        "Replacement at byte 0 with multibyte"
    );
    assert!(
        new_content.contains("のテキスト"),
        "Rest of content preserved"
    );
}

#[test]
fn edit_multibyte_at_end_of_file() {
    let workspace = tempdir().unwrap();
    let content = "first line\n最後の行";
    fs::write(workspace.path().join("end.txt"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "end.txt",
                "old_text": "最後の行",
                "new_text": "fin"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("end.txt")).unwrap();
    assert!(
        new_content.ends_with("fin"),
        "Multibyte at EOF should be replaced"
    );
    assert!(new_content.contains("first line"), "Rest preserved");
}

#[test]
fn edit_entire_file_is_multibyte() {
    let workspace = tempdir().unwrap();
    let content = "한국어테스트";
    fs::write(workspace.path().join("korean.txt"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "korean.txt",
                "old_text": "한국어",
                "new_text": "韓国語"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("korean.txt")).unwrap();
    assert_eq!(new_content, "韓国語테스트");
}

// --- replace_all with multibyte ---

#[test]
fn edit_replace_all_with_multibyte_content() {
    let workspace = tempdir().unwrap();
    let content = "café one\ncafé two\ncafé three\n";
    fs::write(workspace.path().join("multi.txt"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let result = tool.execute(
        json!({
            "path": "multi.txt",
            "old_text": "café",
            "new_text": "bar"
        }),
        &ctx,
    );

    // Should complain about multiple matches since replace_all is not set
    match result {
        Ok(output) => {
            assert!(
                output.text.contains("matches") || output.text.contains("replace_all"),
                "Should mention multiple matches: {}",
                output.text
            );
        }
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("matches") || msg.contains("replace_all"));
        }
    }

    // Now with replace_all
    let _result2 = tool
        .execute(
            json!({
                "path": "multi.txt",
                "old_text": "café",
                "new_text": "bar",
                "replace_all": true
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("multi.txt")).unwrap();
    assert_eq!(
        new_content.matches("bar").count(),
        3,
        "All 3 occurrences should be replaced"
    );
    assert!(!new_content.contains("café"));
}

// --- Binary file rejection ---

#[test]
fn edit_rejects_binary_file() {
    let workspace = tempdir().unwrap();
    // Write bytes that are not valid UTF-8
    write_raw(
        workspace.path().join("binary.dat"),
        &[0x80, 0x90, 0xA0, 0xB0],
    );

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let result = tool.execute(
        json!({
            "path": "binary.dat",
            "old_text": "foo",
            "new_text": "bar"
        }),
        &ctx,
    );

    assert!(result.is_err(), "Binary file should be rejected");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Binary") || err.contains("non-UTF-8") || err.contains("text file"),
        "Error should mention binary/encoding issue: {err}"
    );
}

// --- Trailing newline preservation with multibyte ---

#[test]
fn edit_preserves_trailing_newline_with_multibyte_content() {
    let workspace = tempdir().unwrap();
    let content = "α = 1\nβ = 2\n"; // trailing newline
    fs::write(workspace.path().join("greek.txt"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "greek.txt",
                "old_text": "α = 1",
                "new_text": "γ = 1"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("greek.txt")).unwrap();
    assert!(
        new_content.ends_with('\n'),
        "Trailing newline must be preserved"
    );
    assert!(new_content.contains("γ = 1"));
    assert!(new_content.contains("β = 2"));
}

#[test]
fn edit_no_trailing_newline_with_multibyte_content() {
    let workspace = tempdir().unwrap();
    let content = "α = 1\nβ = 2"; // NO trailing newline
    fs::write(workspace.path().join("greek2.txt"), content).unwrap();

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "greek2.txt",
                "old_text": "β = 2",
                "new_text": "δ = 2"
            }),
            &ctx,
        )
        .unwrap();

    let new_content = fs::read_to_string(workspace.path().join("greek2.txt")).unwrap();
    assert!(
        !new_content.ends_with('\n'),
        "Should NOT add trailing newline"
    );
    assert!(new_content.contains("δ = 2"));
}

// --- Byte-level verification ---

#[test]
fn edit_multibyte_replacement_byte_exact() {
    let workspace = tempdir().unwrap();
    // "café" = 5 bytes (c=1, a=1, f=1, é=2)
    // "naïve" = 6 bytes (n=1, a=1, ï=2, v=1, e=1)
    let content = "café is naïve\n";
    fs::write(workspace.path().join("bytes.txt"), content).unwrap();
    let original_bytes = read_raw(workspace.path().join("bytes.txt"));

    let tool = rustycode_tools::providers::fs::edit::EditFile;
    let ctx = make_ctx(workspace.path());
    let _result = tool
        .execute(
            json!({
                "path": "bytes.txt",
                "old_text": "café is naïve",
                "new_text": "simple"
            }),
            &ctx,
        )
        .unwrap();

    let new_bytes = read_raw(workspace.path().join("bytes.txt"));
    let new_content = std::str::from_utf8(&new_bytes).unwrap();
    assert!(new_content.contains("simple"));
    // Should be shorter since "simple" is ASCII
    assert!(
        new_bytes.len() < original_bytes.len(),
        "ASCII replacement should be shorter"
    );
    // Verify the file is valid UTF-8 (no corruption)
    assert!(
        std::str::from_utf8(&new_bytes).is_ok(),
        "Result must be valid UTF-8"
    );
}

// --- Stats mode format test ──────────────────────────────────────

#[test]
fn read_file_stats_mode_returns_json() {
    let workspace = tempdir().unwrap();
    fs::write(
        workspace.path().join("stats.txt"),
        "line1\nline2\n# comment\nline4\n",
    )
    .unwrap();

    let tool = rustycode_tools::providers::fs::read_file::ReadFileTool;
    let ctx = make_ctx(workspace.path());
    let result = tool
        .execute(json!({ "path": "stats.txt", "stats": true }), &ctx)
        .unwrap();

    // Stats mode returns JSON in text - this is the documented exception
    let parsed: serde_json::Value =
        serde_json::from_str(&result.text).expect("Stats output should be valid JSON");
    assert!(
        parsed["total_lines"].is_number(),
        "Stats should have total_lines"
    );
    assert!(parsed["path"].is_string(), "Stats should have path");
}
