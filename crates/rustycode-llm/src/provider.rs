use async_trait::async_trait;
use futures::Stream;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use strum::{AsRefStr, Display, EnumString};

// Re-export message content types from protocol crate
pub use rustycode_protocol::{ContentBlock, ImageSource, MessageContent};

/// Reference to an Anthropic Agent Skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRef {
    #[serde(rename = "type")]
    pub skill_type: String,
    pub skill_id: String,
    pub version: String,
}

/// Reasoning effort level for Claude API
///
/// Controls how much reasoning effort the model should expend:
/// - `low`: Quick responses, minimal reasoning (fastest, cheapest)
/// - `medium`: Balanced reasoning (default)
/// - `high`: Deeper analysis, more thorough (slower, more expensive)
/// - `xhigh`: Extended capability for long-horizon work, recommended starting point for Opus 4.7+ coding/agentic tasks
/// - `max`: Maximum reasoning depth (slowest, most expensive)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
#[non_exhaustive]
pub enum EffortLevel {
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
    Max,
}

/// Thinking configuration for Claude API (Opus 4.5+, Sonnet 4.5+)
///
/// Controls extended thinking behavior. Adaptive mode lets Claude decide when to think,
/// while Enabled mode always uses thinking with an optional budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub thinking_type: ThinkingType,
    /// Whether to show summarized thinking or omit it from response
    /// Only applies when thinking_type is Adaptive or Enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<ThinkingDisplay>,
    /// Maximum tokens to spend on thinking (only for Enabled mode)
    /// Defaults to 20000 if not specified
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

impl ThinkingConfig {
    /// Create a new adaptive thinking config
    pub fn adaptive() -> Self {
        Self {
            thinking_type: ThinkingType::Adaptive,
            display: None,
            budget_tokens: None,
        }
    }

    /// Create a new enabled thinking config with budget
    pub fn enabled(budget_tokens: u32) -> Self {
        Self {
            thinking_type: ThinkingType::Enabled,
            display: None,
            budget_tokens: Some(budget_tokens),
        }
    }

    /// Set the display mode
    pub fn with_display(mut self, display: ThinkingDisplay) -> Self {
        self.display = Some(display);
        self
    }

    /// Set the budget tokens (only applies to Enabled mode)
    pub fn with_budget(mut self, budget: u32) -> Self {
        self.budget_tokens = Some(budget);
        self
    }
}

/// Output configuration for Claude API
///
/// Controls response generation behavior including reasoning effort
/// and structured output format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Reasoning effort level (low/medium/high/xhigh/max)
    /// Controls how much reasoning effort the model expends
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<EffortLevel>,
    /// Structured output format configuration
    /// When set, the model will respond with JSON conforming to the schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputFormat>,
}

impl OutputConfig {
    /// Create a new output config with effort level
    pub fn with_effort(effort: EffortLevel) -> Self {
        Self {
            effort: Some(effort),
            format: None,
        }
    }

    /// Create a new output config with JSON schema format
    pub fn with_json_schema(schema: serde_json::Value) -> Self {
        Self {
            effort: None,
            format: Some(OutputFormat::json_schema(schema)),
        }
    }
}

/// Output format configuration for structured responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFormat {
    /// The type of structured output format
    #[serde(rename = "type")]
    pub format_type: OutputFormatType,
    /// JSON Schema for structured output validation
    /// Only used when format_type is JsonSchema
    #[serde(rename = "schema", skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
}

impl OutputFormat {
    /// Create a JSON schema output format
    pub fn json_schema(schema: serde_json::Value) -> Self {
        Self {
            format_type: OutputFormatType::JsonSchema,
            json_schema: Some(schema),
        }
    }
}

/// Type of structured output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormatType {
    /// JSON schema structured output
    JsonSchema,
}

/// Build an OpenAI-compatible `response_format` JSON value from an `OutputConfig`.
pub fn build_openai_response_format(
    output_config: &Option<OutputConfig>,
) -> Option<serde_json::Value> {
    let oc = output_config.as_ref()?;
    let format = oc.format.as_ref()?;
    match format.format_type {
        OutputFormatType::JsonSchema => {
            let schema = format
                .json_schema
                .clone()
                .unwrap_or(serde_json::Value::Null);
            Some(serde_json::json!({
                "type": "json_schema",
                "json_schema": {
                    "name": "structured_output",
                    "strict": true,
                    "schema": schema
                }
            }))
        }
    }
}

/// Build a Gemini-compatible response schema from an `OutputConfig`.
pub fn build_gemini_response_schema(
    output_config: &Option<OutputConfig>,
) -> Option<serde_json::Value> {
    let oc = output_config.as_ref()?;
    let format = oc.format.as_ref()?;
    match format.format_type {
        OutputFormatType::JsonSchema => {
            let schema = format
                .json_schema
                .clone()
                .unwrap_or(serde_json::Value::Null);
            Some(serde_json::json!({
                "responseMimeType": "application/json",
                "responseSchema": schema
            }))
        }
    }
}

/// Display mode for thinking blocks
///
/// Controls whether extended thinking content is visible in responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
#[non_exhaustive]
pub enum ThinkingDisplay {
    /// Show summarized thinking content (default)
    #[default]
    Summarized,
    /// Hide thinking content from the response
    Omitted,
}

/// Type of thinking to use
///
/// - `Adaptive`: Claude decides when to think based on task complexity (recommended)
/// - `Enabled`: Always use extended thinking with optional budget
/// - `Disabled`: No extended thinking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ThinkingType {
    Adaptive,
    Enabled,
    Disabled,
}

impl ThinkingType {
    /// Check if a model supports this thinking type
    ///
    /// # Supported Models
    ///
    /// Adaptive and Enabled thinking require:
    /// - Opus 4.5+: `claude-opus-4-20250514`, `claude-opus-4.5-*`
    /// - Opus 4.6+: `claude-opus-4-20250214`, `claude-opus-4.6-*`
    /// - Sonnet 4.5+: `claude-sonnet-4-20250514`, `claude-sonnet-4.5-*`
    /// - Sonnet 4.6+: `claude-sonnet-4-20250214`, `claude-sonnet-4.6-*`
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rustycode_llm::provider::ThinkingType;
    ///
    /// assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-20250514"));
    /// assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-20250214")); // 4.6
    /// assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4.6-20250214"));
    /// assert!(!ThinkingType::Adaptive.supports_model("claude-3-5-sonnet-20241022"));
    /// ```
    pub fn supports_model(&self, model: &str) -> bool {
        // Only adaptive and enabled thinking need model support
        if !matches!(self, ThinkingType::Adaptive | ThinkingType::Enabled) {
            return false;
        }

        let model_lower = model.to_lowercase();

        // Opus 4.5+
        if model_lower.contains("opus-4-20250514") || model_lower.contains("opus-4.5-") {
            return true;
        }

        // Opus 4.6+
        if model_lower.contains("opus-4-20250214")
            || model_lower.contains("opus-4.6-")
            || model_lower.contains("opus-4-6")
        {
            return true;
        }

        // Sonnet 4.5+
        if model_lower.contains("sonnet-4-20250514") || model_lower.contains("sonnet-4.5-") {
            return true;
        }

        // Sonnet 4.6+
        if model_lower.contains("sonnet-4-20250214")
            || model_lower.contains("sonnet-4.6-")
            || model_lower.contains("sonnet-4-6")
        {
            return true;
        }

        // Opus 4.7+ (adaptive-only, manual mode returns 400)
        if model_lower.contains("opus-4-7") || model_lower.contains("opus-4.7-") {
            return true;
        }

        // GLM-5.x reasoning models (z.ai)
        if model_lower.starts_with("glm-5") {
            return true;
        }

        false
    }
}

/// API mode for providers that support multiple endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApiMode {
    /// Chat Completions API (`POST /v1/chat/completions`) — default.
    ChatCompletions,
    /// Responses API (`POST /v1/responses`) — HTTP.
    Responses,
    /// Responses API via WebSocket (OpenAI only, feature-gated).
    ResponsesWs,
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
    /// Tool choice: controls how the model selects tools.
    /// None = provider default, Some("auto")/Some("none")/Some({"type":"any"}) etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
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

    /// Set tool choice to control how the model selects tools.
    ///
    /// Common values: `"auto"` (default), `"required"` (force tool use),
    /// `"none"` (disable tools), or `{"type": "tool", "name": "edit_file"}`.
    pub fn with_tool_choice(mut self, choice: serde_json::Value) -> Self {
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

/// Message role with type-safe variants
///
/// This enum prevents typos and enables zero-allocation role handling
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, AsRefStr)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
#[non_exhaustive]
pub enum MessageRole {
    User,
    Assistant,
    System,
    /// For tool/function calling responses
    Tool(String),
}

impl MessageRole {
    /// Create from string (for API responses)
    pub fn from_role_str(s: &str) -> Result<Self, ProviderError> {
        match s.to_lowercase().as_str() {
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "system" => Ok(MessageRole::System),
            other if other.starts_with("tool_") || other.starts_with("tool:") => {
                Ok(MessageRole::Tool(other[5..].to_string()))
            }
            _ => Err(ProviderError::Api(format!("unknown message role: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: MessageContent,
}

impl ChatMessage {
    pub fn user(content: impl Into<MessageContent>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<MessageContent>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
        }
    }

    pub fn system(content: impl Into<MessageContent>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
        }
    }

    pub fn tool(content: impl Into<MessageContent>, tool_name: String) -> Self {
        Self {
            role: MessageRole::Tool(tool_name),
            content: content.into(),
        }
    }

    pub fn tool_result(content: String, tool_use_id: String) -> Self {
        Self::tool_result_with_error(content, tool_use_id, false)
    }

    /// Create a tool result message with error flag
    pub fn tool_result_with_error(content: String, tool_use_id: String, is_error: bool) -> Self {
        let mut tool_result = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": content
        });
        if is_error {
            tool_result["is_error"] = serde_json::json!(true);
        }
        Self {
            role: MessageRole::User, // Tool results are sent by user role
            content: MessageContent::Simple(tool_result.to_string()),
        }
    }

    /// Get the text content for backward compatibility
    pub fn text(&self) -> String {
        self.content.as_text()
    }

    /// Check if message contains images
    pub fn has_images(&self) -> bool {
        self.content.has_images()
    }
}

/// A thinking block from an extended thinking response.
/// Must be preserved unchanged for multi-turn conversations with tool use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlock {
    /// The block type: "thinking" or "redacted_thinking"
    #[serde(rename = "type")]
    pub block_type: String,
    /// The thinking content (empty for redacted_thinking or display: "omitted")
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub thinking: String,
    /// The encrypted signature (for round-tripping)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub signature: String,
    /// Encrypted data for redacted_thinking blocks
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub content: String,
    /// Model name used for this completion (for tracking and logging)
    pub model: String,
    pub usage: Option<Usage>,
    pub stop_reason: Option<String>,
    /// Citation metadata for search results (when applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<Citation>>,
    /// Thinking blocks for round-tripping in multi-turn conversations with tool use
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_blocks: Option<Vec<ThinkingBlock>>,
    /// Parsed structured output when `output_config.format` was `JsonSchema`.
    /// Contains the validated JSON value returned by the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Value>,
}

/// Normalize provider-specific stop/finish reasons to Anthropic convention.
///
/// The agent loop checks for `"end_turn"` (model finished naturally) and
/// `"tool_use"` (model wants to call tools). Different providers use
/// different strings — this function maps them to the canonical values.
///
/// | Provider    | Raw value      | Normalized    |
/// |-------------|----------------|---------------|
/// | OpenAI      | `"stop"`       | `"end_turn"`  |
/// | OpenAI      | `"tool_calls"` | `"tool_use"`  |
/// | OpenAI      | `"length"`     | `"max_tokens"`|
/// | Cohere      | `"COMPLETE"`   | `"end_turn"`  |
/// | Cohere      | `"TOOL_CALL"`  | `"tool_use"`  |
/// | Cohere      | `"MAX_TOKENS"` | `"max_tokens"`|
/// | Anthropic   | `"end_turn"`   | unchanged     |
/// | Anthropic   | `"tool_use"`   | unchanged     |
pub fn normalize_stop_reason(reason: Option<&str>) -> Option<String> {
    reason
        .map(|r| match r {
            // OpenAI family
            "stop" => "end_turn",
            "tool_calls" => "tool_use",
            "length" => "max_tokens",
            "content_filter" => "end_turn",
            // Cohere
            "COMPLETE" => "end_turn",
            "TOOL_CALL" => "tool_use",
            "MAX_TOKENS" => "max_tokens",
            // Ollama
            "done" => "end_turn",
            // Gemini
            "STOP" => "end_turn",
            "SAFETY" => "end_turn",
            "RECITATION" => "end_turn",
            // Anthropic — already correct (end_turn, tool_use, max_tokens)
            other => other,
        })
        .map(|s| s.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens after the last cache breakpoint (not eligible for cache)
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,

    /// Cache-aware token tracking (Anthropic prompt caching)
    /// Tokens read from cache (billed at 0.1× base input price)
    #[serde(default)]
    pub cache_read_input_tokens: u32,

    /// Tokens written to cache (billed at 1.25× base input price for 5min TTL)
    #[serde(default)]
    pub cache_creation_input_tokens: u32,

    /// Reasoning tokens (if reasoning effort was used)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
}

/// Citation metadata for search results (RAG applications)
/// When a model cites sources in its response, it provides location information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub source: String,     // The source URL or identifier
    pub title: String,      // The title of the source
    pub cited_text: String, // Exact text being cited
    pub index: u32,         // Index of the cited search result (0-based)
}

impl Usage {
    pub fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens.saturating_add(output_tokens),
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_tokens: None,
        }
    }

    /// Create usage with cache information
    pub fn with_cache(
        input_tokens: u32,
        output_tokens: u32,
        cache_read_input_tokens: u32,
        cache_creation_input_tokens: u32,
    ) -> Self {
        // Total input = cache read + cache write + non-cached input
        let total_input = cache_read_input_tokens
            .saturating_add(cache_creation_input_tokens)
            .saturating_add(input_tokens);
        Self {
            input_tokens,
            output_tokens,
            total_tokens: total_input.saturating_add(output_tokens),
            cache_read_input_tokens,
            cache_creation_input_tokens,
            reasoning_tokens: None,
        }
    }

    /// Calculate total input tokens (including cache)
    pub fn total_input_tokens(&self) -> u32 {
        self.cache_read_input_tokens
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.input_tokens)
    }

    /// Check if any cache tokens were used
    pub fn has_cache_usage(&self) -> bool {
        self.cache_read_input_tokens > 0 || self.cache_creation_input_tokens > 0
    }
}

/// Server-Sent Events (SSE) for streaming responses
///
/// These event types mirror Claude's SSE event format as documented in:
/// <https://platform.claude.com/docs/en/build-with-claude/streaming>
#[doc(hidden)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
#[non_exhaustive]
#[allow(dead_code)]
pub(crate) enum SSEEvent {
    /// Plain text content (for providers without full SSE support)
    Text { text: String },

    /// message_start - Initial event with message metadata
    MessageStart {
        message_id: String,
        #[serde(rename = "type")]
        message_type: String,
        role: String,
    },

    /// content_block_start - Start of a content block
    ContentBlockStart {
        index: usize,
        content_block: ContentBlockType,
    },

    /// content_block_delta - Incremental update to a content block
    ContentBlockDelta { index: usize, delta: ContentDelta },

    /// content_block_stop - End of a content block
    ContentBlockStop { index: usize },

    /// message_delta - Final message metadata (stop_reason, usage)
    MessageDelta {
        stop_reason: Option<String>,
        usage: Option<Usage>,
    },

    /// message_stop - End of message
    MessageStop,

    /// ping - Keep-alive event
    Ping,

    /// error - Error event
    Error { error_type: String, message: String },

    /// thinking_delta - Extended thinking content (Claude's thinking feature)
    ThinkingDelta { thinking: String },

    /// signature_delta - Extended thinking signature (for verification)
    SignatureDelta { signature: String },
}

/// Content block types in streaming responses
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub(crate) enum ContentBlockType {
    /// Text content block
    #[serde(rename = "text")]
    Text { text: String },

    /// Tool use content block
    #[serde(rename = "tool_use")]
    ToolUse {
        /// Unique identifier for this tool use block
        id: String,
        /// Name of the tool being called
        name: String,
        /// Partial JSON input (streamed incrementally with eager streaming)
        input: Option<serde_json::Value>,
    },

    /// Thinking content block (extended thinking)
    #[serde(rename = "thinking")]
    Thinking { thinking: String },

    /// Tool result content block (for multi-turn conversations)
    #[serde(rename = "tool_result")]
    ToolResult {
        /// ID of the tool_use this result corresponds to
        tool_use_id: String,
        /// Result content
        content: Option<String>,
        /// Is this an error result?
        is_error: Option<bool>,
    },
}

/// Delta types for content block updates
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub(crate) enum ContentDelta {
    /// Text delta
    Text { text: String },

    /// Partial JSON delta (for tool_use with eager streaming)
    PartialJson { partial_json: String },

    /// Thinking delta (extended thinking)
    Thinking { thinking: String },

    /// Signature delta (for extended thinking verification)
    Signature { signature: String },

    /// Citation metadata delta
    Citations { citations: Vec<Citation> },
}

#[allow(dead_code)]
impl SSEEvent {
    /// Check if this event represents content that should be displayed to the user
    pub fn is_content(&self) -> bool {
        matches!(
            self,
            Self::Text { .. } | Self::ContentBlockDelta { .. } | Self::ThinkingDelta { .. }
        )
    }

    /// Extract text content from this event, if any
    #[allow(clippy::collapsible_match)]
    pub fn as_text(&self) -> Option<String> {
        match self {
            Self::Text { text } => Some(text.clone()),
            Self::ContentBlockDelta { delta, .. } => match delta {
                ContentDelta::Text { text } => Some(text.clone()),
                _ => None,
            },
            Self::ThinkingDelta { thinking } => Some(thinking.clone()),
            _ => None,
        }
    }

    /// Check if this is a final event (end of stream)
    pub fn is_final(&self) -> bool {
        matches!(self, Self::MessageStop | Self::Error { .. })
    }

    /// Check if this event represents thinking/reasoning content
    pub fn is_thinking(&self) -> bool {
        match self {
            Self::ThinkingDelta { .. } => true,
            Self::ContentBlockDelta { delta, .. } => matches!(delta, ContentDelta::Thinking { .. }),
            _ => false,
        }
    }

    /// Check if this event represents a tool use
    pub fn is_tool_use(&self) -> bool {
        matches!(self, Self::ContentBlockStart { content_block, .. }
            if matches!(content_block, ContentBlockType::ToolUse { .. }))
    }

    /// Extract thinking content, if any
    #[allow(clippy::collapsible_match)]
    pub fn as_thinking(&self) -> Option<String> {
        match self {
            Self::ThinkingDelta { thinking } => Some(thinking.clone()),
            Self::ContentBlockDelta { delta, .. } => match delta {
                ContentDelta::Thinking { thinking } => Some(thinking.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Extract tool use info, if any
    #[allow(clippy::collapsible_match)]
    pub fn as_tool_use(&self) -> Option<(String, String)> {
        match self {
            Self::ContentBlockStart { content_block, .. } => match content_block {
                ContentBlockType::ToolUse { id, name, .. } => Some((id.clone(), name.clone())),
                _ => None,
            },
            _ => None,
        }
    }

    /// Create a simple text event (for backward compatibility)
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }
}

/// Stream chunk result — provider-agnostic streaming event or error.
///
/// Providers translate their native wire format into `StreamEvent` before
/// returning from `complete_stream()`.
pub type StreamChunk = Result<rustycode_protocol::stream_event::StreamEvent, ProviderError>;

#[async_trait]
pub trait LLMProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Whether this provider can emit streamed turn events.
    ///
    /// Providers that only support request/response completions should
    /// override this to `false` so higher-level callers can skip the
    /// streaming path entirely.
    fn supports_streaming(&self) -> bool {
        true
    }

    async fn is_available(&self) -> bool;

    async fn list_models(&self) -> Result<Vec<String>, ProviderError>;

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError>;

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError>;

    fn config(&self) -> Option<&ProviderConfig> {
        None
    }
}

#[derive(Clone)]
pub struct ProviderConfig {
    pub api_key: Option<SecretString>,
    pub base_url: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub extra_headers: Option<std::collections::HashMap<String, String>>,
    pub retry_config: Option<crate::retry::RetryConfig>,
}

// Custom Debug implementation that redacts the API key
impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "***REDACTED***"))
            .field("base_url", &self.base_url)
            .field("timeout_seconds", &self.timeout_seconds)
            .field("extra_headers", &self.extra_headers)
            .finish()
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: None,
            timeout_seconds: Some(30),
            extra_headers: None,
            retry_config: Some(crate::retry::RetryConfig::default()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Rate limited. Please wait before retrying.")]
    RateLimited {
        retry_delay: Option<std::time::Duration>,
    },
    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(String),
    #[error("Credits exhausted: {details}")]
    CreditsExhausted {
        details: String,
        top_up_url: Option<String>,
    },
    #[error("Invalid model: {0}")]
    InvalidModel(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Timeout error: {0}")]
    Timeout(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl ProviderError {
    pub fn auth(msg: impl Into<String>) -> Self {
        Self::Auth(msg.into())
    }

    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    pub fn api(msg: impl Into<String>) -> Self {
        Self::Api(msg.into())
    }

    pub fn with_model(self, model: &str) -> Self {
        let prefix = format!("[model: {model}] ");
        match self {
            Self::Auth(s) => Self::Auth(prefix + &s),
            Self::Network(s) => Self::Network(prefix + &s),
            Self::Api(s) => Self::Api(prefix + &s),
            Self::RateLimited { retry_delay } => Self::RateLimited { retry_delay },
            Self::ContextLengthExceeded(s) => Self::ContextLengthExceeded(prefix + &s),
            Self::CreditsExhausted {
                details,
                top_up_url,
            } => Self::CreditsExhausted {
                details: prefix + &details,
                top_up_url,
            },
            Self::InvalidModel(s) => Self::InvalidModel(prefix + &s),
            Self::Serialization(s) => Self::Serialization(prefix + &s),
            Self::Timeout(s) => Self::Timeout(prefix + &s),
            Self::Configuration(s) => Self::Configuration(prefix + &s),
            Self::Unknown(s) => Self::Unknown(prefix + &s),
        }
    }

    /// Check if this error indicates rate limiting
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimited { .. })
    }

    /// Check if this error indicates context length was exceeded
    pub fn is_context_exceeded(&self) -> bool {
        matches!(self, Self::ContextLengthExceeded(_))
    }

    /// Check if this error indicates credits are exhausted
    pub fn is_credits_exhausted(&self) -> bool {
        matches!(self, Self::CreditsExhausted { .. })
    }

    /// Get the retry delay if this is a rate limit error
    pub fn retry_delay(&self) -> Option<std::time::Duration> {
        match self {
            Self::RateLimited { retry_delay } => *retry_delay,
            _ => None,
        }
    }

    /// Get the top-up URL if credits are exhausted
    pub fn top_up_url(&self) -> Option<&str> {
        match self {
            Self::CreditsExhausted { top_up_url, .. } => top_up_url.as_deref(),
            _ => None,
        }
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Network(_) | Self::Timeout(_)
        )
    }
}

// ============================================================================
// Macros for reducing boilerplate in provider implementations
// ============================================================================

/// Macro for getting shared global HTTP client
///
/// # Usage
/// ```ignore
/// impl MyProvider {
///     fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
///         let client = shared_client!();
///         Ok(Self { config, client })
///     }
/// }
/// ```
#[macro_export]
macro_rules! shared_client {
    () => {{
        use $crate::client_pool::global_client;
        (*global_client()).clone()
    }};
}

/// Macro for building HTTP request with provider-specific headers
///
/// # Usage
/// ```ignore
/// let api_key = self.config.api_key.as_ref().unwrap().expose_secret();
/// let mut req = build_request!(
///     self.client.post(&url),
///     headers = [
///         ("Authorization", format!("Bearer {}", api_key)),
///         ("Content-Type", "application/json"),
///     ],
///     extra_headers = &self.config.extra_headers
/// );
/// ```
#[macro_export]
macro_rules! build_request {
    ($base_req:expr, headers = [$(($key:expr, $val:expr)),* $(,)?], extra_headers = $extra_headers:expr) => {{
        let mut req = $base_req;

        // Add standard headers
        $(
            req = req.header($key, $val);
        )*

        // Add extra headers from config (if provided)
        if let Some(extra) = &$extra_headers {
            use $crate::provider::validate_extra_headers;
            let validated = match validate_extra_headers(&Some(extra.clone())) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Skipping invalid extra_headers: {}", e);
                    Vec::new()
                }
            };
            for (name, value) in validated {
                req = req.header(name, value);
            }
        }

        req
    }};
}

/// Macro for generating API key retrieval logic with environment variable fallback
///
/// # Usage
/// ```ignore
/// impl MyProvider {
///     get_api_key!(self, "MY_PROVIDER_API_KEY");
/// }
/// ```
#[macro_export]
macro_rules! get_api_key {
    ($self:expr, $env_var:expr) => {{
        {
            let config_key = $self
                .config
                .api_key
                .as_ref()
                .map(|k| k.expose_secret().to_string());
            let env_key = std::env::var($env_var).ok();
            config_key.or(env_key).ok_or_else(|| {
                $crate::provider::ProviderError::Configuration(
                    concat!(
                        "API key required. Set api_key in config or ",
                        $env_var,
                        " env var"
                    )
                    .to_string(),
                )
            })
        }
    }};
}

/// Macro for implementing standard LLMProvider trait methods
///
/// # Usage
/// ```ignore
/// impl LLMProvider for MyProvider {
///     provider_common!(my_provider, vec!["model1".to_string(), "model2".to_string()]);
///
///     async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
///         // custom implementation
///     }
///
///     async fn complete_stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
///         // custom implementation
///     }
/// }
/// ```
#[macro_export]
macro_rules! provider_common {
    ($name:expr, $models:expr) => {
        fn name(&self) -> &'static str {
            $name
        }

        async fn is_available(&self) -> bool {
            self.config
                .api_key
                .as_ref()
                .map_or(false, |k| !k.expose_secret().is_empty())
        }

        async fn list_models(&self) -> Result<Vec<String>, $crate::provider::ProviderError> {
            Ok($models)
        }

        fn config(&self) -> Option<&$crate::provider::ProviderConfig> {
            Some(&self.config)
        }
    };
}

/// Macro for converting ChatMessage to provider-specific message format
///
/// # Usage
/// ```ignore
/// let messages: Vec<ProviderMessage> = convert_messages!(request.messages, ProviderMessage {
///     role: msg.role,
///     content: msg.content,
/// });
/// ```
#[macro_export]
macro_rules! convert_messages {
    ($input:expr, $msg_ctor:expr) => {{
        $input.into_iter().map(|msg| $msg_ctor).collect::<Vec<_>>()
    }};
}

/// Macro for parsing OpenAI-compatible SSE streaming responses
///
/// # Usage
/// ```ignore
/// let sse_stream = bytes_stream.map(|chunk_result| -> StreamChunk {
///     let chunk = chunk_result.map_err(|e| ProviderError::Network(format!("Failed to read chunk: {}", e)))?;
///     let text = String::from_utf8_lossy(&chunk);
///     let mut chunks = Vec::new();
///
///     parse_openai_sse!(text, chunks);
///
///     Ok(chunks.join(""))
/// });
/// ```
#[macro_export]
macro_rules! parse_openai_sse {
    ($text:expr, $chunks:expr) => {
        for line in $text.lines() {
            if line.is_empty() {
                continue;
            }
            if line.starts_with("data: ") {
                let json_str = line.trim_start_matches("data: ").trim();
                if json_str == "[DONE]" {
                    continue;
                }
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                        if let Some(choice) = choices.first() {
                            if let Some(delta) = choice.get("delta") {
                                if let Some(content) = delta.get("content") {
                                    if let Some(content_str) = content.as_str() {
                                        if !content_str.is_empty() {
                                            $chunks.push(content_str.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
}

/// Validate and sanitize extra headers for security
///
/// This function prevents header injection attacks by:
/// 1. Whitelisting allowed headers
/// 2. Blocking override of security-critical headers
/// 3. Validating header values for CRLF injection
///
/// # Arguments
/// * `extra_headers` - Optional HashMap of custom headers
///
/// # Returns
/// * `Result<Vec<(HeaderName, HeaderValue)>, ProviderError>` - Validated headers or error
///
/// # Security
/// - Blocks: authorization, host, content-type, proxy-authorization
/// - Allows: X-* custom headers
/// - Validates: No CRLF characters in values
pub fn validate_extra_headers(
    extra_headers: &Option<std::collections::HashMap<String, String>>,
) -> Result<Vec<(reqwest::header::HeaderName, reqwest::header::HeaderValue)>, ProviderError> {
    let mut validated_headers = Vec::new();

    if let Some(headers) = extra_headers {
        for (key, value) in headers {
            // Block override of security-critical headers
            match key.to_lowercase().as_str() {
                "authorization"
                | "proxy-authorization"
                | "www-authenticate"
                | "proxy-authenticate" => {
                    return Err(ProviderError::Configuration(format!(
                        "cannot override security header '{}' via extra_headers",
                        key
                    )));
                }
                "host" | "content-type" | "content-length" | "transfer-encoding" => {
                    return Err(ProviderError::Configuration(format!(
                        "cannot override '{}' header via extra_headers",
                        key
                    )));
                }
                _ => {}
            }

            // Validate for CRLF injection (prevent header splitting)
            if value.contains('\r') || value.contains('\n') {
                return Err(ProviderError::Configuration(format!(
                    "header value for '{}' contains invalid newline characters",
                    key
                )));
            }

            // Parse header name and value
            let header_name =
                reqwest::header::HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
                    ProviderError::Configuration(format!("invalid header name '{}': {}", key, e))
                })?;

            let header_value = value.parse().map_err(|e| {
                ProviderError::Configuration(format!("invalid header value for '{}': {}", key, e))
            })?;

            validated_headers.push((header_name, header_value));
        }
    }

    Ok(validated_headers)
}

/// Macro for building HTTP client with standard timeout and headers
///
/// # Usage
/// ```ignore
/// let client = build_http_client!(
///     config,
///     headers,
///     timeout_seconds = 120,
///     connect_timeout = 10
/// );
/// ```
#[macro_export]
macro_rules! build_http_client {
    ($config:expr, $headers:expr, timeout_seconds = $timeout_secs:expr, connect_timeout = $connect_secs:expr) => {{
        use std::time::Duration;

        let timeout = Duration::from_secs($config.timeout_seconds.unwrap_or($timeout_secs));

        reqwest::Client::builder()
            .default_headers($headers)
            .timeout(timeout)
            .connect_timeout(Duration::from_secs($connect_secs))
            .build()
            .map_err(|e| {
                $crate::provider::ProviderError::Configuration(format!(
                    "failed to build HTTP client: {}",
                    e
                ))
            })
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn test_thinking_type_serialization() {
        // Test Adaptive serialization/deserialization
        let adaptive_json = r#""adaptive""#;
        let thinking_type: ThinkingType = serde_json::from_str(adaptive_json).unwrap();
        assert_eq!(thinking_type, ThinkingType::Adaptive);
        assert_eq!(
            serde_json::to_string(&thinking_type).unwrap(),
            adaptive_json
        );

        // Test Enabled serialization
        let enabled_json = r#""enabled""#;
        let thinking_type: ThinkingType = serde_json::from_str(enabled_json).unwrap();
        assert_eq!(thinking_type, ThinkingType::Enabled);
        assert_eq!(serde_json::to_string(&thinking_type).unwrap(), enabled_json);

        // Test Disabled serialization
        let disabled_json = r#""disabled""#;
        let thinking_type: ThinkingType = serde_json::from_str(disabled_json).unwrap();
        assert_eq!(thinking_type, ThinkingType::Disabled);
        assert_eq!(
            serde_json::to_string(&thinking_type).unwrap(),
            disabled_json
        );
    }

    #[test]
    fn test_thinking_display_serialization() {
        let summarized = json!("summarized");
        let display: ThinkingDisplay = serde_json::from_str(&summarized.to_string()).unwrap();
        assert_eq!(display, ThinkingDisplay::Summarized);

        let omitted = json!("omitted");
        let display: ThinkingDisplay = serde_json::from_str(&omitted.to_string()).unwrap();
        assert_eq!(display, ThinkingDisplay::Omitted);
    }

    #[test]
    fn test_thinking_config_adaptive() {
        let config = ThinkingConfig::adaptive();
        assert_eq!(config.thinking_type, ThinkingType::Adaptive);
        assert!(config.display.is_none());
        assert!(config.budget_tokens.is_none());
    }

    #[test]
    fn test_thinking_config_enabled() {
        let config = ThinkingConfig::enabled(10000);
        assert_eq!(config.thinking_type, ThinkingType::Enabled);
        assert_eq!(config.budget_tokens, Some(10000));
        assert!(config.display.is_none());
    }

    #[test]
    fn test_thinking_config_with_display() {
        let config = ThinkingConfig::enabled(10000).with_display(ThinkingDisplay::Omitted);
        assert_eq!(config.thinking_type, ThinkingType::Enabled);
        assert_eq!(config.budget_tokens, Some(10000));
        assert_eq!(config.display, Some(ThinkingDisplay::Omitted));
    }

    #[test]
    fn test_thinking_config_with_budget() {
        let config = ThinkingConfig::adaptive().with_budget(20000);
        assert_eq!(config.thinking_type, ThinkingType::Adaptive);
        assert_eq!(config.budget_tokens, Some(20000));
    }

    #[test]
    fn test_thinking_config_serialization() {
        let config = ThinkingConfig::enabled(10000).with_display(ThinkingDisplay::Omitted);
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(value["type"], "enabled");
        assert_eq!(value["budget_tokens"], 10000);
        assert_eq!(value["display"], "omitted");
    }

    #[test]
    fn test_thinking_config_serialization_adaptive() {
        let config = ThinkingConfig::adaptive().with_budget(20000);
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(value["type"], "adaptive");
        assert_eq!(value["budget_tokens"], 20000);
        // display should be omitted if None
        assert!(value.get("display").is_none());
    }

    #[test]
    fn test_effort_level_serialization() {
        assert_eq!(
            serde_json::to_string(&EffortLevel::Max).unwrap(),
            r#""max""#
        );
        assert_eq!(
            serde_json::to_string(&EffortLevel::Xhigh).unwrap(),
            r#""xhigh""#
        );
        assert_eq!(
            serde_json::to_string(&EffortLevel::High).unwrap(),
            r#""high""#
        );
        assert_eq!(
            serde_json::to_string(&EffortLevel::Medium).unwrap(),
            r#""medium""#
        );
        assert_eq!(
            serde_json::to_string(&EffortLevel::Low).unwrap(),
            r#""low""#
        );
    }

    #[test]
    fn test_output_config_with_effort() {
        let config = OutputConfig::with_effort(EffortLevel::High);
        assert_eq!(config.effort, Some(EffortLevel::High));

        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(value["effort"], "high");
    }

    #[test]
    fn test_output_config_serialization_no_effort() {
        let config = OutputConfig {
            effort: None,
            format: None,
        };
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();
        // When effort is None, the entire object might serialize differently
        // depending on skip_serializing_if
        assert!(value.get("effort").is_none());
    }

    #[test]
    fn test_completion_request_with_thinking_config() {
        let request = CompletionRequest::new(
            "claude-opus-4-6".to_string(),
            vec![ChatMessage::user("Test".to_string())],
        )
        .with_thinking_config(ThinkingConfig::adaptive());

        assert!(request.thinking.is_some());
        assert_eq!(
            request.thinking.as_ref().unwrap().thinking_type,
            ThinkingType::Adaptive
        );
    }

    #[test]
    fn test_completion_request_with_output_config() {
        let request = CompletionRequest::new(
            "claude-sonnet-4-20250214".to_string(),
            vec![ChatMessage::user("Test".to_string())],
        )
        .with_output_config(OutputConfig::with_effort(EffortLevel::Max));

        assert!(request.output_config.is_some());
        assert_eq!(
            request.output_config.as_ref().unwrap().effort,
            Some(EffortLevel::Max)
        );
    }

    #[test]
    fn test_completion_request_with_effort() {
        let request = CompletionRequest::new(
            "claude-opus-4-6".to_string(),
            vec![ChatMessage::user("Test".to_string())],
        )
        .with_effort(EffortLevel::High);

        assert!(request.output_config.is_some());
        assert_eq!(
            request.output_config.as_ref().unwrap().effort,
            Some(EffortLevel::High)
        );
    }

    #[test]
    fn test_output_config_with_json_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name", "age"]
        });
        let config = OutputConfig::with_json_schema(schema.clone());

        assert!(config.effort.is_none());
        assert!(config.format.is_some());
        let format = config.format.as_ref().unwrap();
        assert_eq!(format.format_type, OutputFormatType::JsonSchema);
        assert_eq!(format.json_schema.as_ref(), Some(&schema));
    }

    #[test]
    fn test_output_config_json_schema_serialization() {
        let schema = json!({"type": "object", "properties": {"x": {"type": "number"}}});
        let config = OutputConfig::with_json_schema(schema);
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();

        assert!(value.get("format").is_some());
        let format = &value["format"];
        assert_eq!(format["type"], "json_schema");
        assert!(format.get("schema").is_some());
    }

    #[test]
    fn test_output_config_effort_and_format_together() {
        let schema = json!({"type": "object"});
        let config = OutputConfig {
            effort: Some(EffortLevel::High),
            format: Some(OutputFormat::json_schema(schema)),
        };
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(value["effort"], "high");
        assert!(value.get("format").is_some());
    }

    #[test]
    fn test_thinking_type_supports_opus_4_5() {
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-6"));
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.5-20250514"));
        assert!(ThinkingType::Enabled.supports_model("claude-opus-4-6"));
    }

    #[test]
    fn test_thinking_type_supports_opus_4_6() {
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-20250214"));
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.6-20250214"));
        assert!(ThinkingType::Enabled.supports_model("claude-opus-4-20250214"));
    }

    #[test]
    fn test_thinking_type_supports_sonnet_4_5() {
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-6"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4.5-20250514"));
    }

    #[test]
    fn test_thinking_type_supports_sonnet_4_6() {
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-20250214"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4.6-20250214"));
    }

    #[test]
    fn test_thinking_type_unsupported_for_disabled() {
        assert!(!ThinkingType::Disabled.supports_model("claude-opus-4-6"));
        assert!(!ThinkingType::Disabled.supports_model("claude-sonnet-4-20250214"));
    }

    #[test]
    fn test_thinking_type_unsupported_for_old_models() {
        assert!(!ThinkingType::Adaptive.supports_model("claude-3-opus-20240229"));
        assert!(!ThinkingType::Adaptive.supports_model("claude-3-sonnet-20240229"));
        assert!(!ThinkingType::Enabled.supports_model("claude-3-haiku-20240307"));
    }

    #[test]
    fn test_thinking_type_case_insensitive() {
        assert!(ThinkingType::Adaptive.supports_model("CLAUDE-OPUS-4-20250514"));
        assert!(ThinkingType::Adaptive.supports_model("Claude-Sonnet-4-20250214"));
    }

    #[test]
    fn test_thinking_display_default() {
        assert_eq!(ThinkingDisplay::default(), ThinkingDisplay::Summarized);
    }

    #[test]
    fn test_sse_event_is_thinking() {
        let thinking_event = SSEEvent::ThinkingDelta {
            thinking: "reasoning...".to_string(),
        };
        assert!(thinking_event.is_thinking());

        let text_event = SSEEvent::Text {
            text: "Hello".to_string(),
        };
        assert!(!text_event.is_thinking());
    }

    #[test]
    fn test_sse_event_as_thinking() {
        let thinking_event = SSEEvent::ThinkingDelta {
            thinking: "my reasoning".to_string(),
        };
        assert_eq!(
            thinking_event.as_thinking(),
            Some("my reasoning".to_string())
        );

        let text_event = SSEEvent::Text {
            text: "Hello".to_string(),
        };
        assert_eq!(text_event.as_thinking(), None);
    }

    #[test]
    fn test_stream_chunk_alias_uses_stream_event() {
        let chunk: StreamChunk = Ok(rustycode_protocol::stream_event::StreamEvent::Done);
        assert!(matches!(
            chunk,
            Ok(rustycode_protocol::stream_event::StreamEvent::Done)
        ));
    }

    #[test]
    fn test_provider_config_debug_redacts_api_key() {
        use secrecy::SecretString;

        let config_with_key = ProviderConfig {
            api_key: Some(SecretString::new(
                "sk-ant-api03-secret-key".to_string().into(),
            )),
            base_url: Some("https://api.example.com".to_string()),
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };

        let debug_str = format!("{:?}", config_with_key);

        // Verify API key is redacted
        assert!(debug_str.contains("***REDACTED***"));
        assert!(!debug_str.contains("sk-ant-api03"));
        assert!(!debug_str.contains("secret-key"));

        // Verify other fields are present
        assert!(debug_str.contains("https://api.example.com"));
        assert!(debug_str.contains("120"));
    }

    #[test]
    fn test_provider_config_debug_with_none_api_key() {
        let config_without_key = ProviderConfig {
            api_key: None,
            base_url: None,
            timeout_seconds: Some(30),
            extra_headers: None,
            retry_config: None,
        };

        let debug_str = format!("{:?}", config_without_key);

        // Should still work when api_key is None
        assert!(debug_str.contains("ProviderConfig"));
        assert!(debug_str.contains("30"));
    }

    #[test]
    fn test_thinking_type_supports_opus_4_7() {
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-7"));
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.7-20260401"));
        assert!(ThinkingType::Enabled.supports_model("claude-opus-4-7"));
    }

    #[test]
    fn test_validate_thinking_rejects_enabled_on_opus_4_7() {
        let request = CompletionRequest::new("claude-opus-4-7".to_string(), vec![])
            .with_thinking_type(ThinkingType::Enabled);
        let result = request.validate_thinking();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("adaptive"));
    }

    #[test]
    fn test_validate_thinking_allows_adaptive_on_opus_4_7() {
        let request = CompletionRequest::new("claude-opus-4-7".to_string(), vec![])
            .with_thinking_type(ThinkingType::Adaptive);
        assert!(request.validate_thinking().is_ok());
    }

    // ─── Additional coverage: supports_model edge cases ────────────────

    #[test]
    fn test_supports_model_short_form_ids() {
        // Short-form IDs like "claude-opus-4-6" and "claude-sonnet-4-6"
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-6"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-6"));
        assert!(ThinkingType::Enabled.supports_model("claude-opus-4-6"));
        assert!(ThinkingType::Enabled.supports_model("claude-sonnet-4-6"));
    }

    #[test]
    fn test_supports_model_haiku_never_supports_thinking() {
        assert!(!ThinkingType::Adaptive.supports_model("claude-haiku-4-5"));
        assert!(!ThinkingType::Enabled.supports_model("claude-3-5-haiku-20241022"));
        assert!(!ThinkingType::Adaptive.supports_model("claude-haiku-4-6"));
    }

    #[test]
    fn test_supports_model_empty_and_garbage() {
        assert!(!ThinkingType::Adaptive.supports_model(""));
        assert!(!ThinkingType::Adaptive.supports_model("not-a-model"));
        assert!(!ThinkingType::Adaptive.supports_model("gpt-4"));
        assert!(!ThinkingType::Enabled.supports_model("random-string"));
    }

    #[test]
    fn test_supports_model_date_format_variants() {
        // Opus 4.5 date format
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-20250514"));
        // Opus 4.6 date format
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-20250214"));
        // Sonnet 4.5 date format
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-20250514"));
        // Sonnet 4.6 date format
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-20250214"));
    }

    #[test]
    fn test_supports_model_dotted_format() {
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.5-20250514"));
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.6-20250214"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4.5-20250514"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4.6-20250214"));
    }

    #[test]
    fn test_validate_thinking_enabled_ok_on_older_models() {
        // Opus 4.5 and 4.6 should accept Enabled
        let req = CompletionRequest::new("claude-opus-4.5-20250514".to_string(), vec![])
            .with_thinking_type(ThinkingType::Enabled);
        assert!(req.validate_thinking().is_ok());

        let req = CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![])
            .with_thinking_type(ThinkingType::Enabled);
        assert!(req.validate_thinking().is_ok());
    }

    #[test]
    fn test_validate_thinking_disabled_always_ok() {
        for model in &[
            "claude-opus-4-6",
            "claude-opus-4-7",
            "claude-sonnet-4-6",
            "claude-3-opus",
        ] {
            let req = CompletionRequest::new(model.to_string(), vec![])
                .with_thinking_type(ThinkingType::Disabled);
            assert!(
                req.validate_thinking().is_ok(),
                "Disabled should be ok for {}",
                model
            );
        }
    }

    #[test]
    fn test_validate_thinking_no_config_is_ok() {
        // No thinking config at all should be fine
        let req = CompletionRequest::new("claude-opus-4-7".to_string(), vec![]);
        assert!(req.validate_thinking().is_ok());
    }

    // --- validate_extra_headers tests ---

    #[test]
    fn test_validate_extra_headers_blocks_authorization() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("security header"));
    }

    #[test]
    fn test_validate_extra_headers_blocks_host() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Host".to_string(), "evil.com".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_extra_headers_blocks_crlf_injection() {
        let mut headers = std::collections::HashMap::new();
        headers.insert(
            "X-Custom".to_string(),
            "value\r\nInjected: true".to_string(),
        );
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("newline"));
    }

    #[test]
    fn test_validate_extra_headers_allows_valid() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Request-ID".to_string(), "abc123".to_string());
        headers.insert("X-Custom-Header".to_string(), "value".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_ok());
        let validated = result.unwrap();
        assert_eq!(validated.len(), 2);
    }

    #[test]
    fn test_validate_extra_headers_none_is_ok() {
        let result = validate_extra_headers(&None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_validate_extra_headers_empty_map() {
        let headers = std::collections::HashMap::new();
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_validate_extra_headers_blocks_invalid_name() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Invalid Name\n".to_string(), "value".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_extra_headers_case_insensitive_security() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("authorization".to_string(), "Bearer token".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
    }

    #[test]
    fn test_with_model_preserves_retry_delay() {
        let err = ProviderError::RateLimited {
            retry_delay: Some(std::time::Duration::from_secs(30)),
        };
        let tagged = err.with_model("claude-opus-4-7");
        assert!(tagged.is_rate_limited());
        assert_eq!(
            tagged.retry_delay(),
            Some(std::time::Duration::from_secs(30))
        );
    }

    #[test]
    fn test_with_model_preserves_top_up_url() {
        let err = ProviderError::CreditsExhausted {
            details: "out of credits".to_string(),
            top_up_url: Some("https://console.example.com/billing".to_string()),
        };
        let tagged = err.with_model("gpt-4o");
        assert!(tagged.is_credits_exhausted());
        assert_eq!(
            tagged.top_up_url(),
            Some("https://console.example.com/billing")
        );
        assert!(tagged.to_string().contains("[model: gpt-4o]"));
    }

    #[test]
    fn test_with_model_prefixes_string_variants() {
        let cases: Vec<(ProviderError, &str)> = vec![
            (ProviderError::Auth("bad key".into()), "bad key"),
            (ProviderError::Network("timeout".into()), "timeout"),
            (ProviderError::Api("overloaded".into()), "overloaded"),
            (ProviderError::Timeout("30s".into()), "30s"),
            (ProviderError::InvalidModel("foo".into()), "foo"),
        ];
        for (err, original_msg) in cases {
            let tagged = err.with_model("test-model");
            let display = tagged.to_string();
            assert!(
                display.contains("[model: test-model]"),
                "missing model prefix in: {display}"
            );
            assert!(
                display.contains(original_msg),
                "missing original message in: {display}"
            );
        }
    }

    #[test]
    fn test_usage_saturating_add_no_overflow() {
        let usage = Usage::new(u32::MAX, u32::MAX);
        assert_eq!(usage.total_tokens, u32::MAX);
    }

    #[test]
    fn test_usage_with_cache_saturating() {
        let usage = Usage::with_cache(u32::MAX, 100, 50, 25);
        // total = cache_read + cache_creation + input = MAX + 50 + 25 → saturates
        assert_eq!(usage.total_tokens, u32::MAX);
    }

    #[test]
    fn test_sanitize_error_message_redacts_bearer() {
        let msg = "Request failed: Authorization: Bearer sk-ant-api03-secret123";
        let sanitized = sanitize_error_message(msg);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("sk-ant-api03-secret123"));
    }

    #[test]
    fn test_sanitize_error_message_redacts_query_token() {
        let msg = "GET /api?token=abc123&key=secret failed";
        let sanitized = sanitize_error_message(msg);
        assert!(!sanitized.contains("abc123"));
        assert!(!sanitized.contains("secret"));
        assert!(sanitized.contains("token=[REDACTED]"));
        assert!(sanitized.contains("key=[REDACTED]"));
    }

    #[test]
    fn test_sanitize_error_message_redacts_x_api_key() {
        let msg = "x-api-key: sk-proj-abc123def456";
        let sanitized = sanitize_error_message(msg);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("sk-proj-abc123def456"));
    }

    #[test]
    fn test_sanitize_error_message_preserves_normal_text() {
        let msg = "Request to https://api.example.com/v1/models failed with timeout";
        let sanitized = sanitize_error_message(msg);
        assert_eq!(sanitized, msg);
    }

    #[test]
    fn test_sanitize_error_message_empty_input() {
        assert_eq!(sanitize_error_message(""), "");
    }

    // --- SkillRef / with_skills tests ---

    #[test]
    fn test_skill_ref_serialization() {
        let skill = SkillRef {
            skill_type: "anthropic".into(),
            skill_id: "pptx".into(),
            version: "latest".into(),
        };
        let json = serde_json::to_value(&skill).unwrap();
        assert_eq!(json["type"], "anthropic");
        assert_eq!(json["skill_id"], "pptx");
        assert_eq!(json["version"], "latest");
    }

    #[test]
    fn test_skill_ref_roundtrip() {
        let skill = SkillRef {
            skill_type: "anthropic".into(),
            skill_id: "xlsx".into(),
            version: "1.0".into(),
        };
        let serialized = serde_json::to_string(&skill).unwrap();
        let deserialized: SkillRef = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.skill_type, "anthropic");
        assert_eq!(deserialized.skill_id, "xlsx");
        assert_eq!(deserialized.version, "1.0");
    }

    #[test]
    fn test_with_skills_produces_container_json() {
        let request =
            CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]).with_skills(vec![
                SkillRef {
                    skill_type: "anthropic".into(),
                    skill_id: "pptx".into(),
                    version: "latest".into(),
                },
            ]);

        let container = request.container.unwrap();
        let skills = container["skills"].as_array().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0]["type"], "anthropic");
        assert_eq!(skills[0]["skill_id"], "pptx");
        assert_eq!(skills[0]["version"], "latest");
    }

    #[test]
    fn test_with_skills_multiple_skills() {
        let request =
            CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]).with_skills(vec![
                SkillRef {
                    skill_type: "anthropic".into(),
                    skill_id: "pptx".into(),
                    version: "1.0".into(),
                },
                SkillRef {
                    skill_type: "anthropic".into(),
                    skill_id: "xlsx".into(),
                    version: "2.0".into(),
                },
            ]);

        let container = request.container.unwrap();
        let skills = container["skills"].as_array().unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0]["skill_id"], "pptx");
        assert_eq!(skills[1]["skill_id"], "xlsx");
    }

    #[test]
    fn test_container_none_by_default() {
        let request = CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]);
        assert!(request.container.is_none());
    }

    #[test]
    fn test_container_omitted_from_serialization_when_none() {
        let request = CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]);
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("container").is_none());
    }

    #[test]
    fn test_container_present_in_serialization_when_set() {
        let request =
            CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]).with_skills(vec![
                SkillRef {
                    skill_type: "anthropic".into(),
                    skill_id: "pptx".into(),
                    version: "latest".into(),
                },
            ]);
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("container").is_some());
        assert!(json["container"]["skills"].is_array());
    }
}

/// Supported LLM provider types
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderType {
    Ollama,
    OpenAI,
    Anthropic,
    Gemini,      // Google Gemini
    Copilot,     // GitHub Copilot
    Bedrock,     // AWS Bedrock
    Azure,       // Azure OpenAI
    Cohere,      // Cohere
    Mistral,     // Mistral AI
    Together,    // Together AI
    Perplexity,  // Perplexity AI
    HuggingFace, // Hugging Face Inference API
    OpenRouter,  // OpenRouter
    Nvidia,      // NVIDIA NIM API
    Custom,      // OpenAI-compatible custom provider
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAI => write!(f, "openai"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::Gemini => write!(f, "gemini"),
            Self::Copilot => write!(f, "copilot"),
            Self::Bedrock => write!(f, "bedrock"),
            Self::Azure => write!(f, "azure"),
            Self::Cohere => write!(f, "cohere"),
            Self::Mistral => write!(f, "mistral"),
            Self::Together => write!(f, "together"),
            Self::Perplexity => write!(f, "perplexity"),
            Self::HuggingFace => write!(f, "huggingface"),
            Self::OpenRouter => write!(f, "openrouter"),
            Self::Nvidia => write!(f, "nvidia"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

pub fn validate_endpoint(endpoint: &str) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(endpoint)?;

    match url.scheme() {
        "https" => {}
        "http" if matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1")) => {}
        scheme => anyhow::bail!("unsupported endpoint scheme: {}", scheme),
    }

    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("endpoint must not embed credentials");
    }

    if url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!("endpoint must not include query strings or fragments");
    }

    Ok(())
}

pub fn sanitize_error_message(message: &str) -> String {
    static QUERY_SECRET_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static BEARER_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static API_KEY_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

    let query_secret_re = QUERY_SECRET_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)([?&](?:key|api[-_]?key|token|access_token)=)[^&\s]+")
            .expect("valid regex")
    });
    let bearer_re = BEARER_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)(bearer\s+)[A-Za-z0-9._~-]+").expect("valid regex")
    });
    let api_key_re = API_KEY_RE
        .get_or_init(|| regex::Regex::new(r"(?i)(x-api-key[:=]\s*)[^\s,;]+").expect("valid regex"));

    let redacted = query_secret_re.replace_all(message, "$1[REDACTED]");
    let redacted = bearer_re.replace_all(&redacted, "$1[REDACTED]");
    api_key_re
        .replace_all(&redacted, "$1[REDACTED]")
        .into_owned()
}
