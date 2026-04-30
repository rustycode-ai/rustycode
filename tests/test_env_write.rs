use rustycode_tools::*;
use serde_json::json;
use tempfile::tempdir;

fn main() {
    let workspace = tempdir().expect("workspace tempdir");
    let ctx = ToolContext::new(workspace.path());

    let tool = WriteFileTool;

    let result = tool.execute(
        json!({
            "path": ".env",
            "content": "SECRET=value"
        }),
        &ctx,
    );

    match result {
        Ok(_) => println!("Write succeeded (should have been blocked)"),
        Err(e) => println!("Write blocked: {}", e),
    }
}
