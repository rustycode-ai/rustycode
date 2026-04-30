//! Tool Argument Coercion
//!
//! Sanitizes and coerces LLM tool call arguments to match expected JSON Schema
//! types. LLMs frequently return incorrect types (e.g., string "true" instead
//! of boolean true, string "42" instead of number 42, or objects as JSON strings).
//!
//! Inspired by goose's `coerce_value` in `reply_parts.rs`.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::tool_arg_coercion::coerce_arguments;
//! use serde_json::{json, Value};
//!
//! let args = json!({"count": "42", "verbose": "true", "path": "/tmp"});
//! let schema = json!({
//!     "type": "object",
//!     "properties": {
//!         "count": {"type": "integer"},
//!         "verbose": {"type": "boolean"},
//!         "path": {"type": "string"}
//!     }
//! });
//!
//! let coerced = coerce_arguments(&args, &schema);
//! assert_eq!(coerced["count"], json!(42));
//! assert_eq!(coerced["verbose"], json!(true));
//! assert_eq!(coerced["path"], json!("/tmp"));
//! ```

use serde_json::Value;
use std::collections::HashMap;

/// Coerce tool call arguments to match the expected JSON Schema types.
///
/// Walks the schema's properties and attempts to convert each argument
/// to the expected type. Unknown properties are left unchanged.
///
/// Returns a new `Value::Object` with coerced values.
pub fn coerce_arguments(args: &Value, schema: &Value) -> Value {
    let mut result = match args {
        Value::Object(map) => map.clone(),
        _ => return args.clone(),
    };

    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (key, prop_schema) in properties {
            if let Some(value) = result.get(key) {
                let coerced = coerce_value(value, prop_schema);
                result.insert(key.clone(), coerced);
            }
        }
    }

    Value::Object(result)
}

/// Coerce a single value to match the expected schema type.
///
/// Handles:
/// - String → number/integer/boolean conversion
/// - JSON string unwrapping (e.g., `"{\"a\":1}"` → `{"a":1}`)
/// - Nested object/array coercion
pub fn coerce_value(value: &Value, schema: &Value) -> Value {
    let type_hint = schema.get("type");

    match type_hint {
        Some(Value::String(t)) => coerce_by_type_name(value, t.as_str(), schema),
        Some(Value::Array(types)) => {
            // Try each type in order
            for t in types {
                if let Value::String(type_name) = t {
                    let coerced = coerce_by_type_name(value, type_name.as_str(), schema);
                    if coerced != *value {
                        return coerced;
                    }
                }
            }
            value.clone()
        }
        _ => value.clone(),
    }
}

fn coerce_by_type_name(value: &Value, type_name: &str, schema: &Value) -> Value {
    match type_name {
        "integer" | "number" => coerce_number(value),
        "boolean" => coerce_boolean(value),
        "string" => coerce_string(value),
        "array" => coerce_array(value, schema),
        "object" => coerce_object(value, schema),
        _ => value.clone(),
    }
}

/// Try to coerce a value to a number.
fn coerce_number(value: &Value) -> Value {
    match value {
        Value::Number(_) => value.clone(),
        Value::String(s) => {
            if let Ok(n) = s.parse::<i64>() {
                Value::Number(n.into())
            } else if let Ok(f) = s.parse::<f64>() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or_else(|| value.clone())
            } else {
                value.clone()
            }
        }
        Value::Bool(b) => {
            if *b {
                Value::Number(1.into())
            } else {
                Value::Number(0.into())
            }
        }
        _ => value.clone(),
    }
}

/// Try to coerce a value to a boolean.
fn coerce_boolean(value: &Value) -> Value {
    match value {
        Value::Bool(_) => value.clone(),
        Value::String(s) => match s.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Value::Bool(true),
            "false" | "0" | "no" | "off" | "" => Value::Bool(false),
            _ => value.clone(),
        },
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Bool(i != 0)
            } else {
                value.clone()
            }
        }
        _ => value.clone(),
    }
}

/// Try to coerce a value to a string.
fn coerce_string(value: &Value) -> Value {
    match value {
        Value::String(_) => value.clone(),
        Value::Number(_) | Value::Bool(_) => Value::String(value.to_string()),
        Value::Null => Value::String(String::new()),
        _ => value.to_string().into(),
    }
}

/// Try to coerce a value to an array.
fn coerce_array(value: &Value, schema: &Value) -> Value {
    match value {
        Value::Array(arr) => {
            let items_schema = schema.get("items").cloned().unwrap_or(Value::Null);
            let coerced: Vec<Value> = arr.iter().map(|v| coerce_value(v, &items_schema)).collect();
            Value::Array(coerced)
        }
        Value::String(s) => {
            // Try parsing as JSON array
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                if parsed.is_array() {
                    return coerce_array(&parsed, schema);
                }
            }
            // Try comma-separated
            if s.contains(',') {
                let items: Vec<Value> = s
                    .split(',')
                    .map(|item| Value::String(item.trim().to_string()))
                    .collect();
                return Value::Array(items);
            }
            // Single item array
            Value::Array(vec![Value::String(s.clone())])
        }
        _ => value.clone(),
    }
}

/// Try to coerce a value to an object.
fn coerce_object(value: &Value, schema: &Value) -> Value {
    match value {
        Value::Object(_) => {
            // Recursively coerce nested properties
            coerce_arguments(value, schema)
        }
        Value::String(s) => {
            // Try parsing as JSON object
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                if parsed.is_object() {
                    return coerce_arguments(&parsed, schema);
                }
            }
            value.clone()
        }
        _ => value.clone(),
    }
}

/// Validate that tool call arguments satisfy the required fields in a schema.
///
/// Returns a list of missing required parameter names.
pub fn validate_required(args: &Value, schema: &Value) -> Vec<String> {
    let mut missing = Vec::new();

    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        if let Some(args_obj) = args.as_object() {
            for req in required {
                if let Some(name) = req.as_str() {
                    if !args_obj.contains_key(name) {
                        missing.push(name.to_string());
                    }
                }
            }
        } else {
            // args is not an object, all required are missing
            for req in required {
                if let Some(name) = req.as_str() {
                    missing.push(name.to_string());
                }
            }
        }
    }

    missing
}

/// Extract default values from a schema and fill in missing arguments.
///
/// Returns a new Value with defaults applied for any missing properties
/// that have a `default` field in the schema.
pub fn apply_defaults(args: &Value, schema: &Value) -> Value {
    let mut result = match args {
        Value::Object(map) => map.clone(),
        _ => return args.clone(),
    };

    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (key, prop_schema) in properties {
            if !result.contains_key(key) {
                if let Some(default) = prop_schema.get("default") {
                    result.insert(key.clone(), default.clone());
                }
            }
        }
    }

    Value::Object(result)
}

/// Coerce arguments, apply defaults, and validate required fields.
///
/// This is the main entry point for argument sanitization.
///
/// Returns the coerced arguments and a list of validation errors.
pub fn sanitize_arguments(args: &Value, schema: &Value) -> (Value, Vec<String>) {
    // Step 1: Apply defaults for missing fields
    let with_defaults = apply_defaults(args, schema);

    // Step 2: Coerce types
    let coerced = coerce_arguments(&with_defaults, schema);

    // Step 3: Validate required fields
    let errors = validate_required(&coerced, schema);

    (coerced, errors)
}

/// Sanitize a tool call's arguments string (JSON) against a schema.
///
/// Returns the sanitized JSON string. If the input is not valid JSON,
/// attempts to repair it first.
pub fn sanitize_tool_call_args(args_json: &str, schema: &Value) -> String {
    let args: Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(_) => {
            // Try to repair common JSON issues
            let repaired = crate::json_repair::repair_json(args_json);
            match serde_json::from_str(&repaired) {
                Ok(v) => v,
                Err(_) => return args_json.to_string(),
            }
        }
    };

    let (sanitized, _) = sanitize_arguments(&args, schema);
    sanitized.to_string()
}

/// Statistics about argument coercion
#[derive(Debug, Clone, Default)]
pub struct CoercionStats {
    /// Number of values that were changed
    pub coerced_count: usize,
    /// Map of field name → `from_type` → `to_type`
    pub coercions: HashMap<String, (String, String)>,
}

/// Coerce arguments and track what changed.
///
/// Useful for logging/debugging to understand what the LLM got wrong.
pub fn coerce_arguments_with_stats(args: &Value, schema: &Value) -> (Value, CoercionStats) {
    let mut stats = CoercionStats::default();

    if !args.is_object() {
        return (args.clone(), stats);
    }

    let mut result = match args {
        Value::Object(map) => map.clone(),
        _ => return (args.clone(), stats),
    };

    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (key, prop_schema) in properties {
            if let Some(original) = result.get(key) {
                let coerced = coerce_value(original, prop_schema);
                if coerced != *original {
                    stats.coerced_count += 1;
                    stats
                        .coercions
                        .insert(key.clone(), (type_name(original), type_name(&coerced)));
                    result.insert(key.clone(), coerced);
                }
            }
        }
    }

    (Value::Object(result), stats)
}

fn type_name(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_coerce_string_to_number() {
        let schema = json!({"type": "integer"});
        assert_eq!(coerce_value(&json!("42"), &schema), json!(42));
        assert_eq!(coerce_value(&json!("2.719"), &schema), json!(2.719));
        assert_eq!(coerce_value(&json!(42), &schema), json!(42));
    }

    #[test]
    fn test_coerce_string_to_boolean() {
        let schema = json!({"type": "boolean"});
        assert_eq!(coerce_value(&json!("true"), &schema), json!(true));
        assert_eq!(coerce_value(&json!("false"), &schema), json!(false));
        assert_eq!(coerce_value(&json!("1"), &schema), json!(true));
        assert_eq!(coerce_value(&json!("0"), &schema), json!(false));
        assert_eq!(coerce_value(&json!("yes"), &schema), json!(true));
        assert_eq!(coerce_value(&json!("no"), &schema), json!(false));
    }

    #[test]
    fn test_coerce_boolean_to_number() {
        let schema = json!({"type": "number"});
        assert_eq!(coerce_value(&json!(true), &schema), json!(1));
        assert_eq!(coerce_value(&json!(false), &schema), json!(0));
    }

    #[test]
    fn test_coerce_number_to_boolean() {
        let schema = json!({"type": "boolean"});
        assert_eq!(coerce_value(&json!(1), &schema), json!(true));
        assert_eq!(coerce_value(&json!(0), &schema), json!(false));
    }

    #[test]
    fn test_coerce_string_to_array() {
        let schema = json!({"type": "array"});
        // Comma-separated
        let result = coerce_value(&json!("a, b, c"), &schema);
        assert_eq!(result, json!(["a", "b", "c"]));
        // Single item
        let result = coerce_value(&json!("hello"), &schema);
        assert_eq!(result, json!(["hello"]));
    }

    #[test]
    fn test_coerce_string_to_object() {
        let schema = json!({"type": "object", "properties": {"key": {"type": "string"}}});
        let result = coerce_value(&json!({"key": "value"}), &schema);
        assert_eq!(result, json!({"key": "value"}));
    }

    #[test]
    fn test_coerce_arguments_full() {
        let args = json!({
            "count": "42",
            "verbose": "true",
            "path": "/tmp/test",
            "name": "hello"
        });
        let schema = json!({
            "type": "object",
            "properties": {
                "count": {"type": "integer"},
                "verbose": {"type": "boolean"},
                "path": {"type": "string"},
                "name": {"type": "string"}
            }
        });

        let result = coerce_arguments(&args, &schema);
        assert_eq!(result["count"], json!(42));
        assert_eq!(result["verbose"], json!(true));
        assert_eq!(result["path"], json!("/tmp/test"));
        assert_eq!(result["name"], json!("hello"));
    }

    #[test]
    fn test_validate_required() {
        let schema = json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"},
                "encoding": {"type": "string"}
            }
        });

        let args = json!({"path": "/tmp/test"});
        let missing = validate_required(&args, &schema);
        assert_eq!(missing, vec!["content"]);

        let complete = json!({"path": "/tmp/test", "content": "hello"});
        let missing = validate_required(&complete, &schema);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_apply_defaults() {
        let schema = json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "encoding": {"type": "string", "default": "utf-8"},
                "mode": {"type": "string", "default": "read"}
            }
        });

        let args = json!({"path": "/tmp/test"});
        let result = apply_defaults(&args, &schema);
        assert_eq!(result["path"], json!("/tmp/test"));
        assert_eq!(result["encoding"], json!("utf-8"));
        assert_eq!(result["mode"], json!("read"));
    }

    #[test]
    fn test_sanitize_arguments() {
        let schema = json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {"type": "string"},
                "timeout": {"type": "number", "default": 30}
            }
        });

        let args = json!({"timeout": "60"});
        let (coerced, errors) = sanitize_arguments(&args, &schema);
        assert_eq!(coerced["timeout"], json!(60));
        assert_eq!(errors, vec!["command"]);
    }

    #[test]
    fn test_coerce_preserves_unknown() {
        let args = json!({"unknown_field": "value"});
        let schema = json!({
            "type": "object",
            "properties": {
                "known_field": {"type": "string"}
            }
        });

        let result = coerce_arguments(&args, &schema);
        assert_eq!(result["unknown_field"], json!("value"));
    }

    #[test]
    fn test_coerce_number_to_string() {
        let schema = json!({"type": "string"});
        assert_eq!(coerce_value(&json!(42), &schema), json!("42"));
        assert_eq!(coerce_value(&json!(true), &schema), json!("true"));
    }

    #[test]
    fn test_coerce_stats() {
        let args = json!({"count": "42", "name": "test"});
        let schema = json!({
            "type": "object",
            "properties": {
                "count": {"type": "integer"},
                "name": {"type": "string"}
            }
        });

        let (result, stats) = coerce_arguments_with_stats(&args, &schema);
        assert_eq!(result["count"], json!(42));
        assert_eq!(stats.coerced_count, 1);
        assert!(stats.coercions.contains_key("count"));
        assert_eq!(
            stats.coercions["count"],
            ("string".to_string(), "number".to_string())
        );
    }

    #[test]
    fn test_coerce_array_items() {
        let schema = json!({
            "type": "array",
            "items": {"type": "integer"}
        });
        let result = coerce_value(&json!(["1", "2", "3"]), &schema);
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn test_non_coercible_unchanged() {
        let schema = json!({"type": "integer"});
        assert_eq!(
            coerce_value(&json!("not a number"), &schema),
            json!("not a number")
        );
        assert_eq!(coerce_value(&json!("maybe"), &schema), json!("maybe")); // invalid boolean
    }

    #[test]
    fn test_empty_string_to_false() {
        let schema = json!({"type": "boolean"});
        assert_eq!(coerce_value(&json!(""), &schema), json!(false));
    }
}
