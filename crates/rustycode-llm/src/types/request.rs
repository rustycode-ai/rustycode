use serde::{Deserialize, Serialize};

use super::config::{EffortLevel, OutputConfig, ThinkingConfig, ThinkingType};
use super::message::{ApiMode, ChatMessage, SkillRef};

/// Typed tool choice for controlling how the model selects tools.
///
/// Replaces untyped `serde_json::Value` to provide compile-time exhaustiveness
/// checking. Each variant maps to provider-specific wire formats in the wire layers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolChoice {
    /// Model decides whether to use tools (default)
    Auto,
    /// Model must use at least one tool
    Required,
    /// Model must not use any tools
    None,
    /// Model must use the specified tool
    Named(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
    pub system_prompt: Option<String>,
    pub tools: Option<Vec<serde_json::Value>>,
    /// Thinking configuration (Opus 4.5+, Sonnet 4.5+)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Output configuration for response generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    /// Skills container for Anthropic Agent Skills API.
    /// Contains skill references that enable code execution for document generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Enable parallel tool calls — model may emit multiple tool_use blocks per turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// Session identifier for prompt cache routing (OpenAI prompt_cache_key).
    /// Requests with the same session_id share a cache prefix for higher hit rates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// API mode: Chat Completions (default), Responses API, or Responses WebSocket.
    /// When `None`, providers use their default endpoint (Chat Completions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_mode: Option<ApiMode>,
}

impl CompletionRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            max_tokens: None,
            temperature: None,
            stream: false,
            system_prompt: None,
            tools: None,
            thinking: None,
            output_config: None,
            container: None,
            tool_choice: None,
            parallel_tool_calls: None,
            session_id: None,
            api_mode: None,
        }
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_streaming(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub fn with_system_prompt(mut self, system_prompt: String) -> Self {
        self.system_prompt = Some(system_prompt);
        self
    }

    pub fn with_tools(mut self, tools: Vec<serde_json::Value>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn with_tool_choice(mut self, choice: ToolChoice) -> Self {
        self.tool_choice = Some(choice);
        self
    }

    /// Enable or disable parallel tool calls.
    pub fn with_parallel_tool_calls(mut self, enabled: bool) -> Self {
        self.parallel_tool_calls = Some(enabled);
        self
    }

    /// Set reasoning effort level (low/medium/high/xhigh/max)
    ///
    /// This controls how much reasoning effort the model expends.
    /// Higher effort produces more thorough analysis but is slower.
    /// Merges with existing output_config, preserving format if set.
    pub fn with_effort(mut self, effort: EffortLevel) -> Self {
        match &mut self.output_config {
            Some(cfg) => cfg.effort = Some(effort),
            None => {
                self.output_config = Some(OutputConfig {
                    effort: Some(effort),
                    format: None,
                });
            }
        }
        self
    }

    /// Set output configuration
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rustycode_llm::provider::{CompletionRequest, OutputConfig, EffortLevel};
    ///
    /// let request = CompletionRequest::new(model, messages)
    ///     .with_output_config(OutputConfig::with_effort(EffortLevel::High));
    /// ```
    pub fn with_output_config(mut self, config: OutputConfig) -> Self {
        self.output_config = Some(config);
        self
    }

    /// Request structured JSON output conforming to a JSON Schema.
    ///
    /// Uses the provider's native grammar-constrained decoding when available
    /// (Anthropic Claude 4.x). The response's `structured_output` field will
    /// contain the parsed JSON value.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let schema = serde_json::json!({
    ///     "type": "object",
    ///     "properties": { "answer": { "type": "string" } },
    ///     "required": ["answer"]
    /// });
    /// let request = CompletionRequest::new(model, messages)
    ///     .with_json_schema(schema);
    /// ```
    pub fn with_json_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_config = Some(OutputConfig::with_json_schema(schema));
        self
    }

    /// Set thinking configuration using adaptive mode (recommended)
    ///
    /// Adaptive mode lets Claude decide when to use extended thinking based on task complexity.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rustycode_llm::provider::{CompletionRequest, ThinkingConfig, ThinkingDisplay};
    ///
    /// let request = CompletionRequest::new(model, messages)
    ///     .with_thinking_config(
    ///         ThinkingConfig::adaptive()
    ///             .with_display(ThinkingDisplay::Omitted)
    ///     );
    /// ```
    pub fn with_thinking_config(mut self, config: ThinkingConfig) -> Self {
        self.thinking = Some(config);
        self
    }

    /// Set thinking type (convenience method)
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rustycode_llm::provider::{CompletionRequest, ThinkingType};
    ///
    /// let request = CompletionRequest::new(model, messages)
    ///     .with_thinking_type(ThinkingType::Adaptive);
    /// ```
    pub fn with_thinking_type(mut self, thinking_type: ThinkingType) -> Self {
        self.thinking = Some(ThinkingConfig {
            thinking_type,
            display: None,
            budget_tokens: None,
        });
        self
    }

    /// Enable Anthropic Agent Skills for this request.
    ///
    /// Sets the `container.skills` parameter and requires the code_execution tool.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rustycode_llm::provider::{CompletionRequest, SkillRef};
    ///
    /// let request = CompletionRequest::new(model, messages)
    ///     .with_skills(vec![SkillRef {
    ///         skill_type: "anthropic".into(),
    ///         skill_id: "pptx".into(),
    ///         version: "latest".into(),
    ///     }]);
    /// ```
    pub fn with_skills(mut self, skills: Vec<SkillRef>) -> Self {
        self.container = Some(serde_json::json!({
            "skills": skills.iter().map(|s| serde_json::json!({
                "type": s.skill_type,
                "skill_id": s.skill_id,
                "version": s.version,
            })).collect::<Vec<_>>()
        }));
        self
    }

    /// Validate thinking configuration is compatible with the model
    ///
    /// Returns an error if thinking is configured but the model doesn't support it.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rustycode_llm::provider::{CompletionRequest, ThinkingType};
    ///
    /// let request = CompletionRequest::new(
    ///     "claude-3-5-sonnet-20241022".to_string(),
    ///     messages
    /// )
    /// .with_thinking_type(ThinkingType::Adaptive);
    ///
    /// assert!(request.validate_thinking().is_err());
    /// ```
    pub fn validate_thinking(&self) -> Result<(), String> {
        if let Some(ref thinking) = self.thinking {
            // Disabled thinking is always valid — it means "no thinking"
            if matches!(thinking.thinking_type, ThinkingType::Disabled) {
                return Ok(());
            }

            if !thinking.thinking_type.supports_model(&self.model) {
                return Err(format!(
                    "Thinking type {:?} is not supported by model {}. \
                     Adaptive/Enabled thinking requires Opus 4.5+ or Sonnet 4.5+",
                    thinking.thinking_type, self.model
                ));
            }
            // Opus 4.7+ only supports Adaptive mode; manual (Enabled) returns 400 from API
            if matches!(thinking.thinking_type, ThinkingType::Enabled) {
                let model_lower = self.model.to_lowercase();
                if model_lower.contains("opus-4-7") || model_lower.contains("opus-4.7-") {
                    return Err(format!(
                        "Model {} only supports adaptive thinking. \
                         Use ThinkingType::Adaptive instead of Enabled",
                        self.model
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_choice_serializes_auto() {
        let tc = ToolChoice::Auto;
        let json = serde_json::to_string(&tc).unwrap();
        assert_eq!(json, "\"Auto\"");
    }

    #[test]
    fn tool_choice_serializes_required() {
        let tc = ToolChoice::Required;
        let json = serde_json::to_string(&tc).unwrap();
        assert_eq!(json, "\"Required\"");
    }

    #[test]
    fn tool_choice_serializes_none_variant() {
        let tc = ToolChoice::None;
        let json = serde_json::to_string(&tc).unwrap();
        assert_eq!(json, "\"None\"");
    }

    #[test]
    fn tool_choice_serializes_named() {
        let tc = ToolChoice::Named("Bash".to_string());
        let json = serde_json::to_string(&tc).unwrap();
        assert_eq!(json, "{\"Named\":\"Bash\"}");
    }

    #[test]
    fn tool_choice_roundtrip() {
        let variants = vec![
            ToolChoice::Auto,
            ToolChoice::Required,
            ToolChoice::None,
            ToolChoice::Named("Edit".to_string()),
        ];
        for original in variants {
            let json = serde_json::to_string(&original).unwrap();
            let restored: ToolChoice = serde_json::from_str(&json).unwrap();
            assert_eq!(original, restored);
        }
    }

    #[test]
    fn tool_choice_equality() {
        assert_eq!(ToolChoice::Auto, ToolChoice::Auto);
        assert_ne!(ToolChoice::Auto, ToolChoice::Required);
        assert_eq!(
            ToolChoice::Named("Read".to_string()),
            ToolChoice::Named("Read".to_string())
        );
        assert_ne!(
            ToolChoice::Named("Read".to_string()),
            ToolChoice::Named("Write".to_string())
        );
    }
}
