use crate::executor::manager::{InspectionAction, InspectionResult, ToolCallInfo};
use crate::{ToolContext, ToolPermission};

/// Enforces session permission levels on tool calls.
///
/// Maps tool names to their required permission levels and checks
/// against the session's maximum allowed permission.
pub struct PermissionInspector {
    /// Tools that require elevated permissions
    restricted_tools: Vec<&'static str>,
}

impl Default for PermissionInspector {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionInspector {
    pub fn new() -> Self {
        Self {
            restricted_tools: vec![
                "Bash",
                "Write",
                "text_editor_20250124",
                "GitCommit",
                "ApplyPatch",
                "multi_edit",
                "DockerRun",
                "DockerBuild",
                "DockerStop",
                "HttpPost",
                "HttpPut",
                "HttpDelete",
            ],
        }
    }

    fn is_restricted(&self, tool_name: &str) -> bool {
        self.restricted_tools.contains(&tool_name)
    }
}

impl crate::executor::manager::ToolInspector for PermissionInspector {
    fn name(&self) -> &'static str {
        "permission"
    }

    fn inspect(
        &self,
        call: &ToolCallInfo,
        _history: &[ToolCallInfo],
        ctx: &ToolContext,
    ) -> InspectionResult {
        if self.is_restricted(&call.name) {
            // Check if the context allows this permission level
            if ctx.max_permission == ToolPermission::None {
                return InspectionResult {
                    request_id: call.id.clone(),
                    action: InspectionAction::Deny,
                    reason: format!(
                        "Tool '{}' requires elevated permissions but session has none",
                        call.name
                    ),
                    confidence: 1.0,
                    inspector_name: "permission".to_string(),
                    finding_id: Some("PERM-001".to_string()),
                };
            }

            return InspectionResult {
                request_id: call.id.clone(),
                action: InspectionAction::RequireApproval(Some(format!(
                    "Tool '{}' modifies system state",
                    call.name
                ))),
                reason: format!(
                    "Tool '{}' is a restricted operation requiring approval",
                    call.name
                ),
                confidence: 1.0,
                inspector_name: "permission".to_string(),
                finding_id: None,
            };
        }

        InspectionResult {
            request_id: call.id.clone(),
            action: InspectionAction::Allow,
            reason: format!("Tool '{}' is a read-only operation", call.name),
            confidence: 1.0,
            inspector_name: "permission".to_string(),
            finding_id: None,
        }
    }
}
