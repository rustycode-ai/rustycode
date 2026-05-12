use anyhow::Result;
use rustycode_llm::provider::{ChatMessage, CompletionRequest, LLMProvider, MessageRole};
use rustycode_protocol::{ContentBlock, MessageContent};
use rustycode_tools::ToolRegistry;
use std::path::Path;

use crate::headless::config::{HEADLESS_SYSTEM_PROMPT, RETRY_SYSTEM_PROMPT};
use crate::headless::types::HeadlessTaskResult;

/// Run a single agentic task to completion (headless, no TUI).
///
/// This drives the standard agent loop:
/// 1. Send user prompt (+ tool results) to the LLM
/// 2. Process the streamed response
/// 3. If the LLM requests tool calls, execute them and feed results back
/// 4. Repeat until the LLM stops with no tool calls
///
/// Returns the final text response from the LLM.
pub async fn run_headless_task(
    provider: &dyn LLMProvider,
    model: &str,
    tools_schema: &[serde_json::Value],
    task: &str,
    cwd: &Path,
    tool_registry: &ToolRegistry,
) -> Result<String> {
    Ok(run_headless_task_core(
        provider,
        model,
        tools_schema,
        task,
        cwd,
        1,
        tool_registry,
        None,
        None,
        None,
    )
    .await?
    .final_text)
}

/// Run a headless task using AgentCore (the clean LLM↔tool loop).
///
/// This is the new production path - no heuristics, no nudges, no tracking.
/// The model drives behavior; the loop enforces hard limits only.
///
/// The retry logic (for iterations > 1) is handled by the caller injecting
/// appropriate messages before calling this function.
pub async fn run_headless_task_core(
    provider: &dyn LLMProvider,
    model: &str,
    tools_schema: &[serde_json::Value],
    task: &str,
    cwd: &Path,
    iteration: usize,
    tool_registry: &ToolRegistry,
    prior_messages: Option<Vec<ChatMessage>>,
    system_prompt_override: Option<&str>,
    op_tx_callback: Option<
        Box<dyn FnOnce(tokio::sync::mpsc::UnboundedSender<rustycode_protocol::Op>) + Send>,
    >,
) -> Result<HeadlessTaskResult> {
    let dir_listing = if let Ok(mut entries) = tokio::fs::read_dir(cwd).await {
        let mut names = Vec::new();
        loop {
            match entries.next_entry().await {
                Ok(Some(e)) => {
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = e.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                    if is_dir {
                        names.push(format!("{}/", name));
                    } else {
                        names.push(name);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        names.join("\n")
    } else {
        String::from("(could not read directory)")
    };

    let task_with_context = format!(
        "Working directory: {} (contains {} files/dirs)\n\n{}\n\n---\n\n\
        IMPORTANT: Read this task carefully. Identify the EXACT success criteria before starting. \
        Extract specific requirements: file names, function signatures, expected output format, \
        test commands. Then implement step by step, verifying after EACH change.\n\n---\n\n{}",
        cwd.display(),
        dir_listing.lines().count(),
        dir_listing,
        task
    );

    let had_prior = prior_messages.is_some();
    let mut messages: Vec<ChatMessage> = if let Some(mut prior) = prior_messages {
        let mut file_contents = String::new();
        for msg in &prior {
            if let MessageContent::Blocks(blocks) = &msg.content {
                for block in blocks {
                    if let ContentBlock::ToolResult { content, .. } = block {
                        if content.lines().count() > 5
                            && content.len() > 200
                            && content.len() > file_contents.len()
                        {
                            file_contents = content.clone();
                        }
                    }
                }
            }
        }

        let nudge = if file_contents.is_empty() {
            "The previous attempt FAILED — you only read/explored files without making \
            any changes. This is your FINAL retry.\n\n\
            MANDATORY: Your VERY FIRST tool call MUST be write_file or edit_file. \
            Do NOT run bash grep, read_file, list_dir, or glob. \
            Write the solution NOW, then verify with bash."
                .to_string()
        } else {
            format!(
                "The previous attempt FAILED — you only read/explored files without making \
                any changes. This is your FINAL retry.\n\n\
                MANDATORY: Your VERY FIRST tool call MUST be write_file or edit_file. \
                Do NOT run bash grep, read_file, list_dir, or glob.\n\n\
                You already read the file. Here is the content:\n```\n{}\n```\n\n\
                Based on this content, write the fix NOW. Then verify with bash.",
                file_contents.chars().take(8000).collect::<String>()
            )
        };

        prior.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(nudge),
        });
        prior
    } else {
        vec![ChatMessage::user(task_with_context)]
    };

    if iteration > 1 && !had_prior {
        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Simple(
                "I'll start making changes immediately.\n\n".to_string(),
            ),
        });
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(
                "Good. Now use write_file or edit_file to make your first change. \
                Do NOT read any files — you already know what to do from the previous attempt. \
                Pick a file and WRITE CODE NOW."
                    .to_string(),
            ),
        });
    }

    let config = rustycode_agent_runtime::AgentConfig::from_env();
    let mut events = crate::headless::events::HeadlessAgentBridge::new();
    let (op_tx, op_rx) = tokio::sync::mpsc::unbounded_channel();

    if let Some(callback) = op_tx_callback {
        callback(op_tx);
    }

    let mut session = rustycode_agent_runtime::AgentSession::new(config, cwd.to_path_buf())
        .with_op_receiver(op_rx);
    let system_prompt = if let Some(override_prompt) = system_prompt_override {
        override_prompt.to_string()
    } else if iteration > 1 {
        RETRY_SYSTEM_PROMPT.to_string()
    } else {
        HEADLESS_SYSTEM_PROMPT.to_string()
    };
    let result = session
        .run(
            provider,
            model,
            &system_prompt,
            messages,
            tools_schema,
            tool_registry,
            &mut events,
        )
        .await?;

    Ok(HeadlessTaskResult {
        final_text: result.final_text,
        messages: result.messages,
        total_input_tokens: result.total_input_tokens,
        total_output_tokens: result.total_output_tokens,
        total_cache_read_tokens: result.total_cache_read_tokens,
        total_cache_creation_tokens: result.total_cache_creation_tokens,
    })
}
