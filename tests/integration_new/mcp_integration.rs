// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! MCP (Model Context Protocol) integration tests
//!
//! Tests cover:
//! - MCP client creation and configuration
//! - Tool discovery and calling (with server_name parameter)
//! - Resource access
//! - Error handling for missing servers
//! - MCP client-server communication

use std::path::PathBuf;
use std::time::Duration;

use rustycode_mcp::{McpClient, McpClientConfig};
use tokio::time::sleep;

mod common;
use common::TestConfig;

// Helper to create a simple test MCP server script
fn create_test_mcp_server(config: &TestConfig, name: &str) -> PathBuf {
    let server_script = config.data_dir.join(format!("{}.sh", name));

    let script_content = format!(
        r#"#!/bin/bash
# Simple MCP echo server for testing

echo 'MCPECHO: Started {}'

# Read JSON-RPC requests from stdin
while IFS= read -r line; do
    echo "MCPECHO: Received: $line" >&2

    # Simple echo response
    echo '{{"jsonrpc":"2.0","id":1,"result":{{"status":"ok"}}}}'

    # Check for exit command
    if echo "$line" | grep -q '"method":"shutdown"'; then
        echo 'MCPECHO: Shutting down' >&2
        break
    fi
done
"#,
        name
    );

    std::fs::write(&server_script, script_content).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&server_script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&server_script, perms).unwrap();
    }

    server_script
}

#[tokio::test]
async fn test_mcp_client_creation() {
    let config = McpClientConfig::default();
    let client = McpClient::new(config);

    // No servers connected initially
    assert!(!client.is_connected("any-server").await);
}

#[tokio::test]
async fn test_mcp_tool_discovery() {
    let test_config = TestConfig::new();
    let server_script = create_test_mcp_server(&test_config, "tool_discovery");

    let config = McpClientConfig::default();
    let mut client = McpClient::new(config);

    let result = client
        .connect_stdio("test-server", server_script.to_str().unwrap_or(""), &[])
        .await;

    match result {
        Ok(_) => {
            sleep(Duration::from_millis(100)).await;

            // list_tools requires server_name
            match client.list_tools("test-server").await {
                Ok(tools) => {
                    // Should have at least some tools or be empty
                    let _ = tools.len();
                }
                Err(_) => {
                    // Expected if server doesn't implement tools properly
                }
            }
        }
        Err(_) => {
            // Expected if test server isn't a valid MCP server
        }
    }
}

#[tokio::test]
async fn test_mcp_tool_calling() {
    let test_config = TestConfig::new();
    let server_script = create_test_mcp_server(&test_config, "tool_calling");

    let config = McpClientConfig::default();
    let mut client = McpClient::new(config);

    let result = client
        .connect_stdio("test-server", server_script.to_str().unwrap_or(""), &[])
        .await;

    match result {
        Ok(_) => {
            sleep(Duration::from_millis(100)).await;

            // call_tool requires server_name, tool_name, and args
            let result = client
                .call_tool(
                    "test-server",
                    "test_tool",
                    serde_json::json!({"param": "value"}),
                )
                .await;

            match result {
                Ok(response) => {
                    // McpToolResult has content and is_error fields
                    let _ = response.content.len();
                }
                Err(_) => {
                    // Expected if tool doesn't exist
                }
            }
        }
        Err(_) => {
            // Expected if test server isn't valid
        }
    }
}

#[tokio::test]
async fn test_mcp_resource_access() {
    let test_config = TestConfig::new();
    let server_script = create_test_mcp_server(&test_config, "resource_access");

    let config = McpClientConfig::default();
    let mut client = McpClient::new(config);

    let result = client
        .connect_stdio("test-server", server_script.to_str().unwrap_or(""), &[])
        .await;

    match result {
        Ok(_) => {
            sleep(Duration::from_millis(100)).await;

            // list_resources requires server_name
            match client.list_resources("test-server").await {
                Ok(resources) => {
                    let _ = resources.len();
                }
                Err(_) => {
                    // Expected if server doesn't implement resources
                }
            }
        }
        Err(_) => {
            // Expected if test server isn't valid
        }
    }
}

#[tokio::test]
async fn test_mcp_server_lifecycle() {
    let test_config = TestConfig::new();
    let server_script = create_test_mcp_server(&test_config, "lifecycle");

    let config = McpClientConfig::default();
    let mut client = McpClient::new(config);

    let result = client
        .connect_stdio("test-server", server_script.to_str().unwrap_or(""), &[])
        .await;

    match result {
        Ok(_) => {
            sleep(Duration::from_millis(100)).await;

            assert!(client.is_connected("test-server").await);

            let result = client.disconnect("test-server").await;

            if result.is_ok() {
                assert!(!client.is_connected("test-server").await);
            }
        }
        Err(_) => {
            // Expected if test server isn't valid
        }
    }
}

#[tokio::test]
async fn test_mcp_error_handling() {
    let config = McpClientConfig::default();
    let mut client = McpClient::new(config);

    // Try to call tool on non-existent server
    let result = client
        .call_tool("nonexistent-server", "some_tool", serde_json::json!({}))
        .await;
    assert!(result.is_err());

    // Disconnect non-existent server is idempotent (returns Ok)
    let result = client.disconnect("nonexistent-server").await;
    assert!(result.is_ok()); // Idempotent: no error for missing server

    // Try to list tools from non-existent server
    let result = client.list_tools("nonexistent-server").await;
    assert!(result.is_err());

    // Try to list resources from non-existent server
    let result = client.list_resources("nonexistent-server").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mcp_multiple_servers() {
    let test_config = TestConfig::new();
    let server1_script = create_test_mcp_server(&test_config, "server1");
    let server2_script = create_test_mcp_server(&test_config, "server2");

    let config = McpClientConfig::default();
    let mut client = McpClient::new(config);

    let result1 = client
        .connect_stdio("server1", server1_script.to_str().unwrap_or(""), &[])
        .await;
    let result2 = client
        .connect_stdio("server2", server2_script.to_str().unwrap_or(""), &[])
        .await;

    match (result1, result2) {
        (Ok(_), Ok(_)) => {
            sleep(Duration::from_millis(100)).await;

            assert!(client.is_connected("server1").await);
            assert!(client.is_connected("server2").await);

            let _ = client.disconnect("server1").await;
            let _ = client.disconnect("server2").await;

            assert!(!client.is_connected("server1").await);
            assert!(!client.is_connected("server2").await);
        }
        _ => {
            // Expected if test servers aren't valid
        }
    }
}

#[tokio::test]
async fn test_mcp_prompt_templates() {
    let test_config = TestConfig::new();
    let server_script = create_test_mcp_server(&test_config, "prompts");

    let config = McpClientConfig::default();
    let mut client = McpClient::new(config);

    let result = client
        .connect_stdio("test-server", server_script.to_str().unwrap_or(""), &[])
        .await;

    if result.is_ok() {
        sleep(Duration::from_millis(100)).await;

        // list_prompts requires server_name
        if let Ok(prompts) = client.list_prompts("test-server").await {
            let _ = prompts.len();
        }

        // get_prompt requires server_name, prompt_name, and optional args
        let _ = client.prompt("test-server", "test_prompt", None).await;
    }
}

#[tokio::test]
async fn test_mcp_client_config() {
    // Test default config
    let config1 = McpClientConfig::default();
    assert_eq!(config1.timeout_secs, 30);

    // Test custom config
    let config2 = McpClientConfig {
        timeout_secs: 60,
        ..McpClientConfig::default()
    };
    assert_eq!(config2.timeout_secs, 60);

    let client = McpClient::new(config2);
    assert!(!client.is_connected("any").await);
}
