use crate::{ToolOutput, ToolPermission};
use schemars::JsonSchema;
use serde_json::json;

#[derive(serde::Deserialize, JsonSchema)]
pub struct DecomposeParams {
    /// The task or goal to decompose
    #[serde(default)]
    goal: Option<String>,
    /// Problem domain, constraints, or known information
    #[serde(default)]
    context: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct DecomposeProblemTool;

    name: "reasoning_decompose",
    description: "Break a complex task into 3-5 critical submodules with confidence scores. Use BEFORE starting implementation to identify what you don't know. Returns a structured decomposition with open questions per module and recommended next steps.",
    permission: ToolPermission::None,

    execute(params: DecomposeParams, _ctx) {
        let goal = params.goal.as_deref().unwrap_or("unspecified task");
        let context = params.context.as_deref().unwrap_or("");

        let context_section = if context.is_empty() {
            String::new()
        } else {
            format!("\nContext: {context}")
        };

        let output = json!({
            "phase": "decompose",
            "instruction": format!(
                "Break this goal into 3-5 critical submodules. For each: name it, describe it, list open questions, identify dependencies on other modules, and rate your confidence (0.0-1.0).{context_section}"
            ),
            "goal": goal,
            "modules_template": [
                {"name": "", "description": "", "questions": [], "dependencies": [], "confidence": 0.0}
            ],
            "next_step": "Call reasoning_research for each module with confidence < 0.7"
        });

        let text = format!(
            "## Phase: Decompose\n\nGoal: {goal}{context_section}\n\n\
             Break this into 3-5 submodules. For each module:\n\
             1. Name and describe it\n\
             2. List open questions (what you need to find out)\n\
             3. Note dependencies on other modules\n\
             4. Rate your confidence (0.0-1.0)\n\n\
             **Next step:** Call `reasoning_research` for any module with confidence < 0.7."
        );

        Ok(ToolOutput::with_structured(text, output))
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
        let tool = DecomposeProblemTool;
        assert_eq!(tool.name(), "reasoning_decompose");
        assert_eq!(tool.permission(), ToolPermission::None);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["goal"].is_object());
    }

    #[test]
    fn produces_structured_output() {
        let tool = DecomposeProblemTool;
        let result = tool
            .execute(json!({"goal": "Build auth system"}), &ctx())
            .unwrap();
        assert!(result.text.contains("Decompose"));
        assert!(result.text.contains("Build auth system"));
        let structured = result.structured.unwrap();
        assert_eq!(structured["phase"], "decompose");
    }

    #[test]
    fn includes_context_when_provided() {
        let tool = DecomposeProblemTool;
        let result = tool
            .execute(
                json!({"goal": "Add caching", "context": "Redis backend"}),
                &ctx(),
            )
            .unwrap();
        assert!(result.text.contains("Redis backend"));
    }

    #[test]
    fn missing_goal_uses_default() {
        let tool = DecomposeProblemTool;
        let result = tool.execute(json!({}), &ctx()).unwrap();
        assert!(result.text.contains("unspecified task"));
    }
}
