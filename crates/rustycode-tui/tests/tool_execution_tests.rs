#![allow(
    clippy::bool_to_int_with_if,
    clippy::branches_sharing_code,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::collapsible_else_if,
    clippy::collection_is_never_read,
    clippy::default_trait_access,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::ignore_without_reason,
    clippy::ignored_unit_patterns,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::items_after_statements,
    clippy::iter_on_single_items,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::option_if_let_else,
    clippy::range_plus_one,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::ref_option,
    clippy::search_is_some,
    clippy::semicolon_if_nothing_returned,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_peekable,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::used_underscore_binding,
    clippy::useless_let_if_seq
)]

//! Comprehensive tests for tool execution functionality
//! Tests tool parsing, execution, sanitization, and error handling

use rustycode_protocol::ToolResult;
use rustycode_tui::auto_tool_parser;
use rustycode_tui::tool_helpers;
use std::sync::mpsc::channel;

// Section 1: Tool Call Extraction Tests

#[test]
fn test_extract_inline_tool_json() {
    let response = r#"Some text before
{"calls":[{"name":"bash","arguments":{"command":"ls"}}]}
Some text after"#;

    let payloads = auto_tool_parser::extract_tool_payloads(response);
    assert_eq!(payloads.len(), 1, "Should extract inline tool JSON");
    assert!(
        payloads[0].contains("\"name\":\"bash\""),
        "Should contain tool name"
    );
}

#[test]
fn test_extract_fenced_tool_blocks() {
    let response = r#"Some text before
```tool
{"name":"read_file","arguments":{"path":"test.rs"}}
```
Some text after"#;

    let payloads = auto_tool_parser::extract_tool_payloads(response);
    assert_eq!(payloads.len(), 1, "Should extract fenced tool blocks");
    assert!(
        payloads[0].contains("read_file"),
        "Should contain tool name"
    );
}

#[test]
fn test_extract_multiple_inline_tools() {
    let response = r#"{"calls":[{"name":"bash","arguments":{"command":"ls"}}]}
{"calls":[{"name":"bash","arguments":{"command":"pwd"}}]}"#;

    let payloads = auto_tool_parser::extract_tool_payloads(response);
    assert_eq!(
        payloads.len(),
        2,
        "Should extract multiple inline tool JSON blocks"
    );
}

#[test]
fn test_extract_mixed_tools() {
    let response = r#"Text
```tool
{"name":"bash","arguments":{"command":"ls"}}
```
More text
{"calls":[{"name":"bash","arguments":{"command":"pwd"}}]}
End"#;

    let payloads = auto_tool_parser::extract_tool_payloads(response);
    assert_eq!(
        payloads.len(),
        2,
        "Should extract both fenced and inline tools"
    );
}

#[test]
fn test_ignore_tool_json_in_code_blocks() {
    let response = r#"Here's a code example:
```json
{"calls":[{"name":"bash","arguments":{"command":"ls"}}]}
```
This should not be extracted"#;

    let payloads = auto_tool_parser::extract_tool_payloads(response);
    assert_eq!(
        payloads.len(),
        0,
        "Should ignore tool JSON inside regular code blocks"
    );
}

// Section 2: Tool Call Parsing Tests

#[test]
fn test_parse_single_tool_call() {
    let payload = r#"{"calls":[{"name":"bash","arguments":{"command":"ls -la"}}]}"#;

    let calls =
        auto_tool_parser::parse_tool_calls_payload(payload).expect("Should parse tool calls");

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "bash");
    assert_eq!(calls[0].arguments["command"], "ls -la");
}

#[test]
fn test_parse_multiple_tool_calls() {
    let payload = r#"{"calls":[
        {"name":"bash","arguments":{"command":"ls"}},
        {"name":"bash","arguments":{"command":"pwd"}}
    ]}"#;

    let calls = auto_tool_parser::parse_tool_calls_payload(payload)
        .expect("Should parse multiple tool calls");

    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "bash");
    assert_eq!(calls[1].name, "bash");
}

#[test]
fn test_parse_tool_with_string_arguments() {
    let payload = r#"{"calls":[{"name":"write_file","arguments":{"path":"test.txt","content":"Hello World"}}]}"#;

    let calls = auto_tool_parser::parse_tool_calls_payload(payload)
        .expect("Should parse tool with string arguments");

    assert_eq!(calls[0].arguments["path"], "test.txt");
    assert_eq!(calls[0].arguments["content"], "Hello World");
}

#[test]
fn test_parse_invalid_tool_json() {
    let payload = "{\"calls\":[{\"name\":\"bash\",\"arguments\":{\"command\":\"#}}";

    let result = auto_tool_parser::parse_tool_calls_payload(payload);
    assert!(result.is_err(), "Should fail on invalid JSON");
}

// Section 3: Command Sanitization Tests

#[test]
fn test_sanitize_simple_safe_command() {
    let result = tool_helpers::sanitize_command("ls");
    assert!(result.is_ok(), "Simple ls should be allowed");
}

#[test]
fn test_sanitize_command_with_flags() {
    let result = tool_helpers::sanitize_command("ls -la");
    assert!(result.is_ok(), "ls with flags should be allowed");
}

#[test]
fn test_sanitize_command_with_path() {
    let result = tool_helpers::sanitize_command("ls /tmp");
    assert!(result.is_ok(), "ls with path should be allowed");
}

#[test]
fn test_sanitize_pipe_command() {
    let result = tool_helpers::sanitize_command("ls | head -10");
    assert!(result.is_ok(), "Piped command should be allowed");
}

#[test]
fn test_sanitize_multiple_pipes() {
    let result = tool_helpers::sanitize_command("cat file.txt | grep test | sort | uniq");
    assert!(result.is_ok(), "Multiple pipes should be allowed");
}

#[test]
fn test_sanitize_with_output_redirect() {
    let result = tool_helpers::sanitize_command("ls > output.txt");
    assert!(result.is_ok(), "Output redirect should be allowed");
}

#[test]
fn test_sanitize_block_command_separator() {
    let result = tool_helpers::sanitize_command("ls; rm -rf /");
    assert!(result.is_err(), "Command separator should be blocked");
}

#[test]
fn test_sanitize_block_background_operator() {
    let result = tool_helpers::sanitize_command("ls & echo done");
    assert!(result.is_err(), "Background operator should be blocked");
}

#[test]
fn test_sanitize_block_command_substitution() {
    let result = tool_helpers::sanitize_command("ls $(whoami)");
    assert!(result.is_err(), "Command substitution should be blocked");
}

#[test]
fn test_sanitize_block_backtick_substitution() {
    let result = tool_helpers::sanitize_command("ls `whoami`");
    assert!(result.is_err(), "Backtick substitution should be blocked");
}

#[test]
fn test_sanitize_block_dangerous_git_command() {
    let result = tool_helpers::sanitize_command("git reset --hard");
    assert!(result.is_err(), "Dangerous git commands should be blocked");
}

#[test]
fn test_sanitize_safe_git_command() {
    let result = tool_helpers::sanitize_command("git status");
    assert!(result.is_ok(), "Safe git commands should be allowed");
}

#[test]
fn test_sanitize_block_unsafe_pipeline_command() {
    let result = tool_helpers::sanitize_command("ls | rm -rf /");
    assert!(
        result.is_err(),
        "Pipeline with unsafe command should be blocked"
    );
}

#[test]
fn test_sanitize_empty_command() {
    let result = tool_helpers::sanitize_command("");
    assert!(result.is_err(), "Empty command should be blocked");
}

// Section 4: Auto-Execute Tests

#[test]
fn test_should_auto_execute_ls() {
    assert!(
        tool_helpers::should_auto_execute("ls"),
        "bare 'ls' should match"
    );
    assert!(
        tool_helpers::should_auto_execute("ls -la"),
        "ls with flags should match"
    );
    assert!(
        tool_helpers::should_auto_execute("ls /tmp"),
        "ls with path should match"
    );
}

#[test]
fn test_should_auto_execute_git_commands() {
    assert!(
        tool_helpers::should_auto_execute("git status"),
        "git status should match"
    );
    assert!(
        tool_helpers::should_auto_execute("git log"),
        "git log should match"
    );
    assert!(
        tool_helpers::should_auto_execute("git diff"),
        "git diff should match"
    );
}

#[test]
fn test_should_not_auto_execute_dangerous_commands() {
    assert!(
        !tool_helpers::should_auto_execute("rm -rf ."),
        "rm should not match"
    );
    assert!(
        !tool_helpers::should_auto_execute("vi file.txt"),
        "vi should not match"
    );
}

// Section 5: Tool Result Formatting Tests

#[test]
fn test_format_tool_result_success() {
    let result = ToolResult {
        call_id: "test-1".to_string(),
        success: true,
        output: "file1.txt\nfile2.txt\nfile3.txt".to_string(),
        error: None,
        exit_code: None,
        data: None,
        new_cwd: None,
    };

    let summary = tool_helpers::format_tool_result_summary(&result, "bash");

    assert!(summary.contains("success=true"), "Should indicate success");
    // Small outputs (<2000 chars and <50 lines) now include full output instead of metadata
    assert!(summary.contains("file1.txt"), "Should show full output");
    assert!(summary.contains("file2.txt"), "Should show full output");
    assert!(summary.contains("file3.txt"), "Should show full output");
}

#[test]
fn test_format_tool_result_failure() {
    let result = ToolResult {
        call_id: "test-2".to_string(),
        success: false,
        output: "".to_string(),
        error: Some("Command not found".to_string()),
        exit_code: None,
        data: None,
        new_cwd: None,
    };

    let summary = tool_helpers::format_tool_result_summary(&result, "bash");

    assert!(summary.contains("success=false"), "Should indicate failure");
    assert!(
        summary.contains("Command not found"),
        "Should show error message"
    );
}

#[test]
fn test_format_tool_result_with_structured() {
    let structured = serde_json::json!({"exit_code": 0, "signal": null});
    let result = ToolResult {
        call_id: "test-3".to_string(),
        success: true,
        output: "done".to_string(),
        error: None,
        exit_code: None,
        data: Some(structured),
        new_cwd: None,
    };

    let summary = tool_helpers::format_tool_result_summary(&result, "bash");

    assert!(
        summary.contains("success=true"),
        "Should include success result"
    );
}

#[test]
fn test_format_long_output_truncation() {
    // Threshold is 2000 chars, so we need more than that to trigger truncation
    let long_output = "a".repeat(2001);
    let result = ToolResult {
        call_id: "test-4".to_string(),
        success: true,
        output: long_output.clone(),
        error: None,
        exit_code: None,
        data: None,
        new_cwd: None,
    };

    let summary = tool_helpers::format_tool_result_summary(&result, "bash");

    assert!(summary.contains("…"), "Should truncate long output");
    assert!(
        !summary.contains(&long_output),
        "Should not include full output"
    );
    assert!(
        summary.contains("output_chars=2001"),
        "Should show character count"
    );
    assert!(summary.contains("output_lines=1"), "Should show line count");
}

// Section 6: Error Hint Tests

#[test]
fn test_tool_error_hint_permission() {
    let hint = tool_helpers::get_tool_error_hint("Permission denied while opening file");
    assert_eq!(
        hint,
        Some("💡 Tip: Check file permissions or try with elevated privileges")
    );
}

#[test]
fn test_tool_error_hint_not_found() {
    let hint = tool_helpers::get_tool_error_hint("File not found: /tmp/test.txt");
    assert_eq!(
        hint,
        Some("💡 Tip: Check if the file/path exists and is correct")
    );
}

#[test]
fn test_tool_error_hint_timeout() {
    let hint = tool_helpers::get_tool_error_hint("Operation timeout");
    assert_eq!(
        hint,
        Some("💡 Tip: Operation timed out. Try again or break into smaller steps")
    );
}

#[test]
fn test_tool_error_hint_unknown() {
    let hint = tool_helpers::get_tool_error_hint("Some random error");
    assert!(hint.is_none(), "Unknown errors should not return hints");
}

#[test]
fn test_command_error_hint_permission_denied() {
    let hint = tool_helpers::command_error_hint("ls", "Permission denied");
    assert!(hint.is_some(), "Should provide hint for permission denied");
    assert!(hint.unwrap().contains("sudo"), "Hint should mention sudo");
}

#[test]
fn test_command_error_hint_command_not_found() {
    let hint = tool_helpers::command_error_hint("foo --bar", "command not found: foo");
    assert!(hint.is_some(), "Should provide hint for command not found");
    assert!(
        hint.unwrap().contains("Install foo"),
        "Hint should suggest installation"
    );
}

#[test]
fn test_command_error_hint_no_such_file() {
    let hint =
        tool_helpers::command_error_hint("cat /tmp/missing.txt", "No such file or directory");
    assert!(hint.is_some(), "Should provide hint for missing file");
}

// Section 7: Tool Result Collection Tests

#[test]
fn test_tool_result_channel_send_receive() {
    let (tx, rx) = channel();
    let tool_result = ToolResult {
        call_id: "test-channel".to_string(),
        success: true,
        output: "channel test".to_string(),
        error: None,
        exit_code: None,
        data: None,
        new_cwd: None,
    };

    tx.send(tool_result).unwrap();
    let received = rx.recv().unwrap();

    assert_eq!(received.output, "channel test");
    assert_eq!(received.call_id, "test-channel");
}

// Section 8: Complex Integration Tests

#[test]
fn test_full_tool_execution_flow() {
    // Simulate full flow: extract -> parse -> sanitize
    let response = r#"Execute: ```tool
{"name":"bash","arguments":{"command":"ls /tmp"}}
```"#;

    // Step 1: Extract
    let payloads = auto_tool_parser::extract_tool_payloads(response);
    assert_eq!(payloads.len(), 1);

    // Step 2: Parse
    let calls = auto_tool_parser::parse_tool_calls_payload(&payloads[0]).expect("Should parse");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "bash");

    // Step 3: Sanitize command
    let cmd = calls[0].arguments["command"].as_str().unwrap();
    let result = tool_helpers::sanitize_command(cmd);
    assert!(result.is_ok(), "Command should be safe");
}

#[test]
fn test_mixed_safe_and_unsafe_commands() {
    let safe_commands = vec![
        "ls",
        "pwd",
        "git status",
        "cat file.txt | grep test",
        "find . -name '*.rs' | head -5",
    ];

    for cmd in safe_commands {
        assert!(
            tool_helpers::sanitize_command(cmd).is_ok(),
            "Command '{}' should be safe",
            cmd
        );
    }
}

#[test]
fn test_all_dangerous_patterns_blocked() {
    let dangerous_commands = vec![
        "ls; rm -rf /",
        "cat /etc/passwd &",
        "ls $(echo test)",
        "ls `whoami`",
        "git reset --hard",
        "git clean -fd",
    ];

    for cmd in dangerous_commands {
        assert!(
            tool_helpers::sanitize_command(cmd).is_err(),
            "Command '{}' should be blocked",
            cmd
        );
    }
}

#[test]
fn test_tool_iteration_limits() {
    assert_eq!(
        tool_helpers::MAX_TOOL_ITERATIONS,
        3,
        "Should have reasonable iteration limit"
    );
}
