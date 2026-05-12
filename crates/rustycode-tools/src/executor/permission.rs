use crate::executor::inspector::{InspectionAction, InspectionResult, ToolCallInfo};
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

impl crate::executor::inspector::ToolInspector for PermissionInspector {
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

/// Check if a tool is permitted in the given session mode.
/// In Planning mode, only read-only tools are allowed.
/// In Executing mode, all tools are permitted.
pub fn check_tool_permission(tool_name: &str, mode: rustycode_protocol::SessionMode) -> bool {
    match mode {
        rustycode_protocol::SessionMode::Planning => {
            matches!(
                tool_name,
                rustycode_tools_api::tool_names::READ
                    | rustycode_tools_api::tool_names::LIST_DIR
                    | rustycode_tools_api::tool_names::GREP
                    | "Search"
                    | rustycode_tools_api::tool_names::GLOB
                    | rustycode_tools_api::tool_names::FIND
                    | rustycode_tools_api::tool_names::INSPECT
                    | rustycode_tools_api::tool_names::LSP_DIAGNOSTICS
                    | rustycode_tools_api::tool_names::LSP_HOVER
                    | rustycode_tools_api::tool_names::LSP_DEFINITION
                    | rustycode_tools_api::tool_names::LSP_COMPLETION
                    | rustycode_tools_api::tool_names::LSP_DOCUMENT_SYMBOLS
                    | rustycode_tools_api::tool_names::LSP_REFERENCES
                    | rustycode_tools_api::tool_names::LSP_FULL_DIAGNOSTICS
                    | rustycode_tools_api::tool_names::LSP_CODE_ACTIONS
                    | rustycode_tools_api::tool_names::LSP_RENAME
                    | rustycode_tools_api::tool_names::LSP_FORMATTING
                    | rustycode_tools_api::tool_names::LSP_GET_SYMBOLS_OVERVIEW
                    | rustycode_tools_api::tool_names::LSP_FIND_SYMBOL
                    | rustycode_tools_api::tool_names::LSP_REPLACE_SYMBOL_BODY
                    | rustycode_tools_api::tool_names::LSP_INSERT_BEFORE_SYMBOL
                    | rustycode_tools_api::tool_names::LSP_INSERT_AFTER_SYMBOL
                    | rustycode_tools_api::tool_names::LSP_SAFE_DELETE_SYMBOL
                    | rustycode_tools_api::tool_names::LSP_RENAME_SYMBOL
                    | rustycode_tools_api::tool_names::LSP_ANALYZE_SYMBOL
                    | rustycode_tools_api::tool_names::LSP_EXTRACT_SYMBOL
                    | rustycode_tools_api::tool_names::LSP_INLINE_SYMBOL
                    | "MemorySearch"
                    | "MemoryList"
                    | "SkillList"
                    | "Doctor"
                    | rustycode_tools_api::tool_names::GIT_STATUS
                    | rustycode_tools_api::tool_names::GIT_LOG
                    | rustycode_tools_api::tool_names::GIT_DIFF
            )
        }
        rustycode_protocol::SessionMode::Executing => true,
        _ => true,
    }
}

/// Check if the given permission level is allowed by the context's `max_permission`.
///
/// Permission hierarchy: None < Read < Write < Execute < Network.
/// Returns an error if the required permission exceeds the context's allowance.
pub fn check_permission(
    permission: crate::ToolPermission,
    ctx: &crate::ToolContext,
) -> anyhow::Result<()> {
    let required = permission_level(&permission);
    let allowed = permission_level(&ctx.max_permission);
    if required > allowed {
        anyhow::bail!(
            "permission denied: tool requires {:?} but context allows {:?}",
            permission,
            ctx.max_permission
        );
    }
    Ok(())
}

/// Check if a path is allowed under sandbox rules.
///
/// Validates against:
/// 1. Denied paths (always blocked)
/// 2. Allowed paths (whitelist, if configured)
/// 3. Blocked path components (.ssh, .gnupg, .aws, etc.)
pub fn check_sandbox_path(path: &std::path::Path, ctx: &crate::ToolContext) -> anyhow::Result<()> {
    // Check denied paths first
    for denied in &ctx.sandbox.denied_paths {
        if path.starts_with(denied) {
            anyhow::bail!(
                "sandbox: path '{}' is under denied prefix '{}'",
                path.display(),
                denied.display()
            );
        }
    }

    // Check allowed paths (whitelist mode if configured)
    if let Some(allowed) = &ctx.sandbox.allowed_paths {
        let permitted = allowed.iter().any(|prefix| path.starts_with(prefix));
        if !permitted {
            anyhow::bail!(
                "sandbox: path '{}' is outside allowed directories",
                path.display()
            );
        }
    }

    // Check blocked path components (.ssh, .gnupg, .aws, etc.)
    for component in path.components() {
        if let std::path::Component::Normal(os_str) = component {
            if let Some(s) = os_str.to_str() {
                if crate::security::validation::BLOCKED_PATH_COMPONENTS.contains(&s) {
                    anyhow::bail!(
                        "sandbox: path contains blocked component '{}' for security reasons",
                        s
                    );
                }
            }
        }
    }

    Ok(())
}

/// Numeric level for permission comparison. Higher = more permissive.
const fn permission_level(p: &crate::ToolPermission) -> u8 {
    match p {
        crate::ToolPermission::None => 0,
        crate::ToolPermission::Read => 1,
        crate::ToolPermission::Write => 2,
        crate::ToolPermission::Execute => 3,
        crate::ToolPermission::Network => 4,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::inspector::ToolInspector;
    use serde_json::json;

    fn test_ctx() -> crate::ToolContext {
        crate::ToolContext::new(std::env::temp_dir())
    }

    fn make_call(name: &str, args: serde_json::Value) -> ToolCallInfo {
        ToolCallInfo::new("test-id", name, args)
    }

    #[test]
    fn test_permission_inspector_read_only() {
        let inspector = PermissionInspector::new();
        let ctx = test_ctx();

        let call = make_call("Read", json!({"path": "/tmp/test.txt"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert_eq!(result.action, InspectionAction::Allow);
    }

    #[test]
    fn test_permission_inspector_restricted() {
        let inspector = PermissionInspector::new();
        let ctx = test_ctx();

        let call = make_call("Bash", json!({"command": "rm -rf /"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert!(matches!(
            result.action,
            InspectionAction::RequireApproval(_)
        ));
    }

    #[test]
    fn test_permission_inspector_write_restricted() {
        let inspector = PermissionInspector::new();
        let ctx = test_ctx();

        let call = make_call("Write", json!({"path": "/tmp/test.txt", "content": "hi"}));
        let result = inspector.inspect(&call, &[], &ctx);

        assert!(matches!(
            result.action,
            InspectionAction::RequireApproval(_)
        ));
    }
}
