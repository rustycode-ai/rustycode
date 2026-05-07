//! Anthropic helper functions.
//!
//! Error mapping, response parsing, thinking normalization, and cache utilities.

use crate::provider::ProviderError;
use crate::response_debug::ResponseDebugContext;
use crate::retry::extract_retry_after_ms;
use std::time::Duration;

use super::types::{
    AnthropicMessage, AnthropicRequestContent, CacheControl, ContentBlock, ToolResultBlock,
    ToolResultContent,
};

/// Parse the `content` field of a tool_result JSON value.
/// Anthropic allows either a plain string or an array of typed blocks.
pub fn parse_tool_result_content(value: &serde_json::Value) -> ToolResultContent {
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
pub fn map_anthropic_error(
    status: reqwest::StatusCode,
    error_text: &str,
    headers: &reqwest::header::HeaderMap,
) -> ProviderError {
    let debug = ResponseDebugContext::from_response_headers(headers);
    match status.as_u16() {
        401 | 403 => ProviderError::Auth(debug.format_error_message(error_text)),
        429 => {
            let retry_delay = extract_retry_after_ms(headers).map(Duration::from_millis);
            ProviderError::RateLimited { retry_delay }
        }
        400 => ProviderError::Api(debug.format_error_message(error_text)),
        404 => ProviderError::InvalidModel(debug.format_error_message(error_text)),
        502..=504 => ProviderError::Network(
            debug.format_error_message(&format!("service unavailable: {}", error_text)),
        ),
        529 => ProviderError::Network(
            debug.format_error_message(&format!("Anthropic API overloaded: {}", error_text)),
        ),
        _ => ProviderError::Api(debug.format_error_message(&format!(
            "HTTP {}: {}",
            status.as_u16(),
            error_text
        ))),
    }
}

/// Map Anthropic structured error to ProviderError
/// See: https://platform.claude.com/docs/en/api/errors
pub fn map_anthropic_structured_error(
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

    let mut debug = ResponseDebugContext::from_response_headers(headers);
    debug.error_type = Some(error_type.to_string());

    match error_type {
        "invalid_request_error" => ProviderError::Api(debug.format_error_message(&error_msg)),
        "authentication_error" | "permission_denied_error" => {
            ProviderError::Auth(debug.format_error_message(&error_msg))
        }
        "not_found_error" => ProviderError::InvalidModel(debug.format_error_message(&error_msg)),
        "rate_limit_error" => {
            let retry_delay = extract_retry_after_ms(headers).map(Duration::from_millis);
            ProviderError::RateLimited { retry_delay }
        }
        "api_error" | "internal_server_error" => ProviderError::Api(
            debug.format_error_message(&format!("Anthropic API error: {}", message)),
        ),
        "overloaded_error" => ProviderError::Network(
            debug.format_error_message(&format!("Anthropic API overloaded: {}", message)),
        ),
        _ => map_anthropic_error(status, &error_msg, headers),
    }
}

/// Check if a model ID is Opus 4.7 or later (where thinking.type="enabled" is removed).
pub fn is_opus_47_or_later(model: &str) -> bool {
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
pub fn normalize_thinking_for_model(
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
                Some(crate::provider::EffortLevel::Xhigh | crate::provider::EffortLevel::Max)
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
pub fn apply_cache_to_last_messages(messages: &mut [AnthropicMessage], count: usize) {
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
