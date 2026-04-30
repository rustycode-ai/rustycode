//! Example usage of the ToolCatalog enum
//!
//! This example demonstrates how to use the enum-based tool registry
//! for type-safe tool invocation and serialization.

use rustycode_tools::tool_registry::{BashInput, ReadFileInput, ToolCatalog};
use serde_json::to_value;

fn main() -> anyhow::Result<()> {
    println!("=== ToolCatalog Example ===\n");

    // Example 1: Check if tools exist
    println!("1. Tool existence checks (case-insensitive):");
    println!(
        "   - read_file exists: {}",
        ToolCatalog::contains("read_file")
    );
    println!(
        "   - ReadFile exists: {}",
        ToolCatalog::contains("ReadFile")
    );
    println!(
        "   - READ_FILE exists: {}",
        ToolCatalog::contains("READ_FILE")
    );
    println!(
        "   - unknown_tool exists: {}",
        ToolCatalog::contains("unknown_tool")
    );

    // Example 2: Create a tool instance
    println!("\n2. Creating a ReadFile tool:");
    let read_tool = ToolCatalog::ReadFile(ReadFileInput {
        file_path: "/path/to/file.txt".to_string(),
        offset: Some(10),
        limit: Some(100),
    });
    println!("   - Tool name: {}", read_tool.name());
    println!("   - Description: {}", read_tool.description());
    println!("   - Permission: {:?}", read_tool.permission());

    // Example 3: Serialize a tool to JSON
    println!("\n3. Serializing tool to JSON:");
    let json = to_value(&read_tool)?;
    println!("   {}", serde_json::to_string_pretty(&json)?);

    // Example 4: List all tools
    println!(
        "\n4. All available tools ({} total):",
        ToolCatalog::all_tool_names().len()
    );
    for (i, tool_name) in ToolCatalog::all_tool_names().iter().take(10).enumerate() {
        println!("   {}. {}", i + 1, tool_name);
    }
    println!(
        "   ... and {} more",
        ToolCatalog::all_tool_names().len() - 10
    );

    // Example 5: Create different tool types
    println!("\n5. Different tool types:");
    let bash_tool = ToolCatalog::Bash(BashInput {
        command: "echo 'Hello, World!'".to_string(),
        timeout_secs: Some(30),
        restart: None,
    });
    println!(
        "   - Bash tool: {} (permission: {:?})",
        bash_tool.name(),
        bash_tool.permission()
    );

    Ok(())
}
