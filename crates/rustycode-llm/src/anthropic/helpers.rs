//! Anthropic helper functions.

use crate::provider::ThinkingType;

#[cfg(test)]
use crate::provider::ProviderError;
#[cfg(test)]
use crate::response_debug::ResponseDebugContext;

use super::types::{ToolResultBlock, ToolResultContent};

/// Parse the `content` field of a tool_result JSON value.
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
        ToolResultContent::Text(value.to_string())
    }
}

#[cfg(test)]
pub fn map_anthropic_error(
    status: reqwest::StatusCode,
    body: &str,
    context: &ResponseDebugContext,
) -> ProviderError {
    let retry_after = context.retry_after.map(std::time::Duration::from_secs);
    ProviderError::api_with_retry(status.as_u16(), body, retry_after, context)
}

/// Map Anthropic structured error to ProviderError
#[cfg(test)]
pub fn map_anthropic_structured_error(
    status: reqwest::StatusCode,
    error_type: &str,
    message: &str,
    context: &ResponseDebugContext,
) -> ProviderError {
    let retry_after = context.retry_after.map(std::time::Duration::from_secs);
    ProviderError::api_with_retry(
        status.as_u16(),
        format!("{}: {}", error_type, message),
        retry_after,
        context,
    )
}

pub fn is_opus_47_or_later(model: &str) -> bool {
    model.contains("4-7")
        || model.contains("4.7")
        || model.contains("claude-opus-2026")
        || model.contains("-5")
        || model.contains(".5")
}

/// Normalize thinking config for the target model.
pub fn normalize_thinking_for_model(
    thinking: Option<crate::provider::ThinkingConfig>,
    effort: Option<crate::provider::EffortLevel>,
    model: &str,
) -> Option<serde_json::Value> {
    use crate::provider::{EffortLevel, ThinkingConfig};

    let mut thinking = thinking;

    // Auto-enable adaptive thinking for high effort on capable models
    if thinking.is_none() && is_opus_47_or_later(model) {
        if let Some(e) = effort {
            if matches!(e, EffortLevel::High | EffortLevel::Max) {
                thinking = Some(ThinkingConfig::adaptive());
            }
        }
    }

    // Opus 4.7+ only accepts "adaptive"
    if is_opus_47_or_later(model) {
        if let Some(ref mut t) = thinking {
            if t.thinking_type == ThinkingType::Enabled {
                t.thinking_type = ThinkingType::Adaptive;
            }
        }
    }

    thinking.map(|t| serde_json::to_value(t).unwrap_or_default())
}
