//! JSON schema enforcement for structured outputs.

use crate::error::OrchestrationError;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct OutputSchema {
    schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaValidationResult {
    pub errors: Vec<String>,
}

impl SchemaValidationResult {
    pub const fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn error_message(&self) -> String {
        if self.errors.is_empty() {
            String::new()
        } else {
            self.errors.join("; ")
        }
    }
}

impl OutputSchema {
    #[allow(clippy::missing_const_for_fn)]
    pub fn from_json(schema: Value) -> Self {
        Self { schema }
    }

    pub fn parse(schema_json: &str) -> Result<Self, OrchestrationError> {
        let schema: Value = serde_json::from_str(schema_json)
            .map_err(|err| OrchestrationError::schema(format!("invalid schema JSON: {err}")))?;
        Ok(Self::from_json(schema))
    }

    pub fn validate(&self, value: &Value) -> SchemaValidationResult {
        let validator = match jsonschema::validator_for(&self.schema) {
            Ok(validator) => validator,
            Err(err) => {
                return SchemaValidationResult {
                    errors: vec![format!("invalid schema: {err}")],
                };
            }
        };

        let errors = validator
            .iter_errors(value)
            .map(|err| err.to_string())
            .collect();
        SchemaValidationResult { errors }
    }
}

#[derive(Debug, Clone)]
pub struct TierSchema {
    schema: OutputSchema,
}

impl TierSchema {
    pub fn plan() -> Self {
        Self {
            schema: OutputSchema::from_json(serde_json::json!({
                "type": "object",
                "properties": {
                    "steps": { "type": "array" },
                    "estimated_complexity": { "type": "string" },
                    "risks": { "type": "array" }
                },
                "required": ["steps", "estimated_complexity", "risks"]
            })),
        }
    }

    pub fn code_change() -> Self {
        Self {
            schema: OutputSchema::from_json(serde_json::json!({
                "type": "object",
                "properties": {
                    "files_modified": { "type": "array" },
                    "diff": { "type": "string" },
                    "tests_passed": { "type": "boolean" }
                },
                "required": ["files_modified", "diff", "tests_passed"]
            })),
        }
    }

    pub fn verification() -> Self {
        Self {
            schema: OutputSchema::from_json(serde_json::json!({
                "type": "object",
                "properties": {
                    "passed": { "type": "boolean" },
                    "checks": { "type": "array" }
                },
                "required": ["passed", "checks"]
            })),
        }
    }

    pub fn validate(&self, value: &Value) -> SchemaValidationResult {
        self.schema.validate(value)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_json_against_simple_schema() {
        let schema = OutputSchema::from_json(json!({
            "type": "object",
            "properties": {
                "output": { "type": "string" },
                "success": { "type": "boolean" }
            },
            "required": ["output", "success"]
        }));
        let result = schema.validate(&json!({
            "output": "hello",
            "success": true
        }));
        assert!(result.is_valid());
    }

    #[test]
    fn invalid_json_against_simple_schema() {
        let schema = OutputSchema::from_json(json!({
            "type": "object",
            "properties": {
                "output": { "type": "string" }
            },
            "required": ["output"]
        }));
        let result = schema.validate(&json!({
            "missing_output": true
        }));
        assert!(!result.is_valid());
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn schema_from_raw_json_string() {
        let schema_json = r#"{"type": "object", "required": ["status"]}"#;
        let schema = OutputSchema::parse(schema_json).unwrap();
        let result = schema.validate(&json!({"status": "ok"}));
        assert!(result.is_valid());
    }

    #[test]
    fn schema_parse_invalid_json() {
        let result = OutputSchema::parse("not valid json {{{");
        assert!(result.is_err());
    }

    #[test]
    fn validation_result_error_messages() {
        let schema = OutputSchema::from_json(json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer" }
            },
            "required": ["count"]
        }));
        let result = schema.validate(&json!({"count": "not a number"}));
        assert!(!result.is_valid());
        let error_msg = result.error_message();
        assert!(!error_msg.is_empty());
    }

    #[test]
    fn tier_output_schema_plan() {
        let schema = TierSchema::plan();
        let result = schema.validate(&json!({
            "steps": [
                {"description": "Implement feature X", "files": ["src/main.rs"]}
            ],
            "estimated_complexity": "medium",
            "risks": []
        }));
        assert!(result.is_valid());
    }

    #[test]
    fn tier_output_schema_plan_missing_steps() {
        let schema = TierSchema::plan();
        let result = schema.validate(&json!({
            "estimated_complexity": "medium",
            "risks": []
        }));
        assert!(!result.is_valid());
    }

    #[test]
    fn tier_output_schema_code_change() {
        let schema = TierSchema::code_change();
        let result = schema.validate(&json!({
            "files_modified": ["src/lib.rs"],
            "diff": "+added line\n-removed line",
            "tests_passed": true
        }));
        assert!(result.is_valid());
    }

    #[test]
    fn tier_output_schema_verification() {
        let schema = TierSchema::verification();
        let result = schema.validate(&json!({
            "passed": true,
            "checks": [
                {"name": "compilation", "passed": true},
                {"name": "tests", "passed": true}
            ]
        }));
        assert!(result.is_valid());
    }

    #[test]
    fn tier_output_schema_verification_failed() {
        let schema = TierSchema::verification();
        let result = schema.validate(&json!({
            "passed": false,
            "checks": [
                {"name": "compilation", "passed": false}
            ]
        }));
        assert!(result.is_valid());
    }
}
