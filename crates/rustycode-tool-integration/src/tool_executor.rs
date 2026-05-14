use rustycode_protocol::{ToolCall, ToolPermission, ToolResult};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallInfo {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
    pub permission: ToolPermission,
    pub defer_loading: Option<bool>,
}

pub trait ToolExecutorApi: Send + Sync {
    fn list(&self) -> Vec<ToolCallInfo>;
    fn execute(&self, call: &ToolCall) -> ToolResult;
}
