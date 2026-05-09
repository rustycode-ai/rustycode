use crate::{ToolOutput, ToolPermission};
use schemars::JsonSchema;
use serde_json::json;

#[derive(serde::Deserialize, JsonSchema)]
pub struct ValidateRequirementsParams {
    /// The requirements text to validate
    requirements: Option<String>,
    /// Additional context: existing code, constraints, or related requirements
    context: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct ValidateRequirementsTool;

    name: "ReasoningValidate",
    description: "Validate that requirements are complete, consistent, and testable before implementation. Checks for ambiguity, missing acceptance criteria, and conflicts between requirements.",
    permission: ToolPermission::None,

    execute(params: ValidateRequirementsParams, ctx) {
        let requirements = params.requirements.as_deref().unwrap_or("unspecified requirements");
        let context = params.context.as_deref().unwrap_or("");

        let context_section = if context.is_empty() {
            String::new()
        } else {
            format!("\nContext: {context}")
        };

        let output = json!({
            "phase": "validate_requirements",
            "instruction": format!(
                "Analyze these requirements for completeness, consistency, and testability. Check for: ambiguous language, missing acceptance criteria, conflicting constraints, unmeasurable outcomes, and implicit assumptions.{context_section}"
            ),
            "requirements": requirements,
            "validation_checklist": [
                "Each requirement has clear acceptance criteria",
                "No conflicting or contradictory requirements",
                "All measurable outcomes have defined thresholds",
                "Edge cases and error conditions are addressed",
                "Dependencies on external systems are identified"
            ]
        });

        let text = format!(
            "## Phase: Validate Requirements\n\nRequirements: {requirements}{context_section}\n\n\
             Validate for:\n\
             1. **Completeness** — Are acceptance criteria defined?\n\
             2. **Consistency** — Any conflicting requirements?\n\
             3. **Testability** — Can each requirement be verified?\n\
             4. **Ambiguity** — Any vague or subjective language?\n\
             5. **Assumptions** — What implicit assumptions exist?\n\n\
             **Next step:** Address any issues found, then call `reasoning_integrate`."
        );

        Ok(ToolOutput::text(text).with_metadata(ctx, || output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use crate::ToolContext;
    use serde_json::json;
    use tempfile::tempdir;

    fn ctx() -> ToolContext {
        static DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        let dir = DIR.get_or_init(|| tempdir().expect("tempdir"));
        ToolContext::new(dir.path()).with_structured_output(true)
    }

    #[test]
    fn tool_metadata() {
        let tool = ValidateRequirementsTool;
        assert_eq!(tool.name(), "ReasoningValidate");
        assert_eq!(tool.permission(), ToolPermission::None);
        let schema = tool.parameters_schema();
        // requirements is an Option<String> so it's in properties but not required
        assert!(schema["properties"]["requirements"].is_object());
    }

    #[test]
    fn produces_structured_output() {
        let tool = ValidateRequirementsTool;
        let result = tool
            .execute(json!({"requirements": "User can log in"}), &ctx())
            .unwrap();
        assert!(result.text.contains("Validate Requirements"));
        assert!(result.text.contains("User can log in"));
        assert!(result.structured.is_some());
    }

    #[test]
    fn includes_context_when_provided() {
        let tool = ValidateRequirementsTool;
        let result = tool
            .execute(
                json!({"requirements": "Add auth", "context": "Rust backend"}),
                &ctx(),
            )
            .unwrap();
        assert!(result.text.contains("Rust backend"));
    }

    #[test]
    fn missing_requirements_uses_default() {
        let tool = ValidateRequirementsTool;
        let result = tool.execute(json!({}), &ctx()).unwrap();
        assert!(result.text.contains("unspecified requirements"));
    }
}
