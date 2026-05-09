use crate::{ToolOutput, ToolPermission};
use schemars::JsonSchema;
use serde_json::json;

#[derive(serde::Deserialize, JsonSchema)]
pub struct CheckIntegrationParams {
    /// Description of the changes or code being integrated
    #[serde(default)]
    changes: Option<String>,
    /// Scope of integration check: 'module', 'crate', or 'workspace'
    #[serde(default)]
    scope: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct CheckIntegrationTool;

    name: "ReasoningIntegrate",
    description: "Check how new code integrates with existing codebase. Identifies affected modules, potential breakage points, and required test coverage for safe integration.",
    permission: ToolPermission::None,

    execute(params: CheckIntegrationParams, ctx) {
        let changes = params.changes.as_deref().unwrap_or("unspecified changes");
        let scope = params.scope.as_deref().unwrap_or("crate");

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

        Ok(ToolOutput::text(text).with_metadata(ctx, || output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use crate::ToolContext;
    use tempfile::tempdir;

    fn ctx() -> ToolContext {
        static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        let dir = DIR.get_or_init(|| tempdir().expect("tempdir"));
        ToolContext::new(dir.path()).with_structured_output(true)
    }

    #[test]
    fn tool_metadata() {
        let tool = CheckIntegrationTool;
        assert_eq!(tool.name(), "ReasoningIntegrate");
        assert_eq!(tool.permission(), ToolPermission::None);
        let schema = tool.parameters_schema();
        // With define_tool, schema is auto-generated from struct; check properties exist
        assert!(schema["properties"]["changes"].is_object());
        assert!(schema["properties"]["scope"].is_object());
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
