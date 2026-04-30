#![allow(
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::missing_const_for_fn,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Integration test showing ToolDescription with the Tool trait

use rustycode_macros::ToolDescription;
use rustycode_tools::{Tool, ToolContext, ToolOutput, ToolPermission};
use serde_json::Value;

#[derive(ToolDescription)]
/// A simple file reader tool that reads entire file contents.
struct SimpleFileReader;

#[derive(ToolDescription)]
/// A file writer tool that writes content atomically.
#[allow(dead_code)]
struct AtomicFileWriter;

impl SimpleFileReader {
    // Cache the tool name as a static string to return a reference
    fn cached_name() -> &'static str {
        // Note: In a real implementation, you might use lazy_static or once_cell
        // for this. For this test, we'll just return a literal.
        "simple_file_reader"
    }
}

impl Tool for SimpleFileReader {
    fn name(&self) -> &str {
        Self::cached_name()
    }

    fn description(&self) -> &str {
        Self::description()
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput, anyhow::Error> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;

        Ok(ToolOutput::text(content))
    }
}

#[test]
fn test_tool_integration() {
    let tool = SimpleFileReader;

    // Test metadata methods
    assert_eq!(tool.name(), "simple_file_reader");
    assert!(tool.description().contains("file reader"));
    assert_eq!(tool.permission(), ToolPermission::Read);

    // Test parameters schema
    let schema = tool.parameters_schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["path"].is_object());

    // Test execution with a temporary file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_simple_reader.txt");
    std::fs::write(&test_file, "Hello, ToolDescription!").unwrap();

    let ctx = ToolContext::new(temp_dir);
    let params = serde_json::json!({
        "path": test_file.to_str().unwrap()
    });

    let result = tool.execute(params, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().text, "Hello, ToolDescription!");

    // Cleanup
    std::fs::remove_file(&test_file).ok();
}

#[test]
fn test_static_methods() {
    // Test static methods work without an instance
    assert_eq!(SimpleFileReader::tool_name(), "simple_file_reader");
    assert!(SimpleFileReader::description().contains("file reader"));
    assert_eq!(AtomicFileWriter::tool_name(), "atomic_file_writer");
    assert!(AtomicFileWriter::description().contains("atomically"));

    // Verify the tool name matches the expected format
    assert_eq!(
        SimpleFileReader::cached_name(),
        SimpleFileReader::tool_name()
    );
}
