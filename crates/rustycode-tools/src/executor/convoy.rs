//! Convoy Tool Dispatcher
//!
//! Gates tool execution based on agent role and plan approval state.

use crate::ToolGate;
use rustycode_protocol::{permission_role::ToolBlockedReason, AgentRole, ToolCall, ToolResult};
use std::sync::Arc;

/// Dispatches tool calls after checking permissions against a gate.
pub struct ConvoyDispatcher {
    /// The permission gate (usually `PlanMode`)
    pub gate: Arc<dyn ToolGate>,
}

impl ConvoyDispatcher {
    pub fn new(gate: Arc<dyn ToolGate>) -> Self {
        Self { gate }
    }

    /// Check if a tool call is allowed for the given role
    pub fn check_allowed(&self, role: AgentRole, tool_name: &str) -> Result<(), ToolBlockedReason> {
        self.gate.check_access(role, tool_name)
    }

    /// Wrap a tool execution with permission checking
    pub fn execute_guarded<F>(&self, role: AgentRole, call: &ToolCall, execute_fn: F) -> ToolResult
    where
        F: FnOnce(&ToolCall) -> ToolResult,
    {
        if let Err(reason) = self.check_allowed(role, &call.name) {
            return ToolResult {
                call_id: call.call_id.clone(),
                output: String::new(),
                error: Some(format!("Permission denied: {reason}")),
                success: false,
                exit_code: None,
                data: None,
            };
        }

        execute_fn(call)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockGate;
    impl ToolGate for MockGate {
        fn check_access(&self, role: AgentRole, tool_name: &str) -> Result<(), ToolBlockedReason> {
            if role == AgentRole::Reviewer && tool_name == "write" {
                return Err(ToolBlockedReason::NotAllowedForRole {
                    tool: tool_name.to_string(),
                    role,
                });
            }
            Ok(())
        }
    }

    #[test]
    fn test_execute_guarded_allowed() {
        let dispatcher = ConvoyDispatcher::new(Arc::new(MockGate));
        let call = ToolCall {
            call_id: "1".into(),
            name: "read".into(),
            arguments: serde_json::json!({}),
        };

        let result = dispatcher.execute_guarded(AgentRole::Reviewer, &call, |_| ToolResult {
            call_id: "1".into(),
            output: "success".into(),
            error: None,
            success: true,
            exit_code: None,
            data: None,
        });

        assert!(result.success);
        assert_eq!(result.output, "success");
    }

    #[test]
    fn test_execute_guarded_denied() {
        let dispatcher = ConvoyDispatcher::new(Arc::new(MockGate));
        let call = ToolCall {
            call_id: "2".into(),
            name: "write".into(),
            arguments: serde_json::json!({}),
        };

        let result = dispatcher.execute_guarded(AgentRole::Reviewer, &call, |_| {
            panic!("Should not be called")
        });

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Permission denied"));
    }
}
