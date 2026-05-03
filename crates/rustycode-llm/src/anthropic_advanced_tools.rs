//! Integration between `ExecutableUnit` and LLM `ToolDefinition`
//!
//! Converts `ExecutableUnit`s from the unified callable abstraction into
//! `ToolDefinition`s suitable for sending to LLM providers (Anthropic, OpenAI, etc.).

use crate::tools::ToolDefinition;
use rustycode_executable::ExecutableUnit;

/// Convert an `ExecutableUnit` into a `ToolDefinition` for LLM providers.
///
/// Maps the unit's schema, examples, and metadata into the format expected
/// by the LLM tool-calling API.
pub fn executable_to_tool_definition(unit: &ExecutableUnit) -> ToolDefinition {
    let examples: Vec<serde_json::Value> = unit
        .advanced_metadata
        .examples
        .iter()
        .map(|ex| {
            serde_json::json!({
                "scenario": ex.scenario,
                "input": ex.input,
                "output": ex.output,
            })
        })
        .collect();

    let input_schema = unit
        .schema
        .as_ref()
        .map(|s| s.parameters.clone())
        .unwrap_or(serde_json::json!({"type": "object", "properties": {}}));

    let mut def = ToolDefinition::new(&unit.id, &unit.description, input_schema);

    if !examples.is_empty() {
        def = def.with_examples(examples);
    }

    def
}

/// Convert a batch of `ExecutableUnit`s into `ToolDefinition`s.
pub fn executables_to_tool_definitions(units: &[ExecutableUnit]) -> Vec<ToolDefinition> {
    units.iter().map(executable_to_tool_definition).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_executable::{
        AdvancedToolMetadata, ExecutionContext, ExecutionExample, ExecutionMode, UnitCapabilities,
        UnitSource,
    };
    use std::sync::Arc;

    fn make_test_unit(
        id: &str,
        description: &str,
        examples: Vec<ExecutionExample>,
        schema: Option<rustycode_executable::ToolSchema>,
    ) -> ExecutableUnit {
        ExecutableUnit {
            id: id.to_string(),
            name: id.to_string(),
            description: description.to_string(),
            capabilities: UnitCapabilities {
                can_execute_directly: true,
                can_bundle_knowledge: false,
                can_reason_autonomously: false,
            },
            advanced_metadata: AdvancedToolMetadata {
                examples,
                defer_loading: false,
                search_hints: vec![],
                execution_strategy: ExecutionMode::Direct,
                result_processor: None,
            },
            handler: Arc::new(rustycode_executable::types::callable::NoOpCallable),
            source: UnitSource::NativeTool {
                path: "test".to_string(),
            },
            schema,
            tags: vec![],
            version: None,
        }
    }

    fn make_example(scenario: &str) -> ExecutionExample {
        ExecutionExample {
            scenario: scenario.to_string(),
            input: serde_json::json!({"path": "/tmp/test.txt"}),
            output: serde_json::json!("file contents"),
            context: ExecutionContext::DirectTool {
                immediate_result: true,
                timeout_ms: None,
            },
            explanation: None,
        }
    }

    #[test]
    fn converts_basic_unit_without_schema() {
        let unit = make_test_unit("ping", "Health check", vec![], None);
        let def = executable_to_tool_definition(&unit);

        assert_eq!(def.name, "ping");
        assert_eq!(def.description, "Health check");
        assert_eq!(def.input_schema["type"], "object");
        assert!(def.examples.is_none());
    }

    #[test]
    fn converts_unit_with_schema() {
        let schema = rustycode_executable::ToolSchema {
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
            returns: None,
        };
        let unit = make_test_unit("read", "Read a file", vec![], Some(schema));
        let def = executable_to_tool_definition(&unit);

        assert_eq!(def.input_schema["properties"]["path"]["type"], "string");
        assert!(def.input_schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("path")));
    }

    #[test]
    fn converts_unit_with_examples() {
        let examples = vec![make_example("basic read")];
        let unit = make_test_unit("read", "Read a file", examples, None);
        let def = executable_to_tool_definition(&unit);

        let ex = def.examples.unwrap();
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0]["scenario"], "basic read");
    }

    #[test]
    fn batch_conversion_preserves_order() {
        let units = vec![
            make_test_unit("a", "Tool A", vec![], None),
            make_test_unit("b", "Tool B", vec![], None),
            make_test_unit("c", "Tool C", vec![], None),
        ];
        let defs = executables_to_tool_definitions(&units);

        assert_eq!(defs.len(), 3);
        assert_eq!(defs[0].name, "a");
        assert_eq!(defs[1].name, "b");
        assert_eq!(defs[2].name, "c");
    }

    #[test]
    fn no_examples_field_when_empty() {
        let unit = make_test_unit("x", "X", vec![], None);
        let def = executable_to_tool_definition(&unit);
        assert!(
            def.examples.is_none(),
            "examples should be None (not Some([])) when no examples are provided"
        );
    }
}
