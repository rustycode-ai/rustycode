#![allow(
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value,
    clippy::uninlined_format_args
)]

//! Tests for the ToolDescription derive macro

use rustycode_macros::ToolDescription;

#[derive(ToolDescription)]
/// Simple single-word description.
struct SimpleTool;

#[derive(ToolDescription)]
/// A tool for reading files with additional metadata support.
struct FSRead;

#[derive(ToolDescription)]
/// A tool for writing files atomically.
struct AtomicWrite;

#[derive(ToolDescription)]
/// Tool with multiple words in PascalCase.
#[allow(dead_code)]
struct MultiWordToolName;

#[derive(ToolDescription)]
/// Tool with acronyms.
struct HTTPServer;

#[derive(ToolDescription)]
/// Tool with numbers.
struct Tool2Read;

#[derive(ToolDescription)]
/// Already in snake_case format.
#[allow(dead_code)]
#[allow(non_camel_case_types)]
struct already_snake_case;

#[test]
fn test_simple_description() {
    assert_eq!(SimpleTool::description(), "Simple single-word description.");
}

#[test]
fn test_tool_name_conversion() {
    assert_eq!(SimpleTool::tool_name(), "simple_tool");
    assert_eq!(FSRead::tool_name(), "fs_read");
    assert_eq!(AtomicWrite::tool_name(), "AtomicWrite");
    assert_eq!(MultiWordToolName::tool_name(), "multi_word_tool_name");
    assert_eq!(HTTPServer::tool_name(), "http_server");
    assert_eq!(Tool2Read::tool_name(), "tool2_read");
    assert_eq!(already_snake_case::tool_name(), "already_snake_case");
}

#[test]
fn test_multi_word_description() {
    let desc = FSRead::description();
    assert!(desc.contains("reading files"));
    assert!(desc.contains("additional metadata"));
}

#[test]
fn test_atomic_write_description() {
    let desc = AtomicWrite::description();
    assert!(desc.contains("writing files"));
    assert!(desc.contains("atomically"));
}

#[test]
fn test_edge_cases() {
    // Test acronym handling (HTTPServer -> http_server)
    assert_eq!(HTTPServer::tool_name(), "http_server");

    // Test number handling (Tool2Read -> tool2_read)
    assert_eq!(Tool2Read::tool_name(), "tool2_read");

    // Test already snake_case (already_snake_case -> already_snake_case)
    assert_eq!(already_snake_case::tool_name(), "already_snake_case");
}

#[test]
fn test_multi_word_pascal_case() {
    // MultiWordToolName should become multi_word_tool_name
    assert_eq!(MultiWordToolName::tool_name(), "multi_word_tool_name");
}
