//! Examples of streaming and auto tool calling
//!
//! Run with: cargo run --example streaming_auto_tools

use rustycode_tools::auto_tool::{AutoToolConfig, AutoToolContext};
use rustycode_tools::streaming::ToolStreaming;
use rustycode_tools::{BashTool, ToolContext, ToolRegistry};
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    println!("=== Streaming and Auto Tool Calling Examples ===\n");

    // Example 1: Basic Streaming
    println!("Example 1: Basic Bash Streaming");
    println!("-----------------------------------");
    basic_streaming()?;

    // Example 2: Auto Tool Calling
    println!("\nExample 2: Auto Tool Calling");
    println!("----------------------------");
    auto_tool_calling()?;

    // Example 3: Combined Streaming + Auto Calling
    println!("\nExample 3: Combined Streaming + Auto Tool Calling");
    println!("------------------------------------------------");
    combined_streaming_auto()?;

    Ok(())
}

fn basic_streaming() -> anyhow::Result<()> {
    let tool = BashTool;
    let ctx = ToolContext::new("/tmp");

    // Execute command with streaming
    let receiver = tool.execute_stream(
        serde_json::json!({"command": "echo 'Streaming output test'"}),
        &ctx,
    )?;

    // Consume stream incrementally with timeout
    println!("Streaming output:");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);

    while std::time::Instant::now() < deadline {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => {
                if chunk.is_done {
                    println!("\n[Stream completed]");
                    break;
                }
                if let Some(error) = chunk.error {
                    eprintln!("\n[Error] {}", error);
                    break;
                }
                print!("{}", chunk.text);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Continue waiting
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    Ok(())
}

fn auto_tool_calling() -> anyhow::Result<()> {
    use std::sync::Arc;

    // Create registry and register tools
    let mut registry = ToolRegistry::new();
    registry.register(BashTool);

    let ctx = ToolContext::new("/tmp");
    let mut auto_ctx = AutoToolContext::new(Arc::new(registry), ctx);

    // Call bash tool programmatically
    println!("Calling bash tool programmatically:");
    let result = auto_ctx.call_tool(
        "bash",
        serde_json::json!({"command": "echo 'Hello from auto tool calling!'"}),
    )?;

    println!("{}", result.text);
    println!("Call history: {:?}", auto_ctx.call_history());

    Ok(())
}

fn combined_streaming_auto() -> anyhow::Result<()> {
    use std::sync::Arc;

    let mut registry = ToolRegistry::new();
    registry.register(BashTool);

    let ctx = ToolContext::new("/tmp");
    let auto_ctx = AutoToolContext::with_config(
        Arc::new(registry),
        ctx,
        AutoToolConfig {
            max_depth: 3,
            allow_recursive_calls: false,
            ..Default::default()
        },
    );

    // Use streaming within auto tool context
    println!("Streaming within auto tool context:");

    let bash = BashTool;
    let receiver = bash.execute_stream(
        serde_json::json!({"command": "echo 'Combined example done'"}),
        &auto_ctx.tool_context,
    )?;

    // Process stream with timeout
    let deadline = std::time::Instant::now() + Duration::from_secs(5);

    while std::time::Instant::now() < deadline {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => {
                if chunk.is_done {
                    break;
                }
                if let Some(error) = chunk.error {
                    eprintln!("Error: {}", error);
                    break;
                }
                print!("{}", chunk.text);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    println!("\nAuto tool calls made: {}", auto_ctx.current_depth());

    Ok(())
}
