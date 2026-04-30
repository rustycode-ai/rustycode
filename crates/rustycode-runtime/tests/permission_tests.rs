#![allow(
    clippy::unnecessary_literal_bound,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]

use rustycode_tools::{Tool, ToolContext, ToolOutput, ToolPermission, ToolRegistry};
use rustycode_tools_api::SandboxConfig;
use serde_json::{json, Value};

struct ReadTool;
struct ExecuteTool;

impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read_test"
    }

    fn description(&self) -> &str {
        "A test tool with Read permission"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({"type": "object"})
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        Ok(ToolOutput::text("read operation"))
    }
}

impl Tool for ExecuteTool {
    fn name(&self) -> &str {
        "execute_test"
    }

    fn description(&self) -> &str {
        "A test tool with Execute permission"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({"type": "object"})
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        Ok(ToolOutput::text("execute operation"))
    }
}

#[test]
fn tool_permission_values_are_distinct() {
    assert_eq!(ToolPermission::Read, ToolPermission::Read);
    assert_ne!(ToolPermission::Read, ToolPermission::Write);
    assert_ne!(ToolPermission::Write, ToolPermission::Execute);
    assert_ne!(ToolPermission::Execute, ToolPermission::Network);
}

#[test]
fn tool_context_defaults_to_network_permission_with_open_sandbox() {
    let ctx = ToolContext::new("/tmp");

    assert_eq!(ctx.max_permission, ToolPermission::Network);
    assert!(ctx.sandbox.allowed_paths.is_none());
    assert!(ctx.sandbox.denied_paths.is_empty());
}

#[test]
fn sandbox_builder_records_paths_timeout_and_output_limit() {
    let sandbox = SandboxConfig::new()
        .allow_path("/safe/path")
        .deny_path("/safe/path/private")
        .timeout(30)
        .max_output(4096);

    assert_eq!(
        sandbox.allowed_paths,
        Some(vec![std::path::PathBuf::from("/safe/path")])
    );
    assert_eq!(
        sandbox.denied_paths,
        vec![std::path::PathBuf::from("/safe/path/private")]
    );
    assert_eq!(sandbox.timeout_secs, Some(30));
    assert_eq!(sandbox.max_output_bytes, Some(4096));
}

#[test]
fn tool_context_builder_overrides_sandbox_and_permission() {
    let sandbox = SandboxConfig::new().allow_path("/workspace");
    let ctx = ToolContext::new("/tmp")
        .with_sandbox(sandbox.clone())
        .with_max_permission(ToolPermission::Read);

    assert_eq!(ctx.max_permission, ToolPermission::Read);
    assert_eq!(ctx.sandbox.allowed_paths, sandbox.allowed_paths);
}

#[test]
fn registry_list_includes_declared_permissions() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadTool);
    registry.register(ExecuteTool);

    let tools = registry.list();
    let read = tools.iter().find(|tool| tool.name == "read_test").unwrap();
    let execute = tools
        .iter()
        .find(|tool| tool.name == "execute_test")
        .unwrap();

    assert_eq!(read.permission, ToolPermission::Read);
    assert_eq!(execute.permission, ToolPermission::Execute);
}
