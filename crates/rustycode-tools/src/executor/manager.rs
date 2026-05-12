//! Tool Inspection Manager and core types
//!
//! Holds InspectionResult, InspectionAction, ToolCallInfo, ToolInspector trait,
//! and ToolInspectionManager. Extracted from inspector.rs to enable splitting
//! inspectors into separate files.

use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::ToolContext;

/// Result of inspecting a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionResult {
    /// ID of the tool request being inspected
    pub request_id: String,
    /// Action to take
    pub action: InspectionAction,
    /// Human-readable reason for the decision
    pub reason: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Name of the inspector that produced this result
    pub inspector_name: String,
    /// Optional finding ID for tracking (e.g., "REP-001")
    pub finding_id: Option<String>,
}

/// Action to take based on inspection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum InspectionAction {
    /// Allow the tool to execute
    Allow,
    /// Deny the tool execution completely
    Deny,
    /// Require user approval before execution
    RequireApproval(Option<String>),
}

/// A simplified tool call for inspection
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    /// Unique ID for this call
    pub id: String,
    /// Tool name
    pub name: String,
    /// Tool arguments as JSON
    pub arguments: serde_json::Value,
    /// When the call was made
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

    /// Check if this call matches another (same tool + same args)
    pub fn matches(&self, other: &Self) -> bool {
        self.name == other.name && self.arguments == other.arguments
    }
}

/// Trait for tool inspectors
pub trait ToolInspector: Send + Sync {
    /// Name of this inspector
    fn name(&self) -> &'static str;

    /// Inspect a tool call and return a result
    fn inspect(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        ctx: &ToolContext,
    ) -> InspectionResult;

    /// Whether this inspector is enabled
    fn is_enabled(&self) -> bool {
        true
    }

    /// Reset inspector state (e.g., between sessions)
    fn reset(&self) {}
}

/// Manages a pipeline of tool inspectors
pub struct ToolInspectionManager {
    inspectors: Vec<Box<dyn ToolInspector>>,
}

impl Default for ToolInspectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolInspectionManager {
    pub fn new() -> Self {
        Self {
            inspectors: Vec::new(),
        }
    }

    /// Create a manager with default inspectors
    pub fn with_defaults(max_repetitions: u32) -> Self {
        let mut manager = Self::new();
        manager.add_inspector(Box::new(
            crate::executor::inspector::RepetitionInspector::new(Some(max_repetitions)),
        ));
        manager.add_inspector(Box::new(
            crate::executor::inspector::PermissionInspector::new(),
        ));
        manager
    }

    /// Create a manager with all inspectors including security scanning
    pub fn with_security(max_repetitions: u32) -> Self {
        let mut manager = Self::new();
        manager.add_inspector(Box::new(
            crate::executor::inspector::SecurityInspector::new(),
        ));
        manager.add_inspector(Box::new(crate::executor::inspector::EgressInspector::new()));
        manager.add_inspector(Box::new(crate::executor::inspector::OsvInspector::new()));
        manager.add_inspector(Box::new(
            crate::executor::inspector::RepetitionInspector::new(Some(max_repetitions)),
        ));
        manager.add_inspector(Box::new(
            crate::executor::inspector::PermissionInspector::new(),
        ));
        manager
    }

    /// Add an inspector to the pipeline
    pub fn add_inspector(&mut self, inspector: Box<dyn ToolInspector>) {
        self.inspectors.push(inspector);
    }

    /// Run all inspectors on a tool call
    ///
    /// Returns all results. If any inspector denies, the call should be blocked.
    /// The most restrictive action wins: Deny > `RequireApproval` > Allow.
    pub fn inspect(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        ctx: &ToolContext,
    ) -> Vec<InspectionResult> {
        let mut results = Vec::new();

        for inspector in &self.inspectors {
            if !inspector.is_enabled() {
                continue;
            }

            let result = inspector.inspect(call, history, ctx);
            tracing::debug!(
                "[{}] action={:?} reason={}",
                inspector.name(),
                result.action,
                result.reason
            );
            results.push(result);
        }

        results
    }

    /// Check if a tool call should be allowed
    ///
    /// Returns the most restrictive action from all inspectors.
    pub fn check(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        ctx: &ToolContext,
    ) -> InspectionAction {
        let results = self.inspect(call, history, ctx);

        let mut action = InspectionAction::Allow;
        for result in &results {
            match (&action, &result.action) {
                (_, InspectionAction::Deny) => {
                    return InspectionAction::Deny;
                }
                (InspectionAction::Allow, InspectionAction::RequireApproval(msg)) => {
                    action = InspectionAction::RequireApproval(msg.clone());
                }
                _ => {}
            }
        }
        action
    }

    /// Get the denial reason if any inspector denied the call
    pub fn denial_reason(
        &self,
        call: &ToolCallInfo,
        history: &[ToolCallInfo],
        ctx: &ToolContext,
    ) -> Option<String> {
        let results = self.inspect(call, history, ctx);
        results
            .iter()
            .find(|r| r.action == InspectionAction::Deny)
            .map(|r| r.reason.clone())
    }

    /// Get names of all registered inspectors
    pub fn inspector_names(&self) -> Vec<&'static str> {
        self.inspectors.iter().map(|i| i.name()).collect()
    }

    /// Reset all inspectors
    pub fn reset_all(&self) {
        for inspector in &self.inspectors {
            inspector.reset();
        }
    }
}
