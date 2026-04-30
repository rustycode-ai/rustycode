use rustycode_tools_api::{Tool, ToolContext, ToolOutput, ToolPermission};
use serde_json::{json, Value};

pub struct GuideResearchTool;

impl Tool for GuideResearchTool {
    fn name(&self) -> &str {
        "reasoning_research"
    }

    fn description(&self) -> &str {
        "Get prioritized research targets for a specific module's open questions. Returns structured research guidance with what to investigate, why it matters, and what you should find. Use AFTER reasoning_decompose to plan your research efficiently."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["module_name", "open_question"],
            "properties": {
                "module_name": {
                    "type": "string",
                    "description": "The module to research"
                },
                "open_question": {
                    "type": "string",
                    "description": "The specific question to answer"
                },
                "known_constraints": {
                    "type": "string",
                    "description": "Any constraints or requirements to consider"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        let module_name = params
            .get("module_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown module");
        let open_question = params
            .get("open_question")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown question");
        let known_constraints = params
            .get("known_constraints")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let constraints_section = if known_constraints.is_empty() {
            String::new()
        } else {
            format!("\nConstraints: {known_constraints}")
        };

        let output = json!({
            "phase": "research",
            "module": module_name,
            "question": open_question,
            "research_targets": [
                {"target": "", "why": "", "expected_findings": "", "priority": 1}
            ],
            "instruction": format!(
                "For module '{module_name}', research the question: {open_question}.{constraints_section}\n\
                 Prioritize: (1) existing implementations/libraries, (2) documentation/RFCs, (3) codebase patterns.\n\
                 Report findings using reasoning_validate."
            ),
            "warning": "Track your exploration calls. After 10 without producing code, you must STOP and implement."
        });

        let text = format!(
            "## Phase: Research\n\n\
             Module: **{module_name}**\n\
             Question: {open_question}{constraints_section}\n\n\
             Research priorities:\n\
             1. Existing implementations or libraries — what's available?\n\
             2. Documentation, RFCs, standards — what are the specs?\n\
             3. Codebase patterns — how does this project handle similar cases?\n\n\
             **Next step:** Research these targets, then call `reasoning_validate` with your findings."
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
        let tool = GuideResearchTool;
        assert_eq!(tool.name(), "reasoning_research");
        assert_eq!(tool.permission(), ToolPermission::None);
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("module_name")));
        assert!(required.contains(&json!("open_question")));
    }

    #[test]
    fn produces_structured_output() {
        let tool = GuideResearchTool;
        let result = tool
            .execute(
                json!({"module_name": "auth", "open_question": "How to hash passwords?"}),
                &ctx(),
            )
            .unwrap();
        assert!(result.text.contains("Research"));
        assert!(result.text.contains("auth"));
        assert!(result.text.contains("How to hash passwords?"));
        let structured = result.structured.unwrap();
        assert_eq!(structured["phase"], "research");
    }

    #[test]
    fn includes_constraints_when_provided() {
        let tool = GuideResearchTool;
        let result = tool.execute(
            json!({"module_name": "db", "open_question": "Schema design", "known_constraints": "PostgreSQL only"}),
            &ctx(),
        ).unwrap();
        assert!(result.text.contains("PostgreSQL only"));
    }

    #[test]
    fn missing_params_use_defaults() {
        let tool = GuideResearchTool;
        let result = tool.execute(json!({}), &ctx()).unwrap();
        assert!(result.text.contains("unknown module"));
        assert!(result.text.contains("unknown question"));
    }
}
