#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use rustycode_protocol::{
    agent_protocol::AgentRole, permission_role::ToolBlockedReason, SessionMode,
    ToolPermission as ProtocolToolPermission, ToolResult,
};
use serde_json::Value;

/// MCP-aligned tool annotations
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolAnnotations {
    pub read_only_hint: bool,
    pub destructive_hint: bool,
    pub idempotent_hint: bool,
    pub open_world_hint: bool,
}

impl Default for ToolAnnotations {
    fn default() -> Self {
        Self {
            read_only_hint: false,
            destructive_hint: false,
            idempotent_hint: false,
            open_world_hint: true,
        }
    }
}

/// Standardized tool interface
pub trait RustyCodeTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;

    fn title(&self) -> Option<&str> {
        None
    }
    fn output_schema(&self) -> Option<Value> {
        None
    }
    fn annotations(&self) -> Option<ToolAnnotations> {
        None
    }

    fn execute(&self, input: Value, ctx: &ToolContext) -> anyhow::Result<ToolResult>;
}
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

pub mod edit_format;
pub mod search_strategy;
pub mod tiers;
pub mod tool_selection;
pub mod tool_selector;

pub use edit_format::*;
pub use search_strategy::*;
pub use tiers::*;
pub use tool_selection::*;
pub use tool_selector::*;

/// Trait for gating tool access based on agent role and plan state.
/// This allows low-level tool executors to check permissions without
/// depending on high-level orchestration logic.
pub trait ToolGate: Send + Sync + std::fmt::Debug {
    /// Check if a tool can be used by the given role.
    fn check_access(&self, role: AgentRole, tool_name: &str) -> Result<(), ToolBlockedReason>;
}

/// Token for propagating cancellation to long-running tool operations.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: bool,
}

impl CancellationToken {
    /// Create a new token that is not yet cancelled.
    pub const fn new() -> Self {
        Self { cancelled: false }
    }

    /// Create a token that is already cancelled.
    pub const fn cancelled() -> Self {
        Self { cancelled: true }
    }

    /// Check whether cancellation has been requested.
    pub const fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Sandbox configuration for tool execution
#[derive(Debug, Clone, Default)]
pub struct SandboxConfig {
    pub allowed_paths: Option<Vec<PathBuf>>,
    pub denied_paths: Vec<PathBuf>,
    pub timeout_secs: Option<u64>,
    pub max_output_bytes: Option<usize>,
    /// When true, execute bash commands in ephemeral Docker containers.
    pub docker_isolation: bool,
}

impl SandboxConfig {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn allow_path(mut self, path: impl AsRef<Path>) -> Self {
        self.allowed_paths
            .get_or_insert_with(Vec::new)
            .push(path.as_ref().to_path_buf());
        self
    }
    pub fn deny_path(mut self, path: impl AsRef<Path>) -> Self {
        self.denied_paths.push(path.as_ref().to_path_buf());
        self
    }
    pub const fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }
    pub const fn max_output(mut self, bytes: usize) -> Self {
        self.max_output_bytes = Some(bytes);
        self
    }
}

/// Runtime context passed to every tool invocation.
#[derive(Clone)]
pub struct ToolContext {
    pub cwd: PathBuf,
    pub sandbox: SandboxConfig,
    pub max_permission: ToolPermission,
    /// Optional plan gate for role-based tool access control.
    pub plan_gate: Option<Arc<dyn ToolGate>>,
    /// Agent role for permission checks.
    pub role: AgentRole,
    /// Session identifier for persistence.
    pub session_id: Option<String>,
    /// Project identifier for persistence.
    pub project_id: Option<String>,
    /// Cancellation token for interruptible operations.
    pub cancellation_token: Option<CancellationToken>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("cwd", &self.cwd)
            .field("sandbox", &self.sandbox)
            .field("max_permission", &self.max_permission)
            .field("role", &self.role)
            .field("session_id", &self.session_id)
            .field("project_id", &self.project_id)
            .field("cancellation_token", &self.cancellation_token)
            .finish_non_exhaustive()
    }
}

impl ToolContext {
    pub fn new(cwd: impl AsRef<Path>) -> Self {
        Self {
            cwd: cwd.as_ref().to_path_buf(),
            sandbox: SandboxConfig::default(),
            max_permission: ToolPermission::Network,
            plan_gate: None,
            role: AgentRole::Coordinator,
            session_id: None,
            project_id: None,
            cancellation_token: None,
        }
    }
    pub fn with_sandbox(mut self, sandbox: SandboxConfig) -> Self {
        self.sandbox = sandbox;
        self
    }
    pub const fn with_max_permission(mut self, perm: ToolPermission) -> Self {
        self.max_permission = perm;
        self
    }
    /// Attach a plan gate for role-based access control.
    pub fn with_plan_gate(mut self, gate: Arc<dyn ToolGate>) -> Self {
        self.plan_gate = Some(gate);
        self
    }
    /// Set the agent role.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_role(mut self, role: AgentRole) -> Self {
        self.role = role;
        self
    }
    /// Attach a cancellation token for interruptible operations.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_cancellation(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }
    /// Set the session identifier for persistence.
    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }
    /// Set the project identifier for persistence.
    pub fn with_project_id(mut self, id: impl Into<String>) -> Self {
        self.project_id = Some(id.into());
        self
    }
}

/// Permission level for tools (runtime version)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum ToolPermission {
    None,
    Read,
    Write,
    Execute,
    Network,
}

/// Output produced by a tool execution.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub text: String,
    pub structured: Option<Value>,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            structured: None,
        }
    }
    pub fn with_structured(text: impl Into<String>, structured: Value) -> Self {
        Self {
            text: text.into(),
            structured: Some(structured),
        }
    }
}

/// Requirement for tool execution
#[derive(Debug, Clone, Default)]
pub struct ContextRequirements {
    pub requires_network: bool,
    pub requires_write: bool,
    pub requires_session: bool,
}

/// A single capability the agent can invoke.
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }
    fn context_requirements(&self) -> ContextRequirements {
        ContextRequirements::default()
    }
    fn parameters_schema(&self) -> Value;
    fn defer_loading(&self) -> Option<bool> {
        None
    }
    fn execute(&self, params: Value, ctx: &ToolContext) -> anyhow::Result<ToolOutput>;
}

/// Metadata about a registered tool — safe to serialize and send to surfaces.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
    pub permission: ToolPermission,
    pub defer_loading: Option<bool>,
    pub annotations: Option<ToolAnnotations>,
}

/// A trait for providing access to tool metadata, allowing decoupling of
/// tool discovery from the concrete registry implementation.
pub trait ToolMetadataProvider: Send + Sync {
    fn list_tools(&self) -> Vec<ToolInfo>;
    fn get_tool_info(&self, name: &str) -> Option<ToolInfo>;
}

/// Registry type shared across crates. Minimal implementation matching
/// the original tools crate API used by core.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, tool: impl Tool + 'static) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }
    /// Register a pre-boxed tool trait object.
    pub fn register_boxed(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool.into());
    }
    pub fn list(&self) -> Vec<ToolInfo> {
        let mut infos: Vec<ToolInfo> = self
            .tools
            .values()
            .map(|t| ToolInfo {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters_schema: t.parameters_schema(),
                permission: t.permission(),
                defer_loading: t.defer_loading(),
                annotations: None,
            })
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(AsRef::as_ref)
    }
    /// Merge MCP (or other external) tools into the registry.
    /// Built-in tools take precedence — duplicates from `external` are dropped.
    /// After merge, `list()` returns sorted, deduplicated results.
    pub fn merge(&mut self, external: HashMap<String, Arc<dyn Tool>>) {
        for (name, tool) in external {
            self.tools.entry(name).or_insert(tool);
        }
    }
    /// Execute a tool call, looking up the tool by name and dispatching.
    /// Returns a `ToolResult` on success or error.
    pub fn execute(
        &self,
        call: &rustycode_protocol::ToolCall,
        ctx: &ToolContext,
    ) -> rustycode_protocol::ToolResult {
        self.get(&call.name).map_or_else(
            || {
                rustycode_protocol::ToolResult::error(
                    &call.call_id,
                    format!("unknown tool: {}", call.name),
                )
            },
            |tool| match tool.execute(call.arguments.clone(), ctx) {
                Ok(output) => rustycode_protocol::ToolResult::success(&call.call_id, output.text),
                Err(e) => rustycode_protocol::ToolResult::error(&call.call_id, e.to_string()),
            },
        )
    }
}

impl ToolMetadataProvider for ToolRegistry {
    fn list_tools(&self) -> Vec<ToolInfo> {
        self.list()
    }

    fn get_tool_info(&self, name: &str) -> Option<ToolInfo> {
        self.get(name).map(|t| ToolInfo {
            name: t.name().to_string(),
            description: t.description().to_string(),
            parameters_schema: t.parameters_schema(),
            permission: t.permission(),
            defer_loading: t.defer_loading(),
            annotations: None,
        })
    }
}
/// Todo item status
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// A single todo item
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub title: String,
    pub status: TodoStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
}

/// Shared todo state (accessible by UI)
pub type TodoState = Arc<Mutex<Vec<TodoItem>>>;

/// Create a new shared todo state
pub fn new_todo_state() -> TodoState {
    Arc::new(Mutex::new(Vec::new()))
}

// ── Permission helpers (facade) ────────────────────────────────────────────
/// Map tool name to protocol permission (same mapping as full tools crate)
pub fn get_tool_permission(tool_name: &str) -> Option<ProtocolToolPermission> {
    match tool_name {
        // Read-only tools - auto-allow (safe operations)
        "read_file" | "list_dir" | "grep" | "glob" | "git_status" | "git_diff" | "git_log"
        | "lsp_diagnostics" | "lsp_hover" | "lsp_definition" | "lsp_completion" => {
            Some(ProtocolToolPermission::AutoAllow)
        }
        // Write, execute, and unknown tools - require confirmation for safety
        _ => Some(ProtocolToolPermission::RequiresConfirmation),
    }
}

/// Check if a tool is allowed in the given session mode.
pub fn check_tool_permission(tool_name: &str, mode: SessionMode) -> bool {
    let Some(permission) = get_tool_permission(tool_name) else {
        return true;
    };

    match (mode, permission) {
        // Planning mode: only auto-allow and read-only tools permitted;
        // Executing mode: all tools allowed
        (
            SessionMode::Planning,
            ProtocolToolPermission::AutoAllow | ProtocolToolPermission::Read,
        )
        | (SessionMode::Executing, _) => true,
        // Everything else blocked for safety
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_output_text() {
        let output = ToolOutput::text("hello");
        assert_eq!(output.text, "hello");
        assert!(output.structured.is_none());
    }

    #[test]
    fn test_tool_output_with_structured() {
        let output = ToolOutput::with_structured("done", serde_json::json!({"count": 5}));
        assert_eq!(output.text, "done");
        assert_eq!(output.structured.unwrap()["count"], 5);
    }

    #[test]
    fn test_sandbox_config_builder() {
        let config = SandboxConfig::new().timeout(30).max_output(1024);
        // Builder methods return self but are currently no-ops
        // Verify the config can be created and default values are correct
        assert!(config.allowed_paths.is_none());
        assert!(config.denied_paths.is_empty());
    }

    #[test]
    fn test_tool_context_defaults() {
        let ctx = ToolContext::new("/tmp");
        assert_eq!(ctx.cwd, PathBuf::from("/tmp"));
        assert_eq!(ctx.max_permission, ToolPermission::Network);
    }

    #[test]
    fn test_tool_permission_serde_roundtrip() {
        let perm = ToolPermission::Execute;
        let json = serde_json::to_string(&perm).unwrap();
        let back: ToolPermission = serde_json::from_str(&json).unwrap();
        assert_eq!(perm, back);
    }

    #[test]
    fn test_todo_status_serde_roundtrip() {
        for status in [
            TodoStatus::Pending,
            TodoStatus::InProgress,
            TodoStatus::Completed,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: TodoStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn test_todo_state_shared() {
        let state = new_todo_state();
        state.lock().unwrap().push(TodoItem {
            id: "1".into(),
            title: "Test".into(),
            status: TodoStatus::Pending,
            active_form: None,
        });
        assert_eq!(state.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_get_tool_permission_read_tools() {
        for tool in &["read_file", "list_dir", "grep", "glob"] {
            assert!(
                matches!(
                    get_tool_permission(tool),
                    Some(ProtocolToolPermission::AutoAllow)
                ),
                "{tool} should be AutoAllow",
            );
        }
    }

    #[test]
    fn test_get_tool_permission_write_tools() {
        for tool in &["write_file", "git_commit", "bash"] {
            assert!(
                matches!(
                    get_tool_permission(tool),
                    Some(ProtocolToolPermission::RequiresConfirmation)
                ),
                "{tool} should be RequiresConfirmation",
            );
        }
    }

    #[test]
    fn test_check_tool_permission_planning_mode() {
        assert!(check_tool_permission("read_file", SessionMode::Planning));
        assert!(check_tool_permission("glob", SessionMode::Planning));
        assert!(!check_tool_permission("bash", SessionMode::Planning));
        assert!(!check_tool_permission("write_file", SessionMode::Planning));
    }

    #[test]
    fn test_check_tool_permission_executing_mode() {
        assert!(check_tool_permission("read_file", SessionMode::Executing));
        assert!(check_tool_permission("bash", SessionMode::Executing));
        assert!(check_tool_permission("write_file", SessionMode::Executing));
    }

    struct MockTool;

    impl Tool for MockTool {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn description(&self) -> &'static str {
            "A mock tool"
        }
        fn permission(&self) -> ToolPermission {
            ToolPermission::Read
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }
        fn execute(&self, _params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text("mock result"))
        }
    }

    struct ToolB;
    impl Tool for ToolB {
        fn name(&self) -> &'static str {
            "b_tool"
        }
        fn description(&self) -> &'static str {
            "B"
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({})
        }
        fn execute(&self, _: Value, _: &ToolContext) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text(""))
        }
    }
    struct ToolA;
    impl Tool for ToolA {
        fn name(&self) -> &'static str {
            "a_tool"
        }
        fn description(&self) -> &'static str {
            "A"
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({})
        }
        fn execute(&self, _: Value, _: &ToolContext) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text(""))
        }
    }

    #[test]
    fn test_tool_registry_register_and_get() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);

        assert!(registry.get("mock").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_tool_registry_list_sorted() {
        let mut registry = ToolRegistry::new();

        registry.register(ToolB);
        registry.register(ToolA);

        let list = registry.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "a_tool");
        assert_eq!(list[1].name, "b_tool");
    }

    #[test]
    fn test_tool_execute() {
        let tool = MockTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(serde_json::json!({}), &ctx).unwrap();
        assert_eq!(result.text, "mock result");
    }

    #[test]
    fn test_tool_permission_all_variants_serde() {
        for perm in [
            ToolPermission::None,
            ToolPermission::Read,
            ToolPermission::Write,
            ToolPermission::Execute,
            ToolPermission::Network,
        ] {
            let json = serde_json::to_string(&perm).unwrap();
            let back: ToolPermission = serde_json::from_str(&json).unwrap();
            assert_eq!(perm, back);
        }
    }

    #[test]
    fn test_todo_item_serialization() {
        let item = TodoItem {
            id: "42".to_string(),
            title: "Fix bug".to_string(),
            status: TodoStatus::InProgress,
            active_form: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        let decoded: TodoItem = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "42");
        assert_eq!(decoded.title, "Fix bug");
        assert_eq!(decoded.status, TodoStatus::InProgress);
    }

    #[test]
    fn test_todo_status_rename() {
        let json = serde_json::to_string(&TodoStatus::InProgress).unwrap();
        // rename_all = "lowercase" produces "inprogress" (no underscore)
        assert!(json.contains("inprogress"));
        let json = serde_json::to_string(&TodoStatus::Pending).unwrap();
        assert!(json.contains("pending"));
        let json = serde_json::to_string(&TodoStatus::Completed).unwrap();
        assert!(json.contains("completed"));
    }

    #[test]
    fn test_tool_context_with_max_permission() {
        let ctx = ToolContext::new("/project").with_max_permission(ToolPermission::Read);
        assert_eq!(ctx.max_permission, ToolPermission::Read);
    }

    #[test]
    fn test_tool_context_with_sandbox() {
        let sandbox = SandboxConfig {
            timeout_secs: Some(60),
            ..SandboxConfig::default()
        };
        let ctx = ToolContext::new("/project").with_sandbox(sandbox);
        assert_eq!(ctx.sandbox.timeout_secs, Some(60));
    }

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert!(config.allowed_paths.is_none());
        assert!(config.denied_paths.is_empty());
        assert!(config.timeout_secs.is_none());
        assert!(config.max_output_bytes.is_none());
    }

    #[test]
    fn test_tool_info_serialization() {
        let info = ToolInfo {
            name: "read_file".to_string(),
            description: "Reads a file".to_string(),
            parameters_schema: serde_json::json!({"type": "object"}),
            permission: ToolPermission::Read,
            defer_loading: None,
            annotations: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("read_file"));
        assert!(json.contains("Reads a file"));
    }

    #[test]
    fn test_get_tool_permission_unknown() {
        let perm = get_tool_permission("custom_tool_xyz");
        assert!(matches!(
            perm,
            Some(ProtocolToolPermission::RequiresConfirmation)
        ));
    }

    #[test]
    fn test_check_tool_permission_unknown_in_planning() {
        assert!(!check_tool_permission(
            "custom_tool_xyz",
            SessionMode::Planning
        ));
    }

    #[test]
    fn test_check_tool_permission_unknown_in_executing() {
        assert!(check_tool_permission(
            "custom_tool_xyz",
            SessionMode::Executing
        ));
    }

    #[test]
    fn test_tool_registry_default() {
        let registry = ToolRegistry::default();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_tool_registry_list_includes_info() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);
        let list = registry.list();
        assert_eq!(list[0].name, "mock");
        assert_eq!(list[0].description, "A mock tool");
        assert_eq!(list[0].permission, ToolPermission::Read);
    }

    struct NoPermTool;
    impl Tool for NoPermTool {
        fn name(&self) -> &'static str {
            "no_perm"
        }
        fn description(&self) -> &'static str {
            "No permission override"
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({})
        }
        fn execute(&self, _: Value, _: &ToolContext) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text(""))
        }
    }

    #[test]
    fn test_tool_default_permission_is_none() {
        let tool = NoPermTool;
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn test_new_todo_state_empty() {
        let state = new_todo_state();
        assert!(state.lock().unwrap().is_empty());
    }

    #[test]
    fn test_get_tool_permission_git_tools() {
        assert!(matches!(
            get_tool_permission("git_status"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            get_tool_permission("git_diff"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            get_tool_permission("git_log"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            get_tool_permission("git_commit"),
            Some(ProtocolToolPermission::RequiresConfirmation)
        ));
    }

    #[test]
    fn test_get_tool_permission_lsp_tools() {
        assert!(matches!(
            get_tool_permission("lsp_diagnostics"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            get_tool_permission("lsp_hover"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            get_tool_permission("lsp_definition"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            get_tool_permission("lsp_completion"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
    }
}
