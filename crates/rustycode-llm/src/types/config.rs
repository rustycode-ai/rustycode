use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

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
