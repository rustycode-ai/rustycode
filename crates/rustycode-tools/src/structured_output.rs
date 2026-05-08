use crate::{ToolOutput, ToolPermission};
use anyhow::anyhow;
use serde_json::Value;

#[cfg(test)]
use serde_json::json;

// A synthetic tool for enforcing structured output.
// When a JSON schema is provided via `ctx.structured_output_schema`, this tool validates
// and accepts structured JSON matching the schema. This follows the same
// pattern as Claude Code's `SyntheticOutputTool`.
rustycode_tools_api::define_tool! {
    pub struct StructuredOutputTool;

    name: "StructuredOutput",
    description: "Return your final response as structured JSON. You MUST call this tool exactly once at the end of your response to provide the structured output.",
    permission: ToolPermission::None,

    execute(_params: serde_json::Value, ctx) {
        let schema = ctx.structured_output_schema
            .as_ref()
            .ok_or_else(|| anyhow!("No structured output schema configured"))?;

        let params = _params;

        // Basic structural validation: check that all required properties exist
        if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
            for req in required {
                if let Some(key) = req.as_str() {
                    if params.get(key).is_none() {
                        return Err(anyhow!("Missing required field: {key}"));
                    }
                }
            }
        }

        // Validate property types if schema declares them
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (key, prop_schema) in properties {
                if let Some(value) = params.get(key) {
                    if let Some(expected_type) = prop_schema.get("type").and_then(|t| t.as_str()) {
                        let actual_matches = match expected_type {
                            "string" => value.is_string(),
                            "number" | "integer" => value.is_number(),
                            "boolean" => value.is_boolean(),
                            "array" => value.is_array(),
                            "object" => value.is_object(),
                            _ => true,
                        };
                        if !actual_matches {
                            return Err(anyhow!(
                                "Field '{key}' expected type '{expected_type}' but got '{}'",
                                json_type_of(value)
                            ));
                        }
                    }
                }
            }
        }

        Ok(ToolOutput::text("Structured output provided successfully"))
    }
}

fn json_type_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Check if a tool call result came from the `StructuredOutput` tool.
pub fn is_structured_output_tool(name: &str) -> bool {
    name == "StructuredOutput"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};

    fn test_ctx_with_schema(schema: Value) -> ToolContext {
        ToolContext::new(".").with_structured_output_schema(schema)
    }

    #[test]
    fn test_structured_output_valid() {
        let schema = json!({
            "type": "object",
            "properties": {
                "answer": {"type": "string"}
            },
            "required": ["answer"]
        });
        let tool = StructuredOutputTool;
        assert_eq!(tool.name(), "StructuredOutput");

        let input = json!({"answer": "42"});
        let ctx = test_ctx_with_schema(schema);
        let result = tool.execute(input, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_structured_output_missing_required() {
        let schema = json!({
            "type": "object",
            "properties": {
                "answer": {"type": "string"},
                "confidence": {"type": "number"}
            },
            "required": ["answer", "confidence"]
        });

        let input = json!({"answer": "42"});
        let ctx = test_ctx_with_schema(schema);
        let tool = StructuredOutputTool;
        let result = tool.execute(input, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing required field: confidence"));
    }

    #[test]
    fn test_structured_output_type_mismatch() {
        let schema = json!({
            "type": "object",
            "properties": {
                "count": {"type": "integer"}
            },
            "required": ["count"]
        });

        let input = json!({"count": "not a number"});
        let ctx = test_ctx_with_schema(schema);
        let tool = StructuredOutputTool;
        let result = tool.execute(input, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("expected type 'integer'"));
    }

    #[test]
    fn test_structured_output_no_permission() {
        let tool = StructuredOutputTool;
        assert!(matches!(tool.permission(), ToolPermission::None));
    }

    #[test]
    fn test_is_structured_output_tool() {
        assert!(is_structured_output_tool("StructuredOutput"));
        assert!(!is_structured_output_tool("bash"));
    }

    #[test]
    fn test_structured_output_no_schema() {
        let ctx = ToolContext::new(".");
        let tool = StructuredOutputTool;
        let input = json!({"answer": "42"});
        let result = tool.execute(input, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No structured output schema configured"));
    }
}
