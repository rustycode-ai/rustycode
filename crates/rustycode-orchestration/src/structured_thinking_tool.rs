//! Structured thinking tool schema and AST dispatch.
//!
//! Provides the schema for the LLM to record its reasoning steps, and
//! dispatches to the AST (Adaptive Structured Thinking) pipeline for
//! complex task execution.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::json;

use crate::ast::{AstConfig, AstExecutionResult, AstPipeline, ToolHarness};
use rustycode_prompt::PromptResolver;

pub struct StructuredThinkingToolSchema;

impl StructuredThinkingToolSchema {
    /// Generates the `OpenAI` tool schema for structured thinking.
    #[allow(clippy::missing_const_for_fn)]
    pub fn schema() -> serde_json::Value {
        json!({
            "type": "function",
            "function": {
                "name": "structured_thinking",
                "description": "Use this tool to record each step of your structured reasoning. Call it multiple times as your reasoning evolves.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "thought": {
                            "type": "string",
                            "description": "The current thought or analysis"
                        },
                        "phase": {
                            "type": "integer",
                            "description": "Phase number (1, 2, 3, etc.)"
                        },
                        "type": {
                            "type": "string",
                            "enum": ["decision", "constraint", "validation", "learning", "hypothesis"],
                            "description": "Type of thought"
                        },
                        "confidence": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 100,
                            "description": "Your confidence in this thought (0-100)"
                        },
                        "next_thought_needed": {
                            "type": "boolean",
                            "description": "Whether you want to think further or are ready to conclude"
                        },
                        "references": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "IDs of previous thoughts this references"
                        },
                        "branch_id": {
                            "type": "string",
                            "description": "Optional ID if exploring alternative branches"
                        },
                        "metadata": {
                            "type": "object",
                            "properties": {
                                "algorithm_choice": { "type": "string" },
                                "rationale": { "type": "string" },
                                "alternatives_rejected": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "validation_points": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "approach": {
                                    "type": "string",
                                    "enum": [
                                        "brute_force",
                                        "divide_and_conquer",
                                        "greedy",
                                        "dynamic_programming",
                                        "backtracking",
                                        "bfs_dfs",
                                        "sliding_window",
                                        "two_pointers",
                                        "binary_search",
                                        "topological_sort",
                                        "mathematical",
                                        "simulation",
                                        "research_needed"
                                    ],
                                    "description": "The algorithmic approach or strategy being applied. Use 'research_needed' when confidence is low and more information is required."
                                }
                            }
                        }
                    },
                    "required": ["thought", "phase", "type", "confidence", "next_thought_needed"]
                }
            }
        })
    }

    /// System prompt guidance for using structured thinking tool.
    #[allow(clippy::missing_const_for_fn)]
    pub fn system_prompt_guidance() -> &'static str {
        r"When asked to solve complex problems, use the structured_thinking tool to break down your reasoning. For each thought:
1. Be specific — name the algorithm, approach, or pattern (not 'an algorithm' but 'BFS with early termination')
2. Use the 'approach' field in metadata to classify your strategy from: brute_force, divide_and_conquer, greedy, dynamic_programming, backtracking, bfs_dfs, sliding_window, two_pointers, binary_search, topological_sort, mathematical, simulation
3. Explain rationale and trade-offs
4. Rate your confidence (0-100) in this thought
5. Reference previous thoughts if building on them
6. Call the tool until you reach confidence >= 85 on a resolution, up to 8 calls max

Do not repeat analysis from previous thoughts. Each call must advance the reasoning.

If confidence drops below 60 at any phase:
- Use 'research_needed' as the approach
- Use Grep/Read tools to investigate the codebase before continuing
- Use WebFetch if the problem requires external knowledge
- Resume structured thinking after gathering evidence

If the tool response contains a `loop_warning`, you may be going in circles. Use the `ask_user` tool to request clarification — this is better than repeating the same analysis.

To ask for help, call the `ask_user` tool with your specific question, what you've considered, and how urgent it is (low/medium/high).

Example: For maze exploration, phase 1: analyze algorithm choice with approach=bfs_dfs (confidence 85), phase 2: plan validation (confidence 75), phase 3: implement details (confidence 90)."
    }

    /// Resolve guidance through the prompt layering chain.
    pub fn system_prompt_guidance_resolved(resolver: &PromptResolver) -> String {
        resolver.resolve(
            "tools",
            "structured_thinking",
            Self::system_prompt_guidance(),
        )
    }
}

/// Dispatches a task to the AST pipeline.
///
/// This is the production entry point for AST execution. It creates an
/// `AstPipeline` with a real shell runner, runs it to completion, and
/// returns the rich result.
pub fn execute_with_ast(
    task: &str,
    workspace: PathBuf,
    harness: ToolHarness,
) -> Result<AstExecutionResult> {
    let config = AstConfig {
        ledger_dir: workspace.join(".ast"),
        harness,
        ..AstConfig::default()
    };
    let runner = crate::ast::ShellStepRunner::new(workspace.clone());
    let mut pipeline = AstPipeline::with_runner(config, workspace, runner);
    Ok(pipeline.run_to_completion(task)?)
}

/// Dispatches a task to the AST pipeline in dry-run (simulated) mode.
///
/// Uses `SimulatedRunner` -- no real commands are executed. Useful for
/// planning and testing without side effects.
pub fn execute_with_ast_dry_run(
    task: &str,
    workspace: PathBuf,
    harness: ToolHarness,
) -> Result<AstExecutionResult> {
    let config = AstConfig {
        ledger_dir: workspace.join(".ast"),
        harness,
        ..AstConfig::default()
    };
    let mut pipeline = AstPipeline::with_config(config, workspace);
    Ok(pipeline.run_to_completion(task)?)
}

/// Quick check: does the task look complex enough to benefit from AST?
///
/// Returns `true` for tasks that are likely MODERATE or COMPLEX.
/// Useful for deciding whether to route through AST or use direct execution.
pub fn should_use_ast(task: &str) -> bool {
    // Heuristic: tasks longer than ~80 chars or containing certain keywords
    // are likely complex enough to benefit from structured thinking.
    if task.len() > 80 {
        return true;
    }
    let complex_keywords = [
        "implement",
        "refactor",
        "architecture",
        "redesign",
        "integrate",
        "migrate",
        "port",
        "rewrite",
    ];
    let lower = task.to_lowercase();
    complex_keywords.iter().any(|kw| lower.contains(kw))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::OutputSchema;
    use serde_json::json;

    #[test]
    fn test_schema_is_valid_json() {
        let schema = StructuredThinkingToolSchema::schema();
        assert_eq!(schema["type"], "function");
        assert_eq!(schema["function"]["name"], "structured_thinking");
    }

    #[test]
    fn test_schema_has_required_fields() {
        let schema = StructuredThinkingToolSchema::schema();
        let required = &schema["function"]["parameters"]["required"];
        assert!(required.as_array().unwrap().len() >= 5);
    }

    #[test]
    fn execute_with_ast_dry_run_runs_trivial_task() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_path_buf();
        let result =
            execute_with_ast_dry_run("Fix typo in README.md", workspace, ToolHarness::ClaudeCode);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.status, crate::ast::VerificationStatus::Pass);
        assert!(result.ledger_path.exists());
    }

    #[test]
    fn execute_with_ast_dry_run_returns_assessment() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().to_path_buf();
        let result =
            execute_with_ast_dry_run("Fix typo", workspace, ToolHarness::ClaudeCode).unwrap();
        assert!(result.assessment.is_some());
        assert_eq!(
            result.assessment.unwrap().complexity,
            crate::ast::ComplexityLevel::Trivial
        );
    }

    #[test]
    fn should_use_ast_detects_complex_keywords() {
        assert!(should_use_ast("Implement a new auth system"));
        assert!(should_use_ast("Refactor the database layer"));
        assert!(should_use_ast("a".repeat(90).as_str()));
    }

    #[test]
    fn should_use_ast_rejects_simple_tasks() {
        assert!(!should_use_ast("Fix typo"));
        assert!(!should_use_ast("Add comment"));
    }

    #[test]
    fn schema_produces_valid_openai_function_format() {
        let schema = StructuredThinkingToolSchema::schema();

        assert_eq!(schema["type"], "function");
        assert!(
            schema.get("function").is_some(),
            "must have top-level 'function' key"
        );

        let func = &schema["function"];
        assert_eq!(func["name"], "structured_thinking");
        assert!(func["description"].is_string());
        assert!(
            func.get("parameters").is_some(),
            "must have 'parameters' key"
        );
    }

    #[test]
    fn schema_parameters_are_json_schema_not_nested_function() {
        let schema = StructuredThinkingToolSchema::schema();
        let params = &schema["function"]["parameters"];

        assert_eq!(params["type"], "object");

        assert!(
            params.get("function").is_none(),
            "parameters must NOT contain a nested 'function' — that causes API 400 errors"
        );
        assert!(
            params.get("name").is_none(),
            "parameters must NOT contain 'name' — that's double-wrapping"
        );

        assert!(
            params["properties"]["thought"].is_object(),
            "must have 'thought' property"
        );
        assert!(
            params["properties"]["phase"].is_object(),
            "must have 'phase' property"
        );
        assert!(
            params["properties"]["type"].is_object(),
            "must have 'type' property"
        );
    }

    #[test]
    fn schema_required_fields_match_properties() {
        let schema = StructuredThinkingToolSchema::schema();
        let params = &schema["function"]["parameters"];
        let required = params["required"].as_array().unwrap();
        let properties = params["properties"].as_object().unwrap();

        for field in required {
            let name = field.as_str().unwrap();
            assert!(
                properties.contains_key(name),
                "required field '{}' must exist in properties",
                name
            );
        }
    }

    #[test]
    fn schema_enum_values_are_valid() {
        let schema = StructuredThinkingToolSchema::schema();
        let type_enum = &schema["function"]["parameters"]["properties"]["type"]["enum"];
        let values = type_enum.as_array().unwrap();

        assert_eq!(values.len(), 5);
        assert!(values.contains(&serde_json::json!("decision")));
        assert!(values.contains(&serde_json::json!("constraint")));
        assert!(values.contains(&serde_json::json!("validation")));
        assert!(values.contains(&serde_json::json!("learning")));
        assert!(values.contains(&serde_json::json!("hypothesis")));
    }

    #[test]
    fn schema_confidence_has_min_max_constraints() {
        let schema = StructuredThinkingToolSchema::schema();
        let confidence = &schema["function"]["parameters"]["properties"]["confidence"];

        assert_eq!(confidence["type"], "integer");
        assert_eq!(confidence["minimum"], 0);
        assert_eq!(confidence["maximum"], 100);
    }

    #[test]
    fn schema_metadata_has_nested_properties() {
        let schema = StructuredThinkingToolSchema::schema();
        let metadata = &schema["function"]["parameters"]["properties"]["metadata"];

        assert_eq!(metadata["type"], "object");
        assert!(metadata["properties"]["rationale"].is_object());
        assert!(metadata["properties"]["algorithm_choice"].is_object());
        assert!(metadata["properties"]["alternatives_rejected"]["items"]["type"].is_string());
        assert!(metadata["properties"]["validation_points"]["items"]["type"].is_string());
    }

    #[test]
    fn schema_serializes_to_valid_json_roundtrip() {
        let schema = StructuredThinkingToolSchema::schema();
        let json_str = serde_json::to_string(&schema).unwrap();
        let reparsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(schema, reparsed);
    }

    #[test]
    fn schema_accepts_structured_thinking_payload_with_basic_json_types() {
        let schema = StructuredThinkingToolSchema::schema();
        let params = schema["function"]["parameters"].clone();
        let validator = OutputSchema::from_json(params);

        let valid = json!({
            "thought": "Break the problem into three steps",
            "phase": 1,
            "type": "decision",
            "confidence": 88,
            "next_thought_needed": true,
            "references": ["thought-1", "thought-2"],
            "branch_id": "branch-a",
            "metadata": {
                "algorithm_choice": "tree search",
                "rationale": "We need to explore alternatives before committing",
                "alternatives_rejected": ["greedy", "random walk"],
                "validation_points": ["check invariants", "compare outputs"]
            }
        });

        assert!(validator.validate(&valid).is_valid());

        let invalid = json!({
            "thought": "Break the problem into three steps",
            "phase": "1",
            "type": "decision",
            "confidence": 88,
            "next_thought_needed": true
        });

        assert!(!validator.validate(&invalid).is_valid());
    }
}
