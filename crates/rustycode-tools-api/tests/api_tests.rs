//! Tests for rustycode-tools-api
#![allow(clippy::unwrap_used, clippy::expect_used)]

use rustycode_tools_api::{
    check_tool_permission, get_tool_permission, new_todo_state, SandboxConfig, TodoStatus,
    ToolContext, ToolOutput, ToolPermission, ToolRegistry,
};
use serde_json::json;
use std::path::PathBuf;

// ── SandboxConfig Tests ─────────────────────────────────────────────────────

#[test]
fn sandbox_config_default() {
    let config = SandboxConfig::new();
    assert!(config.allowed_paths.is_none());
    assert!(config.denied_paths.is_empty());
    assert!(config.timeout_secs.is_none());
    assert!(config.max_output_bytes.is_none());
}

#[test]
fn sandbox_config_builder() {
    // Note: Builder methods are stubs in this minimal facade
    // They return self but don't actually modify the config
    let config = SandboxConfig::new()
        .allow_path("/tmp")
        .deny_path("/etc")
        .timeout(30)
        .max_output(1024);

    // Builder methods now actually modify the config
    assert_eq!(config.denied_paths.len(), 1);
    assert_eq!(config.denied_paths[0], PathBuf::from("/etc"));
    assert_eq!(config.timeout_secs, Some(30));
    assert_eq!(config.max_output_bytes, Some(1024));
}

// ── ToolContext Tests ───────────────────────────────────────────────────────

#[test]
fn tool_context_creation() {
    let ctx = ToolContext::new("/home/user/project");
    assert_eq!(ctx.cwd, PathBuf::from("/home/user/project"));
    assert_eq!(ctx.max_permission, ToolPermission::Network);
}

#[test]
fn tool_context_with_sandbox() {
    // Note: with_sandbox is a stub that doesn't actually modify in this facade
    let sandbox = SandboxConfig::new();
    let ctx = ToolContext::new("/tmp").with_sandbox(sandbox);
    // Stub implementation doesn't modify, so sandbox will be default
    assert!(ctx.sandbox.timeout_secs.is_none());
}

#[test]
fn tool_context_with_permission() {
    let ctx = ToolContext::new("/tmp").with_max_permission(ToolPermission::Read);
    assert_eq!(ctx.max_permission, ToolPermission::Read);
}

// ── ToolOutput Tests ────────────────────────────────────────────────────────

#[test]
fn tool_output_text() {
    let output = ToolOutput::text("Hello, world!");
    assert_eq!(output.text, "Hello, world!");
    assert!(output.structured.is_none());
}

#[test]
fn tool_output_with_structured() {
    let data = json!({"key": "value"});
    let output = ToolOutput::with_structured("Result", data.clone());
    assert_eq!(output.text, "Result");
    assert_eq!(output.structured, Some(data));
}

// ── ToolRegistry Tests ──────────────────────────────────────────────────────

#[test]
fn tool_registry_empty_list() {
    let registry = ToolRegistry::new();
    let tools = registry.list();
    assert!(tools.is_empty());
}

#[test]
fn tool_registry_register_and_get() {
    use rustycode_tools_api::Tool;

    struct MockTool;
    impl Tool for MockTool {
        fn name(&self) -> &'static str {
            "mock_tool"
        }
        fn description(&self) -> &'static str {
            "A mock tool"
        }
        fn permission(&self) -> ToolPermission {
            ToolPermission::Read
        }
        fn parameters_schema(&self) -> serde_json::Value {
            json!({})
        }
        fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: &ToolContext,
        ) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text("mock result"))
        }
    }

    let mut registry = ToolRegistry::new();
    registry.register(MockTool);

    let tools = registry.list();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "mock_tool");
    assert_eq!(tools[0].permission, ToolPermission::Read);

    let tool = registry.get("mock_tool");
    assert!(tool.is_some());
    assert_eq!(tool.unwrap().name(), "mock_tool");
}

#[test]
fn tool_registry_get_missing() {
    let registry = ToolRegistry::new();
    assert!(registry.get("nonexistent").is_none());
}

// ── Todo Tests ──────────────────────────────────────────────────────────────

#[test]
fn new_todo_state_creates_empty_list() {
    let state = new_todo_state();
    assert!(state.lock().unwrap().is_empty());
}

#[test]
fn todo_item_serialization() {
    use rustycode_tools_api::TodoItem;

    let item = TodoItem {
        id: "1".to_string(),
        title: "Test todo".to_string(),
        status: TodoStatus::Pending,
        active_form: None,
    };

    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("\"id\":\"1\""));
    assert!(json.contains("\"title\":\"Test todo\""));
    assert!(json.contains("\"status\":\"pending\""));
}

#[test]
fn todo_status_values() {
    // Note: serde rename_all = "lowercase" converts InProgress to "inprogress" (no underscore)
    assert_eq!(
        serde_json::to_string(&TodoStatus::Pending).unwrap(),
        "\"pending\""
    );
    assert_eq!(
        serde_json::to_string(&TodoStatus::InProgress).unwrap(),
        "\"inprogress\""
    );
    assert_eq!(
        serde_json::to_string(&TodoStatus::Completed).unwrap(),
        "\"completed\""
    );
}

// ── ToolPermission Tests ────────────────────────────────────────────────────

#[test]
fn tool_permission_serialization() {
    assert_eq!(
        serde_json::to_string(&ToolPermission::None).unwrap(),
        "\"None\""
    );
    assert_eq!(
        serde_json::to_string(&ToolPermission::Read).unwrap(),
        "\"Read\""
    );
    assert_eq!(
        serde_json::to_string(&ToolPermission::Write).unwrap(),
        "\"Write\""
    );
    assert_eq!(
        serde_json::to_string(&ToolPermission::Execute).unwrap(),
        "\"Execute\""
    );
    assert_eq!(
        serde_json::to_string(&ToolPermission::Network).unwrap(),
        "\"Network\""
    );
}

// ── get_tool_permission Tests ───────────────────────────────────────────────

#[test]
fn get_tool_permission_read_only_tools() {
    use rustycode_protocol::ToolPermission as ProtocolPermission;

    // Read-only tools should be AutoAllow
    assert_eq!(
        get_tool_permission("read_file"),
        Some(ProtocolPermission::AutoAllow)
    );
    assert_eq!(
        get_tool_permission("list_dir"),
        Some(ProtocolPermission::AutoAllow)
    );
    assert_eq!(
        get_tool_permission("grep"),
        Some(ProtocolPermission::AutoAllow)
    );
    assert_eq!(
        get_tool_permission("git_status"),
        Some(ProtocolPermission::AutoAllow)
    );
    assert_eq!(
        get_tool_permission("lsp_diagnostics"),
        Some(ProtocolPermission::AutoAllow)
    );
}

#[test]
fn get_tool_permission_write_tools() {
    use rustycode_protocol::ToolPermission as ProtocolPermission;

    // Write tools require confirmation
    assert_eq!(
        get_tool_permission("write_file"),
        Some(ProtocolPermission::RequiresConfirmation)
    );
    assert_eq!(
        get_tool_permission("git_commit"),
        Some(ProtocolPermission::RequiresConfirmation)
    );
}

#[test]
fn get_tool_permission_execute_tools() {
    use rustycode_protocol::ToolPermission as ProtocolPermission;

    // Bash requires confirmation
    assert_eq!(
        get_tool_permission("bash"),
        Some(ProtocolPermission::RequiresConfirmation)
    );
}

#[test]
fn get_tool_permission_unknown_tools() {
    use rustycode_protocol::ToolPermission as ProtocolPermission;

    // Unknown tools require confirmation for safety
    assert_eq!(
        get_tool_permission("unknown_tool"),
        Some(ProtocolPermission::RequiresConfirmation)
    );
}

// ── check_tool_permission Tests ─────────────────────────────────────────────

#[test]
fn check_tool_permission_planning_mode() {
    use rustycode_protocol::SessionMode;

    // Planning mode: only auto-allow tools permitted
    assert!(check_tool_permission("read_file", SessionMode::Planning));
    assert!(check_tool_permission("list_dir", SessionMode::Planning));
    assert!(!check_tool_permission("write_file", SessionMode::Planning));
    assert!(!check_tool_permission("bash", SessionMode::Planning));
    assert!(!check_tool_permission("git_commit", SessionMode::Planning));
}

#[test]
fn check_tool_permission_executing_mode() {
    use rustycode_protocol::SessionMode;

    // Executing mode: all non-blocked tools allowed
    assert!(check_tool_permission("read_file", SessionMode::Executing));
    assert!(check_tool_permission("write_file", SessionMode::Executing));
    assert!(check_tool_permission("bash", SessionMode::Executing));
    assert!(check_tool_permission("git_commit", SessionMode::Executing));
}
