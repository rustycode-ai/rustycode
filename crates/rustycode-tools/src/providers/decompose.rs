use rustycode_tools_api::{Tool, ToolContext, ToolOutput, ToolPermission};
use serde_json::{json, Value};

pub struct DecomposeProblemTool;

impl Tool for DecomposeProblemTool {
    fn name(&self) -> &str {
        "reasoning_decompose"
    }

    fn description(&self) -> &str {
        "Break a complex task into 3-5 critical submodules with confidence scores. Use BEFORE starting implementation to identify what you don't know. Returns a structured decomposition with open questions per module and recommended next steps."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["goal"],
            "properties": {
                "goal": {
                    "type": "string",
                    "description": "The task or goal to decompose"
                },
                "context": {
                    "type": "string",
                    "description": "Problem domain, constraints, or known information"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        let goal = params
            .get("goal")
            .and_then(|v| v.as_str())
            .unwrap_or("unspecified task");
        let context = params.get("context").and_then(|v| v.as_str()).unwrap_or("");

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
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("goal")));
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
