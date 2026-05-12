pub mod budget;
pub mod egress;
pub mod osv;
pub mod security;

pub use budget::BudgetInspector;
pub use egress::EgressInspector;
pub use osv::OsvInspector;
pub use security::SecurityInspector;

pub use crate::executor::permission::PermissionInspector;
pub use crate::executor::rate_limit::RateLimitInspector;
pub use crate::executor::repetition::RepetitionInspector;

use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::ToolContext;

/// Result of inspecting a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionResult {
    pub request_id: String,
    pub action: InspectionAction,
    pub reason: String,
    pub confidence: f32,
    pub inspector_name: String,
    pub finding_id: Option<String>,
}

/// Action to take based on inspection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum InspectionAction {
    Allow,
    Deny,
    RequireApproval(Option<String>),
}

/// A simplified tool call for inspection
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub timestamp: Instant,
}

impl ToolCallInfo {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
            timestamp: Instant::now(),
        }
    }

    pub fn matches(&self, other: &Self) -> bool {
        self.name == other.name && self.arguments == other.arguments
    }
}

/// Trait for tool inspectors
pub trait ToolInspector: Send + Sync {
    /// Name of this inspector.
    fn name(&self) -> &'static str;

    /// Inspect a tool call and return a result.
    fn inspect(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        ctx: &ToolContext,
    ) -> InspectionResult;

    /// Whether this inspector is enabled.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Reset inspector state (e.g., between sessions).
    fn reset(&self) {}
}
