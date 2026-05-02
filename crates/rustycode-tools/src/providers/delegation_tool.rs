//! Delegation tool — captures delegation intent as structured JSON.
//!
//! For real sub-agent execution, use `DelegationExecutor` from `rustycode-tui`.
//! This tool remains for non-TUI consumers that need intent capture only.
//!
//! This is a V1 "intent-capture" tool. It validates and serializes the
//! delegation request into a JSON result that the TUI event loop can
//! consume to dispatch the actual task. No direct dependency on
//! `rustycode-orchestration` — the real dispatch happens one layer up.

use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// Valid roles a delegated task can assume.
const VALID_ROLES: &[&str] = &[
    "explore",
    "research",
    "code",
    "review",
    "verify",
    "plan",
    "debug",
];

/// Intent-only delegation tool — captures the LLM's delegation intent as structured JSON.
///
/// **Note**: The TUI's `DelegationExecutor` (in `rustycode-tui`) provides real sub-agent
/// execution by spawning `AgentSession` instances with orchestration planning. This tool
/// is kept for non-TUI consumers (benchmarks, headless mode) that need intent capture
/// without actual sub-agent execution.
///
/// If you're building a TUI or interactive session, prefer `DelegationExecutor` from
/// `rustycode-tui::agents::delegation_executor`.
#[derive(Default)]
pub struct DelegationTool;

impl DelegationTool {
    /// Generate a short deterministic-ish task ID for tracking.
    ///
    /// Uses a counter combined with a timestamp fragment. Good enough for
    /// V1 — a proper UUID or snowflake can replace this later.
    fn generate_task_id() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        format!("del_{ts:x}_{seq}")
    }

    /// Validate that `role` is one of the accepted enum values.
    fn validate_role(role: &str) -> Result<()> {
        if VALID_ROLES.contains(&role) {
            Ok(())
        } else {
            Err(anyhow!(
                "invalid role '{}': must be one of {}",
                role,
                VALID_ROLES.join("|")
            ))
        }
    }
}

impl Tool for DelegationTool {
    fn name(&self) -> &'static str {
        "delegate_task"
    }

    fn description(&self) -> &'static str {
        "Spawn a delegated task with its own context. Use for research, exploration, code review, \
         or parallel implementation tasks that benefit from context isolation."
    }

    fn permission(&self) -> ToolPermission {
        // Read-only — this tool only captures intent; the actual dispatch
        // happens in the TUI layer which has its own permission checks.
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["task_description"],
            "properties": {
                "task_description": {
                    "type": "string",
                    "description": "What the delegated task should do"
                },
                "role": {
                    "type": "string",
                    "enum": VALID_ROLES,
                    "description": "Role for the spawned task (default: explore)"
                },
                "path_scope": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths the task should focus on"
                },
                "resume_from": {
                    "type": "string",
                    "description": "Checkpoint to resume from"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let task_description = params
            .get("task_description")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow!("missing required parameter 'task_description' (string)")
            })?;

        if task_description.trim().is_empty() {
            return Err(anyhow!("'task_description' must not be empty"));
        }

        let role = params
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("explore");

        Self::validate_role(role)?;

        let path_scope: Vec<String> = params
            .get("path_scope")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let resume_from = params
            .get("resume_from")
            .and_then(Value::as_str)
            .map(String::from);

        let task_id = Self::generate_task_id();

        let mut result = json!({
            "task_id": task_id,
            "role": role,
            "status": "delegated",
            "task_description": task_description,
        });

        if !path_scope.is_empty() {
            result["path_scope"] = json!(path_scope);
        }

        if let Some(checkpoint) = resume_from {
            result["resume_from"] = json!(checkpoint);
        }

        let text = format!(
            "Task delegated: [{role}] {task_description} (id: {task_id})"
        );

        Ok(ToolOutput::with_structured(text, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolContext;

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    #[test]
    fn test_name() {
        let tool = DelegationTool;
        assert_eq!(tool.name(), "delegate_task");
    }

    #[test]
    fn test_description() {
        let tool = DelegationTool;
        assert!(!tool.description().is_empty());
        assert!(tool.description().contains("delegat"));
    }

    #[test]
    fn test_schema_has_required_fields() {
        let tool = DelegationTool;
        let schema = tool.parameters_schema();

        // Verify required fields
        let required = schema["required"].as_array().expect("required should be array");
        assert!(required.iter().any(|v| v == "task_description"));

        // Verify role enum
        let role_enum = schema["properties"]["role"]["enum"].as_array().expect("role should have enum");
        assert!(role_enum.iter().any(|v| v == "explore"));
        assert!(role_enum.iter().any(|v| v == "debug"));

        // Verify path_scope is array of strings
        let path_type = &schema["properties"]["path_scope"]["type"];
        assert_eq!(path_type, "array");
    }

    #[test]
    fn test_execute_with_valid_input() {
        let tool = DelegationTool;
        let ctx = test_ctx();

        let params = json!({
            "task_description": "Find all unsafe blocks in the codebase",
            "role": "explore",
            "path_scope": ["/src/main.rs", "/src/lib.rs"]
        });

        let output = tool.execute(params, &ctx).expect("execute should succeed");

        assert!(output.text.contains("[explore]"));
        assert!(output.text.contains("Find all unsafe blocks"));

        let structured = output.structured.expect("should have structured output");
        assert_eq!(structured["status"], "delegated");
        assert_eq!(structured["role"], "explore");
        assert_eq!(
            structured["task_description"],
            "Find all unsafe blocks in the codebase"
        );
        assert!(structured["task_id"].as_str().unwrap().starts_with("del_"));

        let paths = structured["path_scope"].as_array().expect("path_scope should be array");
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn test_execute_with_defaults() {
        let tool = DelegationTool;
        let ctx = test_ctx();

        let params = json!({
            "task_description": "Review the auth module"
        });

        let output = tool.execute(params, &ctx).expect("execute should succeed");
        let structured = output.structured.expect("should have structured output");

        assert_eq!(structured["role"], "explore");
        assert_eq!(structured["status"], "delegated");
        assert!(structured.get("path_scope").is_none());
        assert!(structured.get("resume_from").is_none());
    }

    #[test]
    fn test_execute_with_resume_from() {
        let tool = DelegationTool;
        let ctx = test_ctx();

        let params = json!({
            "task_description": "Continue debugging",
            "role": "debug",
            "resume_from": "checkpoint_001"
        });

        let output = tool.execute(params, &ctx).expect("execute should succeed");
        let structured = output.structured.expect("should have structured output");

        assert_eq!(structured["resume_from"], "checkpoint_001");
        assert_eq!(structured["role"], "debug");
    }

    #[test]
    fn test_execute_missing_required_field() {
        let tool = DelegationTool;
        let ctx = test_ctx();

        let params = json!({
            "role": "explore"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("task_description"));
    }

    #[test]
    fn test_execute_empty_description() {
        let tool = DelegationTool;
        let ctx = test_ctx();

        let params = json!({
            "task_description": "   "
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must not be empty"));
    }

    #[test]
    fn test_execute_invalid_role() {
        let tool = DelegationTool;
        let ctx = test_ctx();

        let params = json!({
            "task_description": "Do something",
            "role": "nonexistent_role"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid role"), "expected 'invalid role', got: {err_msg}");
        assert!(err_msg.contains("nonexistent_role"));
    }

    #[test]
    fn test_all_valid_roles_accepted() {
        let tool = DelegationTool;
        let ctx = test_ctx();

        for role in VALID_ROLES {
            let params = json!({
                "task_description": format!("Test task for {role}"),
                "role": role
            });

            let output = tool.execute(params, &ctx)
                .unwrap_or_else(|e| panic!("role '{role}' should be valid: {e}"));
            let structured = output.structured.expect("should have structured output");
            assert_eq!(structured["role"], *role);
        }
    }

    #[test]
    fn test_permission_is_read() {
        let tool = DelegationTool;
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_task_id_format() {
        let id = DelegationTool::generate_task_id();
        assert!(id.starts_with("del_"));
        assert!(id.len() > 10, "task ID should contain timestamp + counter");
    }

    #[test]
    fn test_task_ids_are_unique() {
        let id1 = DelegationTool::generate_task_id();
        let id2 = DelegationTool::generate_task_id();
        assert_ne!(id1, id2, "consecutive task IDs must be unique");
    }
}
