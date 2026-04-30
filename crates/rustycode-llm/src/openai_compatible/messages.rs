//! Message conversion utilities for OpenAI-compatible providers

use super::types::OpenAiCompatibleMessage;
use crate::provider::{ChatMessage, CompletionRequest, MessageRole};

/// Convert a CompletionRequest to OpenAI-compatible messages (simple text-only version).
///
/// This is sufficient for most OpenAI-compatible providers (Azure, Together,
/// OpenRouter, Zhipu, Perplexity, Copilot) that only support text content.
///
/// # Arguments
/// * `request` - The completion request containing messages and optional system prompt
///
/// # Returns
/// A vector of OpenAI-compatible messages
///
/// # Example
/// ```rust,ignore
/// let messages = convert_messages_simple(&request);
/// ```
pub fn convert_messages_simple(request: &CompletionRequest) -> Vec<OpenAiCompatibleMessage> {
    let mut messages = Vec::new();

    // Add system prompt as first message if present
    if let Some(system_prompt) = &request.system_prompt {
        messages.push(OpenAiCompatibleMessage {
            role: "system".to_string(),
            content: Some(system_prompt.clone()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    // Convert each ChatMessage
    for msg in &request.messages {
        let role = match &msg.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool(_) => "tool",
        };

        messages.push(OpenAiCompatibleMessage {
            role: role.to_string(),
            content: Some(msg.content.to_text()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    messages
}

/// Convert messages with explicit system prompt handling.
///
/// Similar to `convert_messages_simple` but allows the system prompt to be
/// provided separately from the request. Used by providers like Azure that
/// may need special system message handling.
///
/// # Arguments
/// * `messages` - The chat messages
/// * `system_prompt` - Optional system prompt to prepend
pub fn convert_messages_with_system(
    messages: &[ChatMessage],
    system_prompt: Option<&str>,
) -> Vec<OpenAiCompatibleMessage> {
    let mut result = Vec::new();

    // Add system prompt if provided
    if let Some(prompt) = system_prompt {
        result.push(OpenAiCompatibleMessage {
            role: "system".to_string(),
            content: Some(prompt.to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    // Convert each ChatMessage
    for msg in messages {
        let role = match &msg.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
            MessageRole::Tool(_) => "tool",
        };

        result.push(OpenAiCompatibleMessage {
            role: role.to_string(),
            content: Some(msg.content.to_text()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    result
}

/// Convert messages with custom role mapping.
///
/// Some providers (like Zhipu) may need custom role mapping for edge cases.
/// This function allows providing a custom role mapper.
///
/// # Arguments
/// * `request` - The completion request
/// * `role_mapper` - Function to map MessageRole to provider-specific role string
pub fn convert_messages_with_role_mapper<F>(
    request: &CompletionRequest,
    role_mapper: F,
) -> Vec<OpenAiCompatibleMessage>
where
    F: Fn(&MessageRole) -> &'static str,
{
    let mut messages = Vec::new();

    if let Some(system_prompt) = &request.system_prompt {
        messages.push(OpenAiCompatibleMessage {
            role: "system".to_string(),
            content: Some(system_prompt.clone()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    for msg in &request.messages {
        messages.push(OpenAiCompatibleMessage {
            role: role_mapper(&msg.role).to_string(),
            content: Some(msg.content.to_text()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    messages
}
