use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DecomposeParams {
    /// The complex task goal
    pub goal: String,
    /// Context or domain info
    pub context: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Module {
    pub name: String,
    pub description: String,
    pub questions: Vec<String>,
    pub dependencies: Vec<String>,
    pub confidence: f32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DecompositionResult {
    pub modules: Vec<Module>,
}

rustycode_tools_api::define_tool! {
    pub struct DecomposeProblemTool;

    name: "decompose_problem",
    description: "Decomposes a complex task into smaller, manageable sub-modules.",
    permission: rustycode_tools_api::ToolPermission::None,

    execute(params: DecomposeParams, _ctx) {
        // In a real implementation, we would call an LLM here with the structured prompt.
        // For this MVP, we return a structured skeleton.
        let result = DecompositionResult {
            modules: vec![Module {
                name: "Initial Analysis".to_string(),
                description: format!("Decompose task: {}", params.goal),
                questions: vec!["What are the core requirements?".to_string()],
                dependencies: vec![],
                confidence: 0.8,
            }],
        };

        Ok(rustycode_tools_api::ToolOutput::text(serde_json::to_string_pretty(&result)?))
    }
}
