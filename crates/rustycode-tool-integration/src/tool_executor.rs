use anyhow::Result;
use chrono::{DateTime, Utc};
use rustycode_protocol::{ToolCall, ToolPermission, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiCall {
    pub model: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub timestamp: DateTime<Utc>,
    pub tool_name: Option<String>,
}

pub trait CostTrackerProvider: Send + Sync {
    fn record_call(&self, call: ApiCall) -> Result<()>;
}
