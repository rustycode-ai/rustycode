//! MCP (Model Context Protocol) slash commands
//!
//! Provides commands for managing MCP servers and tools.

/// Handle MCP slash commands
pub async fn handle_mcp_command(input: &str) -> Result<Option<String>, String> {
    let parts: Vec<&str> = input.split_whitespace().collect();

    if parts.is_empty() || parts.len() < 2 {
        return Ok(Some(mcp_help()));
    }

    let subcommand = parts[1];

    match subcommand {
        "help" | "" => Ok(Some(mcp_help())),
        "list" => handle_mcp_list().await,
        "status" => handle_mcp_status().await,
        "debug" => handle_mcp_debug().await,
        "open" => Ok(Some(
            "MCP Mode opened. Navigate servers with ↑↓, tools with ←→.\n\
            \n\
            Press E to execute tool, P for parallel mode, R for resources.\n\
            Press Q or Esc to close MCP Mode."
                .to_string(),
        )),
        "reload" => handle_mcp_reload().await,
        "enable" => handle_mcp_enable(&parts).await,
        "disable" => handle_mcp_disable(&parts).await,
        "toggle" => handle_mcp_toggle(&parts).await,
        "allowlist" => handle_mcp_allowlist(&parts).await,
        "call" | "exec" => handle_mcp_call(&parts).await,
        _ => Ok(Some(format!(
            "Unknown MCP command: {}\n\n{}",
            subcommand,
            mcp_help()
        ))),
    }
}

/// Handle /mcp enable <server_id>
async fn handle_mcp_enable(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 3 {
        return Ok(Some(
            "Usage: /mcp enable <server_id>\n\nExample: /mcp enable filesystem".to_string(),
        ));
    }

    let server_id = parts[2];

    match toggle_server_enabled(server_id, true) {
        Ok(true) => Ok(Some(format!("✅ Enabled MCP server '{}'", server_id))),
        Ok(false) => Ok(Some(format!("Server '{}' is already enabled", server_id))),
        Err(e) => Ok(Some(format!(
            "❌ Failed to enable server '{}': {}",
            server_id, e
        ))),
    }
}

/// Handle /mcp disable <server_id>
async fn handle_mcp_disable(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 3 {
        return Ok(Some(
            "Usage: /mcp disable <server_id>\n\nExample: /mcp disable prod-db".to_string(),
        ));
    }

    let server_id = parts[2];

    match toggle_server_enabled(server_id, false) {
        Ok(true) => Ok(Some(format!("✅ Disabled MCP server '{}'", server_id))),
        Ok(false) => Ok(Some(format!("Server '{}' is already disabled", server_id))),
        Err(e) => Ok(Some(format!(
            "❌ Failed to disable server '{}': {}",
            server_id, e
        ))),
    }
}

/// Handle /mcp toggle <server_id>
async fn handle_mcp_toggle(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 3 {
        return Ok(Some(
            "Usage: /mcp toggle <server_id>\n\nExample: /mcp toggle prod-db".to_string(),
        ));
    }

    let server_id = parts[2];

    // First check current state
    let configs = rustycode_mcp::McpConfigFile::load_from_standard_locations();
    let mut current_state: Option<bool> = None;

    for (_path, config) in &configs {
        if let Some(server_config) = config.servers.get(server_id) {
            current_state = Some(server_config.enabled);
            break;
        }
    }

    match current_state {
        Some(true) => handle_mcp_disable(parts).await,
        Some(false) => handle_mcp_enable(parts).await,
        None => Ok(Some(format!(
            "❌ Server '{}' not found in config",
            server_id
        ))),
    }
}

/// Toggle a server's enabled state in the config file
fn toggle_server_enabled(server_id: &str, enabled: bool) -> Result<bool, String> {
    use rustycode_mcp::McpConfigFile;
    use std::fs;

    let configs = McpConfigFile::load_from_standard_locations();

    // Find the server in config files
    for (path, config) in &configs {
        if config.servers.contains_key(server_id) {
            // Read the config file
            let content =
                fs::read_to_string(path).map_err(|e| format!("Failed to read config: {}", e))?;

            let mut json: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse config: {}", e))?;

            // Get current state before modifying
            let was_enabled = json
                .get("servers")
                .and_then(|s| s.get(server_id))
                .and_then(|s| s.get("enabled"))
                .and_then(|e| e.as_bool())
                .unwrap_or(true);

            // Update the enabled field
            if let Some(servers) = json.get_mut("servers").and_then(|s| s.as_object_mut()) {
                if let Some(server) = servers.get_mut(server_id).and_then(|s| s.as_object_mut()) {
                    server.insert("enabled".to_string(), serde_json::json!(enabled));

                    // Write back
                    let new_content = serde_json::to_string_pretty(&json)
                        .map_err(|e| format!("Failed to serialize config: {}", e))?;

                    fs::write(path, new_content)
                        .map_err(|e| format!("Failed to write config: {}", e))?;

                    return Ok(was_enabled != enabled); // Return true if state changed
                }
            }
        }
    }

    Err(format!(
        "Server '{}' not found in any config file",
        server_id
    ))
}

/// Handle /mcp allowlist <server> [tool] [--persistent]
async fn handle_mcp_allowlist(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 3 {
        return Ok(Some(mcp_allowlist_help()));
    }

    let action = parts[2];

    match action {
        "add" => {
            if parts.len() < 4 {
                return Ok(Some("Usage: /mcp allowlist add <server> [tool] [--persistent]\n\nExample: /mcp allowlist add filesystem --persistent".to_string()));
            }

            let server_id = parts[3];
            let tool = parts.get(4).copied().filter(|t| *t != "--persistent");
            let persistent = parts.contains(&"--persistent");

            // For now, just report what would happen (actual integration requires McpMode context)
            let target = if persistent {
                "persistently"
            } else {
                "for this session"
            };
            if let Some(t) = tool {
                Ok(Some(format!(
                    "Would allow tool '{}' from server '{}' {}",
                    t, server_id, target
                )))
            } else {
                Ok(Some(format!(
                    "Would allow all tools from server '{}' {}",
                    server_id, target
                )))
            }
        }
        "remove" => {
            if parts.len() < 4 {
                return Ok(Some(
                    "Usage: /mcp allowlist remove <server> [tool]".to_string(),
                ));
            }

            let server_id = parts[3];
            let tool = parts.get(4);

            if let Some(t) = tool {
                Ok(Some(format!(
                    "Would remove tool '{}' from server '{}' allowlist",
                    t, server_id
                )))
            } else {
                Ok(Some(format!(
                    "Would remove all tools from server '{}' allowlist",
                    server_id
                )))
            }
        }
        "list" => {
            // For now, return a placeholder message
            Ok(Some("Allowlist management requires MCP Mode to be active.\n\nUse /mcp open to access the full allowlist UI.".to_string()))
        }
        _ => Ok(Some(mcp_allowlist_help())),
    }
}

/// Handle MCP list command - shows configured servers
async fn handle_mcp_list() -> Result<Option<String>, String> {
    use rustycode_mcp::{ManagerConfig, McpConfigFile, McpServerManager};

    let manager = McpServerManager::new(ManagerConfig::default());

    // Get loaded servers
    let servers = manager.list_servers().await;

    // Also check for config files
    let configs = McpConfigFile::load_from_standard_locations();

    if servers.is_empty() && configs.is_empty() {
        return Ok(Some(
            "No MCP servers configured.\n\
            \n\
            Configure MCP servers in one of these locations:\n\
            • ./mcp.json (project-local)\n\
            • ~/.rustycode/mcp-servers.json (RustyCode config)\n\
            • ~/.mcp.json (home directory)\n\
            \n\
            Example mcp.json:\n\
            {\n\
            \n  \"servers\": {\n\
            \n    \"filesystem\": {\n\
            \n      \"server_id\": \"filesystem\",\n\
            \n      \"name\": \"Filesystem Server\",\n\
            \n      \"command\": \"npx\",\n\
            \n      \"args\": [\"-y\", \"@modelcontextprotocol/server-filesystem\", \".\"],\n\
            \n      \"enable_tools\": true,\n\
            \n      \"enabled\": true\n\
            \n    }\n\
            \n  }\n\
            \n}"
            .to_string(),
        ));
    }

    let mut output = String::from("MCP Servers:\n\n");

    // List servers from config files with status
    for (path, config) in &configs {
        for (server_name, server_config) in &config.servers {
            let is_running = servers.contains(server_name);
            let is_enabled = server_config.enabled;

            let status = match (is_running, is_enabled) {
                (true, true) => "✓ running",
                (true, false) => "○ running (disabled)",
                (false, true) => "● configured",
                (false, false) => "○ disabled",
            };

            let location = if configs.len() > 1 {
                format!(
                    " ({:?})",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            } else {
                String::new()
            };

            output.push_str(&format!("  • {} {}{}\n", server_name, status, location));

            // Show tags if present
            if !server_config.tags.is_empty() {
                output.push_str(&format!("    Tags: {}\n", server_config.tags.join(", ")));
            }

            // Show tool filtering if present
            if !server_config.tools_allowlist.is_empty() {
                output.push_str(&format!(
                    "    Tools: {}\n",
                    server_config.tools_allowlist.join(", ")
                ));
            }
            if !server_config.tools_denylist.is_empty() {
                output.push_str(&format!(
                    "    Blocked: {}\n",
                    server_config.tools_denylist.join(", ")
                ));
            }
        }
    }

    output.push_str("\nCommands:\n");
    output.push_str("  /mcp enable <server>   - Enable a server\n");
    output.push_str("  /mcp disable <server>  - Disable a server\n");
    output.push_str("  /mcp toggle <server>   - Toggle server state\n");

    Ok(Some(output))
}

/// Handle MCP debug command - shows detailed diagnostics
async fn handle_mcp_debug() -> Result<Option<String>, String> {
    use rustycode_mcp::ManagerConfig;
    use rustycode_mcp::McpServerManager;
    use std::fs;
    use std::path::PathBuf;

    let mut output = String::from("MCP Debug Diagnostics\n\n");

    // 1. Configuration files
    output.push_str("--- Configuration Files ---\n\n");

    let mut config_locations: Vec<PathBuf> = vec![PathBuf::from("mcp.json")];

    if let Some(home) = dirs::home_dir() {
        config_locations.push(home.join(".rustycode/mcp-servers.json"));
        config_locations.push(home.join(".mcp.json"));
    }

    for location in &config_locations {
        if location.exists() {
            output.push_str(&format!("+ {:?}\n", location));
            if let Ok(content) = fs::read_to_string(location) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(servers) = json.get("servers").and_then(|s| s.as_object()) {
                        output.push_str(&format!("  Servers: {}\n", servers.len()));
                        for (name, config) in servers {
                            let enabled = config
                                .get("enabled")
                                .and_then(|e| e.as_bool())
                                .unwrap_or(true);
                            output.push_str(&format!("    - {} (enabled: {})\n", name, enabled));
                        }
                    }
                }
            }
        } else {
            output.push_str(&format!("x {:?} (not found)\n", location));
        }
    }

    // 2. Running servers
    output.push_str("\n--- Running Servers ---\n\n");

    let manager = McpServerManager::new(ManagerConfig::default());
    let servers = manager.list_servers().await;

    if servers.is_empty() {
        output.push_str("No servers currently running\n");
    } else {
        for server_id in &servers {
            output.push_str(&format!("* {}\n", server_id));
            let display = manager.get_server_display_state(server_id).await;
            output.push_str(&format!("  Enabled: {}\n", display.enabled));
            if display.is_session_disabled {
                output.push_str("  ! Disabled for session\n");
            }
            if display.is_persistent_disabled {
                output.push_str("  ! Disabled in config\n");
            }
            if display.is_admin_blocked {
                output.push_str("  ! Blocked by admin allowlist\n");
            }
            if display.is_excludelist_blocked {
                output.push_str("  ! Blocked by admin excludelist\n");
            }
        }
    }

    // 3. Allowlist status
    output.push_str("\n--- Tool Allowlist ---\n\n");

    let allowlist_path = dirs::home_dir().map(|p| p.join(".rustycode/mcp-allowlist.json"));

    if let Some(path) = &allowlist_path {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    output.push_str(&format!("+ {:?}\n", path));
                    if let Some(entries) = json.as_array() {
                        output.push_str(&format!("  Entries: {}\n", entries.len()));
                        for entry in entries.iter().take(10) {
                            if let Some(obj) = entry.as_object() {
                                if let Some(tool_type) = obj.get("type").and_then(|t| t.as_str()) {
                                    output.push_str(&format!("    - Type: {}\n", tool_type));
                                }
                            }
                        }
                        if entries.len() > 10 {
                            output.push_str(&format!("    ... and {} more\n", entries.len() - 10));
                        }
                    }
                }
            }
        } else {
            output.push_str("No persistent allowlist configured\n");
        }
    }

    // 4. OAuth tokens (if any)
    output.push_str("\n--- OAuth Configuration ---\n\n");

    let token_path = dirs::home_dir().map(|p| p.join(".rustycode/mcp-tokens.json"));

    if let Some(path) = &token_path {
        if path.exists() {
            output.push_str(&format!("* Token store: {:?}\n", path));
        } else {
            output.push_str("No OAuth tokens stored\n");
        }
    } else {
        output.push_str("Token storage path unavailable\n");
    }

    // 5. Environment
    output.push_str("\n--- Environment ---\n\n");

    if let Some(home) = dirs::home_dir() {
        output.push_str(&format!("RUSTYCODE_HOME: {:?}\n", home.join(".rustycode")));
    }

    // Check for common MCP tools
    let mcp_tools = vec![
        ("npx", "Node.js package executor"),
        ("uvx", "uv package executor"),
    ];

    output.push_str("\n--- Tool Executors ---\n\n");
    for (tool, description) in &mcp_tools {
        let available = which::which(tool).is_ok();
        if available {
            output.push_str(&format!("+ {} ({})\n", tool, description));
        } else {
            output.push_str(&format!("x {} ({}) - not installed\n", tool, description));
        }
    }

    Ok(Some(output))
}

/// Handle MCP status command
async fn handle_mcp_status() -> Result<Option<String>, String> {
    use rustycode_mcp::{ManagerConfig, McpConfigFile, McpServerManager};

    let mut manager = McpServerManager::new(ManagerConfig::default());
    if let Err(e) = manager.load_from_standard_locations().await {
        tracing::warn!("MCP config load failed: {}", e);
    }
    let servers = manager.list_servers().await;

    // Manually check standard locations to get detailed diagnostics
    let mut configs = Vec::new();
    let mut config_errors = Vec::new();
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let locations: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("mcp.json"),
        home.join(".rustycode").join("mcp-servers.json"),
        home.join(".mcp.json"),
    ];

    for location in &locations {
        if location.exists() {
            match McpConfigFile::load_from_path(location) {
                Ok(config) => configs.push((location.clone(), config)),
                Err(e) => config_errors.push(format!("{:?}: {}", location, e)),
            }
        }
    }

    let configured = configs.iter().map(|(_, c)| c.servers.len()).sum::<usize>();
    let running = servers.len();

    let mut output = format!(
        "MCP Status\n\n\
         Configured servers: {}\n\
         Running servers: {}\n",
        configured, running
    );

    if configs.is_empty() {
        output.push_str("\nNo config files found or loaded.\n");
        if !config_errors.is_empty() {
            output.push_str("Errors:\n");
            for err in &config_errors {
                output.push_str(&format!("  - {}\n", err));
            }
        }
    }

    output.push_str("\nUse /mcp list for details\nUse /mcp debug for diagnostics");

    Ok(Some(output))
}

/// Handle MCP reload command - reloads servers from config files
async fn handle_mcp_reload() -> Result<Option<String>, String> {
    use rustycode_mcp::{ManagerConfig, McpServerManager};

    let mut manager = McpServerManager::new(ManagerConfig::default());
    let started = manager.load_from_standard_locations().await?;

    Ok(Some(format!(
        "✅ Reloaded {} MCP server(s) from config files",
        started
    )))
}

/// Handle MCP call/exec command - execute a tool directly (non-interactive)
async fn handle_mcp_call(parts: &[&str]) -> Result<Option<String>, String> {
    if parts.len() < 4 {
        return Ok(Some(
            "Usage: /mcp call <server> <tool> [args...]\n\
            \n\
            Execute an MCP tool directly without opening MCP Mode.\n\
            Arguments are passed as a JSON object.\n\
            \n\
            Examples:\n\
            /mcp call filesystem read_file {\"path\": \"./Cargo.toml\"}\n\
            /mcp call filesystem write_file {\"path\": \"test.txt\", \"content\": \"hello\"}\n\
            /mcp exec filesystem list_files {\"path\": \".\"}"
                .to_string(),
        ));
    }

    let server_id = parts[2];
    let tool_name = parts[3];

    // Parse arguments as JSON if provided
    let args: serde_json::Value = if parts.len() > 4 {
        let args_json = parts[4..].join(" ");
        match serde_json::from_str(&args_json) {
            Ok(v) => v,
            Err(e) => {
                return Ok(Some(format!(
                    "❌ Invalid JSON arguments: {}\n\
                    \n\
                    Arguments must be valid JSON. Example: {{\"path\": \".\"}}",
                    e
                )))
            }
        }
    } else {
        serde_json::json!({})
    };

    // Connect to the server and execute the tool
    use rustycode_mcp::proxy::{ProxyConfig, ToolProxy};

    let configs = rustycode_mcp::McpConfigFile::load_from_standard_locations();
    let server_config = configs
        .iter()
        .find_map(|(_, config)| config.servers.get(server_id))
        .ok_or_else(|| {
            format!(
                "Server '{}' not found in config.\n\
                Use /mcp list to see available servers.",
                server_id
            )
        })?;

    if !server_config.enabled {
        return Ok(Some(format!(
            "Server '{}' is disabled. Use /mcp enable {} to enable it.",
            server_id, server_id
        )));
    }

    let command = server_config.command.clone().ok_or_else(|| {
        format!(
            "Server '{}' uses a remote transport and does not support direct tool execution via /mcp call.",
            server_id
        )
    })?;

    let proxy_config = ProxyConfig {
        server_name: server_id.to_string(),
        command,
        args: server_config.args.clone(),
        tool_prefix: None,
        cache_tools: true,
    };

    // Connect and execute
    let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        match ToolProxy::with_discovery(proxy_config).await {
            Ok(proxy) => {
                // Verify tool exists
                let tools = proxy.get_tools().await;
                let tool_exists = tools.iter().any(|t| t.name == tool_name);

                if !tool_exists {
                    let available = tools
                        .iter()
                        .map(|t| t.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(format!(
                        "Tool '{}' not found on server '{}'.\n\
                            Available tools: {}",
                        tool_name, server_id, available
                    ));
                }

                // Execute the tool
                match proxy.call_tool(tool_name, args).await {
                    Ok(result) => Ok(result),
                    Err(e) => Err(format!("Tool execution failed: {}", e)),
                }
            }
            Err(e) => Err(format!(
                "Failed to connect to server '{}': {}",
                server_id, e
            )),
        }
    })
    .await;

    match result {
        Ok(Ok(tool_result)) => {
            // Format the result for display
            let output = format_tool_result(tool_name, &tool_result);
            Ok(Some(output))
        }
        Ok(Err(e)) => Ok(Some(format!("❌ {}", e))),
        Err(_) => Ok(Some(format!(
            "❌ Timeout: Tool '{}' on server '{}' took too long to execute (>30s)",
            tool_name, server_id
        ))),
    }
}

/// Format tool execution result for display
fn format_tool_result(tool_name: &str, result: &rustycode_mcp::McpToolResult) -> String {
    let mut output = format!("Tool: {}\n\nResult:\n", tool_name);

    // Extract content from MCP tool result
    for item in &result.content {
        match item {
            rustycode_mcp::McpContent::Text { text } => {
                output.push_str(text);
                output.push('\n');
            }
            rustycode_mcp::McpContent::Image { data, mime_type } => {
                output.push_str(&format!(
                    "[Image: {} type, {} bytes]\n",
                    mime_type,
                    data.len()
                ));
            }
            rustycode_mcp::McpContent::Resource { uri, mime_type } => {
                let mime_str = mime_type.as_deref().unwrap_or("unknown");
                output.push_str(&format!("[Resource: {} type {}]\n", mime_str, uri));
            }
            #[allow(unreachable_patterns)]
            _ => {
                output.push_str("[Unknown content]\n");
            }
        }
    }

    if result.is_error == Some(true) {
        output.insert_str(0, "⚠️ Error: ");
    }

    output
}

/// Get MCP help text
fn mcp_help() -> String {
    "MCP (Model Context Protocol) - External Tool Servers\n\
    \n\
    Commands:\n\
    \n\
    * /mcp list                - Show configured MCP servers\n\
    * /mcp status              - Show MCP connection status\n\
    * /mcp debug               - Show detailed diagnostics\n\
    * /mcp reload              - Reload servers from config files\n\
    * /mcp enable <server>     - Enable a server\n\
    * /mcp disable <server>    - Disable a server\n\
    * /mcp toggle <server>     - Toggle server enabled state\n\
    • /mcp allowlist           - Manage tool auto-approval\n\
    • /mcp open                - Open MCP Mode UI\n\
    • /mcp call <srv> <tool>   - Execute tool directly (non-interactive)\n\
    • /mcp exec <srv> <tool>   - Alias for call\n\
    \n\
    Non-interactive execution:\n\
    \n\
    Use /mcp call or /mcp exec to run tools without opening MCP Mode.\n\
    Useful for scripting and quick operations.\n\
    \n\
    Examples:\n\
    /mcp call filesystem read_file {\"path\": \"./Cargo.toml\"}\n\
    /mcp exec filesystem list_files {\"path\": \".\"}\n\
    \n\
    MCP servers provide additional tools through the Model Context Protocol.\n\
    \n\
    Configuration: Create ~/.rustycode/mcp-servers.json or ./mcp.json\n\
    \n\
    Example:\n\
    {\n\
    \n  \"servers\": {\n\
    \n    \"filesystem\": {\n\
    \n      \"server_id\": \"filesystem\",\n\
    \n      \"name\": \"Filesystem Server\",\n\
    \n      \"command\": \"npx\",\n\
    \n      \"args\": [\"-y\", \"@modelcontextprotocol/server-filesystem\", \".\"],\n\
    \n      \"enable_tools\": true,\n\
    \n      \"enabled\": true,\n\
    \n      \"tags\": [\"dev\", \"files\"]\n\
    \n    }\n\
    \n  }\n\
    \n}\n\
    \n\
    More info: https://modelcontextprotocol.io"
        .to_string()
}

/// Get MCP allowlist help text
fn mcp_allowlist_help() -> String {
    "MCP Allowlist - Auto-approve Tool Execution\n\
    \n\
    Commands:\n\
    \n\
    • /mcp allowlist add <server> [tool] [--persistent] - Allow tool(s)\n\
    • /mcp allowlist remove <server> [tool]             - Remove from allowlist\n\
    • /mcp allowlist list                               - List allowed tools\n\
    \n\
    Examples:\n\
    \n\
    /mcp allowlist add filesystem --persistent  # All tools, saved\n\
    /mcp allowlist add filesystem read_file     # Specific tool, session only\n\
    /mcp allowlist remove filesystem            # Remove all allowances\n\
    \n\
    Session allowances expire when RustyCode exits.\n\
    Persistent allowances are saved to ~/.rustycode/mcp-allowlist.json"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_help_not_empty() {
        let help = mcp_help();
        assert!(!help.is_empty());
        assert!(help.contains("MCP (Model Context Protocol)"));
        assert!(help.contains("/mcp list"));
        assert!(help.contains("/mcp enable"));
        assert!(help.contains("/mcp disable"));
        assert!(help.contains("/mcp toggle"));
    }

    #[tokio::test]
    async fn test_handle_mcp_command_help() {
        let result = handle_mcp_command("/mcp help").await.unwrap();
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.contains("MCP (Model Context Protocol)"));
    }

    #[tokio::test]
    async fn test_handle_mcp_command_empty() {
        let result = handle_mcp_command("/mcp").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("MCP (Model Context Protocol)"));
    }

    #[tokio::test]
    async fn test_handle_mcp_command_status() {
        let result = handle_mcp_command("/mcp status").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("MCP Status"));
    }

    #[tokio::test]
    async fn test_handle_mcp_command_open() {
        let result = handle_mcp_command("/mcp open").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("MCP Mode opened"));
    }

    #[tokio::test]
    async fn test_handle_mcp_command_unknown() {
        let result = handle_mcp_command("/mcp unknown_cmd").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Unknown MCP command"));
    }

    #[tokio::test]
    async fn test_handle_mcp_command_enable_no_arg() {
        let result = handle_mcp_command("/mcp enable").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Usage:"));
    }

    #[tokio::test]
    async fn test_handle_mcp_command_disable_no_arg() {
        let result = handle_mcp_command("/mcp disable").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Usage:"));
    }

    #[tokio::test]
    async fn test_handle_mcp_command_toggle_no_arg() {
        let result = handle_mcp_command("/mcp toggle").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Usage:"));
    }

    #[tokio::test]
    async fn test_handle_mcp_list_no_config() {
        let result = handle_mcp_command("/mcp list").await.unwrap();
        assert!(result.is_some());
        let output = result.unwrap();
        // Should either show servers or show "No MCP servers configured" message
        assert!(!output.is_empty());
    }

    #[tokio::test]
    async fn test_handle_mcp_call_no_args() {
        let result = handle_mcp_command("/mcp call").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Usage: /mcp call"));
    }

    #[tokio::test]
    async fn test_handle_mcp_call_missing_tool() {
        let result = handle_mcp_command("/mcp call myserver").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Usage: /mcp call"));
    }

    #[tokio::test]
    async fn test_handle_mcp_exec_alias() {
        let result = handle_mcp_command("/mcp exec").await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Usage: /mcp call"));
    }

    #[test]
    fn test_format_tool_result_text() {
        use rustycode_mcp::{McpContent, McpToolResult};

        let result = McpToolResult {
            content: vec![McpContent::Text {
                text: "Hello, World!".to_string(),
            }],
            structured_content: None,
            is_error: None,
            meta: None,
        };

        let output = format_tool_result("test_tool", &result);
        assert!(output.contains("Tool: test_tool"));
        assert!(output.contains("Hello, World!"));
    }

    #[test]
    fn test_format_tool_result_error() {
        use rustycode_mcp::{McpContent, McpToolResult};

        let result = McpToolResult {
            content: vec![McpContent::Text {
                text: "Something went wrong".to_string(),
            }],
            structured_content: None,
            is_error: Some(true),
            meta: None,
        };

        let output = format_tool_result("failing_tool", &result);
        assert!(output.contains("Error:"));
        assert!(output.contains("Something went wrong"));
    }

    #[test]
    fn test_format_tool_result_image() {
        use rustycode_mcp::{McpContent, McpToolResult};

        let result = McpToolResult {
            content: vec![McpContent::Image {
                data: "base64data".to_string(),
                mime_type: "image/png".to_string(),
            }],
            structured_content: None,
            is_error: None,
            meta: None,
        };

        let output = format_tool_result("image_tool", &result);
        assert!(output.contains("Image: image/png"));
        assert!(output.contains("10 bytes"));
    }

    #[test]
    fn test_format_tool_result_resource() {
        use rustycode_mcp::{McpContent, McpToolResult};

        let result = McpToolResult {
            content: vec![McpContent::Resource {
                uri: "file:///test.txt".to_string(),
                mime_type: Some("text/plain".to_string()),
            }],
            structured_content: None,
            is_error: None,
            meta: None,
        };

        let output = format_tool_result("resource_tool", &result);
        assert!(output.contains("Resource: text/plain"));
        assert!(output.contains("file:///test.txt"));
    }
}
