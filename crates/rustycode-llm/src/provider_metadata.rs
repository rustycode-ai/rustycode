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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolUsagePosture {
    Aggressive,
    Conservative,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum OutputStructure {
    StructuredXml,
    ConciseBullet,
    Freeform,
    CodeFocused,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReasoningGuidance {
    ChainOfThought,
    Direct,
    StepByStep,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelBehaviorProfile {
    pub tool_usage_posture: ToolUsagePosture,
    pub output_structure_preference: OutputStructure,
    pub reasoning_guidance_style: ReasoningGuidance,
    pub parallel_tool_calls: bool,
    pub special_instructions: Vec<String>,
}

impl Default for ModelBehaviorProfile {
    fn default() -> Self {
        Self {
            tool_usage_posture: ToolUsagePosture::Conservative,
            output_structure_preference: OutputStructure::Freeform,
            reasoning_guidance_style: ReasoningGuidance::Direct,
            parallel_tool_calls: true,
            special_instructions: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("model behavior overlay error: {message}")]
pub struct ModelBehaviorOverlayError {
    pub message: String,
}

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

    /// Model-specific behavior profiles keyed by model ID or prefix.
    #[serde(default)]
    pub model_behavior_profiles: HashMap<String, ModelBehaviorProfile>,
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
                let declarations: Vec<serde_json::Value> = tools
                    .iter()
                    .map(|tool| {
                        serde_json::json!({
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters
                        })
                    })
                    .collect();
                serde_json::json!([{ "functionDeclarations": declarations }])
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

    /// Resolve the effective `ModelBehaviorProfile` for a given model ID.
    ///
    /// Resolution order:
    /// 1. Exact match on `model_id` in `model_behavior_profiles`
    /// 2. Longest prefix match (e.g., `"claude-opus"` matches `"claude-opus-4-7-20250501"`)
    /// 3. Fallback to provider-level defaults derived from `PromptOptimizations`
    pub fn resolve_model_profile(
        &self,
        model_id: &str,
    ) -> Result<ModelBehaviorProfile, ModelBehaviorOverlayError> {
        if let Some(profile) = self.model_behavior_profiles.get(model_id) {
            return Ok(self.compose_with_provider_defaults(profile));
        }

        let best_match = self
            .model_behavior_profiles
            .keys()
            .filter(|prefix| model_id.starts_with(prefix.as_str()))
            .max_by_key(|prefix| prefix.len());

        if let Some(prefix_key) = best_match {
            let profile = &self.model_behavior_profiles[prefix_key];
            return Ok(self.compose_with_provider_defaults(profile));
        }

        Ok(self.provider_default_profile())
    }

    fn compose_with_provider_defaults(
        &self,
        model_profile: &ModelBehaviorProfile,
    ) -> ModelBehaviorProfile {
        let provider_default = self.provider_default_profile();

        let mut instructions = provider_default.special_instructions;
        for instr in &model_profile.special_instructions {
            if !instructions.contains(instr) {
                instructions.push(instr.clone());
            }
        }

        ModelBehaviorProfile {
            tool_usage_posture: model_profile.tool_usage_posture.clone(),
            output_structure_preference: model_profile.output_structure_preference.clone(),
            reasoning_guidance_style: model_profile.reasoning_guidance_style.clone(),
            parallel_tool_calls: model_profile.parallel_tool_calls,
            special_instructions: instructions,
        }
    }

    fn provider_default_profile(&self) -> ModelBehaviorProfile {
        let opts = &self.prompt_template.optimizations;
        ModelBehaviorProfile {
            output_structure_preference: if opts.prefer_xml_structure {
                OutputStructure::StructuredXml
            } else {
                OutputStructure::Freeform
            },
            special_instructions: opts.special_instructions.clone(),
            ..ModelBehaviorProfile::default()
        }
    }

    pub fn validate_model_profiles(&self) -> Result<(), ModelBehaviorOverlayError> {
        if !self.tool_calling.supported {
            for (key, profile) in &self.model_behavior_profiles {
                if profile.tool_usage_posture != ToolUsagePosture::Minimal {
                    return Err(ModelBehaviorOverlayError {
                        message: format!(
                            "model profile '{}' has tool_usage_posture {:?} but provider does not support tool calling",
                            key, profile.tool_usage_posture
                        ),
                    });
                }
            }
        }

        if !self.tool_calling.parallel_calling {
            for (key, profile) in &self.model_behavior_profiles {
                if profile.parallel_tool_calls {
                    return Err(ModelBehaviorOverlayError {
                        message: format!(
                            "model profile '{}' requests parallel_tool_calls but provider does not support parallel calling",
                            key
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    pub fn validate_profile(
        &self,
        profile: &ModelBehaviorProfile,
        label: &str,
    ) -> Result<(), ModelBehaviorOverlayError> {
        if !self.tool_calling.supported && profile.tool_usage_posture != ToolUsagePosture::Minimal {
            return Err(ModelBehaviorOverlayError {
                message: format!(
                    "{} has tool_usage_posture {:?} but provider does not support tool calling",
                    label, profile.tool_usage_posture
                ),
            });
        }

        if !self.tool_calling.parallel_calling && profile.parallel_tool_calls {
            return Err(ModelBehaviorOverlayError {
                message: format!(
                    "{} requests parallel_tool_calls but provider does not support parallel calling",
                    label
                ),
            });
        }

        Ok(())
    }

    pub fn generate_system_prompt_for_model(
        &self,
        context: &str,
        model_id: &str,
    ) -> Result<String, ModelBehaviorOverlayError> {
        let profile = self.resolve_model_profile(model_id)?;
        Ok(self.generate_system_prompt_with_profile(context, &profile))
    }

    /// Generate a system prompt augmented with model-specific behavior profile.
    pub fn generate_system_prompt_with_profile(
        &self,
        context: &str,
        profile: &ModelBehaviorProfile,
    ) -> String {
        let mut prompt = self.generate_system_prompt(context);

        match profile.output_structure_preference {
            OutputStructure::StructuredXml => {
                prompt.push_str(
                    "\n\nUse clear XML tags for sections in your responses when appropriate.",
                );
            }
            OutputStructure::ConciseBullet => {
                prompt.push_str("\n\nPrefer concise bullet points for your responses.");
            }
            OutputStructure::CodeFocused => {
                prompt.push_str("\n\nFocus on code blocks and technical output. Minimize prose.");
            }
            OutputStructure::Freeform => {}
            #[allow(unreachable_patterns)]
            _ => {}
        }

        match profile.reasoning_guidance_style {
            ReasoningGuidance::ChainOfThought => {
                prompt.push_str(
                    "\n\nThink through problems step by step, showing your reasoning before giving the final answer.",
                );
            }
            ReasoningGuidance::StepByStep => {
                prompt.push_str("\n\nBreak down complex problems into numbered steps.");
            }
            ReasoningGuidance::Direct => {}
            #[allow(unreachable_patterns)]
            _ => {}
        }

        for instruction in &profile.special_instructions {
            prompt.push_str("\n\n");
            prompt.push_str(instruction);
        }

        prompt
    }
}

/// Registry holding metadata for all providers with cross-provider model lookup.
#[derive(Debug, Clone, Default)]
pub struct ModelBehaviorRegistry {
    providers: HashMap<String, ProviderMetadata>,
}

impl ModelBehaviorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, metadata: ProviderMetadata) {
        self.providers
            .insert(metadata.provider_id.clone(), metadata);
    }

    pub fn get(&self, provider_id: &str) -> Option<&ProviderMetadata> {
        self.providers.get(provider_id)
    }

    pub fn resolve_profile_for_model(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Option<ModelBehaviorProfile> {
        self.providers
            .get(provider_id)
            .and_then(|meta| meta.resolve_model_profile(model_id).ok())
    }
}

/// Schema for a tool/function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Get metadata for a provider by ID
pub fn metadata(provider_id: &str) -> Option<ProviderMetadata> {
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
            name: "Search".to_string(),
            description: "Search the web".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            }),
        }];

        let tool_defs = meta.generate_tool_definitions(&tools);
        assert!(tool_defs.is_array());
        let tools_array = tool_defs.as_array().unwrap();
        assert_eq!(tools_array.len(), 1);
        assert_eq!(tools_array[0]["name"], "Search");
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
    fn test_metadata() {
        let anthropic = metadata("anthropic");
        assert!(anthropic.is_some());
        assert_eq!(anthropic.unwrap().provider_id, "anthropic");

        let openai = metadata("openai");
        assert!(openai.is_some());

        #[cfg(feature = "litert")]
        {
            let litert = metadata("litert-lm");
            assert!(litert.is_some());
            assert_eq!(litert.unwrap().provider_id, "litert-lm");
        }
        #[cfg(not(feature = "litert"))]
        {
            assert!(metadata("litert-lm").is_none());
        }

        let unknown = metadata("unknown_provider");
        assert!(unknown.is_none());
    }

    #[test]
    fn model_behavior_profile_default_values() {
        let profile = ModelBehaviorProfile::default();
        assert_eq!(profile.tool_usage_posture, ToolUsagePosture::Conservative);
        assert_eq!(
            profile.output_structure_preference,
            OutputStructure::Freeform
        );
        assert_eq!(profile.reasoning_guidance_style, ReasoningGuidance::Direct);
        assert!(profile.parallel_tool_calls);
        assert!(profile.special_instructions.is_empty());
    }

    #[test]
    fn tool_usage_posture_serde_roundtrip() {
        let variants = [
            ToolUsagePosture::Aggressive,
            ToolUsagePosture::Conservative,
            ToolUsagePosture::Minimal,
        ];
        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let deserialized: ToolUsagePosture = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, deserialized);
        }
    }

    #[test]
    fn output_structure_serde_roundtrip() {
        let variants = [
            OutputStructure::StructuredXml,
            OutputStructure::ConciseBullet,
            OutputStructure::Freeform,
            OutputStructure::CodeFocused,
        ];
        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let deserialized: OutputStructure = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, deserialized);
        }
    }

    #[test]
    fn reasoning_guidance_serde_roundtrip() {
        let variants = [
            ReasoningGuidance::ChainOfThought,
            ReasoningGuidance::Direct,
            ReasoningGuidance::StepByStep,
        ];
        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let deserialized: ReasoningGuidance = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, deserialized);
        }
    }

    #[test]
    fn model_behavior_profile_serde_roundtrip() {
        let profile = ModelBehaviorProfile {
            tool_usage_posture: ToolUsagePosture::Aggressive,
            output_structure_preference: OutputStructure::CodeFocused,
            reasoning_guidance_style: ReasoningGuidance::StepByStep,
            parallel_tool_calls: false,
            special_instructions: vec!["Use Rust idioms".to_string()],
        };
        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: ModelBehaviorProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile, deserialized);
    }

    #[test]
    fn overlay_error_display() {
        let err = ModelBehaviorOverlayError {
            message: "conflict detected".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "model behavior overlay error: conflict detected"
        );
    }

    fn test_metadata_empty_profiles() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "test".to_string(),
            display_name: "Test".to_string(),
            description: "Test provider".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![],
                optional_fields: vec![],
                env_mappings: HashMap::new(),
            },
            prompt_template: PromptTemplate {
                base_template: "You are an assistant.\n\n{context}".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: true,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec!["Provider instruction".to_string()],
                },
                tool_format: ToolFormat::AnthropicXML,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: None,
                parallel_calling: false,
                streaming_support: true,
            },
            recommended_models: vec![],
            model_behavior_profiles: HashMap::new(),
        }
    }

    #[test]
    fn resolve_model_profile_empty_profiles_returns_provider_default() {
        let meta = test_metadata_empty_profiles();
        let profile = meta.resolve_model_profile("any-model").unwrap();
        assert_eq!(
            profile.output_structure_preference,
            OutputStructure::StructuredXml
        );
        assert_eq!(
            profile.special_instructions,
            vec!["Provider instruction".to_string()]
        );
    }

    #[test]
    fn resolve_model_profile_exact_match() {
        let mut meta = test_metadata_empty_profiles();
        let opus_profile = ModelBehaviorProfile {
            tool_usage_posture: ToolUsagePosture::Aggressive,
            output_structure_preference: OutputStructure::CodeFocused,
            reasoning_guidance_style: ReasoningGuidance::ChainOfThought,
            parallel_tool_calls: false,
            special_instructions: vec!["Opus-specific".to_string()],
        };
        meta.model_behavior_profiles
            .insert("claude-opus-4-7".to_string(), opus_profile.clone());

        let resolved = meta.resolve_model_profile("claude-opus-4-7").unwrap();
        assert_eq!(resolved.tool_usage_posture, ToolUsagePosture::Aggressive);
        assert_eq!(
            resolved.output_structure_preference,
            OutputStructure::CodeFocused
        );
        assert_eq!(
            resolved.reasoning_guidance_style,
            ReasoningGuidance::ChainOfThought
        );
        assert!(!resolved.parallel_tool_calls);
    }

    #[test]
    fn resolve_model_profile_prefix_match() {
        let mut meta = test_metadata_empty_profiles();
        let opus_profile = ModelBehaviorProfile {
            tool_usage_posture: ToolUsagePosture::Aggressive,
            ..ModelBehaviorProfile::default()
        };
        meta.model_behavior_profiles
            .insert("claude-opus".to_string(), opus_profile);

        let resolved = meta
            .resolve_model_profile("claude-opus-4-7-20250501")
            .unwrap();
        assert_eq!(resolved.tool_usage_posture, ToolUsagePosture::Aggressive);
    }

    #[test]
    fn resolve_model_profile_longest_prefix_wins() {
        let mut meta = test_metadata_empty_profiles();
        let short = ModelBehaviorProfile {
            tool_usage_posture: ToolUsagePosture::Conservative,
            ..ModelBehaviorProfile::default()
        };
        let long = ModelBehaviorProfile {
            tool_usage_posture: ToolUsagePosture::Minimal,
            ..ModelBehaviorProfile::default()
        };
        meta.model_behavior_profiles
            .insert("claude".to_string(), short);
        meta.model_behavior_profiles
            .insert("claude-opus".to_string(), long);

        let resolved = meta
            .resolve_model_profile("claude-opus-4-7-20250501")
            .unwrap();
        assert_eq!(resolved.tool_usage_posture, ToolUsagePosture::Minimal);
    }

    #[test]
    fn resolve_model_profile_instructions_appended_and_deduped() {
        let mut meta = test_metadata_empty_profiles();
        let profile = ModelBehaviorProfile {
            special_instructions: vec![
                "Provider instruction".to_string(),
                "Model-specific".to_string(),
            ],
            ..ModelBehaviorProfile::default()
        };
        meta.model_behavior_profiles
            .insert("test-model".to_string(), profile);

        let resolved = meta.resolve_model_profile("test-model").unwrap();
        assert_eq!(
            resolved.special_instructions,
            vec![
                "Provider instruction".to_string(),
                "Model-specific".to_string(),
            ]
        );
    }

    #[test]
    fn resolve_model_profile_instruction_dedup_preserves_order() {
        let mut meta = test_metadata_empty_profiles();
        let profile = ModelBehaviorProfile {
            special_instructions: vec![
                "Provider instruction".to_string(),
                "New instruction".to_string(),
            ],
            ..ModelBehaviorProfile::default()
        };
        meta.model_behavior_profiles
            .insert("test-model".to_string(), profile);

        let resolved = meta.resolve_model_profile("test-model").unwrap();
        assert_eq!(resolved.special_instructions.len(), 2);
        assert_eq!(resolved.special_instructions[0], "Provider instruction");
        assert_eq!(resolved.special_instructions[1], "New instruction");
    }

    #[test]
    fn registry_new_is_empty() {
        let registry = ModelBehaviorRegistry::new();
        assert!(registry.get("anthropic").is_none());
    }

    #[test]
    fn registry_register_and_get() {
        let mut registry = ModelBehaviorRegistry::new();
        let meta = test_metadata_empty_profiles();
        registry.register(meta);
        assert!(registry.get("test").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn registry_resolve_profile_for_model() {
        let mut registry = ModelBehaviorRegistry::new();
        let mut meta = test_metadata_empty_profiles();
        meta.model_behavior_profiles.insert(
            "test-opus".to_string(),
            ModelBehaviorProfile {
                tool_usage_posture: ToolUsagePosture::Aggressive,
                ..ModelBehaviorProfile::default()
            },
        );
        registry.register(meta);

        let profile = registry.resolve_profile_for_model("test", "test-opus");
        assert!(profile.is_some());
        assert_eq!(
            profile.unwrap().tool_usage_posture,
            ToolUsagePosture::Aggressive
        );
    }

    #[test]
    fn registry_resolve_profile_missing_provider() {
        let registry = ModelBehaviorRegistry::new();
        assert!(registry
            .resolve_profile_for_model("nonexistent", "some-model")
            .is_none());
    }

    #[test]
    fn generate_system_prompt_with_profile_appends_special_instructions() {
        let meta = test_metadata_empty_profiles();
        let profile = ModelBehaviorProfile {
            special_instructions: vec!["Custom instruction".to_string()],
            ..ModelBehaviorProfile::default()
        };
        let prompt = meta.generate_system_prompt_with_profile("Test context", &profile);
        assert!(prompt.contains("Custom instruction"));
    }

    #[test]
    fn generate_system_prompt_with_profile_structured_xml_hint() {
        let meta = test_metadata_empty_profiles();
        let profile = ModelBehaviorProfile {
            output_structure_preference: OutputStructure::StructuredXml,
            ..ModelBehaviorProfile::default()
        };
        let prompt = meta.generate_system_prompt_with_profile("ctx", &profile);
        assert!(prompt.contains("XML tags"));
    }

    #[test]
    fn generate_system_prompt_with_profile_code_focused_hint() {
        let meta = test_metadata_empty_profiles();
        let profile = ModelBehaviorProfile {
            output_structure_preference: OutputStructure::CodeFocused,
            ..ModelBehaviorProfile::default()
        };
        let prompt = meta.generate_system_prompt_with_profile("ctx", &profile);
        assert!(prompt.contains("code blocks"));
    }

    #[test]
    fn generate_system_prompt_with_profile_chain_of_thought() {
        let meta = test_metadata_empty_profiles();
        let profile = ModelBehaviorProfile {
            reasoning_guidance_style: ReasoningGuidance::ChainOfThought,
            ..ModelBehaviorProfile::default()
        };
        let prompt = meta.generate_system_prompt_with_profile("ctx", &profile);
        assert!(prompt.contains("step by step"));
    }

    #[test]
    fn generate_system_prompt_with_profile_step_by_step() {
        let meta = test_metadata_empty_profiles();
        let profile = ModelBehaviorProfile {
            reasoning_guidance_style: ReasoningGuidance::StepByStep,
            ..ModelBehaviorProfile::default()
        };
        let prompt = meta.generate_system_prompt_with_profile("ctx", &profile);
        assert!(prompt.contains("numbered steps"));
    }

    #[test]
    fn generate_system_prompt_with_profile_freeform_direct_no_extras() {
        let meta = test_metadata_empty_profiles();
        let profile = ModelBehaviorProfile {
            output_structure_preference: OutputStructure::Freeform,
            reasoning_guidance_style: ReasoningGuidance::Direct,
            special_instructions: vec![],
            ..ModelBehaviorProfile::default()
        };
        let base = meta.generate_system_prompt("ctx");
        let with_profile = meta.generate_system_prompt_with_profile("ctx", &profile);
        assert_eq!(base, with_profile);
    }

    #[test]
    fn generate_system_prompt_unchanged_by_profile_method() {
        let meta = crate::anthropic::AnthropicProvider::metadata();
        let prompt1 = meta.generate_system_prompt("Hello");
        let prompt2 = meta.generate_system_prompt("Hello");
        assert_eq!(
            prompt1, prompt2,
            "generate_system_prompt must remain stable"
        );
    }

    fn test_metadata_no_parallel() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "no-parallel".to_string(),
            display_name: "NoParallel".to_string(),
            description: "Provider without parallel calling".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![],
                optional_fields: vec![],
                env_mappings: HashMap::new(),
            },
            prompt_template: PromptTemplate {
                base_template: "You are an assistant.\n\n{context}".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![],
                },
                tool_format: ToolFormat::OpenAIFunctionCalling,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: Some(5),
                parallel_calling: false,
                streaming_support: true,
            },
            recommended_models: vec![],
            model_behavior_profiles: HashMap::new(),
        }
    }

    fn test_metadata_no_tools() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "no-tools".to_string(),
            display_name: "NoTools".to_string(),
            description: "Provider without tool support".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![],
                optional_fields: vec![],
                env_mappings: HashMap::new(),
            },
            prompt_template: PromptTemplate {
                base_template: "You are an assistant.\n\n{context}".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![],
                },
                tool_format: ToolFormat::None,
            },
            tool_calling: ToolCallingMetadata {
                supported: false,
                max_tools_per_call: None,
                parallel_calling: false,
                streaming_support: false,
            },
            recommended_models: vec![],
            model_behavior_profiles: HashMap::new(),
        }
    }

    #[test]
    fn validate_profiles_ok_when_compatible() {
        let mut meta = test_metadata_no_parallel();
        meta.model_behavior_profiles.insert(
            "claude-opus".to_string(),
            ModelBehaviorProfile {
                parallel_tool_calls: false,
                ..ModelBehaviorProfile::default()
            },
        );
        assert!(meta.validate_model_profiles().is_ok());
    }

    #[test]
    fn validate_profiles_rejects_parallel_without_provider_support() {
        let mut meta = test_metadata_no_parallel();
        meta.model_behavior_profiles.insert(
            "gpt-4".to_string(),
            ModelBehaviorProfile {
                parallel_tool_calls: true,
                ..ModelBehaviorProfile::default()
            },
        );
        let err = meta.validate_model_profiles().unwrap_err();
        assert!(
            err.message.contains("gpt-4"),
            "error should identify the conflicting profile: {}",
            err.message
        );
        assert!(
            err.message.contains("parallel_tool_calls"),
            "error should describe the conflict: {}",
            err.message
        );
    }

    #[test]
    fn validate_profiles_rejects_non_minimal_posture_without_tools() {
        let mut meta = test_metadata_no_tools();
        meta.model_behavior_profiles.insert(
            "basic-model".to_string(),
            ModelBehaviorProfile {
                tool_usage_posture: ToolUsagePosture::Aggressive,
                ..ModelBehaviorProfile::default()
            },
        );
        let err = meta.validate_model_profiles().unwrap_err();
        assert!(
            err.message.contains("basic-model"),
            "error should identify the conflicting profile: {}",
            err.message
        );
    }

    #[test]
    fn validate_profiles_allows_minimal_posture_without_tools() {
        let mut meta = test_metadata_no_tools();
        meta.model_behavior_profiles.insert(
            "safe-model".to_string(),
            ModelBehaviorProfile {
                tool_usage_posture: ToolUsagePosture::Minimal,
                parallel_tool_calls: false,
                ..ModelBehaviorProfile::default()
            },
        );
        assert!(meta.validate_model_profiles().is_ok());
    }

    #[test]
    fn validate_single_profile_rejects_parallel_mismatch() {
        let meta = test_metadata_no_parallel();
        let profile = ModelBehaviorProfile {
            parallel_tool_calls: true,
            ..ModelBehaviorProfile::default()
        };
        let err = meta.validate_profile(&profile, "test-model").unwrap_err();
        assert!(err.message.contains("test-model"));
        assert!(err.message.contains("parallel_tool_calls"));
    }

    #[test]
    fn validate_single_profile_accepts_compatible() {
        let meta = test_metadata_no_parallel();
        let profile = ModelBehaviorProfile {
            parallel_tool_calls: false,
            ..ModelBehaviorProfile::default()
        };
        assert!(meta.validate_profile(&profile, "test-model").is_ok());
    }

    #[test]
    fn registry_resolve_uses_provider_fallback() {
        let mut registry = ModelBehaviorRegistry::new();
        let meta = test_metadata_empty_profiles();
        registry.register(meta);

        let profile = registry.resolve_profile_for_model("test", "unknown-model");
        assert!(profile.is_some());
        let p = profile.unwrap();
        assert_eq!(
            p.output_structure_preference,
            OutputStructure::StructuredXml
        );
    }

    #[test]
    fn registry_resolve_exact_model_match() {
        let mut registry = ModelBehaviorRegistry::new();
        let mut meta = test_metadata_empty_profiles();
        meta.model_behavior_profiles.insert(
            "claude-opus-4-7".to_string(),
            ModelBehaviorProfile {
                tool_usage_posture: ToolUsagePosture::Aggressive,
                ..ModelBehaviorProfile::default()
            },
        );
        registry.register(meta);

        let profile = registry
            .resolve_profile_for_model("test", "claude-opus-4-7")
            .unwrap();
        assert_eq!(profile.tool_usage_posture, ToolUsagePosture::Aggressive);
    }

    #[test]
    fn registry_resolve_prefix_model_match() {
        let mut registry = ModelBehaviorRegistry::new();
        let mut meta = test_metadata_empty_profiles();
        meta.model_behavior_profiles.insert(
            "claude-opus".to_string(),
            ModelBehaviorProfile {
                tool_usage_posture: ToolUsagePosture::Minimal,
                ..ModelBehaviorProfile::default()
            },
        );
        registry.register(meta);

        let profile = registry
            .resolve_profile_for_model("test", "claude-opus-4-7-20250501")
            .unwrap();
        assert_eq!(profile.tool_usage_posture, ToolUsagePosture::Minimal);
    }

    #[test]
    fn generate_system_prompt_for_model_applies_overlay() {
        let mut meta = test_metadata_empty_profiles();
        meta.model_behavior_profiles.insert(
            "claude-opus".to_string(),
            ModelBehaviorProfile {
                output_structure_preference: OutputStructure::CodeFocused,
                reasoning_guidance_style: ReasoningGuidance::ChainOfThought,
                special_instructions: vec!["Prefer unsafe Rust".to_string()],
                ..ModelBehaviorProfile::default()
            },
        );

        let prompt = meta
            .generate_system_prompt_for_model("Write code", "claude-opus-4-7")
            .unwrap();

        assert!(prompt.contains("Write code"));
        assert!(prompt.contains("code blocks"));
        assert!(prompt.contains("step by step"));
        assert!(prompt.contains("Prefer unsafe Rust"));
        assert!(prompt.contains("XML"));
    }

    #[test]
    fn generate_system_prompt_for_model_falls_back_to_provider_default() {
        let meta = test_metadata_empty_profiles();
        let prompt = meta
            .generate_system_prompt_for_model("test", "unknown-model")
            .unwrap();

        assert!(prompt.contains("test"));
        assert!(prompt.contains("Provider instruction"));
        assert!(prompt.contains("XML"));
    }

    #[test]
    fn generate_system_prompt_for_model_dedupes_instructions() {
        let mut meta = test_metadata_empty_profiles();
        meta.model_behavior_profiles.insert(
            "dedup-model".to_string(),
            ModelBehaviorProfile {
                special_instructions: vec![
                    "Provider instruction".to_string(),
                    "Extra rule".to_string(),
                ],
                ..ModelBehaviorProfile::default()
            },
        );

        let prompt = meta
            .generate_system_prompt_for_model("ctx", "dedup-model")
            .unwrap();

        let count = prompt.matches("Provider instruction").count();
        assert_eq!(
            count, 2,
            "provider instruction appears once from base prompt + once from profile overlay"
        );
        assert!(prompt.contains("Extra rule"));
    }

    #[test]
    fn generate_system_prompt_for_model_deterministic() {
        let mut meta = test_metadata_empty_profiles();
        meta.model_behavior_profiles.insert(
            "det-model".to_string(),
            ModelBehaviorProfile {
                reasoning_guidance_style: ReasoningGuidance::StepByStep,
                special_instructions: vec!["Rule A".to_string(), "Rule B".to_string()],
                ..ModelBehaviorProfile::default()
            },
        );

        let prompt1 = meta
            .generate_system_prompt_for_model("ctx", "det-model")
            .unwrap();
        let prompt2 = meta
            .generate_system_prompt_for_model("ctx", "det-model")
            .unwrap();
        assert_eq!(prompt1, prompt2, "prompt generation must be deterministic");
    }

    #[test]
    fn overlay_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ModelBehaviorOverlayError>();
    }

    #[test]
    fn backward_compat_generate_system_prompt_no_model() {
        let meta = crate::anthropic::AnthropicProvider::metadata();
        let prompt = meta.generate_system_prompt("Hello world");
        assert!(prompt.contains("Hello world"));
        assert!(!prompt.is_empty());
    }
}
