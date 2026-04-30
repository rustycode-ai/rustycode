#![allow(
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value,
    clippy::uninlined_format_args
)]

//! Tests for the ToolDescription derive macro with external files

use rustycode_macros::ToolDescription;

#[derive(ToolDescription)]
#[tool_description_file = "../examples/descriptions/tool_with_file.md"]
#[allow(dead_code)]
struct ToolWithFile;

#[test]
fn test_external_file_description() {
    let desc = ToolWithFile::description();
    println!("Description from file:\n{}", desc);

    // Verify the description contains content from the external file
    assert!(desc.contains("Tool with External Description"));
    assert!(desc.contains("external files at compile time"));
    assert!(desc.contains("Features"));
    assert!(desc.contains("Load descriptions from Markdown files"));
}

#[test]
fn test_tool_name_with_file() {
    assert_eq!(ToolWithFile::tool_name(), "tool_with_file");
}
