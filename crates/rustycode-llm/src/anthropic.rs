//! Anthropic Claude LLM provider implementation.
use crate::advisor::{AdvisorConfig, AdvisorTool};
use crate::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, MessageRole, ProviderConfig,
    ProviderError, StreamChunk, Usage,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::retry::extract_retry_after_ms;
use crate::tool_annotations::anthropic_annotations_for_tool_info;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use rustycode_protocol::llm::{
    CompletionRequest as UnifiedCompletionRequest, CompletionResponse as UnifiedCompletionResponse,
    Cost as UnifiedCost, LLMProvider as UnifiedLLMProvider, ModelInfo as UnifiedModelInfo,
    TokenCount as UnifiedTokenCount,
};
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_tools_api::{Tool, ToolProfile, ToolRegistry, ToolSelector};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    /// Thinking configuration (Opus 4.5+, Sonnet 4.5+)
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<serde_json::Value>,
    /// Output configuration for structured outputs and effort control
    #[serde(skip_serializing_if = "Option::is_none")]
    output_config: Option<crate::provider::OutputConfig>,
    /// Skills container for Anthropic Agent Skills API
    #[serde(skip_serializing_if = "Option::is_none")]
    container: Option<serde_json::Value>,
    /// Tool choice: controls how the model selects tools.
    /// None = auto, {"type": "any"} = force tool use, {"type": "tool", "name": "..."} = specific tool
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    /// Enable parallel tool calls — model may emit multiple tool_use blocks per turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    usage: AnthropicUsage,
    model: String,
    // Ignore extra fields that z.ai or other proxies might add
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    response_type: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    role: Option<String>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // Kept for future use
    stop_sequence: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    text: String,
    #[allow(dead_code)] // Kept for future use
    #[serde(default)]
    id: String,
    #[allow(dead_code)] // Kept for future use
    #[serde(default)]
    name: String,
    #[allow(dead_code)] // Kept for future use
    #[serde(default)]
    input: serde_json::Value,
    /// Citations returned by Anthropic API (web search results, etc.)
    #[serde(default)]
    citations: Option<Vec<AnthropicCitation>>,
    /// Thinking content from extended thinking blocks
    #[serde(default)]
    thinking: String,
    /// Signature for extended thinking blocks (encrypted, for round-tripping)
    #[serde(default)]
    #[allow(dead_code)]
    signature: String,
    /// Encrypted data for redacted_thinking blocks (for round-tripping)
    #[serde(default)]
    data: String,
}

/// Citation within an Anthropic content block.
#[derive(Deserialize, Debug)]
#[allow(dead_code)] // Kept for future use
struct AnthropicCitation {
    #[serde(rename = "type")]
    citation_type: String,
    #[serde(default)]
    cited_text: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    search_result_index: Option<u32>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: usize,
    output_tokens: usize,

    /// Cache-aware token tracking (from Anthropic API)
    /// Tokens read from cache (present when prompt caching is enabled)
    #[serde(default)]
    cache_read_input_tokens: usize,

    /// Tokens written to cache (present when prompt caching is enabled)
    #[serde(default)]
    cache_creation_input_tokens: usize,
}

#[derive(Serialize)]
pub(crate) struct AnthropicMessage {
    pub(crate) role: &'static str,
    pub(crate) content: AnthropicRequestContent,
}

#[derive(Serialize, Debug)]
#[serde(untagged)]
#[allow(dead_code)] // Kept for future use
pub(crate) enum AnthropicRequestContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Content for tool_result blocks: either a plain string or an array of typed blocks.
#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub(crate) enum ToolResultContent {
    Text(String),
    Blocks(Vec<ToolResultBlock>),
}

impl PartialEq<&str> for ToolResultContent {
    fn eq(&self, other: &&str) -> bool {
        match self {
            Self::Text(s) => s == *other,
            _ => false,
        }
    }
}

impl PartialEq<str> for ToolResultContent {
    fn eq(&self, other: &str) -> bool {
        match self {
            Self::Text(s) => s == other,
            _ => false,
        }
    }
}

impl PartialEq<String> for ToolResultContent {
    fn eq(&self, other: &String) -> bool {
        match self {
            Self::Text(s) => s == other,
            _ => false,
        }
    }
}

/// A single block inside a tool_result content array.
#[derive(Serialize, Clone, Debug)]
pub(crate) struct ToolResultBlock {
    #[serde(rename = "type")]
    pub(crate) block_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
pub(crate) enum ContentBlock {
    Text {
        #[serde(rename = "type")]
        content_type: &'static str,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Image {
        #[serde(rename = "type")]
        content_type: &'static str,
        source: ImageSource,
    },
    ToolUse {
        #[serde(rename = "type")]
        content_type: &'static str,
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolResult {
        #[serde(rename = "type")]
        content_type: &'static str,
        tool_use_id: String,
        content: ToolResultContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    SearchResult {
        #[serde(rename = "type")]
        content_type: &'static str,
        source: String,
        title: String,
        content: Vec<SearchResultContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<CitationMetadata>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Thinking {
        #[serde(rename = "type")]
        content_type: &'static str,
        thinking: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        signature: String,
    },
    RedactedThinking {
        #[serde(rename = "type")]
        content_type: &'static str,
        #[serde(skip_serializing_if = "String::is_empty")]
        data: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct ImageSource {
    #[serde(rename = "type")]
    pub(crate) source_type: String,
    pub(crate) media_type: String,
    data: String,
}

/// Content block for search results (RAG applications)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResultBlock {
    #[serde(rename = "type")]
    pub content_type: &'static str,
    pub source: String,
    pub title: String,
    pub content: Vec<SearchResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Text block within search result content
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResultContent {
    #[serde(rename = "type")]
    pub content_type: &'static str,
    pub text: String,
}

/// Citation configuration for search results
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CitationMetadata {
    pub enabled: bool,
}

/// Cache control for search results
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub cache_type: &'static str,
}

/// System prompt content block with optional cache control.
#[derive(Serialize, Clone, Debug)]
struct SystemContentBlock {
    #[serde(rename = "type")]
    block_type: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

/// System prompt: either a plain string or an array of content blocks.
#[derive(Serialize, Clone, Debug)]
#[serde(untagged)]
enum SystemPrompt {
    #[allow(dead_code)] // Used when system prompt doesn't need cache_control
    Text(String),
    Blocks(Vec<SystemContentBlock>),
}

/// Anthropic Claude LLM provider
pub struct AnthropicProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    model: String,
    tool_registry: Arc<ToolRegistry>,
    tool_selector: ToolSelector,
    /// Optional advisor tool configuration for executor+advisor pattern
    advisor_config: Option<AdvisorConfig>,
}

/// Check if a model ID is Opus 4.7 or later (where thinking.type="enabled" is removed).
fn is_opus_47_or_later(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("opus-4-7")
        || m.contains("opus-4.7")
        || m.contains("opus-4-8")
        || m.contains("opus-4.8")
        || m.contains("opus-5")
}

/// Normalize thinking config for the target model.
///
/// Opus 4.7+ removed `thinking.type="enabled"` — only `"adaptive"` is accepted.
/// This function auto-downgrades `Enabled → Adaptive` for incompatible models.
///
/// When effort is Xhigh or Max on a thinking-capable model and no thinking config
/// is provided, adaptive thinking is auto-enabled.
fn normalize_thinking_for_model(
    thinking: Option<crate::provider::ThinkingConfig>,
    effort: Option<crate::provider::EffortLevel>,
    model: &str,
) -> Option<serde_json::Value> {
    use crate::provider::ThinkingType;

    let thinking = match thinking {
        Some(mut t) => {
            if matches!(t.thinking_type, ThinkingType::Enabled) && is_opus_47_or_later(model) {
                tracing::warn!(
                    "Model {model} does not support thinking.type=enabled, auto-downgrading to adaptive"
                );
                t.thinking_type = ThinkingType::Adaptive;
            }
            Some(t)
        }
        None => {
            let should_auto_enable = matches!(
                effort,
                Some(
                    crate::provider::EffortLevel::Xhigh | crate::provider::EffortLevel::Max
                )
            ) && ThinkingType::Adaptive.supports_model(model);

            if should_auto_enable {
                tracing::debug!(
                    "Auto-enabling adaptive thinking for {model} with {:?} effort",
                    effort
                );
                Some(crate::provider::ThinkingConfig::adaptive())
            } else {
                None
            }
        }
    };

    thinking.map(|t| serde_json::to_value(t).unwrap_or_default())
}

/// Apply `cache_control: { type: "ephemeral" }` to the last N messages' content blocks.
///
/// Anthropic allows up to 4 cache breakpoints per request. This function targets
/// the last `count` messages, adding cache_control to their final content block.
/// Matches the opencode `cc()` pattern for optimal prefix caching.
fn apply_cache_to_last_messages(messages: &mut [AnthropicMessage], count: usize) {
    let cc = CacheControl {
        cache_type: "ephemeral",
    };

    let start = messages.len().saturating_sub(count);
    for msg in &mut messages[start..] {
        match &mut msg.content {
            AnthropicRequestContent::Blocks(blocks) => {
                if let Some(last_block) = blocks.last_mut() {
                    match last_block {
                        ContentBlock::Text { cache_control, .. } => {
                            *cache_control = Some(cc.clone());
                        }
                        ContentBlock::ToolUse { cache_control, .. } => {
                            *cache_control = Some(cc.clone());
                        }
                        ContentBlock::ToolResult { cache_control, .. } => {
                            *cache_control = Some(cc.clone());
                        }
                        ContentBlock::SearchResult { cache_control, .. } => {
                            *cache_control = Some(cc.clone());
                        }
                        _ => {}
                    }
                }
            }
            AnthropicRequestContent::Text(_) => {
                // Convert simple text to a cached content block
                let text = match &msg.content {
                    AnthropicRequestContent::Text(t) => t.clone(),
                    _ => unreachable!(),
                };
                msg.content = AnthropicRequestContent::Blocks(vec![ContentBlock::Text {
                    content_type: "text",
                    text,
                    cache_control: Some(cc.clone()),
                }]);
            }
        }
    }
}

impl AnthropicProvider {
    /// Internal implementation of complete without retry logic
    pub async fn complete_internal(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let model = &request.model;
        let url = self.endpoint();
        tracing::info!("Anthropic API request to: {} (model: {})", url, model);

        // Convert ChatMessage to AnthropicMessage
        let parsed_messages = self.parse_conversation_messages(&request.messages);

        // Filter out empty or invalid messages that could cause error 1214
        let mut messages: Vec<AnthropicMessage> = parsed_messages
            .into_iter()
            .filter(|msg| {
                // Filter out messages with empty content
                match &msg.content {
                    AnthropicRequestContent::Text(text) => !text.trim().is_empty(),
                    AnthropicRequestContent::Blocks(blocks) => !blocks.is_empty(),
                }
            })
            .collect();

        // Ensure we have at least one valid message
        if messages.is_empty() {
            return Err(ProviderError::Api(
                "No valid messages to send to Anthropic API after filtering".to_string(),
            ));
        }

        // Use intelligent tool selection if tools not explicitly provided
        let mut tools = match request.tools {
            Some(tools) => tools, // Use explicitly provided tools
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
                    .unwrap_or_default()
            }
        };

        // Inject advisor tool if configured via builder or RUSTYCODE_ADVISOR_MODEL env var
        let advisor_tool = self
            .advisor_config
            .as_ref()
            .map(|c| c.advisor.to_anthropic_tool())
            .or_else(|| {
                std::env::var("RUSTYCODE_ADVISOR_MODEL").ok().map(|model| {
                    let advisor = AdvisorTool::new(model);
                    tracing::info!(
                        "Advisor tool auto-enabled via RUSTYCODE_ADVISOR_MODEL={}",
                        advisor.advisor_model
                    );
                    advisor.to_anthropic_tool()
                })
            });

        if let Some(tool) = &advisor_tool {
            tools.push(tool.clone());
        }

        let has_advisor = advisor_tool.is_some();

        let wants_structured_output = request
            .output_config
            .as_ref()
            .and_then(|c| c.format.as_ref())
            .is_some_and(|f| {
                matches!(f.format_type, crate::provider::OutputFormatType::JsonSchema)
            });

        // Enable prompt caching for all Anthropic-compatible endpoints.
        // Most proxies (OpenRouter, z.ai, etc.) support cache_control correctly.
        let use_cache_control = true;

        // Apply cache_control to last 2 conversation messages.
        // Combined with system prompt (1) and tools (1), this uses all 4 allowed breakpoints.
        if use_cache_control {
            apply_cache_to_last_messages(&mut messages, 2);
        }

        let system = request.system_prompt.map(|text| {
            if use_cache_control {
                SystemPrompt::Blocks(vec![SystemContentBlock {
                    block_type: "text",
                    text,
                    cache_control: Some(CacheControl {
                        cache_type: "ephemeral",
                    }),
                }])
            } else {
                SystemPrompt::Text(text)
            }
        });

        let tools = if !tools.is_empty() {
            if use_cache_control {
                if let Some(last_tool) = tools.last_mut().and_then(|t| t.as_object_mut()) {
                    last_tool.insert(
                        "cache_control".to_string(),
                        serde_json::json!({"type": "ephemeral"}),
                    );
                }
            }
            Some(tools)
        } else {
            None
        };

        let anthropic_request = AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            system,
            stream: Some(false),
            tools,
            thinking: normalize_thinking_for_model(
                request.thinking,
                request.output_config.as_ref().and_then(|c| c.effort),
                &request.model,
            ),
            output_config: request.output_config,
            container: request.container.clone(),
            tool_choice: request.tool_choice.as_ref().map(|tc| match tc.as_str() {
                Some("required") => serde_json::json!({"type": "any"}),
                Some("auto") => serde_json::json!({"type": "auto"}),
                Some("none") => serde_json::json!({"type": "none"}),
                _ => tc.clone(),
            }),
            parallel_tool_calls: request.parallel_tool_calls,
        };

        tracing::info!("Sending request with model: {}", request.model);

        // Dump raw request/response to files for debugging
        let http_trace_dir = std::env::var("RTK_HTTP_TRACE_DIR")
            .unwrap_or_else(|_| "/tmp/rtk-http-trace".to_string());
        let _ = std::fs::create_dir_all(&http_trace_dir);
        let trace_seq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        if let Ok(body) = serde_json::to_string_pretty(&anthropic_request) {
            let path = format!("{http_trace_dir}/{trace_seq}_req.json");
            let _ = std::fs::write(&path, &body);
            tracing::info!("[http-trace] Request written to {path}");
        }

        // Build the request, adding beta headers for advisor and skills
        let mut request_builder = self.client.post(&url).json(&anthropic_request);

        if has_advisor {
            request_builder = request_builder.header("anthropic-beta", AdvisorTool::beta_header());
        }

        if request.container.is_some() {
            for header in crate::tools::get_anthropic_skills_beta_headers() {
                request_builder = request_builder.header("anthropic-beta", header);
            }
        }

        let response = request_builder.send().await.map_err(|e| {
            ProviderError::Network(format!("failed to send request (model: {}): {}", model, e))
        })?;

        tracing::info!("Response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            tracing::error!(
                "API error {} (model: {}): {}",
                status,
                self.model,
                error_text
            );

            if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&error_text) {
                if let Some(error_obj) = error_json.get("error").and_then(|e| e.as_object()) {
                    let error_type = error_obj
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("unknown_error");

                    let message = error_obj
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or(&error_text);

                    let param = error_obj.get("param").and_then(|p| p.as_str());

                    return Err(map_anthropic_structured_error(
                        status, error_type, message, param, &headers,
                    ));
                }
            }

            return Err(map_anthropic_error(status, &error_text, &headers));
        }

        let response_text = response.text().await.map_err(|e| {
            ProviderError::Network(format!(
                "failed to read response body (model: {}): {}",
                model, e
            ))
        })?;

        // Dump raw response
        {
            let path = format!("{http_trace_dir}/{trace_seq}_resp.json");
            let _ = std::fs::write(&path, &response_text);
            tracing::info!("[http-trace] Response written to {path}");
        }

        let anthropic_response: AnthropicResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                ProviderError::Serialization(format!(
                    "failed to parse response (model: {}): {}",
                    model, e
                ))
            })?;

        // Extract text, tool_use, and refusal content blocks
        let mut content_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<serde_json::Value> = Vec::new();
        let mut all_citations: Vec<crate::provider::Citation> = Vec::new();
        let mut thinking_blocks: Vec<crate::provider::ThinkingBlock> = Vec::new();
        let mut refused = false;

        for block in &anthropic_response.content {
            if block.content_type == "json" {
                // Structured output responses use type "json" instead of "text"
                content_parts.push(block.text.clone());
            } else if block.content_type == "text" && !block.text.is_empty() {
                content_parts.push(block.text.clone());

                // Extract citations from text blocks (web search results)
                if let Some(ref citations) = block.citations {
                    for c in citations {
                        all_citations.push(crate::provider::Citation {
                            source: c.url.clone(),
                            title: c.title.clone(),
                            cited_text: c.cited_text.clone(),
                            index: c.search_result_index.unwrap_or(0),
                        });
                    }
                }
            } else if block.content_type == "tool_use" {
                // Extract tool_use blocks and serialize them
                let tool_call = serde_json::json!({
                    "id": block.id,
                    "name": block.name,
                    "arguments": block.input
                });
                tool_calls.push(tool_call);
            } else if block.content_type == "thinking" {
                tracing::debug!("Received thinking block ({} chars)", block.thinking.len());
                thinking_blocks.push(crate::provider::ThinkingBlock {
                    block_type: "thinking".to_string(),
                    thinking: block.thinking.clone(),
                    signature: block.signature.clone(),
                    data: String::new(),
                    display: None,
                });
            } else if block.content_type == "redacted_thinking" {
                tracing::debug!(
                    "Received redacted_thinking block ({} bytes)",
                    block.data.len()
                );
                thinking_blocks.push(crate::provider::ThinkingBlock {
                    block_type: "redacted_thinking".to_string(),
                    thinking: String::new(),
                    signature: String::new(),
                    data: block.data.clone(),
                    display: None,
                });
            } else if block.content_type == "refusal" {
                // Handle refusal content blocks (Claude 4 models)
                refused = true;
                if !block.text.is_empty() {
                    tracing::warn!("Model refused: {}", block.text);
                    content_parts.push(format!("[REFUSAL] {}", block.text));
                } else {
                    tracing::warn!("Model refused (no reason provided)");
                    content_parts.push("[REFUSAL]".to_string());
                }
            }
        }

        // Append tool calls as JSON to the content for auto_tool_parser to find
        if !tool_calls.is_empty() {
            let tool_calls_json =
                serde_json::to_string_pretty(&tool_calls).unwrap_or_else(|_| "[]".to_string());
            content_parts.push(format!("```tool\n{}\n```", tool_calls_json));
        }

        // Build usage with cache information if available
        let input_tokens: u32 = anthropic_response
            .usage
            .input_tokens
            .try_into()
            .unwrap_or_else(|_| {
                tracing::warn!("input_tokens overflow, clamping to u32::MAX");
                u32::MAX
            });
        let output_tokens: u32 = anthropic_response
            .usage
            .output_tokens
            .try_into()
            .unwrap_or_else(|_| {
                tracing::warn!("output_tokens overflow, clamping to u32::MAX");
                u32::MAX
            });
        // Zero out cache fields when we didn't request caching —
        // proxies may return fake values that produce nonsensical stats.
        let (cache_read, cache_creation) = if use_cache_control {
            let r: u32 = anthropic_response
                .usage
                .cache_read_input_tokens
                .try_into()
                .unwrap_or_else(|_| {
                    tracing::warn!("cache_read_input_tokens overflow, clamping to u32::MAX");
                    u32::MAX
                });
            let c: u32 = anthropic_response
                .usage
                .cache_creation_input_tokens
                .try_into()
                .unwrap_or_else(|_| {
                    tracing::warn!("cache_creation_input_tokens overflow, clamping to u32::MAX");
                    u32::MAX
                });
            (r, c)
        } else {
            (0, 0)
        };

        let usage = if cache_read > 0 || cache_creation > 0 {
            tracing::info!(
                "Cache: {cache_read} read, {cache_creation} written, input={input_tokens}, output={output_tokens}"
            );
            Usage::with_cache(input_tokens, output_tokens, cache_read, cache_creation)
        } else {
            Usage::new(input_tokens, output_tokens)
        };

        // Extract structured output when output_config.format was JsonSchema
        let structured_output = if wants_structured_output {
            let content_str = content_parts.join("\n");
            match serde_json::from_str::<serde_json::Value>(&content_str) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Structured output JSON parse failed: {e}");
                    None
                }
            }
        } else {
            None
        };

        Ok(CompletionResponse {
            content: content_parts.join("\n"),
            model: anthropic_response.model,
            usage: Some(usage),
            stop_reason: anthropic_response.stop_reason.or_else(|| {
                // Infer stop_reason from content if not explicitly provided
                if refused {
                    Some("refusal".to_string())
                } else if !tool_calls.is_empty() {
                    Some("tool_use".to_string())
                } else {
                    Some("end_turn".to_string())
                }
            }),
            citations: if all_citations.is_empty() {
                None
            } else {
                Some(all_citations)
            },
            thinking_blocks: if thinking_blocks.is_empty() {
                None
            } else {
                Some(thinking_blocks)
            },
            structured_output,
        })
    }

    pub fn new(config: ProviderConfig, model: String) -> Result<Self, ProviderError> {
        // Strict constructor: require API key to be present and non-empty
        if config
            .api_key
            .as_ref()
            .is_none_or(|k| k.expose_secret().trim().is_empty())
        {
            return Err(ProviderError::Configuration(
                "Anthropic API key is required. Set api_key in config or ANTHROPIC_API_KEY env var"
                    .to_string(),
            ));
        }

        // Delegate to the non-strict constructor for common setup
        Self::new_without_validation(config, model)
    }

    /// Create provider without config validation (for custom endpoints/proxies)
    pub fn new_without_validation(
        config: ProviderConfig,
        model: String,
    ) -> Result<Self, ProviderError> {
        // Non-strict constructor: allow missing API key (used for validation-free creation)
        let timeout = config.timeout_seconds.unwrap_or(300);

        // Build headers conditionally with API key if present and non-empty
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(key_secret) = config.api_key.as_ref() {
            let key = key_secret.expose_secret();
            if !key.trim().is_empty() {
                headers.insert(
                    reqwest::header::HeaderName::from_static("x-api-key"),
                    key.parse().map_err(|e| {
                        ProviderError::Configuration(format!("invalid API key format: {}", e))
                    })?,
                );
            }
        }

        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("anthropic-version"),
            reqwest::header::HeaderValue::from_static("2023-06-01"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(timeout))
            .connect_timeout(Duration::from_secs(30))
            .tcp_keepalive({
                #[allow(clippy::duration_suboptimal_units)]
                Duration::from_secs(60)
            })
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .map_err(|e| ProviderError::Network(format!("failed to build HTTP client: {}", e)))?;

        // Initialize tool registry with built-in tools and selector
        let tool_registry = Arc::new(rustycode_tools::default_registry());
        let tool_selector = ToolSelector::new();

        Ok(Self {
            config,
            client,
            model,
            tool_registry,
            tool_selector,
            advisor_config: None,
        })
    }

    /// Enable the advisor pattern for this provider.
    ///
    /// When enabled, requests will include the advisor tool, allowing the
    /// executor model to consult a more capable advisor model for guidance.
    /// The advisor tool is sent as a special `type: "advisor_20260301"` tool
    /// with the required `anthropic-beta` header.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use rustycode_llm::{AnthropicProvider, AdvisorConfig, ProviderConfig};
    ///
    /// let config = ProviderConfig { ... };
    /// let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".into())?
    ///     .with_advisor(AdvisorConfig::new());
    /// ```
    pub fn with_advisor(mut self, config: AdvisorConfig) -> Self {
        self.advisor_config = Some(config);
        self
    }

    fn endpoint(&self) -> String {
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com");

        let endpoint = format!("{}/v1/messages", base.trim_end_matches('/'));

        tracing::debug!(
            "Anthropic endpoint constructed: base_url={}, full_endpoint={}",
            base,
            endpoint
        );

        endpoint
    }

    /// Detect the user's intent from their latest message and select appropriate tools
    fn select_tools_for_prompt(&self, messages: &[ChatMessage]) -> Option<Vec<serde_json::Value>> {
        // Find the last user message to detect intent
        let user_prompt = messages
            .iter()
            .rev()
            .find(|msg| matches!(msg.role, MessageRole::User))
            .map(|msg| msg.content.as_text());

        if let Some(prompt) = user_prompt {
            // Detect profile from prompt
            let profile = ToolProfile::from_prompt(&prompt);

            // Get ranked tools for this profile using tag-based discovery
            let selector = self.tool_selector.clone().with_profile(profile);
            let tool_names = selector.select_tools(&self.tool_registry);

            // Format tools for Anthropic API
            Some(self.format_tools_for_anthropic(&tool_names))
        } else {
            // No user message found, return all tools (or none if preferred)
            None
        }
    }

    /// Format tool definitions for Anthropic API
    fn format_tools_for_anthropic(&self, tool_names: &[String]) -> Vec<serde_json::Value> {
        tool_names
            .iter()
            .filter_map(|name| {
                self.tool_registry
                    .get(name)
                    .map(|tool| self.tool_to_anthropic_format(tool))
            })
            .collect()
    }

    /// Convert a tool to Anthropic's tool format
    fn tool_to_anthropic_format(&self, tool: &dyn Tool) -> serde_json::Value {
        let schema = tool.parameters_schema();
        let mut tool_json = serde_json::json!({
            "name": tool.name(),
            "description": tool.description(),
            "input_schema": schema
        });
        if let Some(annotations) = anthropic_annotations_for_tool_info(
            tool.name(),
            matches!(tool.permission(), rustycode_tools_api::ToolPermission::Read),
        ) {
            tool_json["annotations"] = annotations;
        }
        tool_json
    }

    /// Parse conversation string into individual messages
    /// Input format: "role: content\n\nrole: content\n\n..."
    /// Output: Vec of AnthropicMessage with proper roles
    pub(crate) fn parse_conversation_messages(
        &self,
        messages: &[ChatMessage],
    ) -> Vec<AnthropicMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match &msg.role {
                    MessageRole::System => "user",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool(_) => "user",
                };

                // Handle MessageContent::Blocks with protocol-level ContentBlock variants
                // (used by headless mode and other code paths that construct messages directly)
                if let rustycode_protocol::MessageContent::Blocks(blocks) = &msg.content {
                    let anthropic_blocks: Vec<ContentBlock> = blocks
                        .iter()
                        .map(|b| match b {
                            rustycode_protocol::ContentBlock::ToolResult {
                                tool_use_id,
                                content,
                                is_error,
                            } => ContentBlock::ToolResult {
                                content_type: "tool_result",
                                tool_use_id: tool_use_id.clone(),
                                content: ToolResultContent::Text(content.clone()),
                                is_error: if *is_error { Some(true) } else { None },
                                cache_control: None,
                            },
                            rustycode_protocol::ContentBlock::ToolUse { id, name, input } => {
                                ContentBlock::ToolUse {
                                    content_type: "tool_use",
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: input.clone(),
                                    cache_control: None,
                                }
                            }
                            rustycode_protocol::ContentBlock::Text { text, .. } => {
                                ContentBlock::Text {
                                    content_type: "text",
                                    text: text.clone(),
                                    cache_control: None,
                                }
                            }
                            rustycode_protocol::ContentBlock::Image { source, .. } => {
                                ContentBlock::Image {
                                    content_type: "image",
                                    source: ImageSource {
                                        source_type: source.source_type.clone(),
                                        media_type: source.media_type.clone(),
                                        data: source.data.clone(),
                                    },
                                }
                            }
                            rustycode_protocol::ContentBlock::Thinking { thinking, signature } => {
                                ContentBlock::Thinking {
                                    content_type: "thinking",
                                    thinking: thinking.clone(),
                                    signature: signature.clone(),
                                }
                            }
                            rustycode_protocol::ContentBlock::RedactedThinking { data } => {
                                ContentBlock::RedactedThinking {
                                    content_type: "redacted_thinking",
                                    data: data.clone(),
                                }
                            }
                            _ => ContentBlock::Text {
                                content_type: "text",
                                text: "[unsupported block]".to_string(),
                                cache_control: None,
                            },
                        })
                        .collect();

                    if !anthropic_blocks.is_empty() {
                        // Determine the correct role: tool results must be in a user message
                        let effective_role = if anthropic_blocks
                            .iter()
                            .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
                        {
                            "user"
                        } else {
                            role
                        };
                        return AnthropicMessage {
                            role: effective_role,
                            content: AnthropicRequestContent::Blocks(anthropic_blocks),
                        };
                    }
                }

                // Check if this is a tool result message (JSON format)
                if let Ok(tool_result_json) =
                    serde_json::from_str::<serde_json::Value>(&msg.content.as_text())
                {
                    if tool_result_json["type"] == "tool_result" {
                        // This is a properly formatted tool result
                        let is_error = tool_result_json
                            .get("is_error")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        return AnthropicMessage {
                            role: "user",
                            content: AnthropicRequestContent::Blocks(vec![
                                ContentBlock::ToolResult {
                                    content_type: "tool_result",
                                    tool_use_id: tool_result_json["tool_use_id"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    content: parse_tool_result_content(&tool_result_json["content"]),
                                    is_error: if is_error { Some(true) } else { None },
                                    cache_control: None,
                                },
                            ]),
                        };
                    }

                    // Check if this is a search_result block
                    if tool_result_json["type"] == "search_result" {
                        let content_blocks = if let Some(content_array) =
                            tool_result_json.get("content").and_then(|c| c.as_array())
                        {
                            content_array
                                .iter()
                                .filter_map(|block| {
                                    block.get("text").and_then(|t| t.as_str()).map(|text| {
                                        SearchResultContent {
                                            content_type: "text",
                                            text: text.to_string(),
                                        }
                                    })
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };

                        let citations = tool_result_json.get("citations").and_then(|c| {
                            c.get("enabled")
                                .and_then(|e| e.as_bool())
                                .map(|enabled| CitationMetadata { enabled })
                        });

                        return AnthropicMessage {
                            role: "user",
                            content: AnthropicRequestContent::Blocks(vec![
                                ContentBlock::SearchResult {
                                    content_type: "search_result",
                                    source: tool_result_json["source"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    title: tool_result_json["title"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    content: content_blocks,
                                    citations,
                                    cache_control: None,
                                },
                            ]),
                        };
                    }

                    // Check if this is an array of content blocks (mixed content)
                    if let Ok(content_array) =
                        serde_json::from_str::<Vec<serde_json::Value>>(&msg.content.as_text())
                    {
                        let mut blocks = Vec::new();
                        for item in content_array {
                            if let Some(content_type) = item.get("type").and_then(|t| t.as_str()) {
                                match content_type {
                                    "search_result" => {
                                        let content_blocks = if let Some(content_array) =
                                            item.get("content").and_then(|c| c.as_array())
                                        {
                                            content_array
                                                .iter()
                                                .filter_map(|block| {
                                                    block.get("text").and_then(|t| t.as_str()).map(
                                                        |text| SearchResultContent {
                                                            content_type: "text",
                                                            text: text.to_string(),
                                                        },
                                                    )
                                                })
                                                .collect()
                                        } else {
                                            Vec::new()
                                        };

                                        let citations = item.get("citations").and_then(|c| {
                                            c.get("enabled")
                                                .and_then(|e| e.as_bool())
                                                .map(|enabled| CitationMetadata { enabled })
                                        });

                                        blocks.push(ContentBlock::SearchResult {
                                            content_type: "search_result",
                                            source: item
                                                .get("source")
                                                .and_then(|s| s.as_str())
                                                .unwrap_or("")
                                                .to_string(),
                                            title: item
                                                .get("title")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("")
                                                .to_string(),
                                            content: content_blocks,
                                            citations,
                                            cache_control: None,
                                        });
                                    }
                                    "text" => {
                                        if let Some(text) =
                                            item.get("text").and_then(|t| t.as_str())
                                        {
                                            blocks.push(ContentBlock::Text {
                                                content_type: "text",
                                                text: text.to_string(),
                                                cache_control: None,
                                            });
                                        }
                                    }
                                    _ => {
                                        // For other content types, just add as text
                                        if let Some(text) =
                                            item.get("text").and_then(|t| t.as_str())
                                        {
                                            blocks.push(ContentBlock::Text {
                                                content_type: "text",
                                                text: text.to_string(),
                                                cache_control: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        if !blocks.is_empty() {
                            return AnthropicMessage {
                                role,
                                content: AnthropicRequestContent::Blocks(blocks),
                            };
                        }
                    }
                }

                // Regular text message
                AnthropicMessage {
                    role,
                    content: AnthropicRequestContent::Text(msg.content.to_text()),
                }
            })
            .collect()
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "anthropic".to_string(),
            display_name: "Anthropic".to_string(),
            description: "Claude models with strong reasoning and analysis capabilities".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Anthropic API key from console.anthropic.com".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("sk-ant-...".to_string()),
                        default: None,
                        validation_pattern: Some("^sk-ant-.*".to_string()),
                        validation_error: Some("API key must start with 'sk-ant-'".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (for proxy or compatible services)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api.anthropic.com".to_string()),
                        default: Some("https://api.anthropic.com".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "ANTHROPIC_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are RustyCode, a coding assistant.\n\n{context}\n\n## Claude Guidance\n- Use XML tags for structured output (<analysis>, <plan>, <implementation>)\n- Clear, direct instructions work better than verbose ones\n- Handle complex multi-step instructions in a single prompt when possible".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: true,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Use <thinking> blocks for internal reasoning on complex problems.".to_string(),
                        "Keep instructions concise — Claude attends more to shorter prompts.".to_string(),
                    ],
                },
                tool_format: ToolFormat::AnthropicXML,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: None,
                parallel_calling: false,
                streaming_support: true,
            },
            recommended_models: vec![
                ModelInfo {
                    model_id: "claude-sonnet-4-6".to_string(),
                    display_name: "Claude Sonnet 4.6".to_string(),
                    description: "Best coding model with extended thinking".to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec!["Code generation".to_string(), "Development".to_string(), "Analysis".to_string()],
                    cost_tier: 3,
                },
                ModelInfo {
                    model_id: "claude-opus-4-6".to_string(),
                    display_name: "Claude Opus 4.6".to_string(),
                    description: "Deepest reasoning with extended thinking".to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec!["Complex reasoning".to_string(), "Architecture".to_string(), "Research".to_string()],
                    cost_tier: 5,
                },
                ModelInfo {
                    model_id: "claude-haiku-4-5-20251001".to_string(),
                    display_name: "Claude Haiku 4.5".to_string(),
                    description: "Fast and cost-efficient for lightweight tasks".to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec!["Quick responses".to_string(), "Classification".to_string(), "Agent workers".to_string()],
                    cost_tier: 1,
                },
            ],
        }
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn is_available(&self) -> bool {
        // Check if API key is present
        if self
            .config
            .api_key
            .as_ref()
            .map_or(true, |k| k.expose_secret().trim().is_empty())
        {
            return false;
        }

        // Try a simple health check or just verify the client is working
        true
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return known Anthropic models (as of April 2026)
        Ok(vec![
            // Claude 4.6 (latest)
            "claude-opus-4-6".to_string(),
            "claude-sonnet-4-6".to_string(),
            // Claude 4.7
            "claude-opus-4-7-20260401".to_string(),
            // Claude 4.5 (with extended thinking)
            "claude-opus-4-6".to_string(),
            "claude-sonnet-4-6".to_string(),
            // Claude 4.0
            "claude-opus-4-20250214".to_string(),
            "claude-sonnet-4-20250214".to_string(),
            // Claude 3.7
            "claude-3-7-sonnet-20250219".to_string(),
            // Claude 3.5
            "claude-sonnet-4-6".to_string(),
            "claude-haiku-4-5-20251001".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let retry_config = self.config.retry_config.clone().unwrap_or_default();

        crate::retry::retry_with_backoff(retry_config, || {
            let request = request.clone();
            async move {
                self.complete_internal(request)
                    .await
                    .map_err(anyhow::Error::from)
            }
        })
        .await
        .map_err(|e: anyhow::Error| {
            if let Some(provider_err) = e.downcast_ref::<ProviderError>() {
                provider_err.clone()
            } else {
                ProviderError::Api(e.to_string())
            }
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let retry_config = self.config.retry_config.clone().unwrap_or_default();

        crate::retry::retry_with_backoff(retry_config, || {
            let request = request.clone();
            async move {
                self.complete_stream_internal(request)
                    .await
                    .map_err(anyhow::Error::from)
            }
        })
        .await
        .map_err(|e: anyhow::Error| {
            if let Some(provider_err) = e.downcast_ref::<ProviderError>() {
                provider_err.clone()
            } else {
                ProviderError::Api(e.to_string())
            }
        })
    }
}

impl AnthropicProvider {
    async fn complete_stream_internal(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let url = self.endpoint();

        // Convert ChatMessage to AnthropicMessage
        let mut messages = self.parse_conversation_messages(&request.messages);

        // Use intelligent tool selection if tools not explicitly provided
        let mut tools = match request.tools {
            Some(tools) => tools, // Use explicitly provided tools
            None => {
                // Auto-select tools based on user prompt
                self.select_tools_for_prompt(&request.messages)
                    .unwrap_or_default()
            }
        };

        // Inject advisor tool if configured via builder or env var
        let advisor_tool = self
            .advisor_config
            .as_ref()
            .map(|c| c.advisor.to_anthropic_tool())
            .or_else(|| {
                std::env::var("RUSTYCODE_ADVISOR_MODEL").ok().map(|model| {
                    let advisor = AdvisorTool::new(model);
                    tracing::info!(
                        "Advisor tool auto-enabled via RUSTYCODE_ADVISOR_MODEL={}",
                        advisor.advisor_model
                    );
                    advisor.to_anthropic_tool()
                })
            });

        if let Some(tool) = &advisor_tool {
            tools.push(tool.clone());
        }

        let has_advisor = advisor_tool.is_some();

        let _wants_structured_output = request
            .output_config
            .as_ref()
            .and_then(|c| c.format.as_ref())
            .is_some_and(|f| {
                matches!(f.format_type, crate::provider::OutputFormatType::JsonSchema)
            });

        // Enable prompt caching for all Anthropic-compatible endpoints.
        // Most proxies (OpenRouter, z.ai, etc.) support cache_control correctly.
        let use_cache_control = true;

        // Apply cache_control to last 2 conversation messages.
        // Combined with system prompt (1) and tools (1), this uses all 4 allowed breakpoints.
        if use_cache_control {
            apply_cache_to_last_messages(&mut messages, 2);
        }

        let system = request.system_prompt.map(|text| {
            if use_cache_control {
                SystemPrompt::Blocks(vec![SystemContentBlock {
                    block_type: "text",
                    text,
                    cache_control: Some(CacheControl {
                        cache_type: "ephemeral",
                    }),
                }])
            } else {
                SystemPrompt::Text(text)
            }
        });

        let tools = if !tools.is_empty() {
            if use_cache_control {
                if let Some(last_tool) = tools.last_mut().and_then(|t| t.as_object_mut()) {
                    last_tool.insert(
                        "cache_control".to_string(),
                        serde_json::json!({"type": "ephemeral"}),
                    );
                }
            }
            Some(tools)
        } else {
            None
        };

        let anthropic_request = AnthropicRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            system,
            stream: Some(true),
            tools,
            thinking: normalize_thinking_for_model(
                request.thinking,
                request.output_config.as_ref().and_then(|c| c.effort),
                &request.model,
            ),
            output_config: request.output_config,
            container: request.container.clone(),
            tool_choice: request.tool_choice.as_ref().map(|tc| match tc.as_str() {
                Some("required") => serde_json::json!({"type": "any"}),
                Some("auto") => serde_json::json!({"type": "auto"}),
                Some("none") => serde_json::json!({"type": "none"}),
                _ => tc.clone(),
            }),
            parallel_tool_calls: request.parallel_tool_calls,
        };

        // Build the request, adding beta headers for advisor and skills
        let mut request_builder = self.client.post(&url).json(&anthropic_request);

        if has_advisor {
            request_builder = request_builder.header("anthropic-beta", AdvisorTool::beta_header());
        }

        if request.container.is_some() {
            for header in crate::tools::get_anthropic_skills_beta_headers() {
                request_builder = request_builder.header("anthropic-beta", header);
            }
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            tracing::error!(
                "API error {} (model: {}): {}",
                status,
                self.model,
                error_text
            );

            if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&error_text) {
                if let Some(error_obj) = error_json.get("error").and_then(|e| e.as_object()) {
                    let error_type = error_obj
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("unknown_error");

                    let message = error_obj
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or(&error_text);

                    let param = error_obj.get("param").and_then(|p| p.as_str());

                    return Err(map_anthropic_structured_error(
                        status, error_type, message, param, &headers,
                    ));
                }
            }

            return Err(map_anthropic_error(status, &error_text, &headers));
        }

        // Convert bytes stream to SSE stream
        let bytes_stream = response.bytes_stream();

        // Parse SSE events and emit structured events
        // Buffer partial lines and event type across chunk boundaries using shared state
        let byte_buffer = crate::sse::SseByteBuffer::new();
        let stream_state = Arc::new(std::sync::Mutex::new((
            None::<String>,
            HashMap::<usize, String>::new(),
            HashMap::<usize, String>::new(),
            HashMap::<usize, String>::new(),
            HashMap::<usize, String>::new(),
        )));
        let event_stream = bytes_stream.flat_map(move |chunk_result| {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(format!(
                        "Failed to read chunk: {}",
                        e
                    )))])
                }
            };

            let lines = byte_buffer.feed_chunk(&chunk);
            let mut state = stream_state.lock().unwrap_or_else(|e| e.into_inner());
            let (current_event_type, tool_ids_by_index, thinking_signatures, thinking_block_types, redacted_data) = &mut *state;

            let mut events = Vec::new();

            for line in &lines {

                if line.starts_with("event: ") {
                    *current_event_type = Some(line.trim_start_matches("event: ").to_string());
                    continue;
                }

                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if json_str == "[DONE]" {
                        continue;
                    }

                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        match current_event_type.as_deref() {
                            Some("error") => {
                                if let Some(error_obj) = data.get("error") {
                                    let error_type = error_obj
                                        .get("type")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("streaming_error")
                                        .to_string();
                                    let message = error_obj
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("streaming error occurred")
                                        .to_string();
                                    events.push(Err(ProviderError::Api(format!(
                                        "{}: {}",
                                        error_type, message
                                    ))));
                                }
                            }
                            Some("message_start") => {}
                            Some("content_block_start") => {
                                if let Some(block_obj) = data.get("content_block") {
                                    let index =
                                        data.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                            as usize;

                                    if let Some(block_type) =
                                        block_obj.get("type").and_then(|t| t.as_str())
                                    {
                                        match block_type {
                                            "tool_use" => {
                                                let id = block_obj
                                                    .get("id")
                                                    .and_then(|i| i.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                let name = block_obj
                                                    .get("name")
                                                    .and_then(|n| n.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                tool_ids_by_index.insert(index, id.clone());
                                                events.push(Ok(StreamEvent::ToolCallStarted {
                                                    id,
                                                    name,
                                                }));
                                            }
                                            "thinking" => {
                                                thinking_block_types.insert(index, "thinking".to_string());
                                            }
                                            "redacted_thinking" => {
                                                thinking_block_types.insert(index, "redacted_thinking".to_string());
                                                let data = block_obj
                                                    .get("data")
                                                    .and_then(|d| d.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                redacted_data.insert(index, data);
                                            }
                                            "text" => {}
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            Some("content_block_delta") => {
                                let index = data.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                    as usize;

                                if let Some(delta_obj) = data.get("delta") {
                                    if let Some(delta_type) =
                                        delta_obj.get("type").and_then(|t| t.as_str())
                                    {
                                        match delta_type {
                                            "text_delta" => {
                                                let text = delta_obj
                                                    .get("text")
                                                    .and_then(|t| t.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                if !text.is_empty() {
                                                    events.push(Ok(StreamEvent::TextDelta {
                                                        content: text,
                                                    }));
                                                }
                                            }
                                            "input_json_delta" => {
                                                let partial_json = delta_obj
                                                    .get("partial_json")
                                                    .and_then(|j| j.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                if !partial_json.is_empty() {
                                                    if let Some(id) =
                                                        tool_ids_by_index.get(&index).cloned()
                                                    {
                                                        events.push(Ok(
                                                            StreamEvent::ToolInputDelta {
                                                                id,
                                                                chunk: partial_json,
                                                            },
                                                        ));
                                                    }
                                                }
                                            }
                                            "thinking_delta" => {
                                                let thinking = delta_obj
                                                    .get("thinking")
                                                    .and_then(|t| t.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                if !thinking.is_empty() {
                                                    events.push(Ok(StreamEvent::ThinkingDelta {
                                                        content: thinking,
                                                    }));
                                                }
                                            }
                                            "signature_delta" => {
                                                if let Some(sig) =
                                                    delta_obj.get("signature").and_then(|s| s.as_str())
                                                {
                                                    thinking_signatures
                                                        .entry(index)
                                                        .and_modify(|existing| existing.push_str(sig))
                                                        .or_insert_with(|| sig.to_string());
                                                }
                                            }
                                            "citations_delta" => {}
                                            _ => {
                                                if let Some(text) =
                                                    delta_obj.get("text").and_then(|t| t.as_str())
                                                {
                                                    if !text.is_empty() {
                                                        events.push(Ok(StreamEvent::TextDelta {
                                                            content: text.to_string(),
                                                        }));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Some("content_block_stop") => {
                                let index = data.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                    as usize;
                                tool_ids_by_index.remove(&index);

                                if let Some(block_type) = thinking_block_types.remove(&index) {
                                    let signature =
                                        thinking_signatures.remove(&index).unwrap_or_default();
                                    let data = redacted_data.remove(&index).unwrap_or_default();
                                    events.push(Ok(StreamEvent::ThinkingBlockCompleted {
                                        block_type,
                                        signature,
                                        data,
                                    }));
                                }
                            }
                            Some("message_delta") => {
                                let stop_reason = data
                                    .get("delta")
                                    .and_then(|d| d.get("stop_reason"))
                                    .and_then(|s| s.as_str())
                                    .map(|s| s.to_string());

                                let usage = data.get("usage").and_then(|u| {
                                    let input_tokens: u32 = u
                                        .get("input_tokens")
                                        .and_then(|t| t.as_u64())?
                                        .try_into()
                                        .unwrap_or(u32::MAX);
                                    let output_tokens: u32 = u
                                        .get("output_tokens")
                                        .and_then(|t| t.as_u64())?
                                        .try_into()
                                        .unwrap_or(u32::MAX);
                                    let cache_read_input_tokens: u32 = u
                                        .get("cache_read_input_tokens")
                                        .and_then(|t| t.as_u64())
                                        .unwrap_or(0)
                                        .try_into()
                                        .unwrap_or(u32::MAX);
                                    let cache_creation_input_tokens: u32 = u
                                        .get("cache_creation_input_tokens")
                                        .and_then(|t| t.as_u64())
                                        .unwrap_or(0)
                                        .try_into()
                                        .unwrap_or(u32::MAX);

                                    let total_input = cache_read_input_tokens
                                        .saturating_add(cache_creation_input_tokens)
                                        .saturating_add(input_tokens);

                                    Some(Usage {
                                        input_tokens,
                                        output_tokens,
                                        total_tokens: total_input.saturating_add(output_tokens),
                                        cache_read_input_tokens,
                                        cache_creation_input_tokens,
                                        reasoning_tokens: None,
                                    })
                                });

                                if let Some(usage) = usage {
                                    // Only emit CacheUsage when we requested caching —
                                    // proxies may return fake cache fields.
                                    if use_cache_control
                                        && (usage.cache_read_input_tokens > 0
                                            || usage.cache_creation_input_tokens > 0)
                                    {
                                        let cache_read = usage.cache_read_input_tokens;
                                        let cache_creation = usage.cache_creation_input_tokens;
                                        tracing::info!(
                                            "Cache: {} read, {} written, input={}, output={}",
                                            cache_read,
                                            cache_creation,
                                            usage.input_tokens,
                                            usage.output_tokens
                                        );
                                        events.push(Ok(StreamEvent::CacheUsage {
                                            cache_read_tokens: u64::from(
                                                usage.cache_read_input_tokens,
                                            ),
                                            cache_creation_tokens: u64::from(
                                                usage.cache_creation_input_tokens,
                                            ),
                                        }));
                                    }
                                    events.push(Ok(StreamEvent::TokenUsage {
                                        input_tokens: u64::from(usage.input_tokens),
                                        output_tokens: u64::from(usage.output_tokens),
                                    }));
                                }

                                let reason =
                                    crate::provider::normalize_stop_reason(stop_reason.as_deref())
                                        .unwrap_or_else(|| {
                                            stop_reason
                                                .clone()
                                                .unwrap_or_else(|| "end_turn".to_string())
                                        });
                                events.push(Ok(StreamEvent::TurnCompleted {
                                    stop_reason: reason,
                                }));
                            }
                            Some("message_stop") => {
                                events.push(Ok(StreamEvent::Done));
                            }
                            Some("ping") => {}
                            _ => {}
                        }
                    }
                }
            }

            futures::stream::iter(events)
        });

        Ok(Box::pin(event_stream))
    }
}

/// Parse the `content` field of a tool_result JSON value.
/// Anthropic allows either a plain string or an array of typed blocks.
fn parse_tool_result_content(value: &serde_json::Value) -> ToolResultContent {
    if let Some(s) = value.as_str() {
        ToolResultContent::Text(s.to_string())
    } else if let Some(arr) = value.as_array() {
        let blocks: Vec<ToolResultBlock> = arr
            .iter()
            .filter_map(|block| {
                let bt = block.get("type").and_then(|t| t.as_str()).unwrap_or("text");
                match bt {
                    "text" => Some(ToolResultBlock {
                        block_type: "text",
                        text: block.get("text").and_then(|t| t.as_str()).map(String::from),
                    }),
                    "image" => Some(ToolResultBlock {
                        block_type: "image",
                        text: None,
                    }),
                    _ => None,
                }
            })
            .collect();
        ToolResultContent::Blocks(blocks)
    } else {
        ToolResultContent::Text(String::new())
    }
}

/// Map Anthropic API errors to ProviderError
fn map_anthropic_error(
    status: reqwest::StatusCode,
    error_text: &str,
    headers: &reqwest::header::HeaderMap,
) -> ProviderError {
    match status.as_u16() {
        401 | 403 => ProviderError::Auth(error_text.to_string()),
        429 => {
            let retry_delay = extract_retry_after_ms(headers).map(Duration::from_millis);
            ProviderError::RateLimited { retry_delay }
        }
        400 => ProviderError::Api(error_text.to_string()),
        404 => ProviderError::InvalidModel(error_text.to_string()),
        502..=504 => ProviderError::Network(format!("service unavailable: {}", error_text)),
        529 => ProviderError::Network(format!("Anthropic API overloaded: {}", error_text)),
        _ => ProviderError::Api(format!("HTTP {}: {}", status.as_u16(), error_text)),
    }
}

/// Map Anthropic structured error to ProviderError
/// See: https://platform.claude.com/docs/en/api/errors
fn map_anthropic_structured_error(
    status: reqwest::StatusCode,
    error_type: &str,
    message: &str,
    param: Option<&str>,
    headers: &reqwest::header::HeaderMap,
) -> ProviderError {
    let mut error_msg = format!("{}: {}", error_type, message);
    if let Some(p) = param {
        error_msg.push_str(&format!(" (parameter: {})", p));
    }

    match error_type {
        "invalid_request_error" => ProviderError::Api(error_msg),
        "authentication_error" => ProviderError::Auth(error_msg),
        "permission_denied_error" => ProviderError::Auth(error_msg),
        "not_found_error" => ProviderError::InvalidModel(error_msg),
        "rate_limit_error" => {
            let retry_delay = extract_retry_after_ms(headers).map(Duration::from_millis);
            ProviderError::RateLimited { retry_delay }
        }
        "api_error" | "internal_server_error" => {
            ProviderError::Api(format!("Anthropic API error: {}", message))
        }
        "overloaded_error" => {
            ProviderError::Network(format!("Anthropic API overloaded: {}", message))
        }
        _ => map_anthropic_error(status, &error_msg, headers),
    }
}

/// Unified LLMProvider trait implementation for Anthropic
///
/// This implementation wraps the provider methods to conform to the unified
/// trait from rustycode-protocol, providing a bridge between the two API versions.
#[async_trait]
impl UnifiedLLMProvider for AnthropicProvider {
    async fn list_models(&self) -> anyhow::Result<Vec<UnifiedModelInfo>> {
        // Use the existing provider list_models and convert results
        let model_names = <Self as LLMProvider>::list_models(self)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list models: {}", e))?;

        Ok(model_names
            .into_iter()
            .map(|name| {
                // Map known Anthropic models to their specifications
                let (context_window, cost_input, cost_output) = match name.as_str() {
                    "claude-opus-4-6" | "claude-opus-4.6" => (200000, 0.015, 0.075),
                    "claude-opus-4-7-20260401" => (200000, 0.015, 0.075),
                    "claude-sonnet-4-6" | "claude-sonnet-4.6" => (200000, 0.003, 0.015),
                    "claude-sonnet-4-20250214" => (200000, 0.003, 0.015),
                    "claude-3-7-sonnet-20250219" => (200000, 0.003, 0.015),
                    "claude-haiku-4-5-20251001" => (200000, 0.0008, 0.0024),
                    "claude-opus-4-20250214" => (200000, 0.015, 0.075),
                    _ => (200000, 0.001, 0.003), // Default fallback
                };

                UnifiedModelInfo {
                    name,
                    provider: "anthropic".to_string(),
                    context_window,
                    supports_streaming: true,
                    cost_per_1k_input_tokens: cost_input,
                    cost_per_1k_output_tokens: cost_output,
                }
            })
            .collect())
    }

    async fn is_available(&self) -> anyhow::Result<bool> {
        // Convert provider bool to Result<bool>
        Ok(<Self as LLMProvider>::is_available(self).await)
    }

    async fn complete(
        &self,
        request: UnifiedCompletionRequest,
    ) -> anyhow::Result<UnifiedCompletionResponse> {
        // Convert unified request to provider request
        use rustycode_protocol::MessageContent;

        let v2_request = CompletionRequest {
            model: request.model.clone(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Simple(request.prompt.clone()),
            }],
            max_tokens: request.max_tokens.map(|t| t as u32),
            temperature: request.temperature,
            stream: false,
            system_prompt: request.system,
            tools: None,
            thinking: None,
            output_config: None,
            container: None,
            tool_choice: None,
            parallel_tool_calls: None,
            session_id: None,
            api_mode: None,
        };

        // Call provider complete
        let v2_response = <Self as LLMProvider>::complete(self, v2_request)
            .await
            .map_err(|e| anyhow::anyhow!("Completion failed: {}", e))?;

        // Extract usage info
        let (input_tokens, output_tokens) = if let Some(usage) = v2_response.usage {
            (usage.input_tokens as usize, usage.output_tokens as usize)
        } else {
            (0, 0)
        };

        // Convert provider response to unified response
        Ok(UnifiedCompletionResponse {
            text: v2_response.content,
            tokens_used: UnifiedTokenCount {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens.saturating_add(output_tokens),
            },
            cost: UnifiedCost {
                input_cost: 0.0,
                output_cost: 0.0,
                total_cost: 0.0,
            },
            finish_reason: v2_response.stop_reason.unwrap_or_default(),
        })
    }

    fn name(&self) -> &'static str {
        <Self as LLMProvider>::name(self)
    }

    fn estimate_cost(&self, request: &UnifiedCompletionRequest) -> anyhow::Result<UnifiedCost> {
        // Estimate tokens from prompt (rough calculation: ~1.3 chars per token)
        let input_tokens = request.prompt.len() as f64 / 4.0;
        let output_tokens = request.max_tokens.unwrap_or(1000) as f64 / 2.0;

        // Use model-specific pricing
        let (cost_in, cost_out) = match request.model.as_str() {
            "claude-opus-4-6"
            | "claude-opus-4.6"
            | "claude-opus-4-7-20260401"
            | "claude-opus-4-20250214" => (0.015, 0.075),
            "claude-sonnet-4-6"
            | "claude-sonnet-4.6"
            | "claude-sonnet-4-20250214"
            | "claude-3-7-sonnet-20250219" => (0.003, 0.015),
            "claude-haiku-4-5-20251001" => (0.0008, 0.0024),
            _ => (0.001, 0.003),
        };

        let input_cost = (input_tokens * cost_in) / 1000.0;
        let output_cost = (output_tokens * cost_out) / 1000.0;

        Ok(UnifiedCost {
            input_cost,
            output_cost,
            total_cost: input_cost + output_cost,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn make_config(api_key: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            api_key: api_key.map(|s| SecretString::new(s.to_string().into())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_requires_api_key() {
        let config = make_config(None);
        assert!(AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).is_err());
    }

    #[test]
    fn test_accepts_valid_api_key() {
        let config = make_config(Some("test-key"));
        assert!(AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).is_ok());
    }

    #[test]
    fn test_rejects_empty_api_key() {
        let config = make_config(Some(""));
        assert!(AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).is_err());
    }

    #[test]
    fn test_provider_name() {
        let config = make_config(Some("test-key"));
        let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).unwrap();
        // Use provider trait to avoid ambiguity with unified trait
        assert_eq!(
            <AnthropicProvider as LLMProvider>::name(&provider),
            "anthropic"
        );
    }

    #[test]
    fn test_anthropic_content_deserializes_citations() {
        // Simulate an Anthropic response with citations in a text block
        let json = r#"{
            "type": "text",
            "text": "According to the docs, Rust is safe.",
            "citations": [
                {
                    "type": "web_search_result_location",
                    "cited_text": "Rust is memory safe",
                    "url": "https://doc.rust-lang.org/book/ch01-01.html",
                    "title": "The Rust Programming Language",
                    "search_result_index": 0
                }
            ]
        }"#;

        let content: AnthropicContent = serde_json::from_str(json).unwrap();
        assert_eq!(content.content_type, "text");
        assert_eq!(content.text, "According to the docs, Rust is safe.");
        assert!(content.citations.is_some());

        let citations = content.citations.unwrap();
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].citation_type, "web_search_result_location");
        assert_eq!(citations[0].cited_text, "Rust is memory safe");
        assert_eq!(
            citations[0].url,
            "https://doc.rust-lang.org/book/ch01-01.html"
        );
        assert_eq!(citations[0].title, "The Rust Programming Language");
        assert_eq!(citations[0].search_result_index, Some(0));
    }

    #[test]
    fn test_anthropic_content_no_citations() {
        let json = r#"{
            "type": "text",
            "text": "Hello world"
        }"#;

        let content: AnthropicContent = serde_json::from_str(json).unwrap();
        assert_eq!(content.text, "Hello world");
        assert!(content.citations.is_none());
    }

    #[test]
    fn test_anthropic_metadata_has_claude4_models() {
        let metadata = AnthropicProvider::metadata();
        let model_ids: Vec<&str> = metadata
            .recommended_models
            .iter()
            .map(|m| m.model_id.as_str())
            .collect();

        // Should have Claude 4.x models
        assert!(
            model_ids
                .iter()
                .any(|id| id.contains("sonnet-4") || id.contains("opus-4")),
            "recommended_models should include Claude 4.x, got: {:?}",
            model_ids
        );
        // Should NOT have Claude 3.x models
        assert!(
            !model_ids.iter().any(|id| id.starts_with("claude-3-")),
            "recommended_models should not include Claude 3.x, got: {:?}",
            model_ids
        );
    }

    #[test]
    fn test_map_anthropic_error_404_model_not_found() {
        let status = reqwest::StatusCode::from_u16(404).unwrap();
        let headers = reqwest::header::HeaderMap::new();
        let error = map_anthropic_error(status, "model does not exist", &headers);
        match error {
            ProviderError::InvalidModel(msg) => {
                assert!(msg.contains("model does not exist"));
            }
            other => panic!("expected InvalidModel, got {:?}", other),
        }
    }

    #[test]
    fn test_map_anthropic_error_502_service_unavailable() {
        let status = reqwest::StatusCode::from_u16(502).unwrap();
        let headers = reqwest::header::HeaderMap::new();
        let error = map_anthropic_error(status, "bad gateway", &headers);
        match error {
            ProviderError::Network(msg) => {
                assert!(msg.contains("service unavailable"));
                assert!(msg.contains("bad gateway"));
            }
            other => panic!("expected Network, got {:?}", other),
        }
    }

    #[test]
    fn test_map_anthropic_error_503_service_unavailable() {
        let status = reqwest::StatusCode::from_u16(503).unwrap();
        let headers = reqwest::header::HeaderMap::new();
        let error = map_anthropic_error(status, "service overloaded", &headers);
        match error {
            ProviderError::Network(msg) => {
                assert!(msg.contains("service unavailable"));
            }
            other => panic!("expected Network, got {:?}", other),
        }
    }

    #[test]
    fn test_map_anthropic_error_401_auth() {
        let status = reqwest::StatusCode::from_u16(401).unwrap();
        let headers = reqwest::header::HeaderMap::new();
        let error = map_anthropic_error(status, "invalid key", &headers);
        assert!(matches!(error, ProviderError::Auth(_)));
    }

    #[test]
    fn test_map_anthropic_error_429_rate_limited() {
        let status = reqwest::StatusCode::from_u16(429).unwrap();
        let headers = reqwest::header::HeaderMap::new();
        let error = map_anthropic_error(status, "slow down", &headers);
        assert!(matches!(
            error,
            ProviderError::RateLimited { retry_delay: None }
        ));
    }

    #[test]
    fn test_map_anthropic_structured_error_not_found() {
        let status = reqwest::StatusCode::from_u16(404).unwrap();
        let headers = reqwest::header::HeaderMap::new();
        let error = map_anthropic_structured_error(
            status,
            "not_found_error",
            "model: foo-bar does not exist",
            None,
            &headers,
        );
        match error {
            ProviderError::InvalidModel(msg) => {
                assert!(msg.contains("not_found_error"));
                assert!(msg.contains("foo-bar"));
            }
            other => panic!("expected InvalidModel, got {:?}", other),
        }
    }

    #[test]
    fn test_map_anthropic_structured_error_overloaded() {
        let status = reqwest::StatusCode::from_u16(529).unwrap();
        let headers = reqwest::header::HeaderMap::new();
        let error = map_anthropic_structured_error(
            status,
            "overloaded_error",
            "Anthropic is overloaded",
            None,
            &headers,
        );
        match error {
            ProviderError::Network(msg) => {
                assert!(msg.contains("overloaded"));
            }
            other => panic!("expected Network, got {:?}", other),
        }
    }

    #[test]
    fn test_map_anthropic_structured_error_with_param() {
        let status = reqwest::StatusCode::from_u16(400).unwrap();
        let headers = reqwest::header::HeaderMap::new();
        let error = map_anthropic_structured_error(
            status,
            "invalid_request_error",
            "max_tokens must be positive",
            Some("max_tokens"),
            &headers,
        );
        match error {
            ProviderError::Api(msg) => {
                assert!(msg.contains("parameter: max_tokens"));
            }
            other => panic!("expected Api, got {:?}", other),
        }
    }

    #[test]
    fn test_anthropic_metadata_display_name() {
        let metadata = AnthropicProvider::metadata();
        assert_eq!(metadata.display_name, "Anthropic");
        assert_eq!(metadata.provider_id, "anthropic");
    }

    #[test]
    fn test_anthropic_metadata_tool_calling_supported() {
        let metadata = AnthropicProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
    }

    #[test]
    fn test_anthropic_endpoint_default() {
        let config = make_config(Some("test-key"));
        let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).unwrap();
        assert_eq!(provider.endpoint(), "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn test_anthropic_endpoint_custom_base_url() {
        let mut config = make_config(Some("test-key"));
        config.base_url = Some("https://my-proxy.example.com".to_string());
        let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).unwrap();
        assert_eq!(
            provider.endpoint(),
            "https://my-proxy.example.com/v1/messages"
        );
    }

    #[test]
    fn test_anthropic_usage_deserialization() {
        let json = r#"{
            "input_tokens": 100,
            "output_tokens": 50,
            "cache_read_input_tokens": 30,
            "cache_creation_input_tokens": 10
        }"#;
        let usage: AnthropicUsage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.cache_read_input_tokens, 30);
        assert_eq!(usage.cache_creation_input_tokens, 10);
    }

    #[test]
    fn test_anthropic_usage_defaults_cache_tokens() {
        let json = r#"{
            "input_tokens": 100,
            "output_tokens": 50
        }"#;
        let usage: AnthropicUsage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.cache_read_input_tokens, 0);
        assert_eq!(usage.cache_creation_input_tokens, 0);
    }

    #[test]
    fn test_anthropic_response_deserialization() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Hello!", "id": "", "name": "", "input": null}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
            "model": "claude-sonnet-4-20250514"
        }"#;
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "claude-sonnet-4-20250514");
        assert_eq!(response.content.len(), 1);
        assert_eq!(response.content[0].text, "Hello!");
    }

    #[test]
    fn test_anthropic_request_serialization() {
        let request = AnthropicRequest {
            model: "claude-sonnet-4-6".to_string(),
            messages: vec![],
            max_tokens: 4096,
            temperature: 0.7,
            system: Some(SystemPrompt::Blocks(vec![SystemContentBlock {
                block_type: "text",
                text: "You are helpful".to_string(),
                cache_control: Some(CacheControl {
                    cache_type: "ephemeral",
                }),
            }])),
            stream: Some(false),
            tools: None,
            thinking: None,
            output_config: None,
            container: None,
            tool_choice: None,
            parallel_tool_calls: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"claude-sonnet-4-6\""));
        assert!(json.contains("\"max_tokens\":4096"));
        assert!(json.contains("\"temperature\":0.7"));
        // stream: false should still be serialized since skip_serializing_if is only for None
        // tools and effort should be absent since they are None
        assert!(!json.contains("\"tools\""));
    }

    #[test]
    fn test_cache_control_on_system_and_tools() {
        // System prompt should be serialized as content block array with cache_control
        let request = AnthropicRequest {
            model: "claude-sonnet-4-6".to_string(),
            messages: vec![],
            max_tokens: 4096,
            temperature: 0.7,
            system: Some(SystemPrompt::Blocks(vec![SystemContentBlock {
                block_type: "text",
                text: "You are a helpful assistant".to_string(),
                cache_control: Some(CacheControl {
                    cache_type: "ephemeral",
                }),
            }])),
            stream: None,
            tools: Some(vec![
                serde_json::json!({
                    "name": "get_weather",
                    "description": "Get weather",
                    "input_schema": {"type": "object", "properties": {}}
                }),
                serde_json::json!({
                    "name": "read_file",
                    "description": "Read a file",
                    "input_schema": {"type": "object", "properties": {}},
                    "cache_control": {"type": "ephemeral"}
                }),
            ]),
            thinking: None,
            output_config: None,
            container: None,
            tool_choice: None,
            parallel_tool_calls: None,
        };
        let json = serde_json::to_string(&request).unwrap();

        // System should be array with cache_control on the block
        assert!(json.contains("\"system\":[{"), "system should be an array");
        assert!(
            json.contains("\"cache_control\":{\"type\":\"ephemeral\"}"),
            "system block should have cache_control"
        );

        // No top-level cache_control
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            parsed.get("cache_control").is_none(),
            "no top-level cache_control"
        );
    }

    // ── Protocol-level message roundtrip tests ────────────────────────────────
    //
    // Input messages use rustycode_protocol types (ProtoBlock, MessageContent).
    // The output uses Anthropic's own ContentBlock (super::ContentBlock).

    use rustycode_protocol::{
        ContentBlock as ProtoBlock, ImageSource as ProtoImageSource, MessageContent,
    };

    fn make_anthropic_provider() -> AnthropicProvider {
        AnthropicProvider::new_without_validation(
            make_config(Some("test-key")),
            "claude-sonnet-4-6".to_string(),
        )
        .unwrap()
    }

    /// Helper: extract text from AnthropicRequestContent
    fn extract_anthropic_text(content: &AnthropicRequestContent) -> Option<String> {
        match content {
            AnthropicRequestContent::Text(t) => Some(t.clone()),
            AnthropicRequestContent::Blocks(blocks) => {
                let texts: Vec<&str> = blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join("\n"))
                }
            }
        }
    }

    #[test]
    fn test_roundtrip_simple_text_user_message() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage::user("Hello, world!")];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        assert_eq!(
            extract_anthropic_text(&result[0].content).unwrap(),
            "Hello, world!"
        );
    }

    #[test]
    fn test_roundtrip_simple_text_assistant_message() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage::assistant("Hi there!")];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        assert_eq!(
            extract_anthropic_text(&result[0].content).unwrap(),
            "Hi there!"
        );
    }

    #[test]
    fn test_roundtrip_system_role_maps_to_user() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::System,
            content: MessageContent::simple("System prompt"),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        // Anthropic maps System -> "user"
        assert_eq!(result[0].role, "user");
    }

    #[test]
    fn test_roundtrip_text_block() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ProtoBlock::text("Block content")]),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        let text = extract_anthropic_text(&result[0].content).unwrap();
        assert_eq!(text, "Block content");
    }

    #[test]
    fn test_roundtrip_tool_use_block_in_assistant_role() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ProtoBlock::tool_use(
                "tu_1",
                "read_file",
                serde_json::json!({"path": "main.rs"}),
            )]),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        // ToolUse stays in assistant role
        assert_eq!(result[0].role, "assistant");
        match &result[0].content {
            AnthropicRequestContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolUse {
                        id, name, input, ..
                    } => {
                        assert_eq!(id, "tu_1");
                        assert_eq!(name, "read_file");
                        assert_eq!(input["path"], "main.rs");
                    }
                    other => panic!("expected ToolUse block, got {:?}", other),
                }
            }
            other => panic!("expected Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip_tool_result_block_forced_user_role() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ProtoBlock::tool_result(
                "tu_1",
                "file content here",
            )]),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        // ToolResult forces role to "user" in Anthropic API
        assert_eq!(result[0].role, "user");
        match &result[0].content {
            AnthropicRequestContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } => {
                        assert_eq!(tool_use_id, "tu_1");
                        assert_eq!(content, "file content here");
                    }
                    other => panic!("expected ToolResult block, got {:?}", other),
                }
            }
            other => panic!("expected Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip_tool_error_block_sets_is_error() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ProtoBlock::tool_error(
                "tu_err",
                "command failed: No such file",
            )]),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        match &result[0].content {
            AnthropicRequestContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                        ..
                    } => {
                        assert_eq!(tool_use_id, "tu_err");
                        assert_eq!(content, "command failed: No such file");
                        assert_eq!(*is_error, Some(true));
                    }
                    other => panic!("expected ToolResult block, got {:?}", other),
                }
            }
            other => panic!("expected Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip_tool_result_no_error_omits_flag() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ProtoBlock::tool_result(
                "tu_ok",
                "success output",
            )]),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        match &result[0].content {
            AnthropicRequestContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult { is_error, .. } => {
                        assert_eq!(*is_error, None);
                    }
                    other => panic!("expected ToolResult block, got {:?}", other),
                }
            }
            other => panic!("expected Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip_image_block_is_preserved() {
        // Image blocks are now properly converted to Anthropic image blocks
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ProtoBlock::image(ProtoImageSource::base64(
                "image/png",
                "iVBOR",
            ))]),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        match &result[0].content {
            AnthropicRequestContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Image { source, .. } => {
                        assert_eq!(source.source_type, "base64");
                        assert_eq!(source.media_type, "image/png");
                        assert_eq!(source.data, "iVBOR");
                    }
                    other => panic!("expected Image block, got {:?}", other),
                }
            }
            other => panic!("expected Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip_thinking_block_preserved() {
        // Thinking blocks are preserved as thinking blocks (with signature) for multi-turn
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ProtoBlock::thinking("deep thought", "sig123")]),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        match &result[0].content {
            AnthropicRequestContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                // Thinking block is preserved with thinking content and signature
                let think_json = serde_json::to_value(&blocks[0]).unwrap();
                assert_eq!(think_json["type"], "thinking");
                assert_eq!(think_json["thinking"], "deep thought");
                assert_eq!(think_json["signature"], "sig123");
            }
            other => panic!("expected Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip_mixed_text_and_tool_use() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![
                ProtoBlock::text("I'll read that file."),
                ProtoBlock::tool_use("call_x", "read_file", serde_json::json!({"path": "x.rs"})),
            ]),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "assistant");
        match &result[0].content {
            AnthropicRequestContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
                assert!(
                    matches!(&blocks[0], super::ContentBlock::Text { text, .. } if text == "I'll read that file.")
                );
                assert!(
                    matches!(&blocks[1], super::ContentBlock::ToolUse { id, name, .. } if id == "call_x" && name == "read_file")
                );
            }
            other => panic!("expected Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip_mixed_text_and_tool_result() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![
                ProtoBlock::text("Here is the result:"),
                ProtoBlock::tool_result("call_1", "output data"),
            ]),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        // ToolResult forces role to "user"
        assert_eq!(result[0].role, "user");
        match &result[0].content {
            AnthropicRequestContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
                assert!(
                    matches!(&blocks[0], super::ContentBlock::Text { text, .. } if text == "Here is the result:")
                );
                assert!(
                    matches!(&blocks[1], super::ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_1")
                );
            }
            other => panic!("expected Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip_tool_role_maps_to_user() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage {
            role: MessageRole::Tool("call_id".to_string()),
            content: MessageContent::simple("Tool output"),
        }];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        // Tool role maps to "user" in Anthropic
        assert_eq!(result[0].role, "user");
    }

    #[test]
    fn test_json_tool_result_with_error_sets_is_error() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage::tool_result_with_error(
            "command not found".to_string(),
            "tu_json_err".to_string(),
            true,
        )];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].role, "user");
        match &result[0].content {
            AnthropicRequestContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                        ..
                    } => {
                        assert_eq!(tool_use_id, "tu_json_err");
                        assert_eq!(content, "command not found");
                        assert_eq!(*is_error, Some(true));
                    }
                    other => panic!("expected ToolResult block, got {:?}", other),
                }
            }
            other => panic!("expected Blocks, got {:?}", other),
        }
    }

    #[test]
    fn test_roundtrip_empty_message() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage::user("")];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        // parse_conversation_messages faithfully converts empty content
        // (complete_internal later filters it out before sending)
        let text = extract_anthropic_text(&result[0].content).unwrap();
        assert_eq!(text, "");
    }

    #[test]
    fn test_roundtrip_whitespace_only_message() {
        let provider = make_anthropic_provider();
        let msgs = vec![ChatMessage::user("   \n\t  ")];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        let text = extract_anthropic_text(&result[0].content).unwrap();
        assert_eq!(text, "   \n\t  ");
    }

    #[test]
    fn test_roundtrip_very_long_text_content() {
        let provider = make_anthropic_provider();
        let long_text = "B".repeat(12_000);
        let msgs = vec![ChatMessage::user(long_text.clone())];
        let result = provider.parse_conversation_messages(&msgs);
        assert_eq!(result.len(), 1);
        let text = extract_anthropic_text(&result[0].content).unwrap();
        assert_eq!(text, long_text);
    }

    // ── Stop reason and refusal handling tests ──────────────────────────────

    #[test]
    fn test_anthropic_response_stop_reason_tool_use() {
        let json = r#"{
            "content": [
                {"type": "tool_use", "text": "", "id": "tu_1", "name": "read_file", "input": {"path": "main.rs"}}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
            "model": "claude-sonnet-4-6",
            "stop_reason": "tool_use"
        }"#;
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.stop_reason.as_deref(), Some("tool_use"));
    }

    #[test]
    fn test_anthropic_response_stop_reason_refusal() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "I cannot", "id": "", "name": "", "input": null}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
            "model": "claude-sonnet-4-6",
            "stop_reason": "refusal"
        }"#;
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.stop_reason.as_deref(), Some("refusal"));
    }

    #[test]
    fn test_anthropic_response_stop_reason_max_tokens() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "The answer is...", "id": "", "name": "", "input": null}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 4096, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
            "model": "claude-sonnet-4-6",
            "stop_reason": "max_tokens"
        }"#;
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.stop_reason.as_deref(), Some("max_tokens"));
    }

    #[test]
    fn test_anthropic_response_stop_reason_end_turn() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Done!", "id": "", "name": "", "input": null}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
            "model": "claude-sonnet-4-6",
            "stop_reason": "end_turn"
        }"#;
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.stop_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn test_anthropic_request_serializes_output_config() {
        let request = AnthropicRequest {
            model: "claude-sonnet-4-6".to_string(),
            messages: vec![],
            max_tokens: 4096,
            temperature: 0.7,
            system: None,
            stream: Some(false),
            tools: None,
            thinking: None,
            output_config: Some(crate::provider::OutputConfig::with_effort(
                crate::provider::EffortLevel::High,
            )),
            container: None,
            tool_choice: None,
            parallel_tool_calls: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"output_config\""));
        assert!(json.contains("\"effort\":\"high\""));
    }

    #[test]
    fn test_anthropic_request_serializes_json_schema_output() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "answer": {"type": "string"}
            },
            "required": ["answer"]
        });
        let request = AnthropicRequest {
            model: "claude-sonnet-4-6".to_string(),
            messages: vec![],
            max_tokens: 4096,
            temperature: 0.7,
            system: None,
            stream: Some(false),
            tools: None,
            thinking: None,
            output_config: Some(crate::provider::OutputConfig::with_json_schema(schema)),
            container: None,
            tool_choice: None,
            parallel_tool_calls: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"output_config\""));
        assert!(json.contains("\"schema\""));
    }

    #[test]
    fn test_anthropic_content_thinking_block_deserialization() {
        let json = r#"{
            "type": "thinking",
            "thinking": "Let me analyze this step by step...",
            "signature": "WaUjzkypQ2mUEVM36O2TxuC06KN8xyfbJwyem2dw3URve..."
        }"#;
        let block: AnthropicContent = serde_json::from_str(json).unwrap();
        assert_eq!(block.content_type, "thinking");
        assert_eq!(block.thinking, "Let me analyze this step by step...");
        assert_eq!(
            block.signature,
            "WaUjzkypQ2mUEVM36O2TxuC06KN8xyfbJwyem2dw3URve..."
        );
    }

    #[test]
    fn test_anthropic_content_redacted_thinking_block_deserialization() {
        let json = r#"{
            "type": "redacted_thinking",
            "data": "ErwDkUYICxIMMb3LzNrMu..."
        }"#;
        let block: AnthropicContent = serde_json::from_str(json).unwrap();
        assert_eq!(block.content_type, "redacted_thinking");
        assert_eq!(block.data, "ErwDkUYICxIMMb3LzNrMu...");
    }

    #[test]
    fn test_thinking_block_roundtrip_preservation() {
        use crate::provider::ThinkingBlock;
        let thinking = ThinkingBlock {
            block_type: "thinking".to_string(),
            thinking: "I need to think about this...".to_string(),
            signature: "sig_abc123".to_string(),
            data: String::new(),
            display: None,
        };
        let json = serde_json::to_string(&thinking).unwrap();
        let back: ThinkingBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(back.block_type, "thinking");
        assert_eq!(back.thinking, "I need to think about this...");
        assert_eq!(back.signature, "sig_abc123");
    }

    #[test]
    fn test_completion_response_with_thinking_blocks() {
        use crate::provider::{CompletionResponse, ThinkingBlock};
        let response = CompletionResponse {
            content: "The answer is 42.".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            usage: None,
            stop_reason: Some("end_turn".to_string()),
            citations: None,
            thinking_blocks: Some(vec![ThinkingBlock {
                block_type: "thinking".to_string(),
                thinking: "Deep analysis...".to_string(),
                signature: "sig_xyz".to_string(),
                data: String::new(),
                display: None,
            }]),
            structured_output: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("thinking_blocks"));
        let back: CompletionResponse = serde_json::from_str(&json).unwrap();
        assert!(back.thinking_blocks.is_some());
        let blocks = back.thinking_blocks.unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_type, "thinking");
    }

    #[test]
    fn test_completion_response_with_structured_output() {
        use crate::provider::CompletionResponse;
        let response = CompletionResponse {
            content: r#"{"answer": "42"}"#.to_string(),
            model: "claude-sonnet-4-6".to_string(),
            usage: None,
            stop_reason: Some("end_turn".to_string()),
            citations: None,
            thinking_blocks: None,
            structured_output: Some(serde_json::json!({"answer": "42"})),
        };
        assert!(response.structured_output.is_some());
        let output = response.structured_output.unwrap();
        assert_eq!(output["answer"], "42");
    }

    #[test]
    fn test_completion_request_with_json_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["items"]
        });
        let request =
            crate::provider::CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![])
                .with_json_schema(schema.clone());
        assert!(request.output_config.is_some());
        let oc = request.output_config.unwrap();
        assert!(oc.format.is_some());
        let fmt = oc.format.unwrap();
        assert_eq!(
            fmt.format_type,
            crate::provider::OutputFormatType::JsonSchema
        );
        assert_eq!(fmt.json_schema.unwrap(), schema);
    }

    #[test]
    fn test_normalize_thinking_downgrades_enabled_for_opus_47() {
        let config = crate::provider::ThinkingConfig::enabled(10000);
        let value = normalize_thinking_for_model(Some(config), None, "claude-opus-4-7");
        let value = value.expect("should return Some");
        assert_eq!(value["type"], "adaptive");
        assert_eq!(value["budget_tokens"], 10000);
    }

    #[test]
    fn test_normalize_thinking_keeps_enabled_for_older_models() {
        let config = crate::provider::ThinkingConfig::enabled(10000);
        let value = normalize_thinking_for_model(Some(config), None, "claude-opus-4-6");
        let value = value.expect("should return Some");
        assert_eq!(value["type"], "enabled");
    }

    #[test]
    fn test_normalize_thinking_handles_date_suffixed_model() {
        let config = crate::provider::ThinkingConfig::enabled(10000);
        let value = normalize_thinking_for_model(Some(config), None, "claude-opus-4-7-20250515");
        let value = value.expect("should return Some");
        assert_eq!(value["type"], "adaptive");
    }

    #[test]
    fn test_normalize_thinking_adaptive_unchanged() {
        let config = crate::provider::ThinkingConfig::adaptive();
        let value = normalize_thinking_for_model(Some(config), None, "claude-opus-4-7");
        let value = value.expect("should return Some");
        assert_eq!(value["type"], "adaptive");
    }

    #[test]
    fn test_is_opus_47_or_later() {
        assert!(is_opus_47_or_later("claude-opus-4-7"));
        assert!(is_opus_47_or_later("claude-opus-4-7-20250515"));
        assert!(is_opus_47_or_later("claude-opus-4.7-20250515"));
        assert!(is_opus_47_or_later("claude-opus-5"));
        assert!(!is_opus_47_or_later("claude-opus-4-6"));
        assert!(!is_opus_47_or_later("claude-sonnet-4-6"));
    }

    #[test]
    fn test_tool_use_id_preserved_in_serialized_content() {
        // Simulate an API response with a tool_use block containing an id
        let json = r#"{
            "content": [
                {"type": "text", "text": "I'll run a command.", "id": "", "name": "", "input": null},
                {"type": "tool_use", "text": "", "id": "call_abc123def456", "name": "bash", "input": {"command": "ls /app/"}}
            ],
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
            "model": "glm-5.1",
            "stop_reason": "tool_use"
        }"#;
        let response: AnthropicResponse = serde_json::from_str(json).unwrap();

        // Replicate the serialization logic from complete_internal
        let mut tool_calls: Vec<serde_json::Value> = Vec::new();
        for block in &response.content {
            if block.content_type == "tool_use" {
                let tool_call = serde_json::json!({
                    "id": block.id,
                    "name": block.name,
                    "arguments": block.input
                });
                tool_calls.push(tool_call);
            }
        }

        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_abc123def456");
        assert_eq!(tool_calls[0]["name"], "bash");

        // Verify the serialized JSON contains the id for downstream consumers
        let serialized = serde_json::to_string(&tool_calls).unwrap();
        assert!(
            serialized.contains("call_abc123def456"),
            "id must survive serialization"
        );
    }
}
