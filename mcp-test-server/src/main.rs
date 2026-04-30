//! Example MCP server implementation

use rustycode_mcp::{McpServer, McpServerConfig, McpContent, McpPromptContent, McpPromptMessage};
use rustycode_tools::ToolExecutor;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> rustycode_mcp::McpResult<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info,mcp_test_server=debug")
        .init();

    println!("Starting MCP test server...",);

    // Create tool executor with current directory
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let executor = ToolExecutor::new(cwd);

    // Create MCP server
    let config = McpServerConfig {
        server_name: "mcp-test-server".to_string(),
        server_version: "0.1.0".to_string(),
        enable_tools: true,
        enable_resources: true,
        enable_prompts: true,
        timeout_secs: 30,
    };

    let mut server = McpServer::new("test-server", config);

    // Register tool executor
    server.register_tool_executor(executor);

    // Register some example resources
    server
        .register_resource(
            "file:///etc/hostname",
            "Hostname",
            "System hostname",
            "text/plain",
            || {
                Ok(vec![McpContent::Text {
                    text: std::env::var("HOSTNAME")
                        .unwrap_or_else(|_| "unknown".to_string()),
                }])
            },
        )
        .await;

    server
        .register_resource(
            "mem://stats",
            "Memory Stats",
            "Memory usage statistics",
            "application/json",
            || {
                Ok(vec![McpContent::Text {
                    text: format!(
                        "{{\"uptime\": \"{}\", \"pid\": {}}}",
                        humantime::Duration::from(
                            std::time::Duration::from_secs(
                                std::env::var("UPTIME_SECS")
                                    .ok()
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(0)
                            )
                        ).to_string(),
                        std::process::id()
                    ),
                }])
            },
        )
        .await;

    // Register example prompts
    server
        .register_prompt(
            "code-review",
            "Code review prompt template",
            |args| {
                let language = args
                    .and_then(|a| a.get("language"))
                    .and_then(|l| l.as_str())
                    .unwrap_or("the code");

                Ok(vec![McpPromptMessage {
                    role: "user".to_string(),
                    content: McpPromptContent::Text {
                        text: format!(
                            "Please review the following {} code. \
                            Focus on:\n\
                            1. Code quality and readability\n\
                            2. Potential bugs or edge cases\n\
                            3. Performance considerations\n\
                            4. Security issues\n\n\
                            Provide specific, actionable feedback.",
                            language
                        ),
                    },
                }])
            },
        )
        .await;

    server
        .register_prompt(
            "debug-session",
            "Debug session prompt template",
            |_args| {
                Ok(vec![
                    McpPromptMessage {
                        role: "user".to_string(),
                        content: McpPromptContent::Text {
                            text: "I'm experiencing an issue. Help me debug it.".to_string(),
                        },
                    },
                    McpPromptMessage {
                        role: "assistant".to_string(),
                        content: McpPromptContent::Text {
                            text: "I'll help you debug. Please provide:\n\
                                  1. What you expected to happen\n\
                                  2. What actually happened\n\
                                  3. Error messages or stack traces\n\
                                  4. Relevant code or configuration".to_string(),
                        },
                    },
                ])
            },
        )
        .await;

    // Run server on stdio
    println!("MCP test server running on stdio");
    server.run_stdio().await?;

    Ok(())
}
