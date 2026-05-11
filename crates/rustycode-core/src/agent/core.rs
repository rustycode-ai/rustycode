//! AgentCore — the shared thin LLM↔tool loop.
//!
//! One agent. One loop. No heuristics. The model drives behavior; the loop
//! enforces hard limits only (max turns, wall-clock timeout, context budget).

use anyhow::Result;
use futures::StreamExt;
use rustycode_llm::provider::{ChatMessage, CompletionRequest, LLMProvider, MessageRole};
use rustycode_protocol::tool_names as tn;
use rustycode_protocol::{ContentBlock, MessageContent, ToolCall};
use rustycode_tools::{ToolContext, ToolRegistry};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::info;

use crate::streaming::{StreamEventProcessor, StreamingCallbacks, ToolAccumulator};
use rustycode_agent_runtime::prune_messages;

/// Hard limits for the agent loop. No heuristics — just caps.
pub struct AgentConfig {
    /// Maximum number of LLM↔tool turns.
    pub max_turns: usize,
    /// Wall-clock timeout in seconds.
    pub timeout_secs: u64,
    /// Maximum bytes for a single tool result before truncation.
    pub max_tool_result_bytes: usize,
    /// Temperature for LLM calls.
    pub temperature: f32,
    /// Maximum output tokens for LLM requests (default: 32768).
    pub max_output_tokens: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: 25,
            timeout_secs: 900,
            max_tool_result_bytes: 8000,
            temperature: 0.2,
            max_output_tokens: 32_768,
        }
    }
}

impl AgentConfig {
    pub fn from_env() -> Self {
        let timeout_secs = std::env::var("RUSTYCODE_AGENT_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(900);
        Self {
            timeout_secs,
            ..Default::default()
        }
    }
}

/// Event interface for agent consumers.
pub trait AgentEvents: Send {
    /// Streaming text delta from the assistant.
    fn on_text(&mut self, delta: &str);

    /// A tool call was parsed from the stream.
    fn on_tool_call(&mut self, name: &str, input: &serde_json::Value);

    /// A tool finished executing. Return Some to replace the stored output.
    fn on_tool_result(&mut self, name: &str, output: &str, is_error: bool) -> Option<String> {
        let _ = (name, output, is_error);
        None
    }

    /// The agent loop is done.
    fn on_done(&mut self, result: &AgentResult);
}

/// What the agent loop returns.
pub struct AgentResult {
    /// Final text from the assistant.
    pub final_text: String,
    /// All conversation messages (for carry-forward across retries).
    pub messages: Vec<ChatMessage>,
    /// Token usage.
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
}

/// Stream-accumulation state.
struct CoreStreamCallbacks<'a> {
    assistant_text: &'a mut String,
    completed_tools: &'a mut Vec<CompletedTool>,
    stop_reason: &'a mut Option<String>,
    total_input_tokens: &'a mut u64,
    total_output_tokens: &'a mut u64,
    total_cache_read_tokens: &'a mut u64,
    total_cache_creation_tokens: &'a mut u64,
}

struct CompletedTool {
    id: String,
    name: String,
    input_json: String,
}

impl<'a> StreamingCallbacks for CoreStreamCallbacks<'a> {
    fn on_text(&mut self, text: &str) {
        self.assistant_text.push_str(text);
    }

    fn on_thinking(&mut self, _thinking: &str) {}

    fn on_tool_start(&mut self, _id: &str, _name: &str) {}

    fn on_tool_complete(&mut self, tool: ToolAccumulator) {
        self.completed_tools.push(CompletedTool {
            id: tool.id,
            name: tool.name,
            input_json: tool.partial_json,
        });
    }

    fn on_turn_completed(&mut self, stop_reason: &str) {
        *self.stop_reason = Some(stop_reason.to_string());
    }

    fn on_token_usage(&mut self, input_tokens: u64, output_tokens: u64) {
        *self.total_input_tokens = self.total_input_tokens.saturating_add(input_tokens);
        *self.total_output_tokens = self.total_output_tokens.saturating_add(output_tokens);
    }

    fn on_cache_usage(&mut self, cache_read: u64, cache_creation: u64) {
        *self.total_cache_read_tokens = self.total_cache_read_tokens.saturating_add(cache_read);
        *self.total_cache_creation_tokens = self
            .total_cache_creation_tokens
            .saturating_add(cache_creation);
    }

    fn on_error(&mut self, error_type: &str, message: &str) {
        tracing::error!("Stream error: {} - {}", error_type, message);
    }
}

struct ToolExecOutput {
    output: String,
    success: bool,
}

/// Execute a tool via the shared registry.
fn execute_tool(
    cwd: &Path,
    tool_name: &str,
    tool_json: &str,
    tool_registry: &ToolRegistry,
) -> ToolExecOutput {
    let resolved_name = normalize_tool_name(tool_name);
    let args: serde_json::Value = match serde_json::from_str(tool_json) {
        Ok(v) => v,
        Err(e) => {
            return ToolExecOutput {
                output: format!("Error: Failed to parse tool arguments: {}", e),
                success: false,
            }
        }
    };

    let call = ToolCall {
        call_id: "agent-core".to_string(),
        name: resolved_name.to_string(),
        arguments: args,
    };

    let ctx = ToolContext::new(cwd);
    let result = tool_registry.execute(&call, &ctx);

    if result.success {
        ToolExecOutput {
            output: result.output,
            success: true,
        }
    } else {
        ToolExecOutput {
            output: result
                .error
                .unwrap_or_else(|| "Error executing tool".to_string()),
            success: false,
        }
    }
}

/// Truncate tool output to fit within budget, preserving error context in the tail.
fn truncate_tool_output(output: &str, max_bytes: usize) -> String {
    if output.len() <= max_bytes {
        return output.to_string();
    }

    let out_lower = output.to_lowercase();
    let has_errors = out_lower.contains("error")
        || out_lower.contains("traceback")
        || out_lower.contains("failed")
        || out_lower.contains("segmentation fault")
        || out_lower.contains("command not found");

    let (head_bytes, tail_bytes) = if has_errors {
        (max_bytes / 6, max_bytes * 5 / 6)
    } else {
        (max_bytes / 4, max_bytes * 3 / 4)
    };

    let head_end = output
        .char_indices()
        .take_while(|(i, _)| *i < head_bytes)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);

    let tail_start = output
        .char_indices()
        .rev()
        .skip_while(|(i, _)| output.len() - *i > tail_bytes)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);

    if tail_start > head_end {
        let skipped = tail_start - head_end;
        format!(
            "{}\n\n[...{} bytes truncated...]\n\n{}",
            &output[..head_end],
            skipped,
            &output[tail_start..]
        )
    } else {
        output.to_string()
    }
}

/// Normalize tool names from different providers to our canonical names.
fn normalize_tool_name(name: &str) -> &str {
    match name {
        tn::EDIT => tn::EDIT,
        tn::READ => tn::READ,
        tn::WRITE | "Create" => tn::WRITE,
        tn::BASH | "Shell" => tn::BASH,
        tn::GREP | "Search" => tn::GREP,
        tn::GLOB | "Find" => tn::GLOB,
        _ => name,
    }
}

/// The core agent loop.
///
/// LLM call → stream response → dispatch tools → append results → repeat.
/// Stops when: no tools called, max turns hit, or timeout.
///
/// Callers customize behavior through `AgentEvents` (display, output enrichment).
/// The tool executor uses the provided registry and working directory.
pub async fn run(
    provider: &dyn LLMProvider,
    model: &str,
    system_prompt: &str,
    messages: Vec<ChatMessage>,
    tools_schema: &[serde_json::Value],
    cwd: PathBuf,
    tool_registry: &ToolRegistry,
    config: &AgentConfig,
    events: &mut dyn AgentEvents,
) -> Result<AgentResult> {
    let mut messages = messages;
    let mut final_text = String::new();

    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut total_cache_read_tokens: u64 = 0;
    let mut total_cache_creation_tokens: u64 = 0;

    let start_time = std::time::Instant::now();
    let chunk_timeout = Duration::from_mins(2);
    let max_stream_retries: usize = 3;
    let initial_retry_delay_ms: u64 = 1000;

    for turn in 0..config.max_turns {
        final_text.clear();

        if start_time.elapsed().as_secs() > config.timeout_secs {
            info!(
                "Agent stopped by wall-clock timeout: {}s > {}s",
                start_time.elapsed().as_secs(),
                config.timeout_secs
            );
            break;
        }

        info!("AgentCore turn {}", turn + 1);

        // Prune message history to stay within context budget
        messages = prune_messages(messages);

        let request = CompletionRequest::new(model.to_string(), messages.clone())
            .with_streaming(true)
            .with_max_tokens(config.max_output_tokens)
            .with_temperature(config.temperature)
            .with_system_prompt(system_prompt.to_string())
            .with_tools(tools_schema.to_vec())
            .with_tool_choice(serde_json::json!("auto"));

        // Start stream with retry on transient errors
        let mut stream = {
            let mut result_stream = None;
            let mut final_error = None;

            for attempt in 0..=max_stream_retries {
                match provider.complete_stream(request.clone()).await {
                    Ok(s) => {
                        if attempt > 0 {
                            info!("Stream started on retry attempt {}", attempt);
                        }
                        result_stream = Some(s);
                        break;
                    }
                    Err(e) => {
                        let err_str = format!("{}", e);
                        let is_transient = err_str.contains("429")
                            || err_str.contains("503")
                            || err_str.contains("502")
                            || err_str.contains("500")
                            || err_str.contains("timeout")
                            || err_str.contains("connection");

                        if err_str.contains("context_length")
                            || err_str.contains("too many tokens")
                            || err_str.contains("input too long")
                            || err_str.contains("maximum context")
                            || err_str.contains("token limit")
                            || err_str.contains("reduce the length")
                        {
                            let total = messages.len();
                            if total > 8 {
                                let keep = 6;
                                let trim_from = total - keep;
                                let mut trimmed = Vec::with_capacity(keep + 2);
                                trimmed.push(messages[0].clone());
                                trimmed.push(ChatMessage {
                                    role: MessageRole::User,
                                    content: MessageContent::Simple(
                                        "[Context trimmed due to length. Continue from current state.]"
                                            .to_string(),
                                    ),
                                });
                                for msg in messages.iter().skip(trim_from) {
                                    trimmed.push(msg.clone());
                                }
                                tracing::warn!(
                                    "Context length exceeded, aggressive trim: {} → {} messages",
                                    total,
                                    trimmed.len()
                                );
                                messages = trimmed;
                                continue;
                            }
                        }

                        final_error = Some(e);

                        if is_transient && attempt < max_stream_retries {
                            let delay = initial_retry_delay_ms * (1 << attempt);
                            tracing::warn!(
                                "Stream error (attempt {}/{}): {}. Retrying in {}ms",
                                attempt + 1,
                                max_stream_retries + 1,
                                err_str,
                                delay
                            );
                            tokio::time::sleep(Duration::from_millis(delay)).await;
                        }
                    }
                }
            }

            result_stream.ok_or_else(|| {
                final_error
                    .map(|e| anyhow::anyhow!("Stream failed after retries: {}", e))
                    .unwrap_or_else(|| anyhow::anyhow!("Stream initialization failed"))
            })?
        };

        // Accumulate stream state
        let mut assistant_text = String::new();
        let mut completed_tools: Vec<CompletedTool> = Vec::new();
        let mut stop_reason: Option<String> = None;

        let mut processor = StreamEventProcessor::new();

        loop {
            let chunk = match tokio::time::timeout(chunk_timeout, stream.next()).await {
                Ok(Some(Ok(event))) => event,
                Ok(Some(Err(e))) => {
                    tracing::warn!("Mid-stream error: {}. Ending turn early.", e);
                    break;
                }
                Ok(None) => break,
                Err(_) => {
                    tracing::warn!("Stream chunk timed out. Ending turn early.");
                    break;
                }
            };

            {
                let mut callbacks = CoreStreamCallbacks {
                    assistant_text: &mut assistant_text,
                    completed_tools: &mut completed_tools,
                    stop_reason: &mut stop_reason,
                    total_input_tokens: &mut total_input_tokens,
                    total_output_tokens: &mut total_output_tokens,
                    total_cache_read_tokens: &mut total_cache_read_tokens,
                    total_cache_creation_tokens: &mut total_cache_creation_tokens,
                };
                let keep_going = processor.process_event(chunk, &mut callbacks)?;
                if !keep_going {
                    break;
                }
            }
        }

        if !assistant_text.is_empty() {
            events.on_text(&assistant_text);
        }

        // Handle max_tokens: inject continuation
        if stop_reason.as_deref() == Some("max_tokens") && turn < config.max_turns - 1 {
            info!("Model hit max_tokens, injecting continuation message");

            if !completed_tools.is_empty() {
                tracing::warn!(
                    tool_count = completed_tools.len(),
                    "Tools dropped due to max_tokens truncation"
                );
            }

            if !assistant_text.is_empty() {
                messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: MessageContent::Simple(assistant_text),
                });
            }
            messages.push(ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Simple(
                    "Your response was truncated. Please continue from where you left off."
                        .to_string(),
                ),
            });
            continue;
        }

        if !assistant_text.is_empty() {
            final_text = assistant_text;
        }

        // No tool calls — we're done
        if completed_tools.is_empty() {
            info!("Agent finished (no tool calls)");
            break;
        }

        // Build assistant message with text + tool_use blocks
        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        if !final_text.is_empty() {
            assistant_blocks.push(ContentBlock::text(&final_text));
        }

        // Execute tools and build result blocks
        let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
        for tool in &completed_tools {
            let input: serde_json::Value = match serde_json::from_str(&tool.input_json) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Failed to parse tool JSON for {}: {}", tool.name, e);
                    serde_json::json!({"_raw": tool.input_json})
                }
            };

            assistant_blocks.push(ContentBlock::ToolUse {
                id: tool.id.clone(),
                name: tool.name.clone(),
                input: input.clone(),
            });

            events.on_tool_call(&tool.name, &input);

            // Execute tool
            let exec_result = execute_tool(&cwd, &tool.name, &tool.input_json, tool_registry);
            let truncated = truncate_tool_output(&exec_result.output, config.max_tool_result_bytes);

            let is_error = !exec_result.success
                || truncated.starts_with("Error ")
                || truncated.starts_with("ERROR: ")
                || truncated.starts_with("error: ")
                || (truncated.contains("[exit code:") && !truncated.contains("[exit code: 0]"))
                || truncated.contains("Traceback (most recent call last)")
                || truncated.contains("command not found")
                || truncated.contains("Segmentation fault")
                || truncated.contains("non-zero exit");

            // Let event handler enrich/replace the output
            let final_output = events
                .on_tool_result(&tool.name, &truncated, is_error)
                .unwrap_or(truncated);

            if is_error {
                tool_result_blocks.push(ContentBlock::tool_error(&tool.id, &final_output));
            } else {
                tool_result_blocks.push(ContentBlock::tool_result(&tool.id, &final_output));
            }
        }

        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(assistant_blocks),
        });
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(tool_result_blocks),
        });

        if stop_reason.as_deref() == Some("end_turn") {
            info!("Agent finished (end_turn)");
            break;
        }
    }

    let result = AgentResult {
        final_text,
        messages,
        total_input_tokens,
        total_output_tokens,
        total_cache_read_tokens,
        total_cache_creation_tokens,
    };

    events.on_done(&result);
    Ok(result)
}
