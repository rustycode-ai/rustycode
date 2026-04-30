use rustycode_tools_api::{Tool, ToolContext, ToolOutput, ToolPermission};
use serde_json::{json, Value};

pub struct ValidateRequirementsTool;

impl Tool for ValidateRequirementsTool {
    fn name(&self) -> &str {
        "reasoning_validate"
    }

    fn description(&self) -> &str {
        "Validate that requirements are complete, consistent, and testable before implementation. Checks for ambiguity, missing acceptance criteria, and conflicts between requirements."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["requirements"],
            "properties": {
                "requirements": {
                    "type": "string",
                    "description": "The requirements text to validate"
                },
                "context": {
                    "type": "string",
                    "description": "Additional context: existing code, constraints, or related requirements"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        let requirements = params
            .get("requirements")
            .and_then(|v| v.as_str())
            .unwrap_or("unspecified requirements");
        let context = params.get("context").and_then(|v| v.as_str()).unwrap_or("");

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
        let tool = ValidateRequirementsTool;
        assert_eq!(tool.name(), "reasoning_validate");
        assert_eq!(tool.permission(), ToolPermission::None);
        let schema = tool.parameters_schema();
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("requirements")));
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
