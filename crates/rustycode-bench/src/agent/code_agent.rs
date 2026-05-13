//! Code agent — uses an LLM to solve benchmark tasks with real tool access.
//!
//! Uses the same real tool implementations as the TUI via ToolRegistry.
//! Requires `workspace_path()` from the environment (native mode).

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use rustycode_llm::provider::{ContentBlock, MessageContent, MessageRole};
use rustycode_protocol::intent::classify_intent;
use rustycode_protocol::tool_names as tn;
use rustycode_tools::{ToolContext, ToolRegistry};
use rustycode_tools_api::schema::build_tool_schemas_with_examples;
use serde_json::Value;

use super::observer::create_bench_provider;
use super::BenchAgent;
use crate::agent::tools::build_bench_registry;
use crate::environment::BenchEnvironment;

/// Configuration for the code agent.
#[derive(Debug, Clone)]
pub struct CodeAgentConfig {
    /// Model to use (e.g. "claude-sonnet-4-6", "gpt-4o").
    pub model: String,
    /// LLM provider name: "anthropic", "openai", etc.
    pub provider: String,
    /// Maximum number of tool-use turns.
    pub max_turns: usize,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
    /// System prompt for the agent.
    pub system_prompt: String,
    /// Approximate max characters of conversation context before pruning.
    pub max_context_chars: usize,
}

const DEFAULT_SYSTEM_PROMPT: &str = "Solve the task. Use the tools provided.";

impl Default for CodeAgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_string(),
            provider: "anthropic".to_string(),
            max_turns: 30,
            max_tokens: 8192,
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            max_context_chars: 200_000,
        }
    }
}

/// Agent that uses an LLM to solve benchmark tasks with tool access.
pub struct CodeAgent {
    config: CodeAgentConfig,
    provider: Arc<dyn rustycode_llm::LLMProvider>,
    /// Tracks recent bash commands for repetition detection.
    recent_commands: Vec<String>,
    /// Tracks recent tool call fingerprints (name + sorted args) for loop detection.
    recent_tool_calls: Vec<String>,
    /// Accumulated input (prompt) tokens from the last run().
    input_tokens: u64,
    /// Accumulated output (completion) tokens from the last run().
    output_tokens: u64,
}

/// Number of recent bash commands to track for repetition detection.
const REPETITION_WINDOW: usize = 3;

impl CodeAgent {
    #[must_use]
    pub fn new(config: CodeAgentConfig, provider: Arc<dyn rustycode_llm::LLMProvider>) -> Self {
        Self {
            config,
            provider,
            recent_commands: Vec::new(),
            recent_tool_calls: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    /// Create auto-detected from the config's provider field.
    pub fn auto(config: CodeAgentConfig) -> anyhow::Result<Self> {
        let provider = create_bench_provider(&config.provider, &config.model)?;
        Ok(Self::new(config, provider))
    }

    // ── Tool schema generation ──────────────────────────────────────────

    /// Build tool schemas from the registry in canonical Anthropic format.
    ///
    /// Delegates to `rustycode_tools_api::schema` for schema construction,
    /// metadata stripping, and example injection.
    fn build_tool_schemas() -> Vec<Value> {
        let registry = build_bench_registry();
        let tools = registry.list();
        build_tool_schemas_with_examples(&tools, tool_examples)
    }

    // ── Tool execution ────────────────────────────────────────────────

    /// Normalize tool name from various LLM naming conventions.
    fn normalize_tool_name(name: &str) -> &str {
        tn::normalize_tool_name(name)
    }

    /// Fingerprint a tool call for repetition detection.
    /// Uses the tool name + sorted argument values to detect identical calls.
    fn fingerprint_tool_call(name: &str, input: &Value) -> String {
        let sorted = if let Some(obj) = input.as_object() {
            let mut pairs: Vec<(String, String)> = obj
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            pairs
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",")
        } else {
            input.to_string()
        };
        format!("{name}({sorted})")
    }

    /// Execute a tool via the registry with real tool implementations.
    /// Validates arguments against the tool's schema before execution.
    fn execute_tool(registry: &ToolRegistry, tool_use: &ToolUse, ctx: &ToolContext) -> String {
        let normalized_name = Self::normalize_tool_name(&tool_use.name);

        // Pre-validate required fields from the tool's parameter schema.
        let tool_info = registry
            .list()
            .into_iter()
            .find(|info| info.name == normalized_name);
        if let Some(info) = &tool_info {
            if let Some(schema) = info.parameters_schema.as_object() {
                if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                    let missing: Vec<&str> = required
                        .iter()
                        .filter_map(|r| r.as_str())
                        .filter(|field| tool_use.input.get(*field).is_none_or(|v| v.is_null()))
                        .collect();
                    if !missing.is_empty() {
                        let props = schema
                            .get("properties")
                            .and_then(|p| p.as_object())
                            .map(|p| p.keys().map(|k| k.as_str()).collect::<Vec<_>>().join(", "))
                            .unwrap_or_default();
                        return format!(
                            "ERROR: Tool {} is missing required field(s): {}. \
                             The tool accepts these properties: {}. \
                             Your arguments were: {}",
                            tool_use.name,
                            missing.join(", "),
                            props,
                            serde_json::to_string(&tool_use.input)
                                .unwrap_or_else(|_| "<invalid>".to_string())
                        );
                    }
                }
            }
        }

        if let Some(tool) = registry.get(normalized_name) {
            match tool.execute(tool_use.input.clone(), ctx) {
                Ok(output) => output.text,
                Err(e) => format!("ERROR: {e}"),
            }
        } else if tool_use.input.get("command").is_some() {
            // Fallback: unknown tool with "command" field → bash
            if let Some(bash) = registry.get(tn::BASH) {
                match bash.execute(tool_use.input.clone(), ctx) {
                    Ok(output) => output.text,
                    Err(e) => format!("ERROR: {e}"),
                }
            } else {
                format!("ERROR: Unknown tool: {}", tool_use.name)
            }
        } else {
            format!("ERROR: Unknown tool: {}", tool_use.name)
        }
    }

    // ── Parsing ───────────────────────────────────────────────────────

    /// Parse tool_use blocks from an LLM response.
    fn parse_tool_uses(content: &str) -> Vec<ToolUse> {
        let mut tool_uses = Vec::new();

        // Format 1: ```tool ... ``` code fences
        if let Some(tools) = Self::extract_tool_fences(content) {
            for tool in tools {
                tool_uses.push(tool);
            }
            return tool_uses;
        }

        // Format 2/3/4: direct JSON array
        if let Ok(blocks) = serde_json::from_str::<Vec<Value>>(content) {
            for (i, block) in blocks.iter().enumerate() {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if block_type == "tool_use"
                    || block_type == "tool_call"
                    || block_type == "function_call"
                    || block_type == "function"
                    || (block.get("name").is_some() && block_type != "text")
                {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&format!("tool_{i}"))
                        .to_string();
                    let (name, input) = if let Some(func) = block.get("function") {
                        let n = func
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args = func
                            .get("arguments")
                            .and_then(|v| v.as_str())
                            .and_then(|s| serde_json::from_str::<Value>(s).ok())
                            .unwrap_or(serde_json::json!({}));
                        (n, args)
                    } else {
                        let n = block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let inp = block
                            .get("input")
                            .or_else(|| block.get("arguments"))
                            .cloned()
                            .unwrap_or(serde_json::json!({}));
                        (n, inp)
                    };

                    if !name.is_empty() {
                        let name = Self::normalize_tool_name(&name).to_string();
                        tool_uses.push(ToolUse { id, name, input });
                    }
                }
            }
        }

        // Fallback: scan for individual JSON tool objects in ```json blocks.
        if tool_uses.is_empty() {
            let mut search_from = 0;
            while let Some(start) = content[search_from..].find("```json") {
                let abs_start = search_from + start;
                let json_start = abs_start + "```json".len();
                let json_start = if content.as_bytes().get(json_start) == Some(&b'\n') {
                    json_start + 1
                } else {
                    json_start
                };
                if let Some(end) = content[json_start..].find("```") {
                    let json_str = &content[json_start..json_start + end];
                    if let Ok(obj) = serde_json::from_str::<Value>(json_str) {
                        let name = obj
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !name.is_empty() {
                            let id = obj
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&format!("json_{abs_start}"))
                                .to_string();
                            let input = obj
                                .get("input")
                                .or_else(|| obj.get("arguments"))
                                .cloned()
                                .unwrap_or(serde_json::json!({}));
                            let name = Self::normalize_tool_name(&name).to_string();
                            tool_uses.push(ToolUse { id, name, input });
                        }
                    }
                    search_from = json_start + end + 3;
                } else {
                    break;
                }
            }
        }

        tool_uses
    }

    fn extract_tool_fences(content: &str) -> Option<Vec<ToolUse>> {
        let mut tools = Vec::new();
        let mut search_from = 0;
        while let Some(start) = content[search_from..].find("```tool") {
            let abs_start = search_from + start;
            let json_start = abs_start + "```tool".len();
            let json_start = if content.as_bytes().get(json_start) == Some(&b'\n') {
                json_start + 1
            } else {
                json_start
            };

            if let Some(end) = content[json_start..].find("```") {
                let json_str = &content[json_start..json_start + end];
                if let Ok(calls) = serde_json::from_str::<Vec<Value>>(json_str) {
                    for (i, call) in calls.iter().enumerate() {
                        let id = call
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&format!("tool_{i}_{abs_start}"))
                            .to_string();
                        let (name, input) = if let Some(func) = call.get("function") {
                            let n = func
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let args = func
                                .get("arguments")
                                .and_then(|v| v.as_str())
                                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                .unwrap_or(serde_json::json!({}));
                            (n, args)
                        } else {
                            let n = call
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let inp = call
                                .get("arguments")
                                .or_else(|| call.get("input"))
                                .cloned()
                                .unwrap_or(serde_json::json!({}));
                            (n, inp)
                        };

                        if !name.is_empty() {
                            let name = Self::normalize_tool_name(&name).to_string();
                            tools.push(ToolUse { id, name, input });
                        }
                    }
                }
                search_from = json_start + end + 3;
            } else {
                break;
            }
        }

        if tools.is_empty() {
            None
        } else {
            Some(tools)
        }
    }

    /// Extract text content from response, stripping tool fences.
    fn extract_text(content: &str) -> String {
        if let Ok(blocks) = serde_json::from_str::<Vec<Value>>(content) {
            let text_parts: Vec<&str> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect();
            if !text_parts.is_empty() {
                return text_parts.join("\n");
            }
        }

        let mut result = content.to_string();
        while let Some(start) = result.find("```tool") {
            let end = result[start + 7..].find("```").map(|e| start + 7 + e + 3);
            if let Some(end) = end {
                result = format!("{}{}", &result[..start], &result[end..]);
            } else {
                break;
            }
        }
        result.trim().to_string()
    }

    // ── Context management ────────────────────────────────────────────

    fn estimate_context_chars(messages: &[rustycode_llm::ChatMessage]) -> usize {
        messages
            .iter()
            .map(|m| match &m.content {
                MessageContent::Simple(s) => s.len(),
                MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text, .. } => text.len(),
                        ContentBlock::ToolUse { input, .. } => input.to_string().len(),
                        ContentBlock::ToolResult { content, .. } => content.len(),
                        _ => 0,
                    })
                    .sum(),
                _ => 0,
            })
            .sum()
    }

    /// Prune messages to stay within context budget.
    fn prune_messages(messages: &mut Vec<rustycode_llm::ChatMessage>, max_chars: usize) {
        let total = Self::estimate_context_chars(messages);
        if total <= max_chars {
            return;
        }

        let keep_tail = 8;
        if messages.len() <= keep_tail + 1 {
            return;
        }

        let task_recap = messages
            .first()
            .map(|m| {
                let text = match &m.content {
                    MessageContent::Simple(s) => s.clone(),
                    MessageContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text, .. } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                truncate(&text, 500)
            })
            .unwrap_or_default();

        let remove_end = messages.len().saturating_sub(keep_tail);
        let removed_parts: Vec<String> = messages
            .drain(1..remove_end)
            .map(|m| match m.content {
                MessageContent::Simple(s) => s,
                MessageContent::Blocks(blocks) => blocks
                    .into_iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text, .. } => Some(text),
                        ContentBlock::ToolResult { content, .. } => Some(content),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            })
            .collect();
        let removed = removed_parts.join("\n");

        let summary = if removed.len() > 600 {
            let s: String = removed.chars().take(300).collect();
            let e: String = removed
                .chars()
                .rev()
                .take(300)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("{s}...\n...\n{e}")
        } else {
            removed
        };

        let recap_section = if task_recap.is_empty() {
            String::new()
        } else {
            format!("\n\n[ORIGINAL TASK (reminder)]:\n{task_recap}")
        };
        messages.insert(
            1,
            rustycode_llm::ChatMessage::user(format!(
                "[CONTEXT SUMMARY — earlier work was trimmed to save space]\n{summary}{recap_section}"
            )),
        );

        tracing::info!(
            "[code] Pruned context: {} chars → {} chars",
            total,
            Self::estimate_context_chars(messages)
        );
    }

    async fn write_trace(&self, trace: &str, cwd: &Path) {
        let trace_path = cwd.join("conversation_trace.md");
        let _ = tokio::fs::write(&trace_path, trace).await;
    }
}

// ── Free functions ────────────────────────────────────────────────────

struct ToolUse {
    #[allow(dead_code)]
    id: String,
    name: String,
    input: Value,
}

#[cfg(test)]
use rustycode_protocol::text::strip_ansi_escapes as strip_ansi;

fn truncate(s: &str, max_len: usize) -> String {
    rustycode_protocol::text::truncate_with_ellipsis(s, max_len)
}

/// Input examples for tools where usage is ambiguous from the schema alone.
///
/// Only included for tools with non-obvious parameter formats. Simple tools
/// (Read, Write, Grep, etc.) are self-explanatory from their schemas.
fn tool_examples(tool_name: &str) -> Option<Vec<Value>> {
    match tool_name {
        "ApplyPatch" => Some(vec![serde_json::json!({"type": "input_example", "input": {
            "patch": "--- a/file.py\n+++ b/file.py\n@@ -1 +1 @@\n-old\n+new"
        }})]),
        "Edit" => Some(vec![serde_json::json!({"type": "input_example", "input": {
            "path": "main.py",
            "old_string": "x = 1",
            "new_string": "x = 2"
        }})]),
        _ => None,
    }
}

// ── BenchAgent impl ──────────────────────────────────────────────────

#[async_trait::async_trait]
impl BenchAgent for CodeAgent {
    fn name(&self) -> &'static str {
        "code"
    }

    async fn setup(&mut self, _env: &mut dyn BenchEnvironment) -> anyhow::Result<()> {
        Ok(())
    }

    async fn run(
        &mut self,
        instruction: &str,
        env: &mut dyn BenchEnvironment,
    ) -> anyhow::Result<()> {
        // Reset token counters for this run
        self.input_tokens = 0;
        self.output_tokens = 0;

        let cwd = env
            .workspace_path()
            .context("workspace_path required for CodeAgent — use native runner")?;

        let ctx = ToolContext::new(&cwd);
        let registry = build_bench_registry();
        let tools = Self::build_tool_schemas();

        let task_prompt = format!(
            "You have {} turns total. Use them wisely.\n\n\
             CRITICAL RULES — you will be stopped if you violate these:\n\
             1. Read files to understand the issue, then make the MINIMAL fix.\n\
             2. You get at most 4 file edits total. Use them carefully.\n\
             3. Do NOT add tests, docs, or unrelated changes.\n\
             4. Do NOT re-read or re-check files after editing.\n\
             5. NEVER edit the same file more than twice.\n\
             6. After your last edit, respond with text only — no more tool calls.\n\n\
             {instruction}",
            self.config.max_turns
        );
        let mut messages = vec![rustycode_llm::ChatMessage::user(task_prompt)];

        // Classify intent to steer the conversation frame
        let intent = classify_intent(instruction);
        let system_prompt = format!(
            "You are an expert programmer. Read the relevant files, understand the bug, \
             apply the smallest possible fix, and stop. Do not verify your fix. \
             Do not run tests. Just fix it and respond with a brief summary.\n\n{}",
            intent.prompt_suffix()
        );
        tracing::info!("[code] Intent: {:?} → frame applied", intent);

        let mut conversation_trace = format!(
            "# Conversation Trace\n\n## Instruction\n{instruction}\n## Intent: {:?}\n",
            intent
        );

        let mut made_edits = false;
        let mut turns_since_edit = 0usize;
        let mut total_edits = 0usize;
        let mut consecutive_error_turns = 0usize;
        let mut file_edit_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for turn in 0..self.config.max_turns {
            Self::prune_messages(&mut messages, self.config.max_context_chars);

            let request =
                rustycode_llm::CompletionRequest::new(&self.config.model, messages.clone())
                    .with_system_prompt(system_prompt.clone())
                    .with_max_tokens(self.config.max_tokens)
                    .with_temperature(0.2)
                    .with_tools(tools.clone())
                    .with_tool_choice(serde_json::json!("auto"));

            tracing::info!(
                "[code] Turn {}/{} (provider: {})",
                turn + 1,
                self.config.max_turns,
                self.provider.name()
            );

            let response = rustycode_llm::LLMProvider::complete(&*self.provider, request).await?;

            // Accumulate token usage across turns
            if let Some(usage) = &response.usage {
                self.input_tokens += u64::from(usage.input_tokens);
                self.output_tokens += u64::from(usage.output_tokens);
            }

            // Handle max_tokens truncation
            if response.stop_reason.as_deref() == Some("max_tokens")
                && turn < self.config.max_turns - 1
            {
                tracing::info!("[code] Model hit max_tokens, injecting continuation");
                let text = Self::extract_text(&response.content);
                if !text.is_empty() {
                    messages.push(rustycode_llm::ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(text),
                    });
                }
                messages.push(rustycode_llm::ChatMessage::user("Continue.".to_string()));
                continue;
            }

            let text = Self::extract_text(&response.content);

            // Always log the full raw response content for debugging
            conversation_trace.push_str(&format!(
                "\n--- Turn {} — Raw Response ---\n{}\n",
                turn + 1,
                truncate(&response.content, 4000)
            ));

            if !text.is_empty() {
                tracing::info!("[code] LLM: {}", truncate(&text, 200));
            }

            let tool_uses = Self::parse_tool_uses(&response.content);
            if !tool_uses.is_empty() {
                conversation_trace.push_str(&format!(
                    "\n--- Turn {} — Tool Calls ({} total) ---",
                    turn + 1,
                    tool_uses.len()
                ));
                for (i, tu) in tool_uses.iter().enumerate() {
                    let input_preview = match tu.name.as_str() {
                        tn::BASH => tu
                            .input
                            .get("command")
                            .and_then(|v| v.as_str())
                            .map(|s| truncate(s, 500))
                            .unwrap_or_default(),
                        tn::WRITE | tn::READ | tn::EDIT => {
                            let p = tu
                                .input
                                .get("path")
                                .or(tu.input.get("file_path"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            format!("path={p}")
                        }
                        _ => serde_json::to_string(&tu.input).unwrap_or_default(),
                    };
                    conversation_trace.push_str(&format!(
                        "\n  [{}] {} | {}",
                        i + 1,
                        tu.name,
                        input_preview
                    ));
                }
                conversation_trace.push('\n');
            }

            if tool_uses.is_empty() {
                tracing::info!("[code] No more tool calls — agent finished");
                break;
            }

            // Build assistant message with ContentBlocks
            let mut blocks: Vec<ContentBlock> = Vec::new();
            if !text.is_empty() {
                blocks.push(ContentBlock::text(text));
            }
            for tool_use in &tool_uses {
                blocks.push(ContentBlock::tool_use(
                    &tool_use.id,
                    &tool_use.name,
                    tool_use.input.clone(),
                ));
            }
            messages.push(rustycode_llm::ChatMessage {
                role: MessageRole::Assistant,
                content: MessageContent::Blocks(blocks),
            });

            // Execute all tool calls sequentially via the registry.
            let raw_outputs: Vec<String> = tool_uses
                .iter()
                .map(|t| Self::execute_tool(&registry, t, &ctx))
                .collect();

            // Track edit/write operations for early-stop detection.
            let edited_files: Vec<String> = tool_uses
                .iter()
                .filter(|t| t.name == tn::WRITE || t.name == tn::EDIT)
                .filter_map(|t| {
                    t.input
                        .get("path")
                        .or(t.input.get("file_path"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();

            if !edited_files.is_empty() {
                made_edits = true;
                turns_since_edit = 0;
                total_edits += edited_files.len();
                for f in &edited_files {
                    *file_edit_counts.entry(f.clone()).or_insert(0) += 1;
                }
            } else if made_edits {
                turns_since_edit += 1;
            }

            // Early-stop: model has applied its fix and is now wasting turns.
            // 1) No edits for 2+ turns after making at least one edit.
            // 2) Same file edited 3+ times → thrashing.
            // 3) 4+ total edits across all files → scope creep.
            if made_edits && turns_since_edit >= 2 {
                tracing::info!(
                    "[code] Early stop: {} turns since last edit",
                    turns_since_edit
                );
                break;
            }
            if let Some((file, count)) = file_edit_counts.iter().max_by_key(|(_, c)| *c) {
                if *count >= 3 {
                    tracing::info!(
                        "[code] Early stop: '{}' edited {} times (thrashing)",
                        file,
                        count
                    );
                    break;
                }
            }
            if total_edits >= 4 {
                tracing::info!("[code] Early stop: {} total edits (excessive)", total_edits);
                break;
            }

            // Process results: repetition detection, truncation, error detection.
            let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
            let mut error_count_this_turn = 0usize;
            let total_tools = raw_outputs.len();
            for (i, mut output) in raw_outputs.into_iter().enumerate() {
                let tool_use = &tool_uses[i];

                // Repetition detection for bash commands.
                if tool_use.name == tn::BASH {
                    let normalized = tool_use
                        .input
                        .get("command")
                        .and_then(|v| v.as_str())
                        .map(|c| c.split_whitespace().collect::<Vec<_>>().join(" "))
                        .unwrap_or_default();
                    if !normalized.is_empty() {
                        let is_repeat = self.recent_commands.iter().any(|c| c == &normalized);
                        if is_repeat {
                            output.push_str(
                                "\n\nNOTE: You already ran this exact command recently \
                                 with the same output. Consider a different approach.",
                            );
                            tracing::info!(
                                "[code] Repeated command detected: {}",
                                truncate(&normalized, 80)
                            );
                        }
                        self.recent_commands.push(normalized);
                        if self.recent_commands.len() > REPETITION_WINDOW {
                            self.recent_commands.remove(0);
                        }
                    }
                }

                // Generic repetition detection for ALL tool types.
                let fp = Self::fingerprint_tool_call(&tool_use.name, &tool_use.input);
                let repeat_count = self.recent_tool_calls.iter().filter(|c| *c == &fp).count();
                if repeat_count >= 2 {
                    output.push_str(
                        "\n\nSTOP: You have made this exact same tool call multiple times \
                         with the same arguments. It keeps failing. Use a DIFFERENT tool, \
                         different arguments, or respond with your final answer.",
                    );
                    tracing::warn!(
                        "[code] Repeated tool call ({}x): {}",
                        repeat_count + 1,
                        truncate(&fp, 120)
                    );
                }
                self.recent_tool_calls.push(fp);
                if self.recent_tool_calls.len() > 20 {
                    self.recent_tool_calls.remove(0);
                }

                conversation_trace.push_str(&format!(
                    "\n--- Turn {} — Tool Result ({}) ---\n{}\n",
                    turn + 1,
                    tool_use.name,
                    truncate(&output, 1000)
                ));

                const MAX_TOOL_RESULT_CHARS: usize = 4000;
                let context_output = if output.len() > MAX_TOOL_RESULT_CHARS {
                    let head: String = output.chars().take(3000).collect();
                    let tail_start = output.len().saturating_sub(1000);
                    let tail: String = output.chars().skip(tail_start).collect();
                    format!(
                        "{head}\n\n... [{} chars truncated] ...\n\n{tail}",
                        output.len() - MAX_TOOL_RESULT_CHARS
                    )
                } else {
                    output.clone()
                };

                let is_error = context_output.starts_with("Error ")
                    || context_output.starts_with("ERROR: ")
                    || context_output.starts_with("error: ")
                    || (context_output.contains("[exit code:")
                        && !context_output.contains("[exit code: 0]"))
                    || context_output.contains("command not found")
                    || context_output.contains("No such file or directory")
                    || context_output.contains("Permission denied");

                if is_error {
                    error_count_this_turn += 1;
                    tool_result_blocks
                        .push(ContentBlock::tool_error(&tool_use.id, &context_output));
                } else {
                    tool_result_blocks
                        .push(ContentBlock::tool_result(&tool_use.id, &context_output));
                }
            }

            // All tool results in ONE user message with multiple content blocks.
            messages.push(rustycode_llm::ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Blocks(tool_result_blocks),
            });

            // Track consecutive error turns — break if all tools fail repeatedly.
            if error_count_this_turn == total_tools && total_tools > 0 {
                consecutive_error_turns += 1;
            } else {
                consecutive_error_turns = 0;
            }
            if consecutive_error_turns >= 3 {
                tracing::info!(
                    "[code] Early stop: {} consecutive error-only turns",
                    consecutive_error_turns
                );
                break;
            }

            // Write trace incrementally so it survives timeouts
            self.write_trace(&conversation_trace, &cwd).await;
        }

        // Write conversation trace (final write)
        self.write_trace(&conversation_trace, &cwd).await;

        Ok(())
    }

    fn token_usage(&self) -> (u64, u64) {
        (self.input_tokens, self.output_tokens)
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_fence_single_command() {
        let content = "I'll run a command.\n```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"ls -la\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Bash");
    }

    #[test]
    fn parse_tool_fence_multiple_commands() {
        let content = "```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"mkdir foo\"}}, {\"name\": \"bash\", \"arguments\": {\"command\": \"ls foo\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn parse_tool_fence_with_surrounding_text() {
        let content = "Let me check the files.\n\n```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"cat /app/regex.txt\"}}]\n```\n\nThat looks good.";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn parse_direct_json_tool_use_blocks() {
        let content =
            r#"[{"type":"tool_use","id":"tu_1","name":"Bash","input":{"command":"echo hello"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "tu_1");
    }

    #[test]
    fn parse_direct_json_with_arguments_key() {
        let content = r#"[{"name":"Bash","arguments":{"command":"ls -la"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn parse_empty_string_returns_nothing() {
        assert!(CodeAgent::parse_tool_uses("").is_empty());
        assert!(CodeAgent::parse_tool_uses("Just text, no tools.").is_empty());
    }

    #[test]
    fn parse_tool_fence_preserves_id_from_api() {
        let content = "```tool\n[{\"id\": \"call_abc123\", \"name\": \"bash\", \"arguments\": {\"command\": \"ls /app/\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "call_abc123");
        assert_eq!(tools[0].name, "Bash");
    }

    #[test]
    fn parse_tool_fence_generates_synthetic_id_when_missing() {
        let content = "```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"ls\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert!(tools[0].id.starts_with("tool_"));
    }

    #[test]
    fn parse_mixed_text_and_tool_blocks() {
        let content = r#"[{"type":"text","text":"I'll run this."},{"type":"tool_use","id":"tu_0","name":"Bash","input":{"command":"pwd"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn parse_tool_fence_accepts_any_tool_name() {
        let content = "```tool\n[{\"name\": \"apply_patch\", \"arguments\": {\"patch\": \"--- a/file\\n+++ b/file\\n@@ -1 +1 @@\\n-old\\n+new\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "ApplyPatch");
    }

    #[test]
    fn parse_tool_fence_skips_empty_names() {
        let content = "```tool\n[{\"name\": \"\", \"arguments\": {\"command\": \"ls\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert!(tools.is_empty());
    }

    #[test]
    fn extract_text_from_plain_string() {
        let text = CodeAgent::extract_text("Hello, world!");
        assert_eq!(text, "Hello, world!");
    }

    #[test]
    fn extract_text_strips_tool_fences() {
        let content = "Let me check the files.\n```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"ls\"}}]\n```\nDone.";
        let text = CodeAgent::extract_text(content);
        assert!(!text.contains("```tool"));
        assert!(text.contains("Let me check"));
        assert!(text.contains("Done."));
    }

    #[test]
    fn extract_text_strips_multiple_fences() {
        let content = "Step 1\n```tool\n[{\"name\":\"bash\",\"arguments\":{\"command\":\"a\"}}]\n```\nStep 2\n```tool\n[{\"name\":\"bash\",\"arguments\":{\"command\":\"b\"}}]\n```\nDone";
        let text = CodeAgent::extract_text(content);
        assert!(!text.contains("```tool"));
        assert!(text.contains("Step 1"));
        assert!(text.contains("Step 2"));
    }

    #[test]
    fn extract_text_from_json_blocks() {
        let content =
            r#"[{"type":"text","text":"First part"},{"type":"text","text":"Second part"}]"#;
        let text = CodeAgent::extract_text(content);
        assert_eq!(text, "First part\nSecond part");
    }

    #[test]
    fn extract_tool_fences_returns_none_for_no_fences() {
        assert!(CodeAgent::extract_tool_fences("no fences here").is_none());
    }

    #[test]
    fn extract_tool_fences_ignores_non_tool_fences() {
        assert!(CodeAgent::extract_tool_fences("```bash\nls\n```").is_none());
    }

    #[test]
    fn parse_fence_with_input_key_raw_api_format() {
        let content =
            "```tool\n[{\"name\": \"bash\", \"input\": {\"command\": \"find . -name '*.rs'\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn parse_write_file_tool() {
        let content = r#"[{"type":"tool_use","id":"wf_1","name":"Write","input":{"path":"test.py","content":"print('hi')"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Write");
    }

    #[test]
    fn parse_read_file_tool() {
        let content =
            r#"[{"type":"tool_use","id":"rf_1","name":"Read","input":{"path":"main.py"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Read");
    }

    #[test]
    fn parse_edit_file_tool() {
        let content = r#"[{"type":"tool_use","id":"ef_1","name":"Edit","input":{"path":"a.py","old_string":"x=1","new_string":"x=2"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Edit");
    }

    #[test]
    fn parse_grep_tool() {
        let content =
            r#"[{"type":"tool_use","id":"gr_1","name":"Grep","input":{"pattern":"TODO"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Grep");
    }

    #[test]
    fn parse_glob_tool() {
        let content =
            r#"[{"type":"tool_use","id":"gb_1","name":"Glob","input":{"pattern":"**/*.py"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Glob");
    }

    #[test]
    fn strip_ansi_removes_color_codes() {
        assert_eq!(strip_ansi("\x1b[31mFAILED\x1b[0m"), "FAILED");
        assert_eq!(strip_ansi("\x1b[32mPASSED\x1b[0m test"), "PASSED test");
        assert_eq!(strip_ansi("no escape codes"), "no escape codes");
        assert_eq!(
            strip_ansi("\x1b[1;33mwarn\x1b[0m: \x1b[36mmsg\x1b[0m"),
            "warn: msg"
        );
        assert_eq!(strip_ansi("\x1b[2J\x1b[Hclear"), "clear");
    }

    #[test]
    fn strip_ansi_removes_cursor_and_color() {
        let with_ansi = "\x1B[31mRed text\x1B[0m and \x1B[2J\x1B[Hcursor stuff";
        let clean = strip_ansi(with_ansi);
        assert_eq!(clean, "Red text and cursor stuff");
    }

    #[test]
    fn truncate_multibyte_utf8_safe() {
        let s = "café résumé 数据";
        let truncated = truncate(s, 5);
        assert!(truncated.ends_with("..."));
        assert!(truncated.starts_with("café"));
    }

    #[test]
    fn prune_messages_noop_when_under_limit() {
        let msgs = vec![rustycode_llm::ChatMessage::user("Hello".to_string())];
        let mut msgs = msgs;
        CodeAgent::prune_messages(&mut msgs, 100_000);
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn parse_json_fence_single_tool() {
        let content = "I'll run the tests.\n```json\n{\"name\": \"bash\", \"input\": {\"command\": \"pytest\"}}\n```\nLet me check.";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "Bash");
    }

    #[test]
    fn parse_json_fence_no_tool_name_ignored() {
        let content = "```json\n{\"type\": \"text\", \"content\": \"hello\"}\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert!(tools.is_empty());
    }

    #[test]
    fn build_tool_schemas_has_core_tools() {
        let schemas = CodeAgent::build_tool_schemas();
        let names: Vec<&str> = schemas
            .iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .collect();
        // File tools
        assert!(names.contains(&"Read"), "missing read_file");
        assert!(names.contains(&"Write"), "missing write_file");
        assert!(names.contains(&"Edit"), "missing edit_file");
        assert!(names.contains(&"ListDir"), "missing list_dir");
        // Search tools
        assert!(names.contains(&"Grep"), "missing grep");
        assert!(names.contains(&"Glob"), "missing glob");
        assert!(names.contains(&"ApplyPatch"), "missing apply_patch");
        // Bash
        assert!(names.contains(&"Bash"), "missing bash");
        // Git (read-only)
        assert!(names.contains(&"GitStatus"), "missing git_status");
        assert!(names.contains(&"GitDiff"), "missing git_diff");
        assert!(names.contains(&"GitLog"), "missing git_log");
        // No interactive tools
        assert!(
            !names.contains(&"question"),
            "question should not be registered"
        );
        assert!(
            !names.contains(&"ask_user"),
            "ask_user should not be registered"
        );
    }

    #[test]
    fn build_tool_schemas_examples_only_for_ambiguous_tools() {
        let schemas = CodeAgent::build_tool_schemas();
        // Tools with non-obvious usage get examples
        let apply_patch = schemas
            .iter()
            .find(|s| s["name"].as_str() == Some("ApplyPatch"))
            .expect("ApplyPatch should exist");
        assert!(
            apply_patch["examples"].is_array(),
            "ApplyPatch should have examples"
        );
        let edit = schemas
            .iter()
            .find(|s| s["name"].as_str() == Some("Edit"))
            .expect("Edit should exist");
        assert!(edit["examples"].is_array(), "Edit should have examples");

        // Self-explanatory tools should NOT have examples
        let bash = schemas
            .iter()
            .find(|s| s["name"].as_str() == Some("Bash"))
            .expect("Bash should exist");
        assert!(
            bash.get("examples").is_none(),
            "Bash should not have examples (self-explanatory)"
        );
        let read = schemas
            .iter()
            .find(|s| s["name"].as_str() == Some("Read"))
            .expect("Read should exist");
        assert!(
            read.get("examples").is_none(),
            "Read should not have examples (self-explanatory)"
        );
    }

    #[test]
    fn build_tool_schemas_strips_metadata_and_simplifies_null_types() {
        let schemas = CodeAgent::build_tool_schemas();
        // No schema should have $schema or title
        for schema in &schemas {
            let input = &schema["input_schema"];
            assert!(
                input.get("$schema").is_none(),
                "{} schema should not have $schema",
                schema["name"]
            );
            assert!(
                input.get("title").is_none(),
                "{} schema should not have title",
                schema["name"]
            );
            // No property should have type: ["string", "null"]
            if let Some(props) = input.get("properties").and_then(|p| p.as_object()) {
                for (name, prop) in props {
                    if let Some(arr) = prop.get("type").and_then(|t| t.as_array()) {
                        assert!(
                            !arr.iter().any(|v| v.as_str() == Some("null")),
                            "{}.{name} should not have null in type array",
                            schema["name"]
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn strip_schema_removes_dollar_schema_and_title() {
        use rustycode_tools_api::schema::strip_schema_metadata;
        let input = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": "MyParams",
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "count": {"type": ["integer", "null"]}
            }
        });
        let output = strip_schema_metadata(input);
        assert!(output.get("$schema").is_none());
        assert!(output.get("title").is_none());
        assert_eq!(output["properties"]["count"]["type"], "integer");
    }

    #[test]
    fn normalize_tool_name_maps_common_aliases() {
        assert_eq!(CodeAgent::normalize_tool_name(tn::EDIT), tn::EDIT);
        assert_eq!(CodeAgent::normalize_tool_name(tn::READ), tn::READ);
        assert_eq!(CodeAgent::normalize_tool_name(tn::WRITE), tn::WRITE);
        assert_eq!(CodeAgent::normalize_tool_name(tn::BASH), tn::BASH);
        assert_eq!(CodeAgent::normalize_tool_name(tn::GREP), tn::GREP);
        assert_eq!(CodeAgent::normalize_tool_name(tn::GLOB), tn::GLOB);
        assert_eq!(CodeAgent::normalize_tool_name(tn::BASH), tn::BASH);
        assert_eq!(CodeAgent::normalize_tool_name(tn::EDIT), tn::EDIT);
        assert_eq!(CodeAgent::normalize_tool_name("unknown"), "unknown");
    }
}
