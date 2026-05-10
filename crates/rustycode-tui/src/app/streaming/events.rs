//! Stream event handling for LLM streaming
//!
//! This module handles individual stream events from the LLM provider,
//! including text, thinking, and metadata events.

use std::collections::HashMap;

use anyhow::Result;
use std::sync::mpsc::SyncSender;

use super::ActiveToolUse;
use crate::app::async_::StreamChunk;
use rustycode_protocol::stream_event::StreamEvent;

/// Extract tool input from eager streaming or return an empty string.
///
/// Anthropic-style eager tool use can include the full JSON payload at the
/// start of the block. We keep this helper so the legacy SSE path can reuse it.
pub fn extract_tool_input(input: Option<serde_json::Value>) -> String {
    match input {
        Some(value) if !value.as_object().is_some_and(|obj| obj.is_empty()) => {
            serde_json::to_string(&value).unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// Handle a single stream event, returning false if streaming should stop
pub fn handle_stream_event(
    event: StreamEvent,
    in_tool_use: &mut bool,
    active_tools: &mut HashMap<String, ActiveToolUse>,
    stream_tx: &SyncSender<StreamChunk>,
) -> Result<bool> {
    match event {
        StreamEvent::TextDelta { content } => {
            if let Err(e) = handle_text_event(content, in_tool_use, stream_tx) {
                tracing::debug!("Text event handling failed: {}", e);
            }
        }
        StreamEvent::ThinkingDelta { content } => {
            if let Err(e) = handle_thinking_event(content, in_tool_use, stream_tx) {
                tracing::debug!("Thinking event handling failed: {}", e);
            }
        }
        StreamEvent::ToolCallStarted { id, name } => {
            *in_tool_use = true;
            active_tools.insert(id.clone(), ActiveToolUse::new(id, name, String::new()));
        }
        StreamEvent::ToolInputDelta { id, chunk } => {
            if let Some(tool) = active_tools.get_mut(&id) {
                tool.partial_json.push_str(&chunk);
            }
        }
        StreamEvent::TurnCompleted { stop_reason } => {
            tracing::debug!("Stream stop reason: {}", stop_reason);
            *in_tool_use = false;
            // Active tools are now complete
            active_tools.clear();
            if stream_tx.send(StreamChunk::Done).is_err() {
                tracing::debug!("Channel closed while sending Done on TurnCompleted");
            }
            return Ok(false);
        }
        StreamEvent::TokenUsage {
            input_tokens,
            output_tokens,
        } => {
            if stream_tx
                .send(StreamChunk::TokenUsage {
                    input_tokens: input_tokens as usize,
                    output_tokens: output_tokens as usize,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                })
                .is_err()
            {
                tracing::debug!("Channel closed while sending TokenUsage");
            }
        }
        StreamEvent::CacheUsage {
            cache_read_tokens,
            cache_creation_tokens,
        } => {
            if stream_tx
                .send(StreamChunk::CacheUsage {
                    cache_read_tokens: cache_read_tokens as usize,
                    cache_creation_tokens: cache_creation_tokens as usize,
                })
                .is_err()
            {
                tracing::debug!("Channel closed while sending CacheUsage");
            }
        }
        StreamEvent::Done => {
            *in_tool_use = false;
            active_tools.clear();
            if stream_tx.send(StreamChunk::Done).is_err() {
                tracing::debug!("Channel closed while sending Done");
            }
            return Ok(false);
        }
        // Tool execution lifecycle events — ignore in streaming handler
        StreamEvent::ToolExecStarted { .. } => {}
        StreamEvent::ToolExecCompleted { .. } => {}
        StreamEvent::TurnStarted { .. } => {}
        _ => {}
    }

    Ok(true)
}

/// Handle text content from the stream
///
/// Filters out text that appears to be tool calls to prevent raw JSON
/// from appearing in the UI. Only sends text to TUI when it's actual
/// assistant message content.
pub fn handle_text_event(
    text: String,
    in_tool_use: &mut bool,
    stream_tx: &SyncSender<StreamChunk>,
) -> Result<()> {
    if *in_tool_use {
        tracing::debug!(
            "Suppressing text inside tool use block: {} chars",
            text.len()
        );
        return Ok(());
    }

    // Send text chunk to TUI
    if stream_tx.send(StreamChunk::Text(text)).is_err() {
        tracing::debug!("Channel closed while sending text");
    }

    Ok(())
}

/// Handle thinking/reasoning content
///
/// Sends thinking content to the TUI with a `[thinking]` prefix.
/// Thinking content is suppressed during tool use to maintain clean output.
pub fn handle_thinking_event(
    thinking: String,
    in_tool_use: &bool,
    stream_tx: &SyncSender<StreamChunk>,
) -> Result<()> {
    if *in_tool_use {
        return Ok(());
    }

    if stream_tx.send(StreamChunk::Thinking(thinking)).is_err() {
        tracing::debug!("Channel closed while sending thinking");
    }

    Ok(())
}
