#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use rustycode_protocol::{
    agent_protocol::AgentRole, permission_role::ToolBlockedReason, SessionMode,
    ToolPermission as ProtocolToolPermission, ToolResult,
};
use serde::Serialize;
use serde_json::Value;

pub use schemars;

/// Capability tags that tools self-declare for profile-based autodiscovery.
/// These are the ONLY valid tags — typos are caught at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolTag {
    /// Workspace exploration — read-only discovery tools
    Explore,
    /// Code implementation — write/edit/execute tools
    Implement,
    /// Debugging — diagnostics, inspection, test-run tools
    Debug,
    /// Refactoring — LSP rename/extract/restructure tools
    Refactor,
    /// Operations — git, bash, docker, deployment tools
    Ops,
}

impl ToolTag {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Explore => "explore",
            Self::Implement => "implement",
            Self::Debug => "debug",
            Self::Refactor => "refactor",
            Self::Ops => "ops",
        }
    }
}

impl std::fmt::Display for ToolTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// MCP-aligned tool annotations
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[allow(clippy::struct_excessive_bools)]
#[serde(rename_all = "camelCase")]
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
pub mod file_read_state;
pub mod privilege;
pub mod search_strategy;
pub mod tiers;
pub mod token_accountant;
pub mod tool_names;
pub mod tool_selection;
pub mod tool_selector;
pub mod validation;
pub mod worktree_session;

pub use edit_format::*;
pub use file_read_state::*;
pub use privilege::*;
pub use search_strategy::*;
pub use tiers::*;
pub use token_accountant::*;
pub use tool_names::*;
pub use tool_selection::*;
pub use tool_selector::*;
pub use validation::*;
pub use worktree_session::*;

/// A single recorded tool invocation for audit purposes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditEntry {
    pub tool_name: String,
    pub call_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub output_chars: usize,
}

/// In-memory audit log with a fixed-size ring buffer.
///
/// Initial capacity is capped at 256 to avoid over-allocation — most sessions
/// use fewer than 100 entries.
pub struct AuditLog {
    entries: parking_lot::Mutex<Vec<AuditEntry>>,
    max_entries: usize,
}

impl AuditLog {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: parking_lot::Mutex::new(Vec::with_capacity(max_entries.min(256))),
            max_entries,
        }
    }

    pub fn record(&self, entry: AuditEntry) {
        let mut entries = self.entries.lock();
        if entries.len() >= self.max_entries {
            let excess = entries.len() - self.max_entries + 1;
            entries.drain(0..excess);
        }
        entries.push(entry);
    }

    pub fn snapshot(&self) -> Vec<AuditEntry> {
        self.entries.lock().clone()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditLog")
            .field("entries", &self.entries)
            .field("max_entries", &self.max_entries)
            .finish()
    }
}

/// Trait for gating tool access based on agent role and plan state.
/// This allows low-level tool executors to check permissions without
/// depending on high-level orchestration logic.
pub trait ToolGate: Send + Sync + std::fmt::Debug {
    /// Check if a tool can be used by the given role.
    fn check_access(&self, role: AgentRole, tool_name: &str) -> Result<(), ToolBlockedReason>;
}

/// Trait for sending messages between agents.
/// Implemented by the orchestration layer's MailboxRouter.
/// This trait lives in tools-api to avoid circular dependencies.
pub trait MessageSender: Send + Sync + std::fmt::Debug {
    /// Send a directed message to a specific agent.
    fn send(&self, to: &str, message: &str, summary: &str) -> Result<(), String>;
    /// Broadcast a message to all registered agents.
    fn broadcast(&self, message: &str, summary: &str) -> Result<(), String>;
}

/// Token for propagating cancellation to long-running tool operations.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: bool,
}

impl CancellationToken {
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
    /// When true, execute bash commands inside an OS-level sandbox
    /// (macOS Seatbelt, Linux Landlock, Windows Job Objects).
    pub os_sandbox: bool,
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
    /// Optional registry reference for self-introspection (used by `tool_search`).
    pub registry: Option<Arc<ToolRegistry>>,
    /// When true, file tools may access paths outside the workspace root.
    /// Security-sensitive paths (.env, .ssh, credentials) remain blocked regardless.
    pub allow_outside_workspace: bool,
    /// Tracks file read state for staleness detection.
    pub file_read_state: Option<Arc<FileReadState>>,
    /// Optional message sender for agent-to-agent communication.
    pub message_sender: Option<Arc<dyn MessageSender>>,
    /// Optional JSON schema for structured output validation (StructuredOutputTool).
    pub structured_output_schema: Option<serde_json::Value>,
    /// When true, tools should populate `ToolOutput::structured` metadata.
    /// Only ACP (IDE integration) and the WebSocket tool server consume this data;
    /// the primary CLI/TUI/headless path discards it. Defaults to `false` to avoid
    /// wasted CPU cycles serialising JSON that is never read.
    pub structured_output_enabled: bool,
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
            registry: None,
            allow_outside_workspace: false,
            file_read_state: None,
            message_sender: None,
            structured_output_schema: None,
            structured_output_enabled: false,
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

    /// Allow or deny file access outside the workspace root.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_allow_outside_workspace(mut self, allow: bool) -> Self {
        self.allow_outside_workspace = allow;
        self
    }
    /// Enable structured output for ACP / tool-server consumers.
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_structured_output(mut self, enabled: bool) -> Self {
        self.structured_output_enabled = enabled;
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
    /// Attach a registry reference for tool self-introspection.
    pub fn with_registry(mut self, registry: Arc<ToolRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }
    /// Attach file read state for staleness detection.
    pub fn with_file_read_state(mut self, state: Arc<FileReadState>) -> Self {
        self.file_read_state = Some(state);
        self
    }
    /// Attach a message sender for agent-to-agent communication.
    pub fn with_message_sender(mut self, sender: Arc<dyn MessageSender>) -> Self {
        self.message_sender = Some(sender);
        self
    }
    /// Set the JSON schema for structured output validation.
    pub fn with_structured_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.structured_output_schema = Some(schema);
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
    /// If set, signals that the session CWD should change to this path.
    pub new_cwd: Option<PathBuf>,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            structured: None,
            new_cwd: None,
        }
    }
    pub fn with_structured(text: impl Into<String>, structured: Value) -> Self {
        Self {
            text: text.into(),
            structured: Some(structured),
            new_cwd: None,
        }
    }
    pub fn with_cwd_change(text: impl Into<String>, new_cwd: PathBuf) -> Self {
        Self {
            text: text.into(),
            structured: None,
            new_cwd: Some(new_cwd),
        }
    }

    /// Create a new output with a structured payload that implements Serialize.
    pub fn serialized<T: Serialize>(
        text: impl Into<String>,
        structured: T,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            text: text.into(),
            structured: Some(serde_json::to_value(structured)?),
            new_cwd: None,
        })
    }

    /// Conditionally attach structured metadata based on the execution context.
    ///
    /// The closure is only evaluated when `ctx.structured_output_enabled` is true
    /// (ACP / tool-server path). In the primary CLI/TUI/headless path the closure
    /// is never called, avoiding wasted CPU serialising JSON that would be dropped.
    pub fn with_metadata<F>(mut self, ctx: &ToolContext, metadata: F) -> Self
    where
        F: FnOnce() -> Value,
    {
        if ctx.structured_output_enabled {
            self.structured = Some(metadata());
        }
        self
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
    fn tags(&self) -> &[ToolTag] {
        &[]
    }
    fn output_schema(&self) -> Option<Value> {
        None
    }

    fn annotations(&self) -> ToolAnnotations {
        let mut ann = ToolAnnotations::default();
        if matches!(
            self.permission(),
            ToolPermission::None | ToolPermission::Read
        ) {
            ann.read_only_hint = true;
        }
        ann
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> anyhow::Result<ToolOutput>;

    /// Whether this specific invocation is read-only (input-aware).
    ///
    /// Unlike `permission()` which is static, this can inspect the parameters
    /// to determine if a particular call is safe (e.g., `bash ls` vs `bash rm`).
    /// Default: true if permission is None or Read.
    fn is_read_only(&self, _params: &Value) -> bool {
        matches!(
            self.permission(),
            ToolPermission::None | ToolPermission::Read
        )
    }

    /// Whether this invocation is destructive and should warn the user.
    ///
    /// Return true for operations that cannot be undone (e.g., `rm`, `DROP TABLE`).
    fn is_destructive(&self, _params: &Value) -> bool {
        false
    }

    /// Whether this tool invocation can safely run concurrently with other tools.
    ///
    /// Return false for tools that mutate shared state (e.g., file writes, git operations).
    fn is_concurrency_safe(&self, _params: &Value) -> bool {
        true
    }

    /// Maximum result size in characters before output is persisted to disk.
    ///
    /// Return `None` to disable persistence (e.g., `FileReadTool` to avoid read→persist→read loops).
    /// Default: 30,000 characters.
    fn max_result_size_chars(&self) -> Option<usize> {
        Some(30_000)
    }

    /// Pre-permission validation. Runs before the approval gate.
    ///
    /// Use this for lightweight checks that should block execution early:
    /// - Stale file detection (file modified since last read)
    /// - Binary file rejection
    /// - Input sanitization
    ///
    /// Return `Err` to block execution with a user-facing message.
    fn validate_input(&self, _params: &Value, _ctx: &ToolContext) -> anyhow::Result<()> {
        Ok(())
    }
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
    pub tags: Vec<ToolTag>,
    pub max_result_size_chars: Option<usize>,
    pub is_destructive_default: bool,
}

impl ToolInfo {
    /// Construct `ToolInfo` from any Tool trait object.
    fn from_tool(tool: &dyn Tool) -> Self {
        Self {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            parameters_schema: tool.parameters_schema(),
            permission: tool.permission(),
            defer_loading: tool.defer_loading(),
            annotations: Some(tool.annotations()),
            tags: tool.tags().to_vec(),
            max_result_size_chars: tool.max_result_size_chars(),
            is_destructive_default: tool.is_destructive(&Value::Null),
        }
    }
}

/// A trait for providing access to tool metadata, allowing decoupling of
/// tool discovery from the concrete registry implementation.
pub trait ToolMetadataProvider: Send + Sync {
    fn list_tools(&self) -> Vec<ToolInfo>;
    fn tool_info(&self, name: &str) -> Option<ToolInfo>;
    /// Return only tools that should be eagerly loaded (deferred tools excluded).
    fn list_immediate_tools(&self) -> Vec<ToolInfo> {
        self.list_tools()
            .into_iter()
            .filter(|t| t.name == tool_names::TOOL_SEARCH || t.defer_loading != Some(true))
            .collect()
    }
}

/// Registry type shared across crates. Minimal implementation matching
/// the original tools crate API used by core.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    audit_log: AuditLog,
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
            .map(|t| ToolInfo::from_tool(t.as_ref()))
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    pub fn list_all_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn list_for_tags(&self, required_tags: &[ToolTag]) -> Vec<ToolInfo> {
        let mut infos: Vec<ToolInfo> = self
            .tools
            .values()
            .filter(|t| {
                let tool_tags = t.tags();
                required_tags.iter().any(|rt| tool_tags.contains(rt))
            })
            .map(|t| ToolInfo::from_tool(t.as_ref()))
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(AsRef::as_ref)
    }

    /// Return `ToolInfo` for tools that should be eagerly loaded into the prompt.
    /// Tools with `defer_loading() == Some(true)` are excluded.
    /// The `tool_search` tool is always included regardless of its `defer_loading` setting.
    pub fn list_immediate(&self) -> Vec<ToolInfo> {
        let mut infos: Vec<ToolInfo> = self
            .tools
            .values()
            .filter(|t| t.name() == tool_names::TOOL_SEARCH || t.defer_loading() != Some(true))
            .map(|t| ToolInfo::from_tool(t.as_ref()))
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
    }

    /// Return lightweight stubs for deferred tools — name and first-line description
    /// with an empty `parameters_schema`. The LLM must call `tool_search` to get the full schema.
    pub fn list_deferred_stubs(&self) -> Vec<ToolInfo> {
        let mut infos: Vec<ToolInfo> = self
            .tools
            .values()
            .filter(|t| t.name() != tool_names::TOOL_SEARCH && t.defer_loading() == Some(true))
            .map(|t| {
                let first_line = t.description().lines().next().unwrap_or("");
                let tool_search = tool_names::TOOL_SEARCH;
                let mut info = ToolInfo::from_tool(t.as_ref());
                info.description = format!(
                    "{first_line}\n\n(Deferred tool — call {tool_search} with name \"{}\" to load full schema.)",
                    t.name()
                );
                info.parameters_schema = serde_json::json!({
                    "type": "object",
                    "properties": {}
                });
                info
            })
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        infos
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
    /// Returns a `ToolResult` on success or error. Records an audit entry.
    pub fn execute(
        &self,
        call: &rustycode_protocol::ToolCall,
        ctx: &ToolContext,
    ) -> rustycode_protocol::ToolResult {
        let start = std::time::Instant::now();

        let result = self.get(&call.name).map_or_else(
            || {
                rustycode_protocol::ToolResult::error(
                    &call.call_id,
                    format!("unknown tool: {}", call.name),
                )
            },
            |tool| match tool.execute(call.arguments.clone(), ctx) {
                Ok(output) => {
                    let mut result =
                        rustycode_protocol::ToolResult::success(&call.call_id, output.text);
                    result.new_cwd = output.new_cwd;
                    result.data = output.structured;
                    result
                }
                Err(e) => rustycode_protocol::ToolResult::error(&call.call_id, e.to_string()),
            },
        );

        let duration = start.elapsed();
        let entry = AuditEntry {
            tool_name: call.name.clone(),
            call_id: call.call_id.clone(),
            success: result.error.is_none(),
            error: result.error.clone(),
            duration_ms: duration.as_millis() as u64,
            session_id: ctx.session_id.clone(),
            output_chars: result.output.len(),
        };

        tracing::debug!(
            tool = %entry.tool_name,
            success = entry.success,
            duration_ms = entry.duration_ms,
            output_chars = entry.output_chars,
            "tool execution completed"
        );

        self.audit_log.record(entry);
        result
    }

    /// Access the audit log for inspection.
    pub const fn audit_log(&self) -> &AuditLog {
        &self.audit_log
    }
}

impl ToolMetadataProvider for ToolRegistry {
    fn list_tools(&self) -> Vec<ToolInfo> {
        self.list()
    }

    fn tool_info(&self, name: &str) -> Option<ToolInfo> {
        self.get(name).map(ToolInfo::from_tool)
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
pub fn tool_permission(tool_name: &str) -> Option<ProtocolToolPermission> {
    match tool_name {
        READ | LIST_DIR | GREP | GLOB | FIND | INSPECT | GIT_STATUS | GIT_DIFF | GIT_LOG
        | LSP_DIAGNOSTICS | LSP_HOVER | LSP_DEFINITION | LSP_COMPLETION => {
            Some(ProtocolToolPermission::AutoAllow)
        }
        _ => Some(ProtocolToolPermission::RequiresConfirmation),
    }
}

/// Check if a tool is allowed in the given session mode.
pub fn check_tool_permission(tool_name: &str, mode: SessionMode) -> bool {
    let Some(permission) = tool_permission(tool_name) else {
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

#[macro_export]
macro_rules! define_tool {
    (
        pub struct $name:ident;

        name: $tool_name:expr,
        description: $desc:expr,
        $( permission: $perm:expr, )?
        $( tags: [$($tag:expr),*], )?
        $( defer_loading: $defer:expr, )?
        $( returns: $output_ty:ty, )?
        $( read_only: $read_only:expr, )?
        $( destructive: $destructive:expr, )?
        $( idempotent: $idempotent:expr, )?
        $( open_world: $open_world:expr, )?

        execute($params:ident: $params_ty:ty, $ctx:ident) $body:block
    ) => {
        pub struct $name;

        $crate::__define_tool_impl!(
            $name;
            name: $tool_name,
            description: $desc,
            $( permission: $perm, )?
            $( tags: [$($tag),*], )?
            $( defer_loading: $defer, )?
            $( returns: $output_ty, )?
            $( read_only: $read_only, )?
            $( destructive: $destructive, )?
            $( idempotent: $idempotent, )?
            $( open_world: $open_world, )?
            execute($params: $params_ty, $ctx) $body
        );
    };
    (
        pub struct $name:ident {
            $($field_vis:vis $field_name:ident : $field_type:ty),* $(,)?
        }

        name: $tool_name:expr,
        description: $desc:expr,
        $( permission: $perm:expr, )?
        $( tags: [$($tag:expr),*], )?
        $( defer_loading: $defer:expr, )?
        $( returns: $output_ty:ty, )?
        $( read_only: $read_only:expr, )?
        $( destructive: $destructive:expr, )?
        $( idempotent: $idempotent:expr, )?
        $( open_world: $open_world:expr, )?

        execute(&$self_ident:ident, $params:ident: $params_ty:ty, $ctx:ident) $body:block
    ) => {
        pub struct $name {
            $($field_vis $field_name : $field_type),*
        }

        $crate::__define_tool_impl!(
            $name;
            name: $tool_name,
            description: $desc,
            $( permission: $perm, )?
            $( tags: [$($tag),*], )?
            $( defer_loading: $defer, )?
            $( returns: $output_ty, )?
            $( read_only: $read_only, )?
            $( destructive: $destructive, )?
            $( idempotent: $idempotent, )?
            $( open_world: $open_world, )?
            execute(&$self_ident, $params: $params_ty, $ctx) $body
        );
    };
    // Variant with streaming support (zero-sized struct, no self in execute)
    (
        pub struct $name:ident;

        name: $tool_name:expr,
        description: $desc:expr,
        $( permission: $perm:expr, )?
        $( tags: [$($tag:expr),*], )?
        $( defer_loading: $defer:expr, )?
        $( returns: $output_ty:ty, )?
        $( read_only: $read_only:expr, )?
        $( destructive: $destructive:expr, )?
        $( idempotent: $idempotent:expr, )?
        $( open_world: $open_world:expr, )?

        execute($params:ident: $params_ty:ty, $ctx:ident) $body:block

        execute_stream($stream_params:ident: $stream_params_ty:ty, $stream_ctx:ident) $stream_body:block
    ) => {
        pub struct $name;

        $crate::__define_tool_impl!(
            $name;
            name: $tool_name,
            description: $desc,
            $( permission: $perm, )?
            $( tags: [$($tag),*], )?
            $( defer_loading: $defer, )?
            $( returns: $output_ty, )?
            $( read_only: $read_only, )?
            $( destructive: $destructive, )?
            $( idempotent: $idempotent, )?
            $( open_world: $open_world, )?
            execute($params: $params_ty, $ctx) $body
            execute_stream($stream_params: $stream_params_ty, $stream_ctx) $stream_body
        );
    };
    // Variant with streaming support (struct with fields, self in execute)
    (
        pub struct $name:ident {
            $($field_vis:vis $field_name:ident : $field_type:ty),* $(,)?
        }

        name: $tool_name:expr,
        description: $desc:expr,
        $( permission: $perm:expr, )?
        $( tags: [$($tag:expr),*], )?
        $( defer_loading: $defer:expr, )?
        $( returns: $output_ty:ty, )?
        $( read_only: $read_only:expr, )?
        $( destructive: $destructive:expr, )?
        $( idempotent: $idempotent:expr, )?
        $( open_world: $open_world:expr, )?

        execute(&$self_ident:ident, $params:ident: $params_ty:ty, $ctx:ident) $body:block

        execute_stream(&$stream_self_ident:ident, $stream_params:ident: $stream_params_ty:ty, $stream_ctx:ident) $stream_body:block
    ) => {
        pub struct $name {
            $($field_vis $field_name : $field_type),*
        }

        $crate::__define_tool_impl!(
            $name;
            name: $tool_name,
            description: $desc,
            $( permission: $perm, )?
            $( tags: [$($tag),*], )?
            $( defer_loading: $defer, )?
            $( returns: $output_ty, )?
            $( read_only: $read_only, )?
            $( destructive: $destructive, )?
            $( idempotent: $idempotent, )?
            $( open_world: $open_world, )?
            execute(&$self_ident, $params: $params_ty, $ctx) $body
        );
    };
}

#[macro_export]
macro_rules! __define_tool_impl {
    (
        $name:ident;
        name: $tool_name:expr,
        description: $desc:expr,
        $( permission: $perm:expr, )?
        $( tags: [$($tag:expr),*], )?
        $( defer_loading: $defer:expr, )?
        $( returns: $output_ty:ty, )?
        $( read_only: $read_only:expr, )?
        $( destructive: $destructive:expr, )?
        $( idempotent: $idempotent:expr, )?
        $( open_world: $open_world:expr, )?

        execute($params:ident: $params_ty:ty, $ctx:ident) $body:block
    ) => {
        impl $crate::Tool for $name {
            fn name(&self) -> &'static str {
                $tool_name
            }

            fn description(&self) -> &'static str {
                $desc
            }

            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::to_value($crate::schemars::schema_for!($params_ty)).unwrap()
            }

            $crate::__define_tool_optional!(permission, $($perm)?);
            $crate::__define_tool_optional!(tags, [$($($tag),*)?]);
            $crate::__define_tool_optional!(defer_loading, $($defer)?);
            $crate::__define_tool_output_schema!($($output_ty)?);
            $crate::__define_tool_annotations!(
                $($read_only)?,
                $($destructive)?,
                $($idempotent)?,
                $($open_world)?
            );

            fn execute(&self, params_raw: serde_json::Value, $ctx: &$crate::ToolContext) -> anyhow::Result<$crate::ToolOutput> {
                let $params: $params_ty = serde_json::from_value(params_raw)
                    .map_err(|e| anyhow::anyhow!("Invalid parameters for tool {}: {}", $tool_name, e))?;
                $body
            }
        }

    };
    (
        $name:ident;
        name: $tool_name:expr,
        description: $desc:expr,
        $( permission: $perm:expr, )?
        $( tags: [$($tag:expr),*], )?
        $( defer_loading: $defer:expr, )?
        $( returns: $output_ty:ty, )?
        $( read_only: $read_only:expr, )?
        $( destructive: $destructive:expr, )?
        $( idempotent: $idempotent:expr, )?
        $( open_world: $open_world:expr, )?

        execute(&$self_ident:ident, $params:ident: $params_ty:ty, $ctx:ident) $body:block
    ) => {
        impl $crate::Tool for $name {
            fn name(&self) -> &'static str {
                $tool_name
            }

            fn description(&self) -> &'static str {
                $desc
            }

            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::to_value($crate::schemars::schema_for!($params_ty)).unwrap()
            }

            $crate::__define_tool_optional!(permission, $($perm)?);
            $crate::__define_tool_optional!(tags, [$($($tag),*)?]);
            $crate::__define_tool_optional!(defer_loading, $($defer)?);
            $crate::__define_tool_output_schema!($($output_ty)?);
            $crate::__define_tool_annotations!(
                $($read_only)?,
                $($destructive)?,
                $($idempotent)?,
                $($open_world)?
            );

            fn execute(&$self_ident, params_raw: serde_json::Value, $ctx: &$crate::ToolContext) -> anyhow::Result<$crate::ToolOutput> {
                let $params: $params_ty = serde_json::from_value(params_raw)
                    .map_err(|e| anyhow::anyhow!("Invalid parameters for tool {}: {}", $tool_name, e))?;
                $body
            }
        }

    };
    // Tool impl + ToolStreaming impl (zero-sized struct)
    (
        $name:ident;
        name: $tool_name:expr,
        description: $desc:expr,
        $( permission: $perm:expr, )?
        $( tags: [$($tag:expr),*], )?
        $( defer_loading: $defer:expr, )?
        $( returns: $output_ty:ty, )?
        $( read_only: $read_only:expr, )?
        $( destructive: $destructive:expr, )?
        $( idempotent: $idempotent:expr, )?
        $( open_world: $open_world:expr, )?

        execute($params:ident: $params_ty:ty, $ctx:ident) $body:block
        execute_stream($stream_params:ident: $stream_params_ty:ty, $stream_ctx:ident) $stream_body:block
    ) => {
        impl $crate::Tool for $name {
            fn name(&self) -> &'static str {
                $tool_name
            }

            fn description(&self) -> &'static str {
                $desc
            }

            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::to_value($crate::schemars::schema_for!($params_ty)).unwrap()
            }

            $crate::__define_tool_optional!(permission, $($perm)?);
            $crate::__define_tool_optional!(tags, [$($($tag),*)?]);
            $crate::__define_tool_optional!(defer_loading, $($defer)?);
            $crate::__define_tool_output_schema!($($output_ty)?);
            $crate::__define_tool_annotations!(
                $($read_only)?,
                $($destructive)?,
                $($idempotent)?,
                $($open_world)?
            );

            fn execute(&self, params_raw: serde_json::Value, $ctx: &$crate::ToolContext) -> anyhow::Result<$crate::ToolOutput> {
                let $params: $params_ty = serde_json::from_value(params_raw)
                    .map_err(|e| anyhow::anyhow!("Invalid parameters for tool {}: {}", $tool_name, e))?;
                $body
            }
        }

        impl $crate::streaming::ToolStreaming for $name {
            fn execute_stream(
                &self,
                params_raw: serde_json::Value,
                $stream_ctx: &$crate::ToolContext,
            ) -> anyhow::Result<$crate::streaming::StreamReceiver> {
                let $stream_params: $stream_params_ty = serde_json::from_value(params_raw)
                    .map_err(|e| anyhow::anyhow!("Invalid parameters for tool {}: {}", $tool_name, e))?;
                $stream_body
            }
        }
    };
    // Tool impl + ToolStreaming impl (struct with fields)
    (
        $name:ident;
        name: $tool_name:expr,
        description: $desc:expr,
        $( permission: $perm:expr, )?
        $( tags: [$($tag:expr),*], )?
        $( defer_loading: $defer:expr, )?
        $( returns: $output_ty:ty, )?
        $( read_only: $read_only:expr, )?
        $( destructive: $destructive:expr, )?
        $( idempotent: $idempotent:expr, )?
        $( open_world: $open_world:expr, )?

        execute(&$self_ident:ident, $params:ident: $params_ty:ty, $ctx:ident) $body:block
        execute_stream(&$stream_self_ident:ident, $stream_params:ident: $stream_params_ty:ty, $stream_ctx:ident) $stream_body:block
    ) => {
        impl $crate::Tool for $name {
            fn name(&self) -> &'static str {
                $tool_name
            }

            fn description(&self) -> &'static str {
                $desc
            }

            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::to_value($crate::schemars::schema_for!($params_ty)).unwrap()
            }

            $crate::__define_tool_optional!(permission, $($perm)?);
            $crate::__define_tool_optional!(tags, [$($($tag),*)?]);
            $crate::__define_tool_optional!(defer_loading, $($defer)?);
            $crate::__define_tool_output_schema!($($output_ty)?);
            $crate::__define_tool_annotations!(
                $($read_only)?,
                $($destructive)?,
                $($idempotent)?,
                $($open_world)?
            );

            fn execute(&$self_ident, params_raw: serde_json::Value, $ctx: &$crate::ToolContext) -> anyhow::Result<$crate::ToolOutput> {
                let $params: $params_ty = serde_json::from_value(params_raw)
                    .map_err(|e| anyhow::anyhow!("Invalid parameters for tool {}: {}", $tool_name, e))?;
                $body
            }
        }

        impl $crate::streaming::ToolStreaming for $name {
            fn execute_stream(
                &$stream_self_ident,
                params_raw: serde_json::Value,
                $stream_ctx: &$crate::ToolContext,
            ) -> anyhow::Result<$crate::streaming::StreamReceiver> {
                let $stream_params: $stream_params_ty = serde_json::from_value(params_raw)
                    .map_err(|e| anyhow::anyhow!("Invalid parameters for tool {}: {}", $tool_name, e))?;
                $stream_body
            }
        }
    };
}

#[macro_export]
macro_rules! __define_tool_optional {
    (permission, ) => {};
    (permission, $perm:expr) => {
        fn permission(&self) -> $crate::ToolPermission {
            $perm
        }
    };
    (tags, []) => {};
    (tags, [$($tag:expr),*]) => {
        fn tags(&self) -> &[$crate::ToolTag] {
            &[$($tag),*]
        }
    };
    (defer_loading, ) => {};
    (defer_loading, $defer:expr) => {
        fn defer_loading(&self) -> Option<bool> {
            Some($defer)
        }
    };
}

#[macro_export]
macro_rules! __define_tool_output_schema {
    () => {};
    ($output_ty:ty) => {
        fn output_schema(&self) -> Option<serde_json::Value> {
            Some(serde_json::to_value($crate::schemars::schema_for!($output_ty)).unwrap())
        }
    };
}

#[macro_export]
macro_rules! __define_tool_annotations {
    ($($read_only:expr)?, $($destructive:expr)?, $($idempotent:expr)?, $($open_world:expr)?) => {
        fn annotations(&self) -> $crate::ToolAnnotations {
            #[allow(unused_mut)]
            let mut ann = $crate::ToolAnnotations::default();
            $( ann.read_only_hint = $read_only; )?
            $( ann.destructive_hint = $destructive; )?
            $( ann.idempotent_hint = $idempotent; )?
            $( ann.open_world_hint = $open_world; )?
            ann
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_define_tool_with_annotations() {
        use schemars::JsonSchema;
        use serde::Deserialize;

        #[derive(Deserialize, JsonSchema)]
        struct MyParams {
            #[allow(dead_code)]
            text: String,
        }

        define_tool! {
            pub struct AnnotatedTool;
            name: "annotated",
            description: "desc",
            read_only: true,
            destructive: false,
            idempotent: true,
            open_world: false,
            execute(params: MyParams, _ctx) {
                Ok(ToolOutput::text(params.text))
            }
        }

        let tool = AnnotatedTool;
        let ann = tool.annotations();
        assert!(ann.read_only_hint);
        assert!(!ann.destructive_hint);
        assert!(ann.idempotent_hint);
        assert!(!ann.open_world_hint);
    }

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
        for tool in &["Read", "ListDir", "Grep", "Glob"] {
            assert!(
                matches!(
                    tool_permission(tool),
                    Some(ProtocolToolPermission::AutoAllow)
                ),
                "{tool} should be AutoAllow",
            );
        }
    }

    #[test]
    fn test_get_tool_permission_write_tools() {
        for tool in &["Write", "GitCommit", "Bash"] {
            assert!(
                matches!(
                    tool_permission(tool),
                    Some(ProtocolToolPermission::RequiresConfirmation)
                ),
                "{tool} should be RequiresConfirmation",
            );
        }
    }

    #[test]
    fn test_check_tool_permission_planning_mode() {
        assert!(check_tool_permission("Read", SessionMode::Planning));
        assert!(check_tool_permission("Glob", SessionMode::Planning));
        assert!(!check_tool_permission("Bash", SessionMode::Planning));
        assert!(!check_tool_permission("Write", SessionMode::Planning));
    }

    #[test]
    fn test_check_tool_permission_executing_mode() {
        assert!(check_tool_permission("Read", SessionMode::Executing));
        assert!(check_tool_permission("Bash", SessionMode::Executing));
        assert!(check_tool_permission("Write", SessionMode::Executing));
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
            name: "Read".to_string(),
            description: "Reads a file".to_string(),
            parameters_schema: serde_json::json!({"type": "object"}),
            permission: ToolPermission::Read,
            defer_loading: None,
            annotations: None,
            tags: vec![],
            max_result_size_chars: Some(30_000),
            is_destructive_default: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Read"));
        assert!(json.contains("Reads a file"));
    }

    #[test]
    fn test_get_tool_permission_unknown() {
        let perm = tool_permission("custom_tool_xyz");
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

    struct DeferredTool;
    impl Tool for DeferredTool {
        fn name(&self) -> &'static str {
            "deferred_tool"
        }
        fn description(&self) -> &'static str {
            "First line\nSecond line"
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object", "properties": {"q": {"type": "string"}}})
        }
        fn defer_loading(&self) -> Option<bool> {
            Some(true)
        }
        fn execute(&self, _: Value, _: &ToolContext) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text(""))
        }
    }

    struct ToolSearchMock;
    impl Tool for ToolSearchMock {
        fn name(&self) -> &'static str {
            "ToolSearch"
        }
        fn description(&self) -> &'static str {
            "Searches for tools"
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }
        fn defer_loading(&self) -> Option<bool> {
            Some(true) // tool_search itself is deferred but should always be in immediate
        }
        fn execute(&self, _: Value, _: &ToolContext) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::text(""))
        }
    }

    #[test]
    fn test_list_immediate_excludes_deferred() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);
        registry.register(DeferredTool);

        let immediate = registry.list_immediate();
        assert!(immediate.iter().any(|t| t.name == "mock"));
        assert!(!immediate.iter().any(|t| t.name == "deferred_tool"));
    }

    #[test]
    fn test_list_immediate_always_includes_tool_search() {
        let mut registry = ToolRegistry::new();
        registry.register(ToolSearchMock); // marked deferred but must still appear

        let immediate = registry.list_immediate();
        assert!(immediate.iter().any(|t| t.name == "ToolSearch"));
    }

    #[test]
    fn test_list_deferred_stubs_only_deferred() {
        let mut registry = ToolRegistry::new();
        registry.register(MockTool);
        registry.register(DeferredTool);

        let stubs = registry.list_deferred_stubs();
        assert_eq!(stubs.len(), 1);
        assert_eq!(stubs[0].name, "deferred_tool");
        assert!(stubs[0].description.contains("Deferred tool"));
        assert!(stubs[0].description.contains("ToolSearch"));
        // Stub has empty schema
        assert_eq!(
            stubs[0].parameters_schema["properties"],
            serde_json::json!({})
        );
        assert_eq!(stubs[0].defer_loading, Some(true));
    }

    #[test]
    fn test_list_deferred_stubs_excludes_tool_search() {
        let mut registry = ToolRegistry::new();
        registry.register(ToolSearchMock);
        registry.register(DeferredTool);

        let stubs = registry.list_deferred_stubs();
        assert!(!stubs.iter().any(|t| t.name == "ToolSearch"));
        assert!(stubs.iter().any(|t| t.name == "deferred_tool"));
    }

    #[test]
    fn test_list_deferred_stubs_description_first_line_only() {
        let mut registry = ToolRegistry::new();
        registry.register(DeferredTool);

        let stubs = registry.list_deferred_stubs();
        assert!(stubs[0].description.starts_with("First line"));
        assert!(!stubs[0].description.contains("Second line\n"));
    }

    #[test]
    fn test_get_tool_permission_git_tools() {
        assert!(matches!(
            tool_permission("GitStatus"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            tool_permission("GitDiff"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            tool_permission("GitLog"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            tool_permission("GitCommit"),
            Some(ProtocolToolPermission::RequiresConfirmation)
        ));
    }

    #[test]
    fn test_get_tool_permission_lsp_tools() {
        assert!(matches!(
            tool_permission("LspDiagnostics"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            tool_permission("LspHover"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            tool_permission("LspDefinition"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
        assert!(matches!(
            tool_permission("LspCompletion"),
            Some(ProtocolToolPermission::AutoAllow)
        ));
    }

    #[test]
    fn test_audit_log_exact_capacity_ring_buffer() {
        let log = AuditLog::new(3);
        log.record(AuditEntry {
            tool_name: "tool_a".into(),
            call_id: "1".into(),
            success: true,
            error: None,
            duration_ms: 10,
            session_id: None,
            output_chars: 100,
        });
        log.record(AuditEntry {
            tool_name: "tool_b".into(),
            call_id: "2".into(),
            success: true,
            error: None,
            duration_ms: 20,
            session_id: None,
            output_chars: 200,
        });
        log.record(AuditEntry {
            tool_name: "tool_c".into(),
            call_id: "3".into(),
            success: false,
            error: Some("fail".into()),
            duration_ms: 30,
            session_id: None,
            output_chars: 0,
        });
        // At capacity
        assert_eq!(log.len(), 3);
        let snap = log.snapshot();
        assert_eq!(snap[0].tool_name, "tool_a");

        // Add one more — should evict oldest
        log.record(AuditEntry {
            tool_name: "tool_d".into(),
            call_id: "4".into(),
            success: true,
            error: None,
            duration_ms: 5,
            session_id: None,
            output_chars: 50,
        });
        assert_eq!(log.len(), 3);
        let snap = log.snapshot();
        assert_eq!(snap[0].tool_name, "tool_b");
        assert_eq!(snap[2].tool_name, "tool_d");
    }

    #[test]
    fn test_audit_log_is_empty_and_snapshot_empty() {
        let log = AuditLog::new(10);
        assert!(log.is_empty());
        assert!(log.snapshot().is_empty());
    }
}
