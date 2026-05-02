use serde::{Deserialize, Serialize};
use crate::ExecutionContext;

/// What execution modes this unit can support
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnitCapabilities {
    pub can_execute_directly: bool,
    pub can_bundle_knowledge: bool,
    pub can_reason_autonomously: bool,
}

/// Advanced Claude tool use metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdvancedToolMetadata {
    pub examples: Vec<ExecutionExample>,
    pub defer_loading: bool,
    pub search_hints: Vec<String>,
    pub execution_strategy: ExecutionMode,
    pub result_processor: Option<ResultProcessor>,
}

/// Execution mode directive
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Direct,
    Bundled,
    Autonomous,
    Hybrid,
    /// Programmatic: Claude generates code that invokes this unit
    Programmatic,
}

/// Concrete usage example
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionExample {
    pub scenario: String,
    pub input: serde_json::Value,
    pub output: serde_json::Value,
    pub context: ExecutionContext,
    pub explanation: Option<String>,
}

/// Tool schema
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSchema {
    pub parameters: serde_json::Value,
    pub returns: Option<serde_json::Value>,
}

/// Result processor
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResultProcessor {
    pub extraction_path: Option<String>,
    pub transform: Option<String>,
}
