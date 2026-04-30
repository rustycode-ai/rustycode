#![allow(
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::ignored_unit_patterns,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Example MCP server usage

use rustycode_mcp::{
    types::{McpContent, McpPromptContent, McpPromptMessage},
    McpServer, McpServerConfig,
};
use rustycode_tools::ToolExecutor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info,mcp_server=debug")
        .init();

    println!("MCP Server Example");

    // Create tool executor
    let cwd = std::env::current_dir()?;
    let executor = ToolExecutor::from_cwd(cwd);

    // Create MCP server
    let config = McpServerConfig {
        server_name: "example-server".to_string(),
        server_version: "0.1.0".to_string(),
        enable_tools: true,
        enable_resources: true,
        enable_prompts: true,
        timeout_secs: 30,
    };

    let mut server = McpServer::new("example", config);

    // Register tool executor
    server.register_tool_executor(executor);

    // Register example resources
    server
        .register_resource(
            "example://hello",
            "Hello World",
            "A simple hello world resource",
            "text/plain",
            || {
                Ok(vec![McpContent::Text {
                    text: "Hello, World!".to_string(),
                }])
            },
        )
        .await;

    server
        .register_resource(
            "example://time",
            "Current Time",
            "Current system time",
            "text/plain",
            || {
                Ok(vec![McpContent::Text {
                    text: chrono::Utc::now().to_rfc3339(),
                }])
            },
        )
        .await;

    // Register example prompts
    server
        .register_prompt("code-review", "Code review template", |args| {
            let language = args
                .as_ref()
                .and_then(|a| a.get("language").and_then(|l| l.as_str()))
                .unwrap_or("this code")
                .to_string();

            Ok(vec![McpPromptMessage {
                role: "user".to_string(),
                content: McpPromptContent::Text {
                    text: format!(
                        "Please review the following {}. \
                            Focus on code quality, bugs, performance, and security.",
                        language
                    ),
                },
            }])
        })
        .await;

    server
        .register_prompt("debug-help", "Debugging assistant", |_args| {
            Ok(vec![
                McpPromptMessage {
                    role: "user".to_string(),
                    content: McpPromptContent::Text {
                        text: "I'm experiencing an issue.".to_string(),
                    },
                },
                McpPromptMessage {
                    role: "assistant".to_string(),
                    content: McpPromptContent::Text {
                        text: "I'll help you debug. Please describe the issue:".to_string(),
                    },
                },
            ])
        })
        .await;

    println!("Server configured with:");
    println!("- Tools from rustycode-tools");
    println!("- 2 example resources");
    println!("- 2 example prompts");
    println!("\nTo run as an MCP server, execute this binary");
    println!("It will communicate via stdio (stdin/stdout)");

    // Run server on stdio
    // server.run_stdio().await?;

    Ok(())
}
