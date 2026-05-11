//! Typed tool schema — replaces raw json!() macros for tool definitions.

use rustycode_tools_api::ToolInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Typed JSON Schema builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchema {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, JsonSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<JsonSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

impl JsonSchema {
    pub fn string(description: impl Into<String>) -> Self {
        Self {
            schema_type: Some("string".into()),
            description: Some(description.into()),
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn integer(description: impl Into<String>) -> Self {
        Self {
            schema_type: Some("integer".into()),
            description: Some(description.into()),
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn boolean(description: impl Into<String>) -> Self {
        Self {
            schema_type: Some("boolean".into()),
            description: Some(description.into()),
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn number(description: impl Into<String>) -> Self {
        Self {
            schema_type: Some("number".into()),
            description: Some(description.into()),
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn object(properties: BTreeMap<String, Self>, required: Vec<String>) -> Self {
        Self {
            schema_type: Some("object".into()),
            description: None,
            properties: Some(properties),
            required: if required.is_empty() {
                None
            } else {
                Some(required)
            },
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn array(items: Self) -> Self {
        Self {
            schema_type: Some("array".into()),
            description: None,
            properties: None,
            required: None,
            items: Some(Box::new(items)),
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn enum_of(variants: Vec<&str>) -> Self {
        Self {
            schema_type: Some("string".into()),
            description: None,
            properties: None,
            required: None,
            items: None,
            enum_values: Some(variants.into_iter().map(String::from).collect()),
            any_of: None,
            additional_properties: None,
            default: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_default(mut self, value: serde_json::Value) -> Self {
        self.default = Some(value);
        self
    }

    /// Convert to serde_json::Value for wire serialization.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::Value::Object(Default::default()))
    }
}

impl From<serde_json::Value> for JsonSchema {
    fn from(v: serde_json::Value) -> Self {
        serde_json::from_value(v).unwrap_or_else(|_| Self {
            schema_type: Some("object".into()),
            description: None,
            properties: None,
            required: None,
            items: None,
            enum_values: None,
            any_of: None,
            additional_properties: None,
            default: None,
        })
    }
}

/// Typed tool definition — replaces raw `serde_json::Value` tool definitions.
#[derive(Debug, Clone)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: JsonSchema,
}

impl ToolSchema {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: JsonSchema,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }

    /// Convert to a serde_json::Value in Anthropic wire format.
    pub fn to_anthropic_value(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema.to_value(),
        })
    }

    /// Convert to a serde_json::Value in OpenAI Chat wire format.
    pub fn to_openai_chat_value(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.input_schema.to_value(),
            }
        })
    }

    /// Convert to a serde_json::Value in OpenAI Responses wire format.
    pub fn to_openai_responses_value(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "name": self.name,
            "description": self.description,
            "parameters": self.input_schema.to_value(),
        })
    }

    /// Convert to a serde_json::Value in Gemini wire format.
    pub fn to_gemini_value(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "parameters": self.input_schema.to_value(),
        })
    }

    /// Convert to a serde_json::Value in Bedrock wire format.
    pub fn to_bedrock_value(&self) -> serde_json::Value {
        serde_json::json!({
            "toolSpec": {
                "name": self.name,
                "description": self.description,
                "inputSchema": {
                    "json": self.input_schema.to_value(),
                }
            }
        })
    }
}

impl From<&ToolInfo> for ToolSchema {
    fn from(info: &ToolInfo) -> Self {
        Self {
            name: info.name.clone(),
            description: info.description.clone(),
            input_schema: JsonSchema::from(info.parameters_schema.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_schema_string_serializes() {
        let schema = JsonSchema::string("File path");
        let val = schema.to_value();
        assert_eq!(val["type"], "string");
        assert_eq!(val["description"], "File path");
    }

    #[test]
    fn json_schema_object_with_required() {
        let schema = JsonSchema::object(
            BTreeMap::from([
                ("path".into(), JsonSchema::string("File path")),
                ("content".into(), JsonSchema::string("File content")),
            ]),
            vec!["path".into(), "content".into()],
        );
        let val = schema.to_value();
        assert_eq!(val["type"], "object");
        assert!(val["properties"]["path"].is_object());
        assert_eq!(val["required"][0], "path");
    }

    #[test]
    fn tool_schema_anthropic_format() {
        let tool = ToolSchema::new(
            "Edit",
            "Replace text in a file",
            JsonSchema::object(
                BTreeMap::from([
                    ("file_path".into(), JsonSchema::string("Absolute path")),
                    ("old_string".into(), JsonSchema::string("Text to find")),
                    ("new_string".into(), JsonSchema::string("Replacement text")),
                ]),
                vec!["file_path".into(), "old_string".into(), "new_string".into()],
            ),
        );
        let val = tool.to_anthropic_value();
        assert_eq!(val["name"], "Edit");
        assert_eq!(val["input_schema"]["type"], "object");
        assert!(val.get("function").is_none()); // Anthropic doesn't use function wrapper
    }

    #[test]
    fn tool_schema_openai_chat_format() {
        let tool = ToolSchema::new(
            "Edit",
            "Replace text in a file",
            JsonSchema::object(
                BTreeMap::from([("file_path".into(), JsonSchema::string("Absolute path"))]),
                vec!["file_path".into()],
            ),
        );
        let val = tool.to_openai_chat_value();
        assert_eq!(val["type"], "function");
        assert_eq!(val["function"]["name"], "Edit");
        assert_eq!(val["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn tool_schema_bedrock_format() {
        let tool = ToolSchema::new(
            "Edit",
            "Replace text",
            JsonSchema::object(
                BTreeMap::from([("path".into(), JsonSchema::string("Path"))]),
                vec!["path".into()],
            ),
        );
        let val = tool.to_bedrock_value();
        assert_eq!(val["toolSpec"]["name"], "Edit");
        assert_eq!(val["toolSpec"]["inputSchema"]["json"]["type"], "object");
    }
}
