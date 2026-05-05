//! Message conversion utilities for OpenAI-compatible providers

use super::responses::{ResponsesApiContent, ResponsesApiContentPart, ResponsesApiInputItem};
use super::types::OpenAiCompatibleMessage;
use crate::provider::{ChatMessage, CompletionRequest, ContentBlock, MessageContent, MessageRole};

/// Convert a CompletionRequest to OpenAI-compatible messages (simple text-only version).
///
/// This is sufficient for most OpenAI-compatible providers (Azure, Together,
/// OpenRouter, Zhipu, Perplexity, Copilot) that only support text content.
///
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

/// Convert a `CompletionRequest` to Responses API `input` items.
///
/// Returns `(instructions, input_items)` — the system prompt is extracted as
/// `instructions` (the Responses API's replacement for system messages), and
/// all other messages become typed `ResponsesApiInputItem` values.
///
/// # Content mapping
/// - `ContentBlock::Text` → plain text content
/// - `ContentBlock::Image` → `input_image` content part
/// - `ContentBlock::ToolUse` → `function_call` input item
/// - `ContentBlock::ToolResult` → `function_call_output` input item
pub fn convert_messages_to_responses_input(
    request: &CompletionRequest,
) -> (Option<String>, Vec<ResponsesApiInputItem>) {
    let instructions = request.system_prompt.clone();
    let mut items = Vec::new();

    for msg in &request.messages {
        match &msg.role {
            MessageRole::System => {
                tracing::debug!(
                    "skipping system message in messages array for Responses API (sent via `instructions`)"
                );
            }
            MessageRole::Tool(tool_id) => {
                if let MessageContent::Blocks(blocks) = &msg.content {
                    for block in blocks {
                        if let ContentBlock::ToolResult { content, .. } = block {
                            items.push(ResponsesApiInputItem::FunctionCallOutput {
                                call_id: tool_id.clone(),
                                output: content.clone(),
                            });
                        }
                    }
                } else {
                    items.push(ResponsesApiInputItem::FunctionCallOutput {
                        call_id: tool_id.clone(),
                        output: msg.content.to_text(),
                    });
                }
            }
            MessageRole::User | MessageRole::Assistant => {
                let role = match &msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    _ => unreachable!(),
                };

                match &msg.content {
                    MessageContent::Simple(text) => {
                        items.push(ResponsesApiInputItem::Message {
                            role: role.to_string(),
                            content: ResponsesApiContent::text(text),
                        });
                    }
                    MessageContent::Blocks(blocks) => {
                        let mut text_parts = Vec::new();
                        for block in blocks {
                            match block {
                                ContentBlock::Text { text, .. } => {
                                    text_parts.push(ResponsesApiContentPart::InputText {
                                        text: text.clone(),
                                    });
                                }
                                ContentBlock::Image { source, .. } => {
                                    let url = match source.source_type.as_str() {
                                        "url" => source.data.clone(),
                                        _ => {
                                            if let Some((mime, data)) =
                                                crate::provider::resolve_image_to_base64(source)
                                            {
                                                format!("data:{mime};base64,{data}")
                                            } else {
                                                tracing::warn!(
                                                    "Skipping image with unresolvable source"
                                                );
                                                continue;
                                            }
                                        }
                                    };
                                    text_parts.push(ResponsesApiContentPart::InputImage {
                                        image_url: url,
                                    });
                                }
                                ContentBlock::ToolUse { id, name, input } => {
                                    if !text_parts.is_empty() {
                                        items.push(ResponsesApiInputItem::Message {
                                            role: role.to_string(),
                                            content: ResponsesApiContent::Parts(std::mem::take(
                                                &mut text_parts,
                                            )),
                                        });
                                    }
                                    items.push(ResponsesApiInputItem::FunctionCall {
                                        id: format!("fc_{}", id),
                                        call_id: id.clone(),
                                        name: name.clone(),
                                        arguments: input.to_string(),
                                    });
                                }
                                ContentBlock::ToolResult {
                                    tool_use_id,
                                    content,
                                    ..
                                } => {
                                    items.push(ResponsesApiInputItem::FunctionCallOutput {
                                        call_id: tool_use_id.clone(),
                                        output: content.clone(),
                                    });
                                }
                                ContentBlock::Thinking { .. } => {}
                                _ => {}
                            }
                        }
                        if !text_parts.is_empty() {
                            items.push(ResponsesApiInputItem::Message {
                                role: role.to_string(),
                                content: ResponsesApiContent::Parts(text_parts),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    (instructions, items)
}
