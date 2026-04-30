//! Permission Enforcement Example
//!
//! This example demonstrates how RustyCode's permission system works
//! to protect against unauthorized operations in different session modes.

use rustycode_protocol::{Session, SessionMode, ToolCall};
use rustycode_runtime::AsyncRuntime;
use rustycode_tools::{
    SandboxConfig, ToolContext, ToolPermission, check_tool_permission, get_allowed_tools,
    get_tool_permission,
};
use serde_json::json;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== RustyCode Permission Enforcement Demo ===\n");

    // Initialize runtime
    let runtime = AsyncRuntime::load(std::path::Path::new(".")).await?;
    println!("✓ Runtime initialized\n");

    // ── Demo 1: Permission Checking API ───────────────────────────────────
    println!("--- Demo 1: Permission Checking API ---");

    let read_perm = get_tool_permission("read_file");
    let write_perm = get_tool_permission("write_file");
    let bash_perm = get_tool_permission("bash");

    println!("read_file permission: {:?}", read_perm);
    println!("write_file permission: {:?}", write_perm);
    println!("bash permission: {:?}", bash_perm);

    println!("\nAllowed tools in Planning mode:");
    let planning_tools = get_allowed_tools(SessionMode::Planning);
    for tool in &planning_tools {
        println!("  ✓ {}", tool);
    }

    println!("\nAllowed tools in Executing mode:");
    let executing_tools = get_allowed_tools(SessionMode::Executing);
    for tool in &executing_tools {
        println!("  ✓ {}", tool);
    }

    println!(
        "\nPlanning mode has {} tools, Executing mode has {} tools",
        planning_tools.len(),
        executing_tools.len()
    );

    // ── Demo 2: Planning Mode Restrictions ────────────────────────────────
    println!("\n\n--- Demo 2: Planning Mode Restrictions ---");

    let planning_session = Session::builder()
        .task("Explore codebase")
        .with_mode(SessionMode::Planning)
        .build();

    println!("Created Planning mode session: {}", planning_session.id);

    // Try read operation (should succeed)
    println!("\n1. Attempting read_file in Planning mode...");
    let read_call = ToolCall {
        call_id: "1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "Cargo.toml"}),
    };

    match runtime
        .execute_tool(&planning_session.id, read_call, Path::new("."))
        .await
    {
        Ok(result) if result.success => {
            println!("   ✓ SUCCESS: Read operation allowed");
            println!(
                "   Output preview: {}...",
                &result.output.chars().take(50).collect::<String>()
            );
        }
        Ok(result) => {
            println!("   ✗ FAILED: {}", result.error.unwrap_or_default());
        }
        Err(e) => {
            println!("   ✗ ERROR: {}", e);
        }
    }

    // Try write operation (should fail)
    println!("\n2. Attempting write_file in Planning mode...");
    let write_call = ToolCall {
        call_id: "2".to_string(),
        name: "write_file".to_string(),
        arguments: json!({
            "path": "test_planning.txt",
            "content": "This should fail in planning mode"
        }),
    };

    match runtime
        .execute_tool(&planning_session.id, write_call, Path::new("."))
        .await
    {
        Ok(result) if !result.success => {
            println!("   ✓ BLOCKED: Write operation correctly blocked");
            println!("   Error: {}", result.error.unwrap_or_default());
        }
        Ok(_result) => {
            println!("   ✗ UNEXPECTED: Write operation should have been blocked!");
        }
        Err(e) => {
            println!("   ✗ ERROR: {}", e);
        }
    }

    // Try bash execution (should fail)
    println!("\n3. Attempting bash command in Planning mode...");
    let bash_call = ToolCall {
        call_id: "3".to_string(),
        name: "bash".to_string(),
        arguments: json!({"command": "echo 'test'"}),
    };

    match runtime
        .execute_tool(&planning_session.id, bash_call, Path::new("."))
        .await
    {
        Ok(result) if !result.success => {
            println!("   ✓ BLOCKED: Bash execution correctly blocked");
            println!("   Error: {}", result.error.unwrap_or_default());
        }
        Ok(_result) => {
            println!("   ✗ UNEXPECTED: Bash execution should have been blocked!");
        }
        Err(e) => {
            println!("   ✗ ERROR: {}", e);
        }
    }

    // ── Demo 3: Executing Mode Permissions ─────────────────────────────────
    println!("\n\n--- Demo 3: Executing Mode Permissions ---");

    let executing_session = Session::builder()
        .task("Implement changes")
        .with_mode(SessionMode::Executing)
        .build();

    println!("Created Executing mode session: {}", executing_session.id);

    // Try read operation (should succeed)
    println!("\n1. Attempting read_file in Executing mode...");
    let read_call = ToolCall {
        call_id: "1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "Cargo.toml"}),
    };

    match runtime
        .execute_tool(&executing_session.id, read_call, std::path::Path::new("."))
        .await
    {
        Ok(result) if result.success => {
            println!("   ✓ SUCCESS: Read operation allowed");
        }
        Ok(result) => {
            println!("   ✗ FAILED: {}", result.error.unwrap_or_default());
        }
        Err(e) => {
            println!("   ✗ ERROR: {}", e);
        }
    }

    // Try write operation (should succeed)
    println!("\n2. Attempting write_file in Executing mode...");
    let write_call = ToolCall {
        call_id: "2".to_string(),
        name: "write_file".to_string(),
        arguments: json!({
            "path": "test_executing.txt",
            "content": "This should succeed in executing mode"
        }),
    };

    match runtime
        .execute_tool(&executing_session.id, write_call, std::path::Path::new("."))
        .await
    {
        Ok(result) if result.success => {
            println!("   ✓ SUCCESS: Write operation allowed");
            println!("   Created: test_executing.txt");

            // Cleanup
            let _ = std::fs::remove_file("test_executing.txt");
        }
        Ok(result) => {
            println!("   ✗ FAILED: {}", result.error.unwrap_or_default());
        }
        Err(e) => {
            println!("   ✗ ERROR: {}", e);
        }
    }

    // Try bash execution (should succeed)
    println!("\n3. Attempting bash command in Executing mode...");
    let bash_call = ToolCall {
        call_id: "3".to_string(),
        name: "bash".to_string(),
        arguments: json!({"command": "echo 'Hello from Executing mode!'"}),
    };

    match runtime
        .execute_tool(&executing_session.id, bash_call, std::path::Path::new("."))
        .await
    {
        Ok(result) if result.success => {
            println!("   ✓ SUCCESS: Bash execution allowed");
            println!("   Output: {}", result.output.trim());
        }
        Ok(result) => {
            println!("   ✗ FAILED: {}", result.error.unwrap_or_default());
        }
        Err(e) => {
            println!("   ✗ ERROR: {}", e);
        }
    }

    // ── Demo 4: Sandbox Configuration ──────────────────────────────────────
    println!("\n\n--- Demo 4: Sandbox Configuration ---");

    let sandbox = SandboxConfig::new()
        .allow_path("/workspace/src")
        .deny_path("/workspace/src/internal")
        .timeout(10)
        .max_output(1_048_576); // 1MB

    let ctx = ToolContext::new("/workspace")
        .with_sandbox(sandbox)
        .with_max_permission(ToolPermission::Write);

    println!("Created sandboxed ToolContext:");
    println!("  Working directory: /workspace");
    println!("  Allowed paths: {:?}", ctx.sandbox.allowed_paths);
    println!("  Denied paths: {:?}", ctx.sandbox.denied_paths);
    println!("  Timeout: {:?}", ctx.sandbox.timeout_secs);
    println!("  Max output: {:?} bytes", ctx.sandbox.max_output_bytes);
    println!("  Max permission: {:?}", ctx.max_permission);

    println!("\nSandbox benefits:");
    println!("  ✓ Restricts file access to specific directories");
    println!("  ✓ Blocks access to sensitive paths");
    println!("  ✓ Prevents resource exhaustion with timeouts");
    println!("  ✓ Limits output size to prevent memory issues");
    println!("  ✓ Caps permission level for additional safety");

    // ── Demo 5: Permission Hierarchy ────────────────────────────────────────
    println!("\n\n--- Demo 5: Permission Hierarchy ---");

    println!("Permission levels (lowest to highest):");
    println!("  1. Read    - Read-only operations");
    println!("  2. Write   - File modification");
    println!("  3. Execute - Command execution");
    println!("  4. Network - Network access (future)");

    println!("\nTools by permission level:");

    println!("\n  Read Permission:");
    for tool in &[
        "read_file",
        "list_dir",
        "grep",
        "glob",
        "git_status",
        "git_diff",
        "git_log",
        "lsp_diagnostics",
    ] {
        println!("    - {}", tool);
    }

    println!("\n  Write Permission:");
    for tool in &["write_file", "git_commit"] {
        println!("    - {}", tool);
    }

    println!("\n  Execute Permission:");
    {
        let tool = &"bash";
        println!("    - {}", tool);
    }

    println!("\n  Network Permission:");
    println!("    - (none currently, reserved for future)");

    // ── Demo 6: Security Best Practices ────────────────────────────────────
    println!("\n\n--- Demo 6: Security Best Practices ---");

    println!("✓ Always start in Planning mode for exploration");
    println!("✓ Review plans carefully before approving");
    println!("✓ Use sandboxing for untrusted code");
    println!("✓ Monitor tool execution logs");
    println!("✓ Set appropriate timeouts and limits");
    println!("✓ Validate user input before tool execution");
    println!("✓ Implement audit logging for sensitive operations");

    println!("\n\n=== Demo Complete ===");
    println!("\nKey Takeaways:");
    println!("  1. Planning mode blocks Write and Execute operations");
    println!("  2. Executing mode allows all operations");
    println!("  3. Permission checking happens at multiple levels");
    println!("  4. Sandbox configuration adds additional security layers");
    println!("  5. The system is designed to be safe by default");

    Ok(())
}

/// Helper function to display tool permission status
#[allow(dead_code)]
fn show_permission_status(tool_name: &str, mode: SessionMode) {
    let allowed = check_tool_permission(tool_name, mode);
    let permission = get_tool_permission(tool_name);

    println!(
        "  {}: {:?} permission - {} in {:?} mode",
        tool_name,
        permission,
        if allowed {
            "✓ Allowed"
        } else {
            "✗ Blocked"
        },
        mode
    );
}
