use crate::{ToolContext, ToolInfo, ToolPermission, ToolRegistry};
use rustycode_protocol::{AgentRole, ToolCall, ToolResult};

/// Tool executor combining a registry and a context.
/// Used by the `auto_tool` module for programmatic tool invocation.
pub struct ToolExecutor {
    pub registry: std::sync::Arc<ToolRegistry>,
    pub context: ToolContext,
}

impl ToolExecutor {
    pub const fn new(registry: std::sync::Arc<ToolRegistry>, context: ToolContext) -> Self {
        Self { registry, context }
    }

    /// Create a new executor from a working directory with an empty registry.
    pub fn from_cwd(cwd: std::path::PathBuf) -> Self {
        Self {
            registry: std::sync::Arc::new(ToolRegistry::new()),
            context: ToolContext::new(cwd),
        }
    }

    /// List all registered tools.
    pub fn list(&self) -> Vec<ToolInfo> {
        self.registry.list()
    }

    /// Execute a tool call using the registry and stored context.
    pub fn execute(&self, call: &ToolCall) -> ToolResult {
        self.registry.execute(call, &self.context)
    }

    /// Builder: set the agent role on the context.
    pub fn with_role(mut self, role: AgentRole) -> Self {
        self.context = self.context.with_role(role);
        self
    }

    /// Builder: attach a plan gate on the context.
    pub fn with_plan_gate(mut self, gate: std::sync::Arc<dyn crate::ToolGate>) -> Self {
        self.context = self.context.with_plan_gate(gate);
        self
    }

    /// Builder: enable structured output for ACP / tool-server consumers.
    pub fn with_structured_output(mut self, enabled: bool) -> Self {
        self.context = self.context.with_structured_output(enabled);
        self
    }

    /// Execute a tool call using the registry. The optional `_session` parameter
    /// is ignored in this stub implementation.
    pub fn execute_with_session(&self, call: &ToolCall, _session: Option<()>) -> ToolResult {
        self.registry.execute(call, &self.context)
    }
}

impl Clone for ToolExecutor {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            context: self.context.clone(),
        }
    }
}

impl rustycode_tool_integration::tool_executor::ToolExecutorApi for ToolExecutor {
    fn list(&self) -> Vec<rustycode_tool_integration::tool_executor::ToolInfo> {
        self.registry
            .list()
            .into_iter()
            .map(|info| rustycode_tool_integration::tool_executor::ToolInfo {
                name: info.name,
                description: info.description,
                parameters_schema: info.parameters_schema,
                permission: match info.permission {
                    ToolPermission::None => rustycode_protocol::ToolPermission::Blocked,
                    ToolPermission::Read => rustycode_protocol::ToolPermission::Read,
                    ToolPermission::Write => rustycode_protocol::ToolPermission::Write,
                    ToolPermission::Execute => rustycode_protocol::ToolPermission::Execute,
                    ToolPermission::Network => rustycode_protocol::ToolPermission::Execute,
                    _ => rustycode_protocol::ToolPermission::RequiresConfirmation,
                },
                defer_loading: info.defer_loading,
            })
            .collect()
    }

    fn execute(&self, call: &rustycode_protocol::ToolCall) -> rustycode_protocol::ToolResult {
        self.registry.execute(call, &self.context)
    }
}
