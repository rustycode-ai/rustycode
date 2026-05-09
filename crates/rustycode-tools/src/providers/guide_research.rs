use crate::{ToolOutput, ToolPermission};
use schemars::JsonSchema;
use serde_json::json;

#[derive(serde::Deserialize, JsonSchema)]
pub struct GuideResearchParams {
    /// The module to research
    #[serde(default)]
    module_name: Option<String>,
    /// The specific question to answer
    #[serde(default)]
    open_question: Option<String>,
    /// Any constraints or requirements to consider
    #[serde(default)]
    known_constraints: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct GuideResearchTool;

    name: "reasoning_research",
    description: "Get prioritized research targets for a specific module's open questions. Returns structured research guidance with what to investigate, why it matters, and what you should find. Use AFTER reasoning_decompose to plan your research efficiently.",
    permission: ToolPermission::None,

    execute(params: GuideResearchParams, ctx) {
        let module_name = params.module_name.as_deref().unwrap_or("unknown module");
        let open_question = params.open_question.as_deref().unwrap_or("unknown question");
        let known_constraints = params.known_constraints.as_deref().unwrap_or("");

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
        ToolContext::new(dir.path())
    }

    #[test]
    fn tool_metadata() {
        let tool = GuideResearchTool;
        assert_eq!(tool.name(), "reasoning_research");
        assert_eq!(tool.permission(), ToolPermission::None);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["module_name"].is_object());
        assert!(schema["properties"]["open_question"].is_object());
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
