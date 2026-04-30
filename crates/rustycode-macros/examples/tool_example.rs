#![allow(
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Example of using the tool macro and ToolDescription derive macro.

use rustycode_macros::{tool, ToolDescription};

#[tool(name = "read_file", permission = "read")]
/// Read a file from the filesystem.
#[allow(dead_code)]
fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tool(name = "write_file", permission = "write")]
/// Write content to a file.
#[allow(dead_code)]
fn write_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content).map_err(|e| e.to_string())
}

#[tool(name = "echo", permission = "none")]
/// Echo back the input.
#[allow(dead_code)]
fn echo(message: String) -> String {
    message
}

// Example of using ToolDescription derive macro
#[derive(ToolDescription)]
/// A tool for reading files with additional metadata support.
#[allow(dead_code)]
struct FSRead;

#[derive(ToolDescription)]
/// A tool for writing files atomically.
#[allow(dead_code)]
struct AtomicWrite;

// Example with external description file
// Note: You need to create the file "examples/descriptions/tool_from_file.md" for this to work
// #[derive(ToolDescription)]
// #[tool_description_file = "descriptions/tool_from_file.md"]
// struct ToolFromFile;

fn main() {
    use rustycode_tools::Tool;

    // Test the existing tool attribute macro
    let tool = echo_Tool;
    println!("=== Tool Attribute Macro Example ===");
    println!("Tool name: {}", tool.name());
    println!("Tool description: {}", tool.description());
    println!("Tool permission: {:?}", tool.permission());
    println!("Tool schema: {}", tool.parameters_schema());
    println!();

    // Test the new ToolDescription derive macro
    println!("=== ToolDescription Derive Macro Example ===");
    println!("FSRead description: {}", FSRead::description());
    println!("FSRead tool name: {}", FSRead::tool_name());
    println!();

    println!("AtomicWrite description: {}", AtomicWrite::description());
    println!("AtomicWrite tool name: {}", AtomicWrite::tool_name());
    println!();

    // Demonstrate snake_case conversion
    println!("=== Snake Case Conversion Examples ===");
    println!("ReadFile -> {}", FSRead::tool_name()); // "fs_read"
    println!("AtomicWrite -> {}", AtomicWrite::tool_name()); // "atomic_write"
}
