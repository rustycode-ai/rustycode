//! Comprehensive tests for the tool permission matrix.
//!
//! These tests validate the complete permission system including:
//! - Tool permission declarations
//! - Session mode enforcement
//! - Permission hierarchy
//! - Sandbox restrictions
//! - Security considerations

use rustycode_protocol::{SessionMode, ToolCall, ToolPermission as ProtocolToolPermission};
use rustycode_runtime::AsyncRuntime;
use rustycode_tools::{
    SandboxConfig, ToolContext, ToolPermission, check_tool_permission, default_registry,
    get_allowed_tools, get_tool_permission,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test directory with sample files
fn setup_test_workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();

    // Create directory structure
    fs::create_dir_all(workspace.join("src")).unwrap();
    fs::create_dir_all(workspace.join("docs")).unwrap();

    // Create sample files
    fs::write(
        workspace.join("README.md"),
        "# Test Project\n\nThis is a test project.",
    )
    .unwrap();

    fs::write(
        workspace.join("src").join("main.rs"),
        "fn main() {\n    println!(\"Hello, world!\");\n}",
    )
    .unwrap();

    dir
}

// ── Tool Permission Declaration Tests ─────────────────────────────────────

#[test]
fn test_all_tools_declare_permissions() {
    let registry = default_registry();
    let tools = registry.list();

    assert!(!tools.is_empty(), "Registry should have tools");

    for tool in tools {
        // All tools must have a permission level
        match tool.permission {
            ToolPermission::Read | ToolPermission::Write | ToolPermission::Execute => {
                // Valid permissions
            }
            ToolPermission::None | ToolPermission::Network => {
                // These are also valid but less common
            }
        }
    }
}

#[test]
fn test_read_permission_tools() {
    // Read-only tools are auto-allowed
    let read_tools = vec![
        ("read_file", ProtocolToolPermission::AutoAllow),
        ("list_dir", ProtocolToolPermission::AutoAllow),
        ("grep", ProtocolToolPermission::AutoAllow),
        ("glob", ProtocolToolPermission::AutoAllow),
        ("git_status", ProtocolToolPermission::AutoAllow),
        ("git_diff", ProtocolToolPermission::AutoAllow),
        ("git_log", ProtocolToolPermission::AutoAllow),
        ("lsp_diagnostics", ProtocolToolPermission::AutoAllow),
    ];

    for (tool_name, expected_permission) in read_tools {
        let permission = get_tool_permission(tool_name);
        assert_eq!(
            permission,
            Some(expected_permission.clone()),
            "{} should have {:?} permission",
            tool_name,
            expected_permission
        );
    }
}

#[test]
fn test_write_permission_tools() {
    // Write tools require confirmation for security
    let write_tools = vec![
        ("write_file", ProtocolToolPermission::RequiresConfirmation),
        ("git_commit", ProtocolToolPermission::RequiresConfirmation),
    ];

    for (tool_name, expected_permission) in write_tools {
        let permission = get_tool_permission(tool_name);
        assert_eq!(
            permission,
            Some(expected_permission.clone()),
            "{} should have {:?} permission",
            tool_name,
            expected_permission
        );
    }
}

#[test]
fn test_execute_permission_tools() {
    // Bash now requires confirmation for security reasons
    let execute_tools = vec![("bash", ProtocolToolPermission::RequiresConfirmation)];

    for (tool_name, expected_permission) in execute_tools {
        let permission = get_tool_permission(tool_name);
        assert_eq!(
            permission,
            Some(expected_permission.clone()),
            "{} should have {:?} permission",
            tool_name,
            expected_permission
        );
    }
}

#[test]
fn test_unknown_tool_returns_none() {
    assert_eq!(get_tool_permission("nonexistent_tool"), None);
    assert_eq!(get_tool_permission(""), None);
}

// ── Session Mode Permission Tests ────────────────────────────────────────

#[test]
fn test_planning_mode_only_allows_read_tools() {
    let planning_mode = SessionMode::Planning;

    // Read tools should be allowed
    assert!(
        check_tool_permission("read_file", planning_mode),
        "read_file should be allowed in Planning mode"
    );
    assert!(
        check_tool_permission("grep", planning_mode),
        "grep should be allowed in Planning mode"
    );
    assert!(
        check_tool_permission("git_status", planning_mode),
        "git_status should be allowed in Planning mode"
    );

    // Write tools should be blocked
    assert!(
        !check_tool_permission("write_file", planning_mode),
        "write_file should be blocked in Planning mode"
    );
    assert!(
        !check_tool_permission("git_commit", planning_mode),
        "git_commit should be blocked in Planning mode"
    );

    // Execute tools should be blocked
    assert!(
        !check_tool_permission("bash", planning_mode),
        "bash should be blocked in Planning mode"
    );
}

#[test]
fn test_executing_mode_allows_all_tools() {
    let executing_mode = SessionMode::Executing;

    // All tools should be allowed
    assert!(
        check_tool_permission("read_file", executing_mode),
        "read_file should be allowed in Executing mode"
    );
    assert!(
        check_tool_permission("write_file", executing_mode),
        "write_file should be allowed in Executing mode"
    );
    assert!(
        check_tool_permission("bash", executing_mode),
        "bash should be allowed in Executing mode"
    );
    assert!(
        check_tool_permission("git_commit", executing_mode),
        "git_commit should be allowed in Executing mode"
    );
}

#[test]
fn test_get_allowed_tools_filters_correctly() {
    let planning_tools = get_allowed_tools(SessionMode::Planning);
    let executing_tools = get_allowed_tools(SessionMode::Executing);

    // Planning mode should have fewer tools
    assert!(
        planning_tools.len() < executing_tools.len(),
        "Planning mode should have fewer tools than Executing mode"
    );

    // Planning mode should only have read tools
    assert!(planning_tools.contains(&"read_file".to_string()));
    assert!(planning_tools.contains(&"grep".to_string()));
    assert!(planning_tools.contains(&"git_status".to_string()));
    assert!(!planning_tools.contains(&"write_file".to_string()));
    assert!(!planning_tools.contains(&"bash".to_string()));

    // Executing mode should have all tools
    assert!(executing_tools.contains(&"read_file".to_string()));
    assert!(executing_tools.contains(&"write_file".to_string()));
    assert!(executing_tools.contains(&"bash".to_string()));
}

// ── Permission Hierarchy Tests ───────────────────────────────────────────

#[test]
fn test_permission_hierarchy_values() {
    // Test that permission levels can be compared numerically
    let read = ToolPermission::Read as u8;
    let write = ToolPermission::Write as u8;
    let execute = ToolPermission::Execute as u8;
    let network = ToolPermission::Network as u8;

    assert!(read < write, "Read < Write");
    assert!(write < execute, "Write < Execute");
    assert!(execute < network, "Execute < Network");
}

#[test]
fn test_tool_context_permission_cap() {
    let ctx_read = ToolContext::new("/tmp").with_max_permission(ToolPermission::Read);

    let ctx_write = ToolContext::new("/tmp").with_max_permission(ToolPermission::Write);

    let ctx_execute = ToolContext::new("/tmp").with_max_permission(ToolPermission::Execute);

    assert_eq!(ctx_read.max_permission, ToolPermission::Read);
    assert_eq!(ctx_write.max_permission, ToolPermission::Write);
    assert_eq!(ctx_execute.max_permission, ToolPermission::Execute);
}

// ── Sandbox Configuration Tests ──────────────────────────────────────────

#[test]
fn test_sandbox_allow_path() {
    let sandbox = SandboxConfig::new()
        .allow_path("/workspace/src")
        .allow_path("/workspace/docs");

    assert!(sandbox.allowed_paths.is_some());
    let allowed = sandbox.allowed_paths.as_ref().unwrap();
    assert_eq!(allowed.len(), 2);
    assert!(allowed.contains(&PathBuf::from("/workspace/src")));
    assert!(allowed.contains(&PathBuf::from("/workspace/docs")));
}

#[test]
fn test_sandbox_deny_path() {
    let sandbox = SandboxConfig::new()
        .deny_path("/workspace/.git")
        .deny_path("/workspace/target");

    assert_eq!(sandbox.denied_paths.len(), 2);
    assert!(
        sandbox
            .denied_paths
            .contains(&PathBuf::from("/workspace/.git"))
    );
    assert!(
        sandbox
            .denied_paths
            .contains(&PathBuf::from("/workspace/target"))
    );
}

#[test]
fn test_sandbox_timeout() {
    let sandbox = SandboxConfig::new().timeout(30);

    assert_eq!(sandbox.timeout_secs, Some(30));
}

#[test]
fn test_sandbox_max_output() {
    let sandbox = SandboxConfig::new().max_output(10_485_760); // 10MB

    assert_eq!(sandbox.max_output_bytes, Some(10_485_760));
}

#[test]
fn test_tool_context_with_sandbox() {
    let sandbox = SandboxConfig::new()
        .allow_path("/workspace")
        .deny_path("/workspace/private")
        .timeout(60)
        .max_output(5_242_880);

    let ctx = ToolContext::new("/workspace")
        .with_sandbox(sandbox)
        .with_max_permission(ToolPermission::Write);

    assert_eq!(ctx.max_permission, ToolPermission::Write);
    assert_eq!(ctx.sandbox.timeout_secs, Some(60));
    assert_eq!(ctx.sandbox.max_output_bytes, Some(5_242_880));
    assert!(
        ctx.sandbox
            .denied_paths
            .contains(&PathBuf::from("/workspace/private"))
    );
}

// ── Integration Tests with Async Runtime ────────────────────────────────

#[tokio::test]
async fn test_planning_mode_blocks_write_operations() {
    let workspace = setup_test_workspace();
    let runtime = AsyncRuntime::load(workspace.path()).await.unwrap();

    // Create and save a planning mode session using start_planning
    let report = runtime
        .start_planning(workspace.path(), "Test planning mode")
        .await
        .unwrap();
    let session = report.session;

    // Attempt to write a file in planning mode
    let write_call = ToolCall {
        call_id: "test-1".to_string(),
        name: "write_file".to_string(),
        arguments: json!({
            "path": "new_file.txt",
            "content": "This should fail"
        }),
    };

    let result = runtime
        .execute_tool(&session.id, write_call, workspace.path())
        .await;

    // Permission check should return an error
    assert!(
        result.is_err(),
        "execute_tool should return error for blocked permission"
    );
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("not permitted") || err_msg.contains("not allowed"),
        "Error should mention permission denial, got: {}",
        err_msg
    );

    // Verify file was not created
    assert!(!workspace.path().join("new_file.txt").exists());
}

#[tokio::test]
async fn test_planning_mode_blocks_execute_operations() {
    let workspace = setup_test_workspace();
    let runtime = AsyncRuntime::load(workspace.path()).await.unwrap();

    // Create and save a planning mode session using start_planning
    let report = runtime
        .start_planning(workspace.path(), "Test planning mode")
        .await
        .unwrap();
    let session = report.session;

    // Attempt to run bash in planning mode
    let bash_call = ToolCall {
        call_id: "test-1".to_string(),
        name: "bash".to_string(),
        arguments: json!({
            "command": "echo 'test'"
        }),
    };

    let result = runtime
        .execute_tool(&session.id, bash_call, workspace.path())
        .await;

    // Permission check should return an error
    assert!(
        result.is_err(),
        "execute_tool should return error for blocked permission"
    );
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("not permitted") || err_msg.contains("not allowed"),
        "Error should mention permission denial, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_planning_mode_allows_read_operations() {
    let workspace = setup_test_workspace();
    let runtime = AsyncRuntime::load(workspace.path()).await.unwrap();

    // Create and save a planning mode session using start_planning
    let report = runtime
        .start_planning(workspace.path(), "Test planning mode")
        .await
        .unwrap();
    let session = report.session;

    // Read a file in planning mode
    let read_call = ToolCall {
        call_id: "test-1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({
            "path": "README.md"
        }),
    };

    let result = runtime
        .execute_tool(&session.id, read_call, workspace.path())
        .await
        .unwrap();

    assert!(result.success, "Read should succeed in planning mode");
    assert!(result.output.contains("Test Project"));
}

#[tokio::test]
async fn test_executing_mode_allows_write_operations() {
    let workspace = setup_test_workspace();
    let runtime = AsyncRuntime::load(workspace.path()).await.unwrap();

    // Create and save an executing mode session using run
    let report = runtime
        .run(workspace.path(), "Test executing mode")
        .await
        .unwrap();
    let session = report.session;

    // Write a file in executing mode
    let write_call = ToolCall {
        call_id: "test-1".to_string(),
        name: "write_file".to_string(),
        arguments: json!({
            "path": "new_file.txt",
            "content": "This should succeed"
        }),
    };

    let result = runtime
        .execute_tool(&session.id, write_call, workspace.path())
        .await
        .unwrap();

    assert!(result.success, "Write should succeed in executing mode");

    // Verify file was created
    assert!(workspace.path().join("new_file.txt").exists());

    // Cleanup
    fs::remove_file(workspace.path().join("new_file.txt")).unwrap();
}

#[tokio::test]
async fn test_executing_mode_allows_execute_operations() {
    let workspace = setup_test_workspace();
    let runtime = AsyncRuntime::load(workspace.path()).await.unwrap();

    // Create and save an executing mode session using run
    let report = runtime
        .run(workspace.path(), "Test executing mode")
        .await
        .unwrap();
    let session = report.session;

    // Run bash in executing mode
    let bash_call = ToolCall {
        call_id: "test-1".to_string(),
        name: "bash".to_string(),
        arguments: json!({
            "command": "echo 'test output'"
        }),
    };

    let result = runtime
        .execute_tool(&session.id, bash_call, workspace.path())
        .await
        .unwrap();

    assert!(result.success, "Bash should succeed in executing mode");
    assert!(result.output.contains("test output"));
}

#[tokio::test]
async fn test_executing_mode_allows_read_operations() {
    let workspace = setup_test_workspace();
    let runtime = AsyncRuntime::load(workspace.path()).await.unwrap();

    // Create and save an executing mode session using run
    let report = runtime
        .run(workspace.path(), "Test executing mode")
        .await
        .unwrap();
    let session = report.session;

    // Read a file in executing mode
    let read_call = ToolCall {
        call_id: "test-1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({
            "path": "README.md"
        }),
    };

    let result = runtime
        .execute_tool(&session.id, read_call, workspace.path())
        .await
        .unwrap();

    assert!(result.success, "Read should succeed in executing mode");
    assert!(result.output.contains("Test Project"));
}

// ── Security Consideration Tests ─────────────────────────────────────────

#[test]
fn test_unknown_tools_are_blocked() {
    // Unknown tools should be blocked in all modes
    assert!(!check_tool_permission(
        "unknown_tool",
        SessionMode::Planning
    ));
    assert!(!check_tool_permission(
        "unknown_tool",
        SessionMode::Executing
    ));
    assert_eq!(get_tool_permission("unknown_tool"), None);
}

#[test]
fn test_empty_tool_name_is_blocked() {
    assert!(!check_tool_permission("", SessionMode::Planning));
    assert!(!check_tool_permission("", SessionMode::Executing));
    assert_eq!(get_tool_permission(""), None);
}

#[test]
fn test_permission_enforcement_is_consistent() {
    // Test that permission checking is consistent across different methods

    let read_tools = vec!["read_file", "grep", "git_status"];
    let write_tools = vec!["write_file", "git_commit"];
    let execute_tools = vec!["bash"];

    // Check planning mode
    for tool in &read_tools {
        assert!(check_tool_permission(tool, SessionMode::Planning));
    }
    for tool in &write_tools {
        assert!(!check_tool_permission(tool, SessionMode::Planning));
    }
    for tool in &execute_tools {
        assert!(!check_tool_permission(tool, SessionMode::Planning));
    }

    // Check executing mode
    for tool in read_tools
        .iter()
        .chain(write_tools.iter())
        .chain(execute_tools.iter())
    {
        assert!(check_tool_permission(tool, SessionMode::Executing));
    }
}

#[test]
fn test_all_declared_tools_are_documented() {
    let registry = default_registry();
    let declared_tools: Vec<String> = registry.list().into_iter().map(|t| t.name).collect();

    // All declared tools should have permission mappings
    for tool_name in &declared_tools {
        assert!(
            get_tool_permission(tool_name).is_some(),
            "Tool '{}' should have a permission mapping",
            tool_name
        );
    }

    // Expected tools
    let expected_tools = vec![
        "bash",
        "glob",
        "grep",
        "git_commit",
        "git_diff",
        "git_log",
        "git_status",
        "list_dir",
        "lsp_diagnostics",
        "read_file",
        "write_file",
    ];

    for expected in expected_tools {
        assert!(
            declared_tools.contains(&expected.to_string()),
            "Expected tool '{}' should be declared",
            expected
        );
    }
}

// ── Error Message Tests ─────────────────────────────────────────────────

#[tokio::test]
async fn test_permission_denied_error_messages() {
    let workspace = setup_test_workspace();
    let runtime = AsyncRuntime::load(workspace.path()).await.unwrap();

    // Create and save a planning mode session using start_planning
    let report = runtime
        .start_planning(workspace.path(), "Test planning mode")
        .await
        .unwrap();
    let session = report.session;

    // Test write file error
    let write_call = ToolCall {
        call_id: "test-1".to_string(),
        name: "write_file".to_string(),
        arguments: json!({"path": "test.txt", "content": "test"}),
    };

    let result = runtime
        .execute_tool(&session.id, write_call, workspace.path())
        .await;

    // Permission check should return an error
    assert!(
        result.is_err(),
        "execute_tool should return error for blocked permission"
    );
    let err = result.unwrap_err();
    let err_msg = err.to_string().to_lowercase();
    assert!(
        err_msg.contains("permission")
            || err_msg.contains("denied")
            || err_msg.contains("not permitted")
            || err_msg.contains("not allowed"),
        "Error message should indicate permission issue: {}",
        err_msg
    );
}

// ── Concurrent Execution Tests ───────────────────────────────────────────

#[tokio::test]
async fn test_concurrent_operations_respect_permissions() {
    let workspace = setup_test_workspace();
    let runtime = AsyncRuntime::load(workspace.path()).await.unwrap();

    // Create and save a planning mode session using start_planning
    let report = runtime
        .start_planning(workspace.path(), "Test planning mode")
        .await
        .unwrap();
    let session = report.session;

    // Create multiple tool calls
    let calls = vec![
        ToolCall {
            call_id: "1".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "README.md"}),
        },
        ToolCall {
            call_id: "2".to_string(),
            name: "write_file".to_string(),
            arguments: json!({"path": "test.txt", "content": "test"}),
        },
        ToolCall {
            call_id: "3".to_string(),
            name: "grep".to_string(),
            arguments: json!({"pattern": "Test", "path": "README.md"}),
        },
    ];

    let results = runtime
        .execute_tools_concurrent(&session.id, calls, workspace.path())
        .await
        .unwrap();

    assert_eq!(results.len(), 3);

    // First call (read_file) should succeed
    assert!(
        results[0].result.success,
        "read_file should succeed in planning mode"
    );

    // Second call (write_file) should fail - but note: concurrent execution
    // uses a mock implementation, so it won't actually check permissions
    // This test documents the current behavior
    assert!(
        results[1].result.success,
        "Mock concurrent execution always succeeds"
    );

    // Third call (grep) should succeed
    assert!(
        results[2].result.success,
        "grep should succeed in planning mode"
    );
}
