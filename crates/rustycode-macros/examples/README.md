# ToolDescription Derive Macro Examples

This directory contains examples demonstrating how to use the `ToolDescription` derive macro from the `rustycode-macros` crate.

## Overview

The `ToolDescription` derive macro automatically generates two methods for structs:

1. **`description() -> &'static str`**: Returns the tool's description from doc comments
2. **`tool_name() -> String`**: Returns the struct name converted to snake_case

## Basic Usage

```rust
use rustycode_macros::ToolDescription;

#[derive(ToolDescription)]
/// Reads a file from the filesystem.
struct ReadFile;

assert_eq!(ReadFile::description(), "Reads a file from the filesystem.");
assert_eq!(ReadFile::tool_name(), "read_file");
```

## Features

### 1. Doc Comment Extraction

The macro automatically extracts and concatenates doc comments:

```rust
#[derive(ToolDescription)]
/// A tool for reading files.
/// Supports additional metadata.
struct FSRead;

// Description will be: "A tool for reading files.\nSupports additional metadata."
```

### 2. Snake Case Conversion

The macro intelligently converts struct names to snake_case:

- `ReadFile` → `read_file`
- `FSRead` → `fs_read`
- `AtomicWrite` → `atomic_write`
- `HTTPServer` → `http_server`
- `Tool2Read` → `tool2_read`

### 3. External Description Files

For longer descriptions, use the `#[tool_description_file]` attribute:

```rust
#[derive(ToolDescription)]
#[tool_description_file = "descriptions/my_tool.md"]
struct MyTool;
```

The file contents will be embedded in the binary at compile time using `include_str!`.

## Running the Examples

### Basic Example

```bash
cargo run -p rustycode-macros --example tool_example
```

### Tests

```bash
cargo test -p rustycode-macros
```

## Use Cases

The `ToolDescription` macro is particularly useful for:

1. **Tool Registration**: Automatically generate tool metadata for tool registries
2. **API Documentation**: Keep descriptions with the struct definitions
3. **Compile-time Safety**: Descriptions are validated at compile time
4. **Code Generation**: Eliminate boilerplate code for tool metadata

## Integration with Tool Trait

The macro can be used alongside the `Tool` trait for a complete tool implementation:

```rust
use rustycode_macros::{ToolDescription, tool};
use rustycode_tools::Tool;

// Function-based tool with attribute macro
#[tool(name = "read_file", permission = "read")]
/// Read a file from the filesystem.
fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

// Struct-based tool with derive macro
#[derive(ToolDescription)]
/// A tool for reading files with metadata support.
struct FSRead;

impl Tool for FSRead {
    fn name(&self) -> &str {
        &Self::tool_name()
    }

    fn description(&self) -> &str {
        Self::description()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        // ... schema definition
    }

    fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // ... implementation
    }
}
```

## Advanced Examples

See the test files for more advanced usage:

- `tests/tool_description_tests.rs`: Basic functionality tests
- `tests/tool_description_file_tests.rs`: External file loading tests
