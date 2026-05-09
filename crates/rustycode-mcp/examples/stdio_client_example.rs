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

//! Example: Using the simplified MCP stdio client
//!
//! This example demonstrates how to use the `McpStdioClient` to connect
//! to MCP servers via stdio transport.

use rustycode_mcp::stdio_client::{McpClientManager, McpServerConfig, McpStdioClient};

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("=== MCP Stdio Client Example ===\n");

    // Example 1: Single client connection
    println!("Example 1: Connecting to a filesystem MCP server");

    let config = McpServerConfig::new("filesystem", "npx")
        .with_args(vec![
            "-y".to_string(),
            "@anthropic/mcp-server-filesystem".to_string(),
            "/tmp".to_string(),
        ])
        .with_enabled(true);

    let mut client = McpStdioClient::new(config);

    match client.connect() {
        Ok(_) => {
            println!("✓ Connected to server: {}", client.name());

            // List available tools
            let tools = client.tools();
            println!("✓ Available tools ({}):", tools.len());
            for tool in tools {
                println!("  - {}: {}", tool.name, tool.description);
            }

            // Example: Call a tool (if filesystem server has read_file)
            if let Some(read_tool) = client.find_tool("Read") {
                println!("\n✓ Found tool: {}", read_tool.name);

                // Note: This would fail if /tmp/test.txt doesn't exist
                // It's just to show the API
                println!("  (Tool call example skipped - would need actual file)");
            }

            // Client automatically disconnects when dropped
        }
        Err(e) => {
            println!("✗ Failed to connect: {}", e);
            println!("  (This is expected if npx or the MCP server is not installed)");
        }
    }

    println!("\nExample 2: Using the client manager");

    // Example 2: Multiple servers with manager
    let manager = McpClientManager::new();

    // Add a server (commented out to avoid actual connection attempts)
    /*
    let postgres_config = McpServerConfig::new("postgres", "npx")
        .with_args(vec![
            "-y".to_string(),
            "@anthropic/mcp-server-postgres".to_string(),
            "postgresql://localhost:5432/mydb".to_string(),
        ]);

    match manager.add_server(postgres_config) {
        Ok(_) => println!("✓ Connected to postgres server"),
        Err(e) => println!("✗ Failed to connect: {}", e),
    }
    */

    // List all connected servers
    let servers = manager.connected_servers();
    println!("Connected servers: {:?}", servers);

    // Get all tools from all servers
    let all_tools = manager.all_tools();
    println!("Total tools across all servers: {}", all_tools.len());
    for (server, tool) in all_tools {
        println!("  {}.{} - {}", server, tool.name, tool.description);
    }

    // Example 3: Creating config from JSON
    println!("\nExample 3: Creating config from JSON");

    let json_config = r#"{
        "name": "github",
        "command": "npx",
        "args": ["-y", "@anthropic/mcp-server-github"],
        "env": {"GITHUB_TOKEN": "your-token-here"},
        "enabled": false
    }"#;

    let config: McpServerConfig = serde_json::from_str(json_config)?;
    println!(
        "✓ Parsed config: {} (enabled: {})",
        config.name, config.enabled
    );

    println!("\n=== Example Complete ===");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = McpServerConfig::new("test", "node")
            .with_args(vec!["server.js".to_string()])
            .with_enabled(true);

        assert_eq!(config.name, "test");
        assert_eq!(config.command, "node");
        assert_eq!(config.args.len(), 1);
        assert!(config.enabled);
    }

    #[test]
    fn test_manager_empty() {
        let manager = McpClientManager::new();
        assert!(manager.connected_servers().is_empty());
        assert!(manager.all_tools().is_empty());
    }
}
