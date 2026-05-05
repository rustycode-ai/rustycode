use anyhow::Result;
use chrono::{DateTime, Utc};
use rustycode_protocol::{ApiCall, CostTrackerProvider, ToolCall, ToolPermission, ToolResult};

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
    pub permission: ToolPermission,
    pub defer_loading: Option<bool>,
}

pub trait ToolExecutorApi: Send + Sync {
    fn list(&self) -> Vec<ToolInfo>;
    fn execute(&self, call: &ToolCall) -> ToolResult;
}
