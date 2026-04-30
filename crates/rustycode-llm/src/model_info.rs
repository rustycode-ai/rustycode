//! Model Capability Metadata
//!
//! Provides structured metadata about LLM models including context limits,
//! token costs, and capability flags. Used for provider-agnostic model
//! selection, cost estimation, and capability checking.
//!
//! Inspired by goose's `ModelInfo` and `ProviderMetadata` in `providers/base.rs`.
//!
//! # Example
//!
//! ```
//! use rustycode_llm::model_info::{ModelInfo, ModelCapabilities};
//!
//! let info = ModelInfo::new("gpt-4o", 128_000);
//! assert_eq!(info.context_limit(), 128_000);
//! assert!(!info.is_reasoning_model());
//!
//! let reasoning = ModelInfo::new("o3-mini", 200_000).with_reasoning(true);
//! assert!(reasoning.is_reasoning_model());
//! ```

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default context limit when model is unknown.
pub const DEFAULT_CONTEXT_LIMIT: usize = 128_000;

/// Default max output tokens when not specified.
pub const DEFAULT_MAX_OUTPUT_TOKENS: usize = 4_096;

/// Reasoning model name prefixes.
const REASONING_PREFIXES: &[&str] = &["o1", "o2", "o3", "o4", "gpt-5"];

/// Information about a specific model's capabilities and costs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    /// The model identifier (e.g., "gpt-4o", "claude-sonnet-4-20250514")
    pub name: String,
    /// Maximum context window in tokens
    pub context_limit: usize,
    /// Maximum output tokens (if known)
    pub max_output_tokens: Option<usize>,
    /// Cost per million input tokens in USD
    pub input_cost_per_mtok: Option<f64>,
    /// Cost per million output tokens in USD
    pub output_cost_per_mtok: Option<f64>,
    /// Model capabilities
    pub capabilities: ModelCapabilities,
}

/// Model capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelCapabilities {
    /// Whether this is a reasoning model (o1/o3/o4/gpt-5)
    pub reasoning: bool,
    /// Whether the model supports tool/function calling
    pub tool_calling: bool,
    /// Whether the model supports streaming
    pub streaming: bool,
    /// Whether the model supports vision/image inputs
    pub vision: bool,
    /// Whether the model supports cache control (Anthropic)
    pub cache_control: bool,
    /// Whether the model supports JSON mode
    pub json_mode: bool,
}

impl ModelCapabilities {
    /// All capabilities enabled.
    pub fn all() -> Self {
        Self {
            reasoning: false,
            tool_calling: true,
            streaming: true,
            vision: true,
            cache_control: true,
            json_mode: true,
        }
    }

    /// Basic capabilities (no vision, no cache control).
    pub fn basic() -> Self {
        Self {
            reasoning: false,
            tool_calling: true,
            streaming: true,
            vision: false,
            cache_control: false,
            json_mode: true,
        }
    }

    /// Reasoning model capabilities.
    pub fn reasoning() -> Self {
        Self {
            reasoning: true,
            tool_calling: true,
            streaming: true,
            vision: false,
            cache_control: false,
            json_mode: true,
        }
    }
}

impl ModelInfo {
    /// Create a new ModelInfo with just name and context limit.
    pub fn new(name: impl Into<String>, context_limit: usize) -> Self {
        Self {
            name: name.into(),
            context_limit,
            max_output_tokens: None,
            input_cost_per_mtok: None,
            output_cost_per_mtok: None,
            capabilities: ModelCapabilities::default(),
        }
    }

    /// Create with cost information (per million tokens).
    pub fn with_cost(
        name: impl Into<String>,
        context_limit: usize,
        input_cost: f64,
        output_cost: f64,
    ) -> Self {
        Self {
            name: name.into(),
            context_limit,
            max_output_tokens: None,
            input_cost_per_mtok: Some(input_cost),
            output_cost_per_mtok: Some(output_cost),
            capabilities: ModelCapabilities::default(),
        }
    }

    /// Set max output tokens.
    pub fn max_output(mut self, tokens: usize) -> Self {
        self.max_output_tokens = Some(tokens);
        self
    }

    /// Set reasoning capability.
    pub fn with_reasoning(mut self, enabled: bool) -> Self {
        self.capabilities.reasoning = enabled;
        self
    }

    /// Set tool calling capability.
    pub fn with_tool_calling(mut self, enabled: bool) -> Self {
        self.capabilities.tool_calling = enabled;
        self
    }

    /// Set vision capability.
    pub fn with_vision(mut self, enabled: bool) -> Self {
        self.capabilities.vision = enabled;
        self
    }

    /// Set streaming capability.
    pub fn with_streaming(mut self, enabled: bool) -> Self {
        self.capabilities.streaming = enabled;
        self
    }

    /// Set cache control capability.
    pub fn with_cache_control(mut self, enabled: bool) -> Self {
        self.capabilities.cache_control = enabled;
        self
    }

    /// Set JSON mode capability.
    pub fn with_json_mode(mut self, enabled: bool) -> Self {
        self.capabilities.json_mode = enabled;
        self
    }

    /// Set all capabilities at once.
    pub fn with_capabilities(mut self, caps: ModelCapabilities) -> Self {
        self.capabilities = caps;
        self
    }

    /// Get the effective context limit.
    pub fn context_limit(&self) -> usize {
        self.context_limit
    }

    /// Get the effective max output tokens.
    pub fn max_output_tokens(&self) -> usize {
        self.max_output_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS)
    }

    /// Check if this is a reasoning model.
    pub fn is_reasoning_model(&self) -> bool {
        self.capabilities.reasoning || is_reasoning_model_name(&self.name)
    }

    /// Check if the model supports tool calling.
    pub fn supports_tool_calling(&self) -> bool {
        self.capabilities.tool_calling
    }

    /// Check if the model supports vision.
    pub fn supports_vision(&self) -> bool {
        self.capabilities.vision
    }

    /// Estimate cost for a given number of input/output tokens.
    ///
    /// Returns cost in USD.
    pub fn estimate_cost(&self, input_tokens: usize, output_tokens: usize) -> f64 {
        let input_cost = self
            .input_cost_per_mtok
            .map(|c| (input_tokens as f64 / 1_000_000.0) * c)
            .unwrap_or(0.0);
        let output_cost = self
            .output_cost_per_mtok
            .map(|c| (output_tokens as f64 / 1_000_000.0) * c)
            .unwrap_or(0.0);
        input_cost + output_cost
    }

    /// How many context tokens are available for content after reserving
    /// space for the output.
    pub fn available_for_content(&self) -> usize {
        self.context_limit.saturating_sub(self.max_output_tokens())
    }
}

/// Check if a model name indicates a reasoning model (o1/o2/o3/o4/gpt-5).
///
/// Also handles prefixed names like "databricks-o3-mini".
pub fn is_reasoning_model_name(name: &str) -> bool {
    // Strip common prefixes
    let prefixes = &["goose-", "databricks-"];
    let base = prefixes
        .iter()
        .find_map(|p| name.strip_prefix(p))
        .unwrap_or(name);

    REASONING_PREFIXES.iter().any(|p| base.starts_with(p))
}

// ── Known Model Registry ──────────────────────────────────────────────────
// Pre-configured metadata for common models so callers don't need to look
// it up from provider APIs. Inspired by goose's `canonical.rs`.

static KNOWN_MODELS: Lazy<HashMap<&'static str, ModelInfo>> = Lazy::new(|| {
    let mut m = HashMap::new();

    // OpenAI models - GPT-5.x (latest)
    m.insert(
        "gpt-5.2",
        ModelInfo::with_cost("gpt-5.2", 128_000, 12.0, 48.0)
            .max_output(32_768)
            .with_capabilities(ModelCapabilities::all()),
    );
    m.insert(
        "gpt-5.1",
        ModelInfo::with_cost("gpt-5.1", 128_000, 5.0, 20.0)
            .max_output(32_768)
            .with_capabilities(ModelCapabilities::all()),
    );
    m.insert(
        "gpt-5-pro",
        ModelInfo::with_cost("gpt-5-pro", 128_000, 8.0, 32.0)
            .max_output(32_768)
            .with_capabilities(ModelCapabilities::all()),
    );

    // OpenAI models - GPT-4.x
    m.insert(
        "gpt-4.1",
        ModelInfo::with_cost("gpt-4.1", 128_000, 2.0, 8.0)
            .max_output(32_768)
            .with_capabilities(ModelCapabilities::all()),
    );
    m.insert(
        "gpt-4.1-mini",
        ModelInfo::with_cost("gpt-4.1-mini", 128_000, 0.40, 1.60)
            .max_output(32_768)
            .with_capabilities(ModelCapabilities::all()),
    );
    m.insert(
        "gpt-4.1-nano",
        ModelInfo::with_cost("gpt-4.1-nano", 128_000, 0.10, 0.40)
            .max_output(32_768)
            .with_capabilities(ModelCapabilities::all()),
    );
    m.insert(
        "gpt-4o",
        ModelInfo::with_cost("gpt-4o", 128_000, 2.50, 10.0)
            .max_output(16_384)
            .with_capabilities(ModelCapabilities::all()),
    );
    m.insert(
        "gpt-4o-mini",
        ModelInfo::with_cost("gpt-4o-mini", 128_000, 0.15, 0.60)
            .max_output(16_384)
            .with_capabilities(ModelCapabilities::all()),
    );
    m.insert(
        "gpt-4-turbo",
        ModelInfo::with_cost("gpt-4-turbo", 128_000, 10.0, 30.0)
            .max_output(4_096)
            .with_capabilities(ModelCapabilities::all()),
    );

    // OpenAI models - o-series (reasoning)
    m.insert(
        "o4-mini",
        ModelInfo::with_cost("o4-mini", 200_000, 1.10, 4.40)
            .max_output(100_000)
            .with_capabilities(ModelCapabilities::reasoning()),
    );
    m.insert(
        "o3",
        ModelInfo::with_cost("o3", 200_000, 10.0, 40.0)
            .max_output(100_000)
            .with_capabilities(ModelCapabilities::reasoning()),
    );
    m.insert(
        "o3-mini",
        ModelInfo::with_cost("o3-mini", 200_000, 1.10, 4.40)
            .max_output(100_000)
            .with_capabilities(ModelCapabilities::reasoning()),
    );
    m.insert(
        "o1",
        ModelInfo::with_cost("o1", 200_000, 15.0, 60.0)
            .max_output(100_000)
            .with_capabilities(ModelCapabilities::reasoning()),
    );
    m.insert(
        "o1-mini",
        ModelInfo::with_cost("o1-mini", 128_000, 3.0, 12.0)
            .max_output(65_536)
            .with_capabilities(ModelCapabilities::reasoning()),
    );

    // Anthropic models - Claude 4.6 (latest)
    m.insert(
        "claude-sonnet-4-6",
        ModelInfo::with_cost("claude-sonnet-4-6", 200_000, 3.0, 15.0)
            .max_output(16_384)
            .with_cache_control(true)
            .with_tool_calling(true)
            .with_streaming(true)
            .with_vision(true)
            .with_json_mode(true),
    );
    m.insert(
        "claude-opus-4-6",
        ModelInfo::with_cost("claude-opus-4-6", 200_000, 15.0, 75.0)
            .max_output(32_000)
            .with_cache_control(true)
            .with_tool_calling(true)
            .with_streaming(true)
            .with_vision(true)
            .with_json_mode(true),
    );

    // Anthropic models - Claude 4.5
    m.insert(
        "claude-sonnet-4-6",
        ModelInfo::with_cost("claude-sonnet-4-6", 200_000, 3.0, 15.0)
            .max_output(16_384)
            .with_cache_control(true)
            .with_tool_calling(true)
            .with_streaming(true)
            .with_vision(true)
            .with_json_mode(true),
    );
    m.insert(
        "claude-opus-4-6",
        ModelInfo::with_cost("claude-opus-4-6", 200_000, 15.0, 75.0)
            .max_output(32_000)
            .with_cache_control(true)
            .with_tool_calling(true)
            .with_streaming(true)
            .with_vision(true)
            .with_json_mode(true),
    );

    // Anthropic models - Claude Haiku
    m.insert(
        "claude-haiku-4-5-20251001",
        ModelInfo::with_cost("claude-haiku-4-5-20251001", 200_000, 0.80, 4.0)
            .max_output(8_192)
            .with_cache_control(true)
            .with_tool_calling(true)
            .with_streaming(true)
            .with_vision(true)
            .with_json_mode(true),
    );

    // Google models
    m.insert(
        "gemini-2.5-pro",
        ModelInfo::with_cost("gemini-2.5-pro", 1_048_576, 1.25, 10.0)
            .max_output(65_536)
            .with_capabilities(ModelCapabilities::all()),
    );
    m.insert(
        "gemini-2.5-flash",
        ModelInfo::with_cost("gemini-2.5-flash", 1_048_576, 0.15, 0.60)
            .max_output(65_536)
            .with_capabilities(ModelCapabilities::all()),
    );
    m.insert(
        "gemini-2.0-flash",
        ModelInfo::with_cost("gemini-2.0-flash", 1_048_576, 0.10, 0.40)
            .max_output(8_192)
            .with_capabilities(ModelCapabilities::all()),
    );

    // Ollama / local models
    m.insert(
        "llama3.1:8b",
        ModelInfo::new("llama3.1:8b", 128_000)
            .max_output(4_096)
            .with_capabilities(ModelCapabilities::basic()),
    );
    m.insert(
        "qwen2.5-coder:7b",
        ModelInfo::new("qwen2.5-coder:7b", 131_072)
            .max_output(8_192)
            .with_capabilities(ModelCapabilities::basic()),
    );
    m.insert(
        "deepseek-coder-v2:16b",
        ModelInfo::new("deepseek-coder-v2:16b", 128_000)
            .max_output(4_096)
            .with_capabilities(ModelCapabilities::basic()),
    );

    m
});

/// Well-known model registry with pre-configured metadata.
pub struct KnownModels;

impl KnownModels {
    /// Get info for a known model, or a sensible default.
    pub fn get(model_name: &str) -> ModelInfo {
        KNOWN_MODELS.get(model_name).cloned().unwrap_or_else(|| {
            ModelInfo::new(model_name, DEFAULT_CONTEXT_LIMIT)
                .with_capabilities(ModelCapabilities::all())
        })
    }

    /// Check if a model is in the known registry.
    pub fn contains(model_name: &str) -> bool {
        KNOWN_MODELS.contains_key(model_name)
    }

    /// Get all known model names.
    pub fn all_names() -> Vec<&'static str> {
        let mut names: Vec<&'static str> = KNOWN_MODELS.keys().copied().collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info_new() {
        let info = ModelInfo::new("test-model", 100_000);
        assert_eq!(info.name, "test-model");
        assert_eq!(info.context_limit(), 100_000);
        assert_eq!(info.max_output_tokens(), DEFAULT_MAX_OUTPUT_TOKENS);
    }

    #[test]
    fn test_model_info_builder() {
        let info = ModelInfo::new("test", 50_000)
            .max_output(8_192)
            .with_reasoning(true)
            .with_vision(true)
            .with_tool_calling(true);

        assert_eq!(info.max_output_tokens(), 8_192);
        assert!(info.is_reasoning_model());
        assert!(info.supports_vision());
        assert!(info.supports_tool_calling());
    }

    #[test]
    fn test_model_info_with_cost() {
        let info = ModelInfo::with_cost("gpt-4o", 128_000, 2.50, 10.0);
        assert_eq!(info.input_cost_per_mtok, Some(2.50));
        assert_eq!(info.output_cost_per_mtok, Some(10.0));
    }

    #[test]
    fn test_estimate_cost() {
        let info = ModelInfo::with_cost("test", 100_000, 1.0, 2.0);
        let cost = info.estimate_cost(1_000_000, 500_000);
        assert!((cost - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_estimate_cost_no_pricing() {
        let info = ModelInfo::new("free-model", 100_000);
        assert_eq!(info.estimate_cost(1_000_000, 1_000_000), 0.0);
    }

    #[test]
    fn test_available_for_content() {
        let info = ModelInfo::new("test", 128_000).max_output(16_384);
        assert_eq!(info.available_for_content(), 111_616);
    }

    #[test]
    fn test_is_reasoning_model_name() {
        assert!(is_reasoning_model_name("o1"));
        assert!(is_reasoning_model_name("o1-preview"));
        assert!(is_reasoning_model_name("o3-mini"));
        assert!(is_reasoning_model_name("o4-mini"));
        assert!(is_reasoning_model_name("gpt-5"));
        assert!(is_reasoning_model_name("gpt-5-turbo"));
        assert!(is_reasoning_model_name("databricks-o3-mini"));
        assert!(is_reasoning_model_name("goose-o4-mini"));
        assert!(!is_reasoning_model_name("gpt-4o"));
        assert!(!is_reasoning_model_name("claude-sonnet-4-6"));
    }

    #[test]
    fn test_model_info_is_reasoning() {
        let reasoning = ModelInfo::new("o3-mini", 200_000).with_reasoning(true);
        assert!(reasoning.is_reasoning_model());

        let not_reasoning = ModelInfo::new("gpt-4o", 128_000);
        assert!(!not_reasoning.is_reasoning_model());

        // Name-based detection even without flag
        let by_name = ModelInfo::new("o1-preview", 128_000);
        assert!(by_name.is_reasoning_model());
    }

    #[test]
    fn test_known_models_get() {
        let gpt4o = KnownModels::get("gpt-4o");
        assert_eq!(gpt4o.name, "gpt-4o");
        assert_eq!(gpt4o.context_limit, 128_000);
        assert!(gpt4o.supports_tool_calling());

        let claude = KnownModels::get("claude-sonnet-4-6");
        assert_eq!(claude.context_limit, 200_000);
    }

    #[test]
    fn test_known_models_unknown() {
        let unknown = KnownModels::get("future-model-3000");
        assert_eq!(unknown.name, "future-model-3000");
        assert_eq!(unknown.context_limit, DEFAULT_CONTEXT_LIMIT);
    }

    #[test]
    fn test_known_models_contains() {
        assert!(KnownModels::contains("gpt-4o"));
        assert!(KnownModels::contains("claude-sonnet-4-6"));
        assert!(!KnownModels::contains("nonexistent-model"));
    }

    #[test]
    fn test_known_models_all_names() {
        let names = KnownModels::all_names();
        assert!(names.contains(&"gpt-4o"));
        assert!(names.contains(&"claude-sonnet-4-6"));
        assert!(names.contains(&"o3-mini"));

        // Verify sorted
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn test_model_info_serialize_deserialize() {
        let info = ModelInfo::new("test", 100_000)
            .max_output(4_096)
            .with_vision(true);

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, deserialized);
    }
}
