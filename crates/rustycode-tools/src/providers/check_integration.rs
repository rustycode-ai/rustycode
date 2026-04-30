use rustycode_tools_api::{Tool, ToolContext, ToolOutput, ToolPermission};
use serde_json::{json, Value};

pub struct CheckIntegrationTool;

impl Tool for CheckIntegrationTool {
    fn name(&self) -> &str {
        "reasoning_integrate"
    }

    fn description(&self) -> &str {
        "Check how new code integrates with existing codebase. Identifies affected modules, potential breakage points, and required test coverage for safe integration."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["changes"],
            "properties": {
                "changes": {
                    "type": "string",
                    "description": "Description of the changes or code being integrated"
                },
                "scope": {
                    "type": "string",
                    "description": "Scope of integration check: 'module', 'crate', or 'workspace'",
                    "default": "crate"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        let changes = params
            .get("changes")
            .and_then(|v| v.as_str())
            .unwrap_or("unspecified changes");
        let scope = params
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("crate");

        let output = json!({
            "phase": "check_integration",
            "instruction": format!(
                "Analyze how these changes integrate with the existing codebase at {scope} scope. Identify affected modules, potential breakage points, and required test coverage."
            ),
            "changes": changes,
            "scope": scope,
            "integration_checklist": [
                "Public API compatibility maintained",
                "No breaking changes to downstream consumers",
                "Error handling paths covered",
                "Thread safety preserved",
                "Test coverage adequate for changed paths"
            ]
        });

        let text = format!(
            "## Phase: Check Integration\n\nChanges: {changes}\nScope: {scope}\n\n\
             Verify:\n\
             1. **API compatibility** — No breaking public API changes?\n\
             2. **Downstream impact** — Which consumers are affected?\n\
             3. **Error handling** — Are new error paths covered?\n\
             4. **Thread safety** — Concurrency invariants preserved?\n\
             5. **Test coverage** — Sufficient tests for changed paths?"
        );

        Ok(ToolOutput::with_structured(text, output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn ctx() -> ToolContext {
        static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        let dir = DIR.get_or_init(|| tempdir().expect("tempdir"));
        ToolContext::new(dir.path())
    }

    #[test]
    fn tool_metadata() {
        let tool = CheckIntegrationTool;
        assert_eq!(tool.name(), "reasoning_integrate");
        assert_eq!(tool.permission(), ToolPermission::None);
        let schema = tool.parameters_schema();
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("changes")));
    }

    #[test]
    fn produces_structured_output() {
        let tool = CheckIntegrationTool;
        let result = tool
            .execute(json!({"changes": "Refactored auth module"}), &ctx())
            .unwrap();
        assert!(result.text.contains("Check Integration"));
        assert!(result.text.contains("Refactored auth module"));
        assert!(result.structured.is_some());
    }

    #[test]
    fn default_scope_is_crate() {
        let tool = CheckIntegrationTool;
        let result = tool
            .execute(json!({"changes": "Updated API"}), &ctx())
            .unwrap();
        assert!(result.text.contains("crate"));
    }

    #[test]
    fn custom_scope_overrides_default() {
        let tool = CheckIntegrationTool;
        let result = tool
            .execute(
                json!({"changes": "Updated API", "scope": "workspace"}),
                &ctx(),
            )
            .unwrap();
        assert!(result.text.contains("workspace"));
    }

    #[test]
    fn missing_changes_uses_default() {
        let tool = CheckIntegrationTool;
        let result = tool.execute(json!({}), &ctx()).unwrap();
        assert!(result.text.contains("unspecified changes"));
    }
}
