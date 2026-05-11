//! Tool schema utilities for LLM consumption.
//!
//! Cleans schemars-generated JSON schemas by removing bloat (`$schema`, `title`)
//! and simplifying `Option<T>` union types (`["string","null"]` → `"string"`).

use crate::ToolInfo;
use serde_json::Value;

/// Build tool schemas from a list of tool info in canonical Anthropic format.
///
/// The LLM provider layer normalizes this to the correct wire format for each
/// provider. Strips `$schema`, `title`, and simplifies nullable union types.
pub fn build_tool_schemas(tools: &[ToolInfo]) -> Vec<Value> {
    crate::build_canonical_tool_schemas(tools)
}

/// Build tool schemas with optional examples for ambiguous tools.
///
/// `examples_fn` receives a tool name and returns `Some(Vec<Value>)` of
/// `{"type": "input_example", "input": {...}}` objects if the tool needs
/// examples, or `None` if the schema is self-explanatory.
pub fn build_tool_schemas_with_examples(
    tools: &[ToolInfo],
    examples_fn: impl Fn(&str) -> Option<Vec<Value>>,
) -> Vec<Value> {
    tools
        .iter()
        .map(|info| {
            let mut schema = info.to_canonical_schema();
            if let Some(examples) = examples_fn(&info.name) {
                schema["examples"] = Value::Array(examples);
            }
            schema
        })
        .collect()
}

/// Strip unnecessary metadata from schemars-generated JSON schemas.
///
/// Removes:
/// - `$schema` URL (wastes tokens, no value for LLMs)
/// - `title` (internal struct name, not useful)
/// - `["T", "null"]` union types → simplified to `"T"` (optional fields
///   are already expressed by being absent from `required`)
pub fn strip_schema_metadata(schema: Value) -> Value {
    let mut v = schema;
    strip_recursive(&mut v);
    v
}

fn strip_recursive(schema: &mut Value) {
    match schema {
        Value::Object(map) => {
            map.remove("$schema");
            map.remove("title");
            // Simplify ["string", "null"] → "string"
            if let Some(ty) = map.get_mut("type").and_then(|t| t.as_array_mut()) {
                if ty.len() == 2 && ty.iter().any(|v| v.as_str() == Some("null")) {
                    let non_null = ty.iter().find(|v| v.as_str() != Some("null")).cloned();
                    if let Some(simplified) = non_null {
                        map.insert("type".to_string(), simplified);
                    }
                }
            }
            for (_, val) in map.iter_mut() {
                strip_recursive(val);
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                strip_recursive(val);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_removes_dollar_schema_and_title() {
        let input = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "MyParams",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "count": {"type": ["integer", "null"]}
            }
        });
        let output = strip_schema_metadata(input);
        assert!(output.get("$schema").is_none());
        assert!(output.get("title").is_none());
        assert_eq!(output["properties"]["count"]["type"], "integer");
    }

    #[test]
    fn strip_simplifies_nullable_string() {
        let input = serde_json::json!({
            "type": "object",
            "properties": {
                "content": {"type": ["string", "null"], "description": "file content"},
                "path": {"type": "string"}
            }
        });
        let output = strip_schema_metadata(input);
        assert_eq!(output["properties"]["content"]["type"], "string");
        assert_eq!(output["properties"]["path"]["type"], "string");
    }

    #[test]
    fn strip_handles_nested_schemas() {
        let input = serde_json::json!({
            "type": "object",
            "title": "Nested",
            "properties": {
                "items": {
                    "type": "array",
                    "title": "Items",
                    "items": {
                        "type": "object",
                        "title": "Item",
                        "properties": {
                            "value": {"type": ["number", "null"]}
                        }
                    }
                }
            }
        });
        let output = strip_schema_metadata(input);
        assert!(output.get("title").is_none());
        assert!(output["properties"]["items"].get("title").is_none());
        assert!(output["properties"]["items"]["items"]
            .get("title")
            .is_none());
        assert_eq!(
            output["properties"]["items"]["items"]["properties"]["value"]["type"],
            "number"
        );
    }
}
