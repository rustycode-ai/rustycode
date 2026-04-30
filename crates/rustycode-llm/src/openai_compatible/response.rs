//! Response building for OpenAI-compatible providers

use super::tools::append_tool_calls_to_content;
use super::types::{OpenAiCompatibleResponse, OpenAiCompatibleUsage};
use crate::provider::{CompletionResponse, ProviderError, Usage};

/// Build a CompletionResponse from OpenAI-compatible response parts.
///
/// This is the standard response builder used across all OpenAI-compatible
/// providers. It handles content extraction, tool call formatting, and usage
/// mapping.
///
/// # Arguments
/// * `response` - The parsed OpenAI-compatible response
///
/// # Returns
/// A CompletionResponse or a ProviderError if the response is malformed.
///
/// # Example
/// ```rust,ignore
/// let completion = build_completion_response(&parsed_response)?;
/// ```
pub fn build_completion_response(
    response: &OpenAiCompatibleResponse,
) -> Result<CompletionResponse, ProviderError> {
    let choice = response
        .choices
        .first()
        .ok_or_else(|| ProviderError::api("no choices in response"))?;

    // Extract content, appending tool calls if present
    let mut content = choice.message.content.clone().unwrap_or_default();

    append_tool_calls_to_content(
        &mut content,
        choice.message.tool_calls.as_deref().unwrap_or(&[]),
    );

    // Map usage
    let usage = response.usage.as_ref().map(|u| Usage {
        input_tokens: u.prompt_tokens,
        output_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
        cache_read_input_tokens: u
            .prompt_tokens_details
            .as_ref()
            .and_then(|d| d.cached_tokens)
            .unwrap_or(0),
        cache_creation_input_tokens: 0,
        reasoning_tokens: None,
    });

    Ok(CompletionResponse {
        content,
        model: response.model.clone(),
        usage,
        stop_reason: choice.finish_reason.clone(),
        citations: None,
        thinking_blocks: choice.message.reasoning_content.clone().map(|r| {
            vec![crate::provider::ThinkingBlock {
                block_type: "thinking".to_string(),
                thinking: r,
                signature: String::new(),
                data: String::new(),
            }]
        }),
        structured_output: None,
    })
}

/// Build a CompletionResponse from individual parts.
///
/// Useful when providers need custom content extraction logic but still
/// want to use the standard usage mapping.
///
/// # Arguments
/// * `content` - The response content string
/// * `model` - The model name
/// * `stop_reason` - Optional stop reason
/// * `usage` - Optional usage information
pub fn build_completion_response_from_parts(
    content: String,
    model: String,
    stop_reason: Option<String>,
    usage: Option<&OpenAiCompatibleUsage>,
) -> CompletionResponse {
    let usage = usage.map(|u| Usage {
        input_tokens: u.prompt_tokens,
        output_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
        cache_read_input_tokens: u
            .prompt_tokens_details
            .as_ref()
            .and_then(|d| d.cached_tokens)
            .unwrap_or(0),
        cache_creation_input_tokens: 0,
        reasoning_tokens: None,
    });

    CompletionResponse {
        content,
        model,
        usage,
        stop_reason,
        citations: None,
        thinking_blocks: None,
        structured_output: None,
    }
}
