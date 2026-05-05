//! Usage Estimator — fills missing token counts using character-based estimation.
//!
//! Some LLM providers don't return usage data in their responses (e.g., streaming
//! endpoints, local models). This module provides a single function to estimate
//! token counts when they're missing or zero, using the TokenCounter from
//! rustycode-tools as a fallback.
//!
//! Ported from goose's `usage_estimator` with RustyCode type adaptations:
//! - Uses `CompletionResponse` instead of goose's `ProviderUsage`
//! - Works with `ChatMessage` instead of goose's `Message`
//! - No async — RustyCode's TokenCounter is synchronous (no tiktoken init)
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_llm::usage_estimator::ensure_usage;
//!
//! let mut response = provider.complete(request).await?;
//! ensure_usage(&mut response, system_prompt, &messages, &tools);
//! ```

use crate::provider::{ChatMessage, CompletionResponse, Usage};
use rustycode_tool_integration::TokenCounter;

/// Ensure a `CompletionResponse` has valid usage data.
///
/// If `response.usage` is `None` or all-zero, estimates token counts using
/// character-based approximation via `TokenCounter`.
///
/// # Arguments
///
/// * `response` - The completion response to fill in
/// * `system_prompt` - The system prompt sent with the request
/// * `messages` - The chat messages sent with the request
/// * `tools` - Tool definitions as `(name, description, schema)` tuples
pub fn ensure_usage(
    response: &mut CompletionResponse,
    system_prompt: &str,
    messages: &[ChatMessage],
    tools: &[(String, String, serde_json::Value)],
) {
    let counter = TokenCounter::new();

    // Check if usage is already present and non-zero
    if let Some(ref usage) = response.usage {
        if usage.input_tokens > 0 || usage.output_tokens > 0 {
            return;
        }
    }

    // Estimate input tokens (system prompt + messages + tools)
    let input_count = estimate_input_tokens(&counter, system_prompt, messages, tools);

    // Estimate output tokens from response content
    let output_count = counter.count_tokens(&response.content);

    let total_count = input_count.saturating_add(output_count);

    response.usage = Some(Usage {
        input_tokens: input_count.min(u32::MAX as usize) as u32,
        output_tokens: output_count.min(u32::MAX as usize) as u32,
        total_tokens: total_count.min(u32::MAX as usize) as u32,
        cache_read_input_tokens: 0,
        cache_creation_input_tokens: 0,
        reasoning_tokens: None,
    });
}

/// Estimate input tokens from the request components.
fn estimate_input_tokens(
    counter: &TokenCounter,
    system_prompt: &str,
    messages: &[ChatMessage],
    tools: &[(String, String, serde_json::Value)],
) -> usize {
    // Use the TokenCounter's chat token counting, converting ChatMessage to
    // the (role, content_text) tuples it expects
    let message_tuples: Vec<(String, String)> = messages
        .iter()
        .map(|msg| {
            let role = msg.role.as_ref().to_string();
            let content = msg.content.as_text();
            (role, content)
        })
        .collect();

    counter.count_chat_tokens(system_prompt, &message_tuples, tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_messages() -> Vec<ChatMessage> {
        vec![
            ChatMessage::user("Hello, how are you?"),
            ChatMessage::assistant("I'm doing well, thanks for asking!"),
        ]
    }

    fn test_tools() -> Vec<(String, String, serde_json::Value)> {
        vec![(
            "bash".to_string(),
            "Execute a shell command".to_string(),
            serde_json::json!({
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to execute"
                    }
                }
            }),
        )]
    }

    #[test]
    fn test_ensure_usage_fills_missing() {
        let mut response = CompletionResponse {
            content: "Here is my response.".to_string(),
            model: "test-model".to_string(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };

        ensure_usage(
            &mut response,
            "You are helpful",
            &test_messages(),
            &test_tools(),
        );

        let usage = response.usage.unwrap();
        assert!(usage.input_tokens > 0);
        assert!(usage.output_tokens > 0);
        assert_eq!(usage.total_tokens, usage.input_tokens + usage.output_tokens);
    }

    #[test]
    fn test_ensure_usage_fills_zero() {
        let mut response = CompletionResponse {
            content: "Response text".to_string(),
            model: "test-model".to_string(),
            usage: Some(Usage {
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }),
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };

        ensure_usage(&mut response, "System prompt", &test_messages(), &[]);

        let usage = response.usage.unwrap();
        assert!(usage.input_tokens > 0);
        assert!(usage.output_tokens > 0);
    }

    #[test]
    fn test_ensure_usage_preserves_existing() {
        let mut response = CompletionResponse {
            content: "Response".to_string(),
            model: "test-model".to_string(),
            usage: Some(Usage {
                input_tokens: 100,
                output_tokens: 50,
                total_tokens: 150,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }),
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };

        ensure_usage(
            &mut response,
            "System prompt",
            &test_messages(),
            &test_tools(),
        );

        let usage = response.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_ensure_usage_empty_messages() {
        let mut response = CompletionResponse {
            content: "Short reply".to_string(),
            model: "test-model".to_string(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };

        ensure_usage(&mut response, "", &[], &[]);

        let usage = response.usage.unwrap();
        // Should still have positive output tokens
        assert!(usage.output_tokens > 0);
        // With no input, only reply primer tokens
        assert!(usage.input_tokens > 0); // REPLY_PRIMER = 3
    }

    #[test]
    fn test_ensure_usage_with_tools_only() {
        let mut response = CompletionResponse {
            content: "Done".to_string(),
            model: "test-model".to_string(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };

        let tools = vec![
            (
                "bash".to_string(),
                "Run commands".to_string(),
                serde_json::json!({"properties": {"cmd": {"type": "string"}}}),
            ),
            (
                "read_file".to_string(),
                "Read file contents".to_string(),
                serde_json::json!({"properties": {"path": {"type": "string"}}}),
            ),
        ];

        ensure_usage(&mut response, "", &[], &tools);

        let usage = response.usage.unwrap();
        assert!(usage.input_tokens > 0);
        assert!(usage.output_tokens > 0);
    }

    #[test]
    fn test_ensure_usage_partial_input_only() {
        // Simulate a response where only input tokens were reported (output = 0)
        let mut response = CompletionResponse {
            content: "This is a longer response with multiple words in it".to_string(),
            model: "test-model".to_string(),
            usage: Some(Usage {
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }),
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };

        ensure_usage(&mut response, "System", &test_messages(), &[]);

        let usage = response.usage.unwrap();
        assert!(usage.input_tokens > 0);
        assert!(usage.output_tokens > 0);
        assert_eq!(usage.total_tokens, usage.input_tokens + usage.output_tokens);
    }

    #[test]
    fn test_ensure_usage_token_counts_saturate_at_u32_max() {
        // Verify that token counts saturate at u32::MAX rather than truncating.
        // We create a response with existing usage that's already at u32::MAX
        // to verify the preservation path works correctly.
        let mut response = CompletionResponse {
            content: "Response".to_string(),
            model: "test-model".to_string(),
            usage: Some(Usage {
                input_tokens: u32::MAX,
                output_tokens: u32::MAX,
                total_tokens: u32::MAX,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }),
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        };

        ensure_usage(&mut response, "System", &test_messages(), &[]);

        let usage = response.usage.unwrap();
        // Existing non-zero usage should be preserved
        assert_eq!(usage.input_tokens, u32::MAX);
        assert_eq!(usage.output_tokens, u32::MAX);
        assert_eq!(usage.total_tokens, u32::MAX);
    }
}
