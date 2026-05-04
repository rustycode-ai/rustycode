//! Provider metadata for dynamic configuration and prompt optimization.
//!
//! This module provides metadata about each LLM provider that enables:
//! - Dynamic UI generation for configuration forms
//! - Provider-specific system prompt generation
//! - Tool calling format adaptation
//! - Prompt optimization based on model capabilities

use crate::provider::ProviderError;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata for a single provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    /// Provider identifier (e.g., "openai", "anthropic")
    pub provider_id: String,

    /// Human-readable display name
    pub display_name: String,

    /// Provider description for UI
    pub description: String,

    /// Configuration schema for dynamic form generation
    pub config_schema: ConfigSchema,

    /// System prompt template with provider-specific optimizations
    pub prompt_template: PromptTemplate,

    /// Tool calling format and capabilities
    pub tool_calling: ToolCallingMetadata,

    /// Recommended models for this provider
    pub recommended_models: Vec<ModelInfo>,
}

/// Schema for provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    /// Required configuration fields
    pub required_fields: Vec<ConfigField>,

    /// Optional configuration fields
    pub optional_fields: Vec<ConfigField>,

    /// Environment variable mappings
    pub env_mappings: HashMap<String, String>,
}

/// Definition of a single configuration field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigField {
    /// Field identifier
    pub name: String,

    /// Human-readable label
    pub label: String,

    /// Field description
    pub description: String,

    /// Field type
    pub field_type: ConfigFieldType,

    /// Placeholder text for input fields
    pub placeholder: Option<String>,

    /// Default value
    pub default: Option<String>,

    /// Validation pattern (regex)
    pub validation_pattern: Option<String>,

    /// Validation error message
    pub validation_error: Option<String>,

    /// Whether this field is sensitive (should be masked)
    pub sensitive: bool,
}

/// Configuration field types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigFieldType {
    /// Text string
    String,
    /// API key or secret
    APIKey,
    /// URL/endpoint
    URL,
    /// Numeric value
    Number,
    /// Dropdown selection
    Select(Vec<String>),
    /// Boolean toggle
    Boolean,
}

/// System prompt template with provider-specific optimizations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Base system prompt template (supports {variables})
    pub base_template: String,

    /// Provider-specific prompt optimizations
    pub optimizations: PromptOptimizations,

    /// Tool calling format template
    pub tool_format: ToolFormat,
}

/// Prompt optimization strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptOptimizations {
    /// Whether to use XML-style structure (Claude preference)
    pub prefer_xml_structure: bool,

    /// Whether to include examples in prompts
    pub include_examples: bool,

    /// Preferred prompt length guideline
    pub preferred_prompt_length: PromptLength,

    /// Special instructions for this provider
    pub special_instructions: Vec<String>,
}

/// Prompt length preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PromptLength {
    /// Keep prompts concise
    Concise,
    /// Medium length prompts (default)
    Medium,
    /// Detailed prompts with full context
    Detailed,
}

/// Tool calling format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolFormat {
    /// Claude-style XML tool definitions
    AnthropicXML,

    /// OpenAI-style function calling with JSON Schema
    OpenAIFunctionCalling,

    /// Gemini-style tool declarations
    GeminiTools,

    /// No tool calling support
    None,
}

/// Metadata about tool calling capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallingMetadata {
    /// Whether tool calling is supported
    pub supported: bool,

    /// Maximum number of tools in single call
    pub max_tools_per_call: Option<usize>,

    /// Whether parallel tool calling is supported
    pub parallel_calling: bool,

    /// Whether streaming tool calls are supported
    pub streaming_support: bool,
}

/// Information about a specific model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier
    pub model_id: String,

    /// Human-readable name
    pub display_name: String,

    /// Model description
    pub description: String,

    /// Context window size
    pub context_window: usize,

    /// Whether this model supports tool calling
    pub supports_tools: bool,

    /// Recommended use cases
    pub use_cases: Vec<String>,

    /// Cost tier (1=free, 5=expensive)
    pub cost_tier: u8,
}

/// Validate provider config using metadata schema
pub fn validate_config_from_schema(
    config: &crate::provider::ProviderConfig,
    schema: &ConfigSchema,
    provider_name: &str,
) -> Result<(), ProviderError> {
    // Check required fields
    for field in &schema.required_fields {
        match field.name.as_str() {
            "api_key" => {
                // Build env var hint from schema mappings
                let env_var = schema.env_mappings.get("api_key").map(|s| s.as_str());
                let env_hint = match env_var {
                    Some(v) => format!(" Set api_key in config or {} env var.", v),
                    None => " Set api_key in config.".to_string(),
                };

                let api_key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| {
                        ProviderError::Configuration(format!(
                            "{} API key is required.{}",
                            provider_name, env_hint
                        ))
                    })?
                    .expose_secret();

                if api_key.trim().is_empty() {
                    return Err(ProviderError::Configuration(format!(
                        "{} API key cannot be empty",
                        provider_name
                    )));
                }

                // Apply validation pattern if present
                if let Some(pattern) = &field.validation_pattern {
                    let regex = regex::Regex::new(pattern).map_err(|_| {
                        ProviderError::Configuration(format!(
                            "Invalid validation pattern for {}: {}",
                            provider_name, pattern
                        ))
                    })?;

                    if !regex.is_match(api_key) {
                        return Err(ProviderError::Configuration(
                            field.validation_error.clone().unwrap_or_else(|| {
                                format!("{} API key validation failed", provider_name)
                            }),
                        ));
                    }
                }
            }
            "base_url" => {
                // Optional field, skip validation
            }
            _ => {
                // Unknown field, skip
            }
        }
    }

    Ok(())
}

impl ProviderMetadata {
    /// Validate a ProviderConfig against this provider's schema
    pub fn validate_config(
        &self,
        config: &crate::provider::ProviderConfig,
    ) -> Result<(), ProviderError> {
        validate_config_from_schema(config, &self.config_schema, &self.display_name)
    }

    /// Generate system prompt for this provider (tools NOT included - they go in request JSON)
    pub fn generate_system_prompt(&self, context: &str) -> String {
        let mut prompt = self
            .prompt_template
            .base_template
            .replace("{context}", context);

        // Add provider-specific optimizations
        if self.prompt_template.optimizations.prefer_xml_structure {
            prompt.push_str(
                "\n\nFormat your responses using clear XML-style structure when appropriate.",
            );
        }

        if self.prompt_template.optimizations.include_examples {
            prompt.push_str("\n\nInclude concrete examples in your explanations when helpful.");
        }

        // Add provider-specific special instructions
        for instruction in &self.prompt_template.optimizations.special_instructions {
            prompt.push_str("\n\n");
            prompt.push_str(instruction);
        }

        prompt
    }

    /// Generate tool definitions for the request JSON (not system prompt!)
    pub fn generate_tool_definitions(&self, tools: &[ToolSchema]) -> serde_json::Value {
        if !self.tool_calling.supported || tools.is_empty() {
            return serde_json::json!([]);
        }

        match self.prompt_template.tool_format {
            ToolFormat::AnthropicXML => {
                // Claude tools go in request JSON as array of tool definitions
                let tool_definitions: Vec<serde_json::Value> = tools
                    .iter()
                    .map(|tool| {
                        serde_json::json!({
                            "name": tool.name,
                            "description": tool.description,
                            "input_schema": {
                                "type": "object",
                                "properties": tool.parameters,
                                "required": []
                            }
                        })
                    })
                    .collect();
                serde_json::json!(tool_definitions)
            }
            ToolFormat::OpenAIFunctionCalling => {
                // OpenAI tools go in request JSON as functions array
                let functions: Vec<serde_json::Value> = tools
                    .iter()
                    .map(|tool| {
                        serde_json::json!({
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": {
                                "type": "object",
                                "properties": tool.parameters,
                                "required": []
                            }
                        })
                    })
                    .collect();
                serde_json::json!(functions)
            }
            ToolFormat::GeminiTools => {
                // Gemini tools format
                let tool_declarations: Vec<serde_json::Value> = tools
                    .iter()
                    .map(|tool| {
                        serde_json::json!({
                            "functionDeclarations": [{
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.parameters
                            }]
                        })
                    })
                    .collect();
                serde_json::json!(tool_declarations)
            }
            ToolFormat::None => serde_json::json!([]),
            #[allow(unreachable_patterns)]
            _ => serde_json::json!([]),
        }
    }

    /// Generate tool use instructions for system prompt (when tools are available)
    pub fn generate_tool_instructions(&self) -> String {
        if !self.tool_calling.supported {
            return String::new();
        }

        match self.prompt_template.tool_format {
            ToolFormat::AnthropicXML => {
                "When you need to use a tool, respond with:\n<tool_use>{tool_name}</tool_use>\n<tool_input>{parameters_json}</tool_input>\n\nThen wait for the tool results before continuing.".to_string()
            }
            ToolFormat::OpenAIFunctionCalling => {
                "When you need to call a function, respond with a JSON object containing 'function_name' and 'parameters'.".to_string()
            }
            ToolFormat::GeminiTools => {
                "You have access to tools. Use them when needed to complete the user's request.".to_string()
            }
            ToolFormat::None => String::new(),
            #[allow(unreachable_patterns)]
            _ => String::new(),
        }
    }
}

/// Schema for a tool/function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: String,
}

/// Get metadata for a provider by ID
pub fn get_metadata(provider_id: &str) -> Option<ProviderMetadata> {
    match provider_id.to_lowercase().as_str() {
        "anthropic" => Some(crate::anthropic::AnthropicProvider::metadata()),
        "openai" => Some(crate::openai::OpenAiProvider::metadata()),
        "gemini" | "google" => Some(crate::gemini::GeminiProvider::metadata()),
        "litert-lm" | "litert_lm" | "litert" => {
            #[cfg(feature = "litert")]
            {
                Some(crate::litert_lm::LiteRtLmProvider::metadata())
            }
            #[cfg(not(feature = "litert"))]
            None
        }
        "together" | "together_ai" => Some(crate::together::TogetherProvider::metadata()),
        "cohere" => Some(crate::cohere::CohereProvider::metadata()),
        "copilot" | "github" => Some(crate::copilot::CopilotProvider::metadata()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_metadata() {
        let meta = crate::anthropic::AnthropicProvider::metadata();
        assert_eq!(meta.provider_id, "anthropic");
        assert_eq!(meta.config_schema.required_fields.len(), 1);
        assert_eq!(meta.config_schema.required_fields[0].name, "api_key");
        assert!(meta.tool_calling.supported);
    }

    #[test]
    fn test_system_prompt_generation() {
        let meta = crate::anthropic::AnthropicProvider::metadata();
        let prompt = meta.generate_system_prompt("Help the user.");
        assert!(prompt.contains("Help the user"));
        assert!(prompt.contains("RustyCode"));
        assert!(prompt.contains("XML"));
    }

    #[test]
    fn test_tool_definitions_request_json() {
        let meta = crate::anthropic::AnthropicProvider::metadata();
        let tools = vec![ToolSchema {
            name: "search".to_string(),
            description: "Search the web".to_string(),
            parameters: "{query: string}".to_string(),
        }];

        let tool_defs = meta.generate_tool_definitions(&tools);
        assert!(tool_defs.is_array());
        let tools_array = tool_defs.as_array().unwrap();
        assert_eq!(tools_array.len(), 1);
        assert_eq!(tools_array[0]["name"], "search");
        assert_eq!(tools_array[0]["description"], "Search the web");
    }

    #[test]
    fn test_tool_instructions() {
        let meta = crate::anthropic::AnthropicProvider::metadata();
        let instructions = meta.generate_tool_instructions();
        assert!(instructions.contains("<tool_use>"));
        assert!(instructions.contains("<tool_input>"));
    }

    #[test]
    fn test_get_metadata() {
        let anthropic = get_metadata("anthropic");
        assert!(anthropic.is_some());
        assert_eq!(anthropic.unwrap().provider_id, "anthropic");

        let openai = get_metadata("openai");
        assert!(openai.is_some());

        let litert = get_metadata("litert-lm");
        assert!(litert.is_some());
        assert_eq!(litert.unwrap().provider_id, "litert-lm");

        let unknown = get_metadata("unknown_provider");
        assert!(unknown.is_none());
    }
}
