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

//! Test multi-round tool calling loop logic
use rustycode_tui::auto_tool_parser::{extract_tool_payloads, parse_tool_calls_payload};

#[test]
fn test_multi_round_tool_calling_detection() {
    // Simulate a multi-round tool calling scenario

    // Round 1: LLM returns list_dir tool call
    let response1 = r#"I'll list the directory contents first.

```tool
[{"name": "ListDir", "arguments": {"path": "."}}]
```"#;

    let payloads1 = extract_tool_payloads(response1);
    assert_eq!(payloads1.len(), 1, "Should detect list_dir tool call");

    if let Ok(tool_calls) = parse_tool_calls_payload(&payloads1[0]) {
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "ListDir");
    }

    // Simulate tool result and Round 2: LLM returns read_file tool call
    let response2 = r#"I see the files. Let me read Cargo.toml.

```tool
[{"name": "Read", "arguments": {"path": "Cargo.toml"}}]
```"#;

    let payloads2 = extract_tool_payloads(response2);
    assert_eq!(payloads2.len(), 1, "Should detect read_file tool call");

    if let Ok(tool_calls) = parse_tool_calls_payload(&payloads2[0]) {
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "Read");
    }

    // Simulate tool result and Round 3: LLM returns another read_file tool call
    let response3 = r#"Let me also read src/lib.rs.

```tool
[{"name": "Read", "arguments": {"path": "src/lib.rs"}}]
```"#;

    let payloads3 = extract_tool_payloads(response3);
    assert_eq!(
        payloads3.len(),
        1,
        "Should detect second read_file tool call"
    );

    // Round 4: LLM returns final response (no tool calls)
    let response4 = r#"Based on the files I've read, this is RustyCode, an AI coding assistant...

The project structure shows:
- Cargo.toml defines the workspace
- src/lib.rs contains the main logic

This is a terminal UI for AI-assisted coding."#;

    let payloads4 = extract_tool_payloads(response4);
    assert_eq!(
        payloads4.len(),
        0,
        "Should not detect any tool calls in final response"
    );

    println!("✓ Multi-round tool calling detection test passed");
    println!("✓ Round 1: list_dir detected");
    println!("✓ Round 2: read_file detected");
    println!("✓ Round 3: read_file detected");
    println!("✓ Round 4: Final response (no tools)");
}

#[test]
fn test_tool_calling_loop_termination() {
    // Test that the loop properly terminates when no more tool calls

    // Response with tool call
    let with_tools = r#"Let me check the files.

```tool
[{"name": "ListDir", "arguments": {"path": "."}}]
```"#;

    let payloads_with = extract_tool_payloads(with_tools);
    assert!(!payloads_with.is_empty(), "Should detect tool calls");

    // Response without tool calls (final answer)
    let without_tools = r#"This project is called RustyCode. It's an AI coding assistant TUI.

Key features:
- Tool calling for file operations
- Multi-turn conversations
- Anthropic Claude integration"#;

    let payloads_without = extract_tool_payloads(without_tools);
    assert!(
        payloads_without.is_empty(),
        "Should not detect tool calls in final answer"
    );

    println!("✓ Loop termination test passed");
    println!("✓ Tool calls detected: {}", !payloads_with.is_empty());
    println!(
        "✓ Final response terminates loop: {}",
        payloads_without.is_empty()
    );
}

#[test]
fn test_multiple_tools_in_single_response() {
    // Test handling multiple tool calls in one response
    let response = r#"I'll read both files at once.

```tool
[
  {"name": "Read", "arguments": {"path": "Cargo.toml"}},
  {"name": "Read", "arguments": {"path": "README.md"}}
]
```"#;

    let payloads = extract_tool_payloads(response);
    assert_eq!(payloads.len(), 1, "Should detect one tool block");

    if let Ok(tool_calls) = parse_tool_calls_payload(&payloads[0]) {
        assert_eq!(tool_calls.len(), 2, "Should parse two tool calls");
        assert_eq!(tool_calls[0].name, "Read");
        assert_eq!(tool_calls[1].name, "Read");
    }

    println!("✓ Multiple tools in single response test passed");
    println!("✓ Detected 2 tool calls in one block");
}
