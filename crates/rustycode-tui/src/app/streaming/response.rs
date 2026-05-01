//! Main LLM response streaming function
//!
//! This module contains the core `stream_llm_response` function that handles
//! the full conversation lifecycle including tool use detection, execution, and continuation.

use anyhow::{Context, Result};
use futures::StreamExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use crate::app::async_::StreamChunk;

/// Send a stream chunk, logging if the channel is closed.
///
/// Channel closure typically means the TUI consumer exited (user cancelled, session ended).
fn send_chunk(tx: &SyncSender<StreamChunk>, chunk: StreamChunk) {
    if let Err(e) = tx.send(chunk) {
        tracing::debug!("Stream chunk dropped (channel closed): {:?}", e.0);
    }
}

/// RAII guard that guarantees `StreamChunk::Done` is sent when dropped.
///
/// The TUI relies on receiving `Done` to release the streaming guard and clear
/// `is_streaming`. Without this, early returns via `?` would leave the TUI in a
/// permanently stuck state. Double-sending `Done` is safe — the receiver ignores extras.
struct DoneGuard {
    stream_tx: SyncSender<StreamChunk>,
}

impl DoneGuard {
    fn new(stream_tx: SyncSender<StreamChunk>) -> Self {
        Self { stream_tx }
    }
}

impl Drop for DoneGuard {
    fn drop(&mut self) {
        send_chunk(&self.stream_tx, StreamChunk::Done);
    }
}
use crate::task_extraction::extract_todos_from_tool_result;
use crate::{ErrorTracker, FileReadCache};
use rustycode_config::api_key_env_name;
use rustycode_llm::provider::{ChatMessage, CompletionRequest, MessageRole};
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_protocol::{ContentBlock, MessageContent};
use secrecy::ExposeSecret;

use super::tool_detection::handle_message_delta;
use super::tool_execution::{execute_tool, snapshot_files_for_undo};
use super::{parse_tool_parameters, ActiveToolUse, ToolExecutionResult, ToolUseAction};

/// Configuration for streaming LLM responses
///
/// Builder pattern to handle the many parameters needed for `stream_llm_response`.
pub struct StreamConfig {
    pub content: String,
    pub cwd: std::path::PathBuf,
    pub stream_tx: SyncSender<StreamChunk>,
    pub workspace_context: Option<String>,
    pub stop_signal: Option<Arc<AtomicBool>>,
    pub tools_schema: Option<Vec<serde_json::Value>>,
    pub approval_rx: Option<std::sync::mpsc::Receiver<bool>>,
    pub question_rx: Option<std::sync::mpsc::Receiver<String>>,
    pub agent_mode: Option<crate::agent_mode::AgentMode>,
    pub file_read_cache: Option<Arc<StdMutex<FileReadCache>>>,
    pub error_tracker: Option<Arc<StdMutex<ErrorTracker>>>,
    pub todo_state: Option<rustycode_tools::todo::TodoState>,
    pub conversation_history: Option<Vec<ChatMessage>>,
    pub tool_registry: Option<Arc<rustycode_tools::ToolRegistry>>,
    pub plan_mode: Option<rustycode_orchestration::plan_mode::PlanMode>,
    pub ai_mode: Option<crate::agent_mode::AiMode>,
    pub orchestration_guidance: Option<String>,
    pub phase_context: Option<String>,
    pub orchestration:
        Option<Arc<StdMutex<crate::app::orchestration_integration::OrchestrationIntegration>>>,
}

impl StreamConfig {
    /// Create a new config with required parameters
    pub fn new(content: &str, cwd: &Path, stream_tx: SyncSender<StreamChunk>) -> Self {
        Self {
            content: content.to_string(),
            cwd: cwd.to_path_buf(),
            stream_tx,
            workspace_context: None,
            stop_signal: None,
            tools_schema: None,
            approval_rx: None,
            question_rx: None,
            agent_mode: None,
            file_read_cache: None,
            error_tracker: None,
            todo_state: None,
            conversation_history: None,
            tool_registry: None,
            plan_mode: None,
            ai_mode: None,
            orchestration_guidance: None,
            phase_context: None,
            orchestration: None,
        }
    }

    pub fn workspace_context_opt(mut self, ctx: Option<String>) -> Self {
        self.workspace_context = ctx;
        self
    }

    pub fn stop_signal_opt(mut self, signal: Option<Arc<AtomicBool>>) -> Self {
        self.stop_signal = signal;
        self
    }

    pub fn tools_schema_opt(mut self, schema: Option<Vec<serde_json::Value>>) -> Self {
        self.tools_schema = schema;
        self
    }

    pub fn approval_rx_opt(mut self, rx: Option<std::sync::mpsc::Receiver<bool>>) -> Self {
        self.approval_rx = rx;
        self
    }

    pub fn question_rx_opt(mut self, rx: Option<std::sync::mpsc::Receiver<String>>) -> Self {
        self.question_rx = rx;
        self
    }

    pub fn agent_mode_opt(mut self, mode: Option<crate::agent_mode::AgentMode>) -> Self {
        self.agent_mode = mode;
        self
    }

    pub fn file_read_cache_opt(mut self, cache: Option<Arc<StdMutex<FileReadCache>>>) -> Self {
        self.file_read_cache = cache;
        self
    }

    pub fn error_tracker_opt(mut self, tracker: Option<Arc<StdMutex<ErrorTracker>>>) -> Self {
        self.error_tracker = tracker;
        self
    }

    pub fn todo_state_opt(mut self, state: Option<rustycode_tools::todo::TodoState>) -> Self {
        self.todo_state = state;
        self
    }

    pub fn conversation_history_opt(mut self, history: Option<Vec<ChatMessage>>) -> Self {
        self.conversation_history = history;
        self
    }

    pub fn tool_registry_opt(
        mut self,
        registry: Option<Arc<rustycode_tools::ToolRegistry>>,
    ) -> Self {
        self.tool_registry = registry;
        self
    }

    pub fn plan_mode_opt(
        mut self,
        mode: Option<rustycode_orchestration::plan_mode::PlanMode>,
    ) -> Self {
        self.plan_mode = mode;
        self
    }

    pub fn ai_mode_opt(mut self, mode: Option<crate::agent_mode::AiMode>) -> Self {
        self.ai_mode = mode;
        self
    }

    pub fn orchestration_guidance_opt(mut self, guidance: Option<String>) -> Self {
        self.orchestration_guidance = guidance;
        self
    }

    pub fn phase_context_opt(mut self, ctx: Option<String>) -> Self {
        self.phase_context = ctx;
        self
    }

    pub fn orchestration_opt(
        mut self,
        orch: Option<
            Arc<StdMutex<crate::app::orchestration_integration::OrchestrationIntegration>>,
        >,
    ) -> Self {
        self.orchestration = orch;
        self
    }
}

/// Fix conversation structure before sending to LLM.
///
/// Ensures messages alternate properly and removes problematic patterns
/// that would cause API errors with providers like Anthropic/Claude.
///
/// Important: preserves tool_use/tool_result ordering. Anthropic requires
/// assistant messages with tool_use blocks to be immediately followed by
/// user messages with tool_result content.
fn fix_conversation_messages(messages: &mut Vec<ChatMessage>) {
    use rustycode_llm::provider::MessageRole;

    // Remove leading non-system/non-user messages
    while messages
        .first()
        .is_some_and(|m| !matches!(m.role, MessageRole::System | MessageRole::User))
    {
        messages.remove(0);
    }

    // Remove leading orphaned tool_result messages whose parent assistant+tool_use
    // was lost (e.g., from message cap/pruning). These are user-role messages with
    // Simple content containing "type":"tool_result" JSON. Without their parent
    // assistant message, the API will reject them.
    while let Some(msg) = messages.first() {
        if msg.role != MessageRole::User {
            break;
        }
        let text = msg.content.as_text();
        if text.contains("\"type\":\"tool_result\"") {
            tracing::debug!("fix_conversation_messages: dropping orphaned tool_result");
            messages.remove(0);
        } else {
            break;
        }
    }

    // Remove trailing assistant messages only if they DON'T contain tool_use.
    // Tool-use assistant messages must be kept because they're followed by
    // tool_result user messages in the same turn.
    while messages.last().is_some_and(|m| {
        if m.role != MessageRole::Assistant {
            return false;
        }
        // Keep assistant messages that have tool_use blocks (Blocks content)
        !matches!(&m.content, MessageContent::Blocks(blocks) if blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { .. })))
    }) {
        messages.pop();
    }

    // Merge consecutive same-role messages (except system and tool-related).
    // Tool_use/tool_result messages must NOT be merged — they have specific
    // ordering requirements from the API.
    let mut i = 1;
    while i < messages.len() {
        // Don't merge if either message involves tool blocks
        let prev_has_tools = matches!(&messages[i - 1].content, MessageContent::Blocks(blocks) if blocks.iter().any(|b| b.is_tool_use()));
        let curr_has_tools = matches!(&messages[i].content, MessageContent::Blocks(blocks) if blocks.iter().any(|b| b.is_tool_use()));

        if prev_has_tools || curr_has_tools {
            i += 1;
            continue;
        }

        if messages[i].role == messages[i - 1].role
            && !matches!(messages[i].role, MessageRole::System)
        {
            // Merge content
            let merged_content = match (&messages[i - 1].content, &messages[i].content) {
                (MessageContent::Simple(a), MessageContent::Simple(b)) => {
                    MessageContent::Simple(format!("{}\n{}", a, b))
                }
                _ => messages[i].content.clone(),
            };
            messages[i - 1].content = merged_content;
            messages.remove(i);
        } else {
            i += 1;
        }
    }

    // Ensure we have at least a user message
    if messages.is_empty() {
        messages.push(ChatMessage::user("(conversation continued)".to_string()));
    }
}

/// Stream an LLM response and send chunks to the TUI
///
/// This is the main entry point for streaming LLM conversations with tool support.
/// It handles the full lifecycle: loading provider config, streaming responses,
/// detecting and executing tool calls, and continuing conversations with tool results.
///
/// # Arguments
///
/// * `config` - Stream configuration containing all parameters
///
/// # Returns
///
/// Returns `Ok(())` on successful completion, or an error if setup fails.
/// Note that stream errors are sent via the channel rather than returned.
async fn stream_llm_response_agent(config: StreamConfig) -> Result<()> {
    let StreamConfig {
        content,
        cwd,
        stream_tx,
        workspace_context,
        stop_signal,
        tools_schema,
        approval_rx,
        question_rx,
        agent_mode,
        file_read_cache: _,
        error_tracker: _,
        todo_state: _,
        conversation_history,
        tool_registry,
        plan_mode: _,
        ai_mode: _,
        orchestration_guidance,
        phase_context,
        orchestration: _,
    } = config;

    let _done_guard = DoneGuard::new(stream_tx.clone());

    if stop_signal
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    {
        send_chunk(&stream_tx, StreamChunk::Done);
        return Ok(());
    }

    let (provider_type, model, v2_config) =
        rustycode_llm::load_provider_config_from_env().context("Failed to load provider config")?;

    let needs_api_key = !matches!(
        provider_type.to_lowercase().as_str(),
        "ollama" | "local" | "lmstudio" | "litert-lm" | "litert_lm" | "litert"
    );
    if needs_api_key && v2_config.api_key.is_none() {
        send_chunk(&stream_tx,StreamChunk::Error(format!(
            "No API key configured for provider '{}'. Please set the {} environment variable or add it to your config.json.",
            provider_type,
            api_key_env_name(&provider_type)
        )));
        send_chunk(&stream_tx, StreamChunk::Done);
        return Ok(());
    }

    let provider = if v2_config.api_key.is_some() {
        rustycode_llm::create_provider_with_config(&provider_type, &model, v2_config).context(
            format!(
                "Failed to create provider {} with model {}",
                provider_type, model
            ),
        )?
    } else {
        rustycode_llm::create_provider(&provider_type, &model).context(format!(
            "Failed to create provider {} with model {}",
            provider_type, model
        ))?
    };

    let mut system_parts = vec![
        "You are RustyCode, an AI coding assistant.\n\
        \n\
        Output complete working code. No placeholders, no TODOs, no explanations of what you would do.\n\
        \n\
        - Read files before modifying them\n\
        - Make targeted changes, not broad refactors\n\
        - Run tests to verify your changes\n\
        - Use parallel tool calls when operations are independent\n\
        - For complex tasks: write code incrementally, verify each step, then continue"
            .to_string(),
        workspace_context
            .map(|context| format!("## Project\n{}", context))
            .unwrap_or_else(|| "No workspace context available.".to_string()),
        format!(
            "Platform: {} | Date: {}",
            std::env::consts::OS,
            chrono::Utc::now().format("%Y-%m-%d")
        ),
        "Planning mode policy:\n\
        - If a requested action is blocked by planning mode, say you are stalled, name the blocker briefly, and ask the user to switch to implementation mode with /plan.\n\
        - If a required instruction file is missing or empty, say so explicitly and stop.\n\
        - If planning appears complete, say you are ready to switch to implementation mode and wait for the user's confirmation.\n\
        - Do not silently stop after a blocker; explain the next step."
            .to_string(),
    ];

    if let Some(mode) = agent_mode {
        system_parts.push(mode.system_prompt_suffix().to_string());
    }

    system_parts.push(
        "Orchestration tier guidance:\n\
        - For simple tasks (reading files, listing, searching): proceed directly with available tools.\n\
        - For complex tasks (refactoring, multi-file changes, debugging): break the task into steps, verify each step, and escalate if stuck.\n\
        - If you detect you are repeating the same failed approach, switch strategy rather than retrying.\n\
        - After making changes, always verify (build/test/lint) before declaring success."
            .to_string(),
    );

    if let Some(ref guidance) = orchestration_guidance {
        system_parts.push(guidance.clone());
    }

    if let Some(ref ctx) = phase_context {
        system_parts.push(format!("Previous orchestration context:\n{}", ctx));
    }

    if let Ok(custom_prompt) = std::env::var("RUSTYCODE_SYSTEM_PROMPT") {
        if !custom_prompt.is_empty() {
            system_parts.push(custom_prompt);
        }
    } else if let Ok(prompt_file) = std::env::var("RUSTYCODE_SYSTEM_PROMPT_FILE") {
        if !prompt_file.is_empty() {
            if let Ok(content) = std::fs::read_to_string(&prompt_file) {
                if !content.trim().is_empty() {
                    system_parts.push(content);
                }
            }
        }
    }

    if let Some(cwd_str) = cwd.to_str() {
        let project_prompt = Path::new(cwd_str).join(".rustycode_system_prompt");
        if project_prompt.exists() {
            if let Ok(content) = std::fs::read_to_string(&project_prompt) {
                if !content.trim().is_empty() {
                    system_parts.push(content);
                }
            }
        }

        let agents_md = Path::new(cwd_str).join("AGENTS.md");
        if agents_md.exists() {
            if let Ok(content) = std::fs::read_to_string(&agents_md) {
                if !content.trim().is_empty() {
                    system_parts.push(format!("## Project Instructions (AGENTS.md)\n{}", content));
                }
            }
        }
    }

    let system_message = system_parts.join("\n\n");
    let mut messages = vec![rustycode_llm::provider::ChatMessage::system(
        system_message.clone(),
    )];

    if let Some(history) = conversation_history {
        for msg in history {
            if !matches!(msg.role, rustycode_llm::provider::MessageRole::System) {
                messages.push(msg);
            }
        }
    }
    messages.push(rustycode_llm::provider::ChatMessage::user(content));

    let tool_registry =
        tool_registry.unwrap_or_else(|| std::sync::Arc::new(rustycode_tools::ToolRegistry::new()));
    let tools_schema = tools_schema.unwrap_or_default();
    let agent_config = rustycode_agent::AgentConfig::from_env();
    let mut session = rustycode_agent::AgentSession::new(agent_config, cwd);
    // Interactive TUI needs all tools — default tier (6 hardcoded names) filters everything out
    session
        .activation
        .promote(rustycode_tools_api::tiers::ToolTier::Full);
    let mut bridge = crate::app::pipeline::agent_manager::TuiAgentBridge::new(stream_tx.clone())
        .with_approval_rx(approval_rx.unwrap_or_else(|| {
            let (_tx, rx) = std::sync::mpsc::channel();
            rx
        }))
        .with_question_rx(question_rx.unwrap_or_else(|| {
            let (_tx, rx) = std::sync::mpsc::channel();
            rx
        }));

    if let Err(err) = session
        .run(
            provider.as_ref(),
            &model,
            &system_message,
            messages,
            &tools_schema,
            tool_registry.as_ref(),
            &mut bridge,
        )
        .await
        .context("AgentSession streaming failed")
    {
        send_chunk(&stream_tx, StreamChunk::Error(err.to_string()));
        send_chunk(&stream_tx, StreamChunk::Done);
        return Err(err);
    }

    Ok(())
}

pub async fn stream_llm_response(config: StreamConfig) -> Result<()> {
    stream_llm_response_agent(config).await
}

#[allow(clippy::too_many_arguments, dead_code)]
pub async fn stream_llm_response_legacy(config: StreamConfig) -> Result<()> {
    let StreamConfig {
        content,
        cwd,
        stream_tx,
        workspace_context,
        stop_signal,
        tools_schema,
        approval_rx,
        question_rx,
        agent_mode,
        file_read_cache,
        error_tracker,
        todo_state,
        conversation_history,
        tool_registry,
        plan_mode,
        ai_mode,
        orchestration_guidance,
        phase_context,
        orchestration,
    } = config;

    let _done_guard = DoneGuard::new(stream_tx.clone());

    // Load provider type and model from config
    let (provider_type, model, v2_config) = match rustycode_llm::load_provider_config_from_env() {
        Ok(v) => v,
        Err(e) => {
            return Err(e.context("Failed to load provider config"));
        }
    };

    tracing::debug!("Using provider: {} with model: {}", provider_type, model);
    tracing::debug!("API Key configured: {}", v2_config.api_key.is_some());
    tracing::debug!("Base URL: {:?}", v2_config.base_url);

    // Validate API key (skip for providers that don't require one)
    let needs_api_key = !matches!(
        provider_type.to_lowercase().as_str(),
        "ollama" | "local" | "lmstudio" | "litert-lm" | "litert_lm" | "litert"
    );
    if needs_api_key && v2_config.api_key.is_none() {
        send_chunk(&stream_tx,StreamChunk::Error(
            format!("No API key configured for provider '{}'. Please set the {} environment variable or add it to your config.json.",
                provider_type,
                api_key_env_name(&provider_type))
        ));
        // Must send Done so the TUI releases the query guard and clears is_streaming.
        // Without this, the guard stays active and blocks all future messages.
        send_chunk(&stream_tx, StreamChunk::Done);
        return Ok(());
    }

    // Validate Anthropic API key format (accept both old "sk-ant-" and new format)
    if provider_type.to_lowercase() == "anthropic" || provider_type.to_lowercase() == "claude" {
        let api_key = match v2_config.api_key.as_ref() {
            Some(key) => key,
            None => {
                send_chunk(
                    &stream_tx,
                    StreamChunk::Error(
                        "No API key configured. Please set up your API key in settings."
                            .to_string(),
                    ),
                );
                send_chunk(&stream_tx, StreamChunk::Done);
                return Ok(());
            }
        };
        let key_str = api_key.expose_secret();
        if key_str.len() < 20 {
            send_chunk(&stream_tx,StreamChunk::Error(
                format!("Invalid Anthropic API key format. API key appears too short ({} chars). Expected at least 20 characters.",
                    key_str.len())
            ));
            send_chunk(&stream_tx, StreamChunk::Done);
            return Ok(());
        }
    }

    // Create provider
    let provider = if v2_config.api_key.is_some() {
        match rustycode_llm::create_provider_with_config(&provider_type, &model, v2_config) {
            Ok(p) => p,
            Err(e) => {
                return Err(e.context(format!(
                    "Failed to create provider {} with model {}",
                    provider_type, model
                )));
            }
        }
    } else {
        match rustycode_llm::create_provider(&provider_type, &model) {
            Ok(p) => p,
            Err(e) => {
                return Err(e.context(format!(
                    "Failed to create provider {} with model {}",
                    provider_type, model
                )));
            }
        }
    };

    // Build initial messages array with optional workspace context
    let mut messages = Vec::new();

    // Build system message with workspace context, coding guidance, and agent mode
    let workspace_section = if let Some(context) = workspace_context {
        format!("## Project\n{}", context)
    } else {
        "No workspace context available.".to_string()
    };

    let mut system_parts = vec![
        "You are RustyCode, an AI coding assistant.\n\
        \n\
        Output complete working code. No placeholders, no TODOs, no explanations of what you would do.\n\
        \n\
        - Read files before modifying them\n\
        - Make targeted changes, not broad refactors\n\
        - Run tests to verify your changes\n\
        - Use parallel tool calls when operations are independent\n\
        - For complex tasks: write code incrementally, verify each step, then continue"
            .to_string(),
        workspace_section,
        format!(
            "Platform: {} | Date: {}",
            std::env::consts::OS,
            chrono::Utc::now().format("%Y-%m-%d")
        ),
        "Planning mode policy:\n\
        - If a requested action is blocked by planning mode, say you are stalled, name the blocker briefly, and ask the user to switch to implementation mode with /plan.\n\
        - If a required instruction file is missing or empty, say so explicitly and stop.\n\
        - If planning appears complete, say you are ready to switch to implementation mode and wait for the user's confirmation.\n\
        - Do not silently stop after a blocker; explain the next step.".to_string(),
    ];

    if let Some(mode) = agent_mode {
        system_parts.push(mode.system_prompt_suffix().to_string());
    }

    // Add orchestration tier self-decision guidance
    system_parts.push(
        "Orchestration tier guidance:\n\
        - For simple tasks (reading files, listing, searching): proceed directly with available tools.\n\
        - For complex tasks (refactoring, multi-file changes, debugging): break the task into steps, verify each step, and escalate if stuck.\n\
        - If you detect you are repeating the same failed approach, switch strategy rather than retrying.\n\
        - After making changes, always verify (build/test/lint) before declaring success.".to_string(),
    );

    if let Some(ref guidance) = orchestration_guidance {
        system_parts.push(guidance.clone());
    }

    if let Some(ref ctx) = phase_context {
        system_parts.push(format!("Previous orchestration context:\n{}", ctx));
    }

    // Load custom system prompt additions (Goose pattern: RUSTYCODE_SYSTEM_PROMPT_FILE)
    // Supports both a file path and inline text via RUSTYCODE_SYSTEM_PROMPT
    if let Ok(custom_prompt) = std::env::var("RUSTYCODE_SYSTEM_PROMPT") {
        if !custom_prompt.is_empty() {
            system_parts.push(custom_prompt);
        }
    } else if let Ok(prompt_file) = std::env::var("RUSTYCODE_SYSTEM_PROMPT_FILE") {
        if !prompt_file.is_empty() {
            match std::fs::read_to_string(&prompt_file) {
                Ok(content) if !content.trim().is_empty() => {
                    system_parts.push(content);
                    tracing::info!("Loaded custom system prompt from {}", prompt_file);
                }
                Ok(_) => {} // Empty file, skip
                Err(e) => {
                    tracing::warn!("Failed to read system prompt file {}: {}", prompt_file, e);
                }
            }
        }
    }
    // Also load .rustycode_system_prompt from project root (Goose hints pattern)
    if let Some(cwd_str) = cwd.to_str() {
        let project_prompt = Path::new(cwd_str).join(".rustycode_system_prompt");
        if project_prompt.exists() {
            if let Ok(content) = std::fs::read_to_string(&project_prompt) {
                if !content.trim().is_empty() {
                    system_parts.push(content);
                }
            }
        }
        // Also load AGENTS.md (Goose/Claw pattern for project-level AI instructions)
        let agents_md = Path::new(cwd_str).join("AGENTS.md");
        if agents_md.exists() {
            if let Ok(content) = std::fs::read_to_string(&agents_md) {
                if !content.trim().is_empty() {
                    system_parts.push(format!("## Project Instructions (AGENTS.md)\n{}", content));
                }
            }
        }
    }

    let system_message = system_parts.join("\n\n");

    messages.push(ChatMessage::system(system_message));

    // Include conversation history for multi-turn context
    let mut history_included_user_msg = false;
    if let Some(history) = conversation_history {
        // Skip system messages from history (we have our own)
        for msg in history {
            if !matches!(msg.role, MessageRole::System) {
                messages.push(msg);
            }
        }
        // Check if the last message in history is already this user message
        // (happens when caller pushes user msg to messages before building history)
        // For image messages, as_str() returns text + "[Image]" placeholders,
        // so we check if the text content starts with our message text.
        history_included_user_msg = messages.last().is_some_and(|m| {
            if m.role != MessageRole::User {
                return false;
            }
            let msg_text = m.content.as_str();
            // Exact match (text-only messages)
            if msg_text == content {
                return true;
            }
            // Image messages: text starts with content, followed by "[Image]" blocks
            msg_text.starts_with(&content)
        });
    }

    // Add the current user message only if not already in history
    if !history_included_user_msg {
        messages.push(ChatMessage::user(content.to_string()));
    }

    // Conversation limits to prevent unbounded memory growth
    const MAX_MESSAGES: usize = 50;
    const MAX_CONVERSATION_BYTES: usize = 10 * 1024 * 1024; // 10MB
    const MAX_TOOL_TURNS: usize = 200; // Prevent infinite tool-use loops

    // Conversation-level truncation for tool results.
    // Individual tools truncate at 30-80 lines / 10-50KB, but MCP tools, custom tools,
    // or critical content bypasses may produce larger output. This safety net ensures
    // no single tool result exceeds a reasonable size for the LLM context window.
    const TOOL_RESULT_MAX_BYTES: usize = 25 * 1024; // 25KB per result (generous but bounded)
    let truncate_for_conversation = |content: String| -> String {
        if content.len() <= TOOL_RESULT_MAX_BYTES {
            return content;
        }
        // Find a safe char boundary at or before the byte limit
        let bytes = content.as_bytes();
        let mut end = TOOL_RESULT_MAX_BYTES;
        // Walk back to find a clean line break (also ensures UTF-8 safety)
        while end > 0 && bytes[end] != b'\n' {
            end -= 1;
        }
        if end == 0 {
            // No line break found — find nearest char boundary at the limit
            end = TOOL_RESULT_MAX_BYTES;
            while end > 0 && !content.is_char_boundary(end) {
                end -= 1;
            }
        }
        // end is guaranteed to be a valid char boundary (either at \n or at char_boundary)
        let kept = &content[..end];
        let omitted_bytes = content.len() - end;
        format!(
            "{}\n\n[... {} bytes omitted — output truncated for context window]",
            kept.trim_end(),
            omitted_bytes
        )
    };

    // Helper function to prune messages when limits are exceeded
    let prune_messages = |msgs: &mut Vec<ChatMessage>| {
        if msgs.len() <= MAX_MESSAGES {
            return;
        }

        let total_size: usize = msgs.iter().map(|m| m.content.as_text().len()).sum();

        if total_size > MAX_CONVERSATION_BYTES || msgs.len() > MAX_MESSAGES {
            let system_messages: Vec<_> = msgs
                .iter()
                .filter(|m| matches!(m.role, rustycode_llm::MessageRole::System))
                .cloned()
                .collect();

            let keep_count = MAX_MESSAGES.saturating_sub(system_messages.len());
            let start_idx = msgs.len().saturating_sub(keep_count);
            let original_len = msgs.len();

            let mut recent_messages = msgs.split_off(start_idx);

            // Drop leading orphaned tool_result messages whose parent
            // assistant+tool_use was cut by the split.
            while let Some(msg) = recent_messages.first() {
                if msg.role != MessageRole::User {
                    break;
                }
                if msg.content.as_text().contains("\"type\":\"tool_result\"") {
                    tracing::debug!(
                        "Prune: dropping orphaned tool_result (parent tool_use was cut)"
                    );
                    recent_messages.remove(0);
                } else {
                    break;
                }
            }

            tracing::info!(
                "Pruned conversation from {} to {} messages",
                original_len,
                recent_messages.len() + system_messages.len()
            );

            let mut final_messages = system_messages;
            final_messages.append(&mut recent_messages);
            *msgs = final_messages;
        }
    };

    // Conversation continuation loop
    let mut turn_count: usize = 0;
    let mut empty_thinking_retries: usize = 0;
    let mut last_stop_reason: Option<String> = None;
    let mut tools_schema = tools_schema;
    loop {
        turn_count += 1;
        if turn_count > MAX_TOOL_TURNS {
            tracing::warn!("Reached max tool turns ({}), breaking loop", MAX_TOOL_TURNS);
            send_chunk(
                &stream_tx,
                StreamChunk::Error(format!(
                    "Reached maximum tool-use turns ({}). Stopping to prevent infinite loop.",
                    MAX_TOOL_TURNS
                )),
            );
            break;
        }

        // Fix conversation structure before sending to LLM
        fix_conversation_messages(&mut messages);

        // Create request with current messages
        let mut request = CompletionRequest::new(model.clone(), messages.clone())
            .with_streaming(true)
            .with_max_tokens(8192)
            .with_temperature(0.1);

        // Add tools schema if provided (only on first request)
        if let Some(ref tools) = tools_schema {
            request = request.with_tools(tools.clone());
            tracing::info!("Including {} tool definitions in request", tools.len());
        } else {
            tracing::warn!("No tools schema provided - tool use will not be available!");
        }

        // Stream the response
        let mut stream = provider.complete_stream(request).await.map_err(|e| {
            let msg = format!("{}", e);
            if provider_type.to_lowercase() == "ollama" {
                anyhow::anyhow!(
                    "Cannot connect to Ollama. Is it running? Start with: ollama serve\nDetails: {}",
                    msg
                )
            } else if msg.contains("connection refused") || msg.contains("Connection refused") {
                anyhow::anyhow!(
                    "Connection refused by provider '{}'. Check that the service is running and the base URL is correct.\nDetails: {}",
                    provider_type, msg
                )
            } else {
                anyhow::anyhow!("Failed to start stream: {}", msg)
            }
        })?;

        // Track active tool use accumulation (indexed for parallel tool calls)
        let mut active_tools: std::collections::HashMap<String, ActiveToolUse> =
            std::collections::HashMap::new();
        let mut in_tool_use = false;
        let mut tool_executions: Vec<ToolExecutionResult> = Vec::new();
        let mut tool_turn_count: usize = 0;
        let mut assistant_response = String::new();
        let mut content_blocks: Vec<ContentBlock> = Vec::new();
        let mut thinking_content = String::new();
        let thinking_signature = String::new();
        let mut stop_action = ToolUseAction::None;
        let stream_start = std::time::Instant::now();
        let mut thinking_timeout_fired = false;
        let mut last_thinking_chunk: Option<String> = None;

        // Stall tracking: consecutive read-only (exploration) turns
        let mut consecutive_exploration_turns: usize = 0;
        let mut total_exploration_calls: usize = 0;
        let mut total_code_calls: usize = 0;
        // Post-code stall: thinking-only turns after code was produced
        #[allow(unused_mut)]
        let mut code_produced_turn: Option<usize> = None;
        #[allow(unused_mut)]
        let mut consecutive_thinking_after_code: usize = 0;
        #[allow(clippy::duration_suboptimal_units)]
        const MAX_THINKING_DURATION: Duration = Duration::from_secs(90);
        #[allow(clippy::duration_suboptimal_units)]
        const EARLY_CUTOFF_DURATION: Duration = Duration::from_secs(60);
        const EARLY_CUTOFF_MIN_THINKING_BYTES: usize = 5000;
        #[allow(clippy::duration_suboptimal_units)]
        const MAX_STREAM_DURATION: Duration = Duration::from_secs(600);

        loop {
            let elapsed = stream_start.elapsed();
            if !thinking_timeout_fired && thinking_content.len() > 100 {
                let early_cutoff = elapsed > EARLY_CUTOFF_DURATION
                    && thinking_content.len() > EARLY_CUTOFF_MIN_THINKING_BYTES;
                let hard_timeout = elapsed > MAX_THINKING_DURATION;
                if early_cutoff || hard_timeout {
                    tracing::warn!(
                        "Thinking timeout ({:.1}s, {} bytes, early={})",
                        elapsed.as_secs_f64(),
                        thinking_content.len(),
                        early_cutoff,
                    );
                    thinking_timeout_fired = true;
                    send_chunk(
                        &stream_tx,
                        StreamChunk::Text("\n[Thinking timeout: forcing response]\n".to_string()),
                    );
                    break;
                }
            }
            let turn_elapsed = stream_start.elapsed();
            if turn_elapsed > MAX_STREAM_DURATION {
                tracing::warn!(
                    "Stream timeout exceeded ({:.1}s)",
                    turn_elapsed.as_secs_f64()
                );
                send_chunk(&stream_tx,StreamChunk::Error(
                    "Stream exceeded maximum duration (10 minutes). Task may be too complex for a single session.".into(),
                ));
                send_chunk(&stream_tx, StreamChunk::Done);
                break;
            }

            // 5min per-chunk timeout: models generating large write_file calls can go quiet for extended periods
            let chunk_result =
                match tokio::time::timeout(Duration::from_mins(5), stream.next()).await {
                    Ok(Some(result)) => result,
                    Ok(None) => break, // stream ended normally
                    Err(_) => {
                        tracing::warn!("Stream timed out after 300s with no data");
                        send_chunk(&stream_tx,StreamChunk::Error(
                            "Stream timed out (300s without data). The provider may be overloaded."
                                .into(),
                        ));
                        send_chunk(&stream_tx, StreamChunk::Done);
                        break;
                    }
                };
            // Check for cancellation signal
            if stop_signal
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
            {
                send_chunk(&stream_tx, StreamChunk::Done);
                return Ok(());
            }

            match chunk_result {
                Ok(event) => match event {
                    StreamEvent::TextDelta { content } => {
                        if !in_tool_use {
                            assistant_response.push_str(&content);
                        }
                        if !content.is_empty() {
                            let _ = crate::app::streaming::events::handle_text_event(
                                content,
                                &mut in_tool_use,
                                &stream_tx,
                            );
                        }
                    }
                    StreamEvent::ThinkingDelta { content } => {
                        thinking_content.push_str(&content);
                        if Some(&content) != last_thinking_chunk.as_ref() {
                            last_thinking_chunk = Some(content.clone());
                            let _ = crate::app::streaming::events::handle_thinking_event(
                                content,
                                &in_tool_use,
                                &stream_tx,
                            );
                        }
                    }
                    StreamEvent::ToolCallStarted { id, name } => {
                        in_tool_use = true;
                        active_tools
                            .insert(id.clone(), ActiveToolUse::new(id, name, String::new()));
                    }
                    StreamEvent::ToolInputDelta { id, chunk } => {
                        if let Some(tool) = active_tools.get_mut(&id) {
                            tool.push_json(&chunk);
                        } else if let Some(tool) = active_tools.values_mut().next() {
                            tool.push_json(&chunk);
                        }
                    }
                    StreamEvent::TokenUsage {
                        input_tokens,
                        output_tokens,
                    } => {
                        send_chunk(
                            &stream_tx,
                            StreamChunk::TokenUsage {
                                input_tokens: input_tokens as usize,
                                output_tokens: output_tokens as usize,
                                cache_read_tokens: 0,
                                cache_creation_tokens: 0,
                            },
                        );
                    }
                    StreamEvent::CacheUsage {
                        cache_read_tokens,
                        cache_creation_tokens,
                    } => {
                        send_chunk(
                            &stream_tx,
                            StreamChunk::TokenUsage {
                                input_tokens: 0,
                                output_tokens: 0,
                                cache_read_tokens: cache_read_tokens as usize,
                                cache_creation_tokens: cache_creation_tokens as usize,
                            },
                        );
                    }
                    StreamEvent::TurnCompleted { stop_reason } => {
                        last_stop_reason = Some(stop_reason.clone());
                        stop_action = handle_message_delta(Some(&stop_reason));
                        in_tool_use = false;
                    }
                    StreamEvent::Done => {
                        if !assistant_response.is_empty() {
                            send_chunk(
                                &stream_tx,
                                StreamChunk::ExtractTasks {
                                    text: assistant_response.clone(),
                                },
                            );
                        }
                    }
                    StreamEvent::ToolExecStarted { .. }
                    | StreamEvent::ToolExecCompleted { .. }
                    | StreamEvent::TurnStarted { .. }
                    | _ => {}
                },
                Err(e) => {
                    // Typed error matching using ProviderError variants,
                    // with fallback to string matching for wrapped errors.
                    use rustycode_llm::provider::ProviderError;
                    let enhanced = match &e {
                        ProviderError::RateLimited { retry_delay, .. } => {
                            let wait_hint = retry_delay
                                .map(|d| format!(" (retry after {}s)", d.as_secs()))
                                .unwrap_or_default();
                            format!(
                                "Rate limited by provider '{}'.{wait_hint}\n\
                                Consider using a different model or provider if this persists.\n\
                                Details: {}",
                                provider_type, e
                            )
                        }
                        ProviderError::CreditsExhausted { top_up_url, .. } => {
                            let top_up = top_up_url
                                .as_ref()
                                .map(|url| format!("\nTop up credits: {}", url))
                                .unwrap_or_default();
                            format!(
                                "API credits exhausted for provider '{}'.{top_up}\n\
                                Details: {}",
                                provider_type, e
                            )
                        }
                        ProviderError::ContextLengthExceeded(_) => {
                            format!(
                                "Context length exceeded for model '{}'. Try:\n\
                                - Start a new conversation (/clear)\n\
                                - Use a model with larger context\n\
                                Details: {}",
                                model, e
                            )
                        }
                        ProviderError::InvalidModel(_) => {
                            format!(
                                "Model '{}' not found. Check the model name is correct.\n\
                                Try: claude-sonnet-4-6, claude-opus-4-6, or claude-haiku-4-5\n\
                                Details: {}",
                                model, e
                            )
                        }
                        ProviderError::Auth(_) => {
                            format!("Authentication failed for provider '{}'. Your API key may be invalid or expired.\n\
                                Set the correct key in your config or environment variable.\n\
                                Details: {}", provider_type, e)
                        }
                        ProviderError::Network(_) => {
                            format!("Network error connecting to provider '{}'. Check your internet connection.\n\
                                Please retry if you think this is a transient error.\n\
                                Details: {}", provider_type, e)
                        }
                        ProviderError::Timeout(_) => {
                            format!("Request timed out for provider '{}'. The server may be overloaded.\n\
                                Please retry if you think this is a transient error.\n\
                                Details: {}", provider_type, e)
                        }
                        _ => {
                            // Fallback to string matching for wrapped/unknown errors
                            let error_msg = format!("{}", e);
                            if error_msg.contains("429") || error_msg.contains("rate limit") {
                                format!(
                                    "Rate limited by provider '{}'. Please wait and try again.\n\
                                    Details: {}",
                                    provider_type, error_msg
                                )
                            } else if error_msg.contains("404") || error_msg.contains("not_found") {
                                format!("Model '{}' not found. Try: claude-sonnet-4-6, claude-opus-4-6, or claude-haiku-4-5\n\
                                    Details: {}", model, error_msg)
                            } else if error_msg.contains("403") || error_msg.contains("forbidden") {
                                format!("Access denied by provider '{}'. Your account may not have access to model '{}'.\n\
                                    Details: {}", provider_type, model, error_msg)
                            } else {
                                format!(
                                    "Stream interrupted: {}. Try resending your message.",
                                    error_msg
                                )
                            }
                        }
                    };
                    send_chunk(&stream_tx, StreamChunk::Error(enhanced));
                    send_chunk(&stream_tx, StreamChunk::Done);
                    return Ok(());
                }
            }

            tokio::time::sleep(Duration::from_millis(10)).await;

            // Check for cancellation during sleep
            if stop_signal
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
            {
                send_chunk(&stream_tx, StreamChunk::Done);
                return Ok(());
            }
        }

        tracing::info!(
            "Turn {} done: stop_action={:?}, assistant_response={} chars, thinking={} chars, tool_execs={}, content_blocks={}",
            turn_count,
            stop_action,
            assistant_response.len(),
            thinking_content.len(),
            tool_executions.len(),
            content_blocks.len(),
        );

        // Stall detection: if the model has spent N consecutive turns
        // only reading (no code output), inject a nudge to write code.
        // GLM-5.1 makes exactly 1 tool call per turn, so turn-based
        // counting works. Thinking-timeout turns (tool_execs=0) also
        // count toward the exploration stall counter.
        let thinking_stall = tool_executions.is_empty() && thinking_timeout_fired;
        if !tool_executions.is_empty() || thinking_stall {
            use crate::app::stall_detector::ToolCategory;
            let sd = crate::app::stall_detector::StallDetector::classify_tool;

            let has_code = !tool_executions.is_empty()
                && tool_executions.iter().any(|t| {
                    let cat = sd(&t.tool_name);
                    cat == ToolCategory::Code || cat == ToolCategory::Shell
                });
            let exploration_count = if !tool_executions.is_empty() {
                tool_executions
                    .iter()
                    .filter(|t| {
                        let cat = sd(&t.tool_name);
                        cat == ToolCategory::Exploration || cat == ToolCategory::Unknown
                    })
                    .count()
            } else {
                0
            };

            if has_code {
                total_code_calls += tool_executions.len() - exploration_count;
                consecutive_exploration_turns = 0;
                code_produced_turn = Some(tool_turn_count);
                consecutive_thinking_after_code = 0;
            } else if thinking_stall {
                consecutive_exploration_turns += 1;
                total_exploration_calls += 1;
                if code_produced_turn.is_some() {
                    consecutive_thinking_after_code += 1;
                }
            } else {
                total_exploration_calls += exploration_count;
                consecutive_exploration_turns += 1;
            }

            // Post-code thinking spiral: model wrote code, then spent
            // multiple turns only thinking without producing new output.
            // This catches the "rewrite working code" anti-pattern.
            if consecutive_thinking_after_code >= 2 {
                tracing::warn!(
                    "Post-code thinking spiral detected ({} thinking turns after code produced at turn {}): injecting stop nudge",
                    consecutive_thinking_after_code,
                    code_produced_turn.unwrap_or(0)
                );
                let stop_nudge = "<system-reminder>\n\
                     You already produced working code. STOP overthinking and ship it.\n\n\
                     - Your implementation is good enough — output the final result\n\
                     - Do NOT rewrite code that already works\n\
                     - If the task has verification, run it and report the result\n\
                     - If the task is complete, say so clearly\n\
                     </system-reminder>"
                    .to_string();
                send_chunk(
                    &stream_tx,
                    StreamChunk::Text(
                        "\n[Post-code thinking spiral detected — nudging model to finalize]\n"
                            .to_string(),
                    ),
                );
                messages.push(ChatMessage::user(stop_nudge));
                prune_messages(&mut messages);
                #[allow(unused_assignments)]
                {
                    consecutive_thinking_after_code = 0;
                }
                continue;
            }

            let stall_threshold = crate::app::stall_detector::STALL_THRESHOLD;
            let stall_critical = crate::app::stall_detector::MAX_EXPLORATION_TURNS;
            if consecutive_exploration_turns >= stall_threshold {
                let nudge = if consecutive_exploration_turns >= stall_critical {
                    format!(
                        "<system-reminder>\n\
                         EXPLORATION BUDGET EXHAUSTED. You have made {} consecutive read-only turns \
                         ({} exploration calls, {} code calls). You MUST now write code.\n\n\
                         - Use write_file, edit_file, or bash to produce output IMMEDIATELY\n\
                         - No more read_file, grep, glob, or exploration tools\n\
                         - Output complete working code. No placeholders, no TODOs\n\
                         - If you need more information, make reasonable assumptions and write the code\n\
                         - A partial implementation is better than continued exploration\n\
                         </system-reminder>",
                        consecutive_exploration_turns,
                        total_exploration_calls,
                        total_code_calls,
                    )
                } else {
                    format!(
                        "<system-reminder>\n\
                         You have spent {} turns exploring ({} read calls, {} write calls). \
                         Transition to implementation now.\n\n\
                         - You have enough information to start writing code\n\
                         - Use write_file or edit_file to produce the solution\n\
                         - Write incrementally if needed, but start producing code THIS turn\n\
                         </system-reminder>",
                        consecutive_exploration_turns, total_exploration_calls, total_code_calls,
                    )
                };

                tracing::warn!(
                    "Exploration stall detected ({} consecutive read-only turns): injecting nudge",
                    consecutive_exploration_turns
                );
                send_chunk(
                    &stream_tx,
                    StreamChunk::Text(
                        "\n[Exploration stall detected — nudging model to write code]\n"
                            .to_string(),
                    ),
                );
                messages.push(ChatMessage::user(nudge));
                prune_messages(&mut messages);
                continue;
            }
        }

        if !active_tools.is_empty() {
            if matches!(stop_action, ToolUseAction::None) {
                stop_action = ToolUseAction::ExecuteTools;
            }

            for (_, tool) in active_tools.drain() {
                if stop_signal
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::Relaxed))
                {
                    tracing::info!("Tool execution cancelled by user");
                    send_chunk(
                        &stream_tx,
                        StreamChunk::Text("\n[Tool execution cancelled]\n".to_string()),
                    );
                    send_chunk(&stream_tx, StreamChunk::Done);
                    return Ok(());
                }

                tracing::info!(
                    "Tool use complete: {} ({}), executing...",
                    tool.name,
                    tool.id
                );

                let tool_type = crate::tool_approval::risk::classify_tool_type(&tool.name);
                let command_str = tool.partial_json.clone();
                let yolo = matches!(ai_mode, Some(crate::agent_mode::AiMode::Yolo));
                let auto_approved_tool = tool.name == "structured_thinking";
                let needs_approval = !yolo
                    && !auto_approved_tool
                    && !crate::tool_approval::risk::should_auto_approve(&tool_type, &command_str);

                let should_execute = if needs_approval && approval_rx.is_some() {
                    let command = format!("{}: {}", tool.name, {
                        let v = parse_tool_parameters(&tool.partial_json);
                        if let Some(obj) = v.as_object() {
                            obj.iter()
                                .take(2)
                                .map(|(k, v)| format!("{}={}", k, v))
                                .collect::<Vec<_>>()
                                .join(" ")
                        } else {
                            tool.partial_json.clone()
                        }
                    });

                    send_chunk(
                        &stream_tx,
                        StreamChunk::ApprovalRequest {
                            tool_name: tool.name.clone(),
                            tool_id: tool.id.clone(),
                            description: format!("Execute tool: {}", tool.name),
                            diff: Some(command),
                        },
                    );

                    let rx = match approval_rx.as_ref() {
                        Some(rx) => rx,
                        None => {
                            send_chunk(
                                &stream_tx,
                                StreamChunk::Error(
                                    "Error: approval channel not available".to_string(),
                                ),
                            );
                            tool_executions.push(ToolExecutionResult {
                                tool_use_id: tool.id.clone(),
                                tool_name: tool.name.clone(),
                                result_content: "Error: approval channel not available".to_string(),
                            });
                            continue;
                        }
                    };
                    match rx.recv_timeout(Duration::from_mins(5)) {
                        Ok(true) => {
                            send_chunk(
                                &stream_tx,
                                StreamChunk::ApprovalApproved {
                                    tool_id: tool.id.clone(),
                                },
                            );
                            true
                        }
                        Ok(false) => {
                            send_chunk(
                                &stream_tx,
                                StreamChunk::ApprovalRejected {
                                    tool_id: tool.id.clone(),
                                },
                            );
                            send_chunk(
                                &stream_tx,
                                StreamChunk::Text(
                                    "[Tool execution rejected by user]\n".to_string(),
                                ),
                            );
                            false
                        }
                        Err(_) => {
                            let risk = crate::tool_approval::risk::classify_tool_risk(
                                &tool_type,
                                &command_str,
                            );
                            let is_safe =
                                matches!(risk, crate::tool_approval::risk::RiskLevel::Safe);
                            if is_safe {
                                tracing::warn!(
                                    "Tool approval timed out for {}, auto-approving safe tool",
                                    tool.name
                                );
                                send_chunk(
                                    &stream_tx,
                                    StreamChunk::ApprovalApproved {
                                        tool_id: tool.id.clone(),
                                    },
                                );
                                true
                            } else {
                                tracing::warn!(
                                    "Tool approval timed out for {}, auto-rejecting ({:?} risk)",
                                    tool.name,
                                    risk
                                );
                                send_chunk(
                                    &stream_tx,
                                    StreamChunk::ApprovalRejected {
                                        tool_id: tool.id.clone(),
                                    },
                                );
                                send_chunk(
                                    &stream_tx,
                                    StreamChunk::Text(
                                        "[Tool execution rejected: approval timed out]\n"
                                            .to_string(),
                                    ),
                                );
                                false
                            }
                        }
                    }
                } else {
                    true
                };

                if !should_execute {
                    tool_executions.push(ToolExecutionResult {
                        tool_use_id: tool.id.clone(),
                        tool_name: tool.name.clone(),
                        result_content: "[Tool execution rejected by user]".to_string(),
                    });
                    continue;
                }

                if tool.name == "question" && question_rx.is_some() {
                    let params = parse_tool_parameters(&tool.partial_json);
                    let question_text = params
                        .get("question")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Please answer");
                    let options = params
                        .get("options")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let default = params
                        .get("default")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let multi_select = params
                        .get("multiple")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let question_options: Vec<crate::app::async_::QuestionOption> = options
                        .iter()
                        .map(|opt| crate::app::async_::QuestionOption {
                            label: opt.clone(),
                            description: String::new(),
                        })
                        .collect();

                    send_chunk(
                        &stream_tx,
                        StreamChunk::QuestionRequest {
                            question_id: tool.id.clone(),
                            question_text: question_text.to_string(),
                            header: "Question".to_string(),
                            options: question_options,
                            multi_select,
                        },
                    );

                    let rx = match question_rx.as_ref() {
                        Some(rx) => rx,
                        None => {
                            send_chunk(
                                &stream_tx,
                                StreamChunk::Error(
                                    "Error: question channel not available".to_string(),
                                ),
                            );
                            tool_executions.push(ToolExecutionResult {
                                tool_use_id: tool.id.clone(),
                                tool_name: tool.name.clone(),
                                result_content: "Error: question channel not available".to_string(),
                            });
                            continue;
                        }
                    };

                    let answer = match rx.recv_timeout(Duration::from_mins(2)) {
                        Ok(a) => a,
                        Err(_) => {
                            if let Some(def) = default {
                                def
                            } else {
                                tool_executions.push(ToolExecutionResult {
                                    tool_use_id: tool.id.clone(),
                                    tool_name: tool.name.clone(),
                                    result_content:
                                        "Error: question timed out with no default answer"
                                            .to_string(),
                                });
                                continue;
                            }
                        }
                    };

                    send_chunk(
                        &stream_tx,
                        StreamChunk::QuestionAnswered {
                            question_id: tool.id.clone(),
                            answer: answer.clone(),
                        },
                    );

                    let result_content = format!(
                        "**Question:** {}\n\n**Your response:** {}",
                        question_text, answer
                    );

                    tool_executions.push(ToolExecutionResult {
                        tool_use_id: tool.id.clone(),
                        tool_name: tool.name.clone(),
                        result_content,
                    });

                    continue;
                }

                send_chunk(
                    &stream_tx,
                    StreamChunk::ToolStart {
                        tool_name: tool.name.clone(),
                        tool_id: tool.id.clone(),
                        input_json: tool.partial_json.clone(),
                    },
                );

                let tool_start = std::time::Instant::now();

                if let Some(batch) = snapshot_files_for_undo(&cwd, &tool.name, &tool.partial_json) {
                    send_chunk(&stream_tx, StreamChunk::FileSnapshot { batch });
                }

                let result = execute_tool(
                    &cwd,
                    &tool.name,
                    &tool.partial_json,
                    file_read_cache.as_ref(),
                    error_tracker.as_ref(),
                    todo_state.as_ref(),
                    tool_registry.as_ref(),
                    plan_mode.as_ref(),
                    orchestration.as_ref(),
                );
                let tool_elapsed = tool_start.elapsed().as_millis() as u64;

                if tool.name == "tool_search" {
                    if let Ok(search_payload) = serde_json::from_str::<serde_json::Value>(&result) {
                        if let Some(tool_defs) =
                            search_payload.get("tools").and_then(|v| v.as_array())
                        {
                            let mut merged_tools = tools_schema.clone().unwrap_or_default();

                            for tool_def in tool_defs {
                                let name =
                                    tool_def.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                if name.is_empty() {
                                    continue;
                                }
                                let already_present = merged_tools
                                    .iter()
                                    .any(|t| t.get("name").and_then(|v| v.as_str()) == Some(name));
                                if !already_present {
                                    merged_tools.push(tool_def.clone());
                                }
                            }

                            if !merged_tools.is_empty() {
                                tracing::info!(
                                    "Tool search loaded {} tool definitions",
                                    tool_defs.len()
                                );
                                tools_schema = Some(merged_tools);
                            }
                        }
                    }
                }

                tool_executions.push(ToolExecutionResult {
                    tool_use_id: tool.id.clone(),
                    tool_name: tool.name.clone(),
                    result_content: result.clone(),
                });

                send_chunk(
                    &stream_tx,
                    StreamChunk::ToolComplete {
                        tool_name: tool.name.clone(),
                        tool_id: tool.id.clone(),
                        duration_ms: tool_elapsed,
                        success: !result.starts_with("Error"),
                        output_size: result.len(),
                        output: Some(result.clone()),
                    },
                );

                let tool_todos = extract_todos_from_tool_result(&tool.name, &result);
                if !tool_todos.is_empty() {
                    send_chunk(
                        &stream_tx,
                        StreamChunk::ExtractTasks {
                            text: result.clone(),
                        },
                    );
                }
            }
        }

        // Some providers/users get to the end of a tool turn without a
        // trustworthy stop_reason. If we have tool executions, we should still
        // continue the conversation so the model can see tool results and
        // produce the next step instead of silently stalling after one call.
        if !tool_executions.is_empty() && matches!(stop_action, ToolUseAction::None) {
            tracing::warn!(
                "Tool executions were produced but stop_reason was None; continuing turn anyway"
            );
            stop_action = ToolUseAction::ExecuteTools;
        }

        // Decide what to do next based on stop_reason
        match stop_action {
            ToolUseAction::ExecuteTools => {
                if tool_executions.is_empty() {
                    tracing::warn!("stop_reason='tool_use' but no tools were executed");
                    break;
                }

                tool_turn_count += 1;
                if tool_turn_count >= MAX_TOOL_TURNS {
                    tracing::warn!(
                        "Reached max tool turns ({}) — stopping to prevent infinite loop",
                        MAX_TOOL_TURNS
                    );
                    send_chunk(&stream_tx, StreamChunk::SystemMessage(
                        format!("Reached maximum tool turns ({}). Stopping current turn. \
                                 Auto-continue will resume if enabled, or press Enter to continue manually.",
                                MAX_TOOL_TURNS),
                    ));
                    break;
                }

                if !thinking_content.is_empty() {
                    content_blocks.push(ContentBlock::thinking(
                        thinking_content.clone(),
                        thinking_signature.clone(),
                    ));
                }
                if !assistant_response.is_empty() {
                    content_blocks.push(ContentBlock::text(assistant_response.clone()));
                }

                // Append assistant message only if there is substantive content to send.
                // If there are structured content blocks, push them as a Blocks-based message.
                // Otherwise, push the plain textual assistant response only if non-empty.
                if !content_blocks.is_empty() {
                    messages.push(ChatMessage::assistant(MessageContent::Blocks(
                        content_blocks.clone(),
                    )));
                } else if !assistant_response.is_empty() {
                    messages.push(ChatMessage::assistant(assistant_response.clone()));
                }

                for tool_result in &tool_executions {
                    let truncated_content =
                        truncate_for_conversation(tool_result.result_content.clone());
                    let tool_result_msg = ChatMessage::tool_result(
                        tool_result.tool_use_id.clone(),
                        truncated_content,
                    );
                    messages.push(tool_result_msg);
                }

                tool_executions.clear();
                prune_messages(&mut messages);
                continue;
            }
            ToolUseAction::Stop | ToolUseAction::None => {
                // BUG16 fix: if thinking timeout fired but the model produced
                // nothing (no text, no tools), inject a forced continuation so
                // the model gets another chance to respond rather than showing
                // a blank "Ready" state.
                if thinking_timeout_fired
                    && assistant_response.is_empty()
                    && tool_executions.is_empty()
                    && empty_thinking_retries == 0
                {
                    empty_thinking_retries += 1;
                    tracing::info!(
                        "Thinking timeout produced empty response on turn {}; injecting continuation prompt",
                        turn_count
                    );
                    let thinking_summary_len = thinking_content.len();
                    if !thinking_content.is_empty() {
                        content_blocks.push(ContentBlock::thinking(
                            thinking_content.clone(),
                            thinking_signature.clone(),
                        ));
                    }
                    messages.push(ChatMessage::assistant(MessageContent::Blocks(
                        content_blocks.clone(),
                    )));
                    messages.push(ChatMessage::user(
                        format!(
                            "STOP THINKING. You spent {} bytes of reasoning with no output. \
                             Respond IMMEDIATELY with a tool call — use write_file, edit_file, or bash. \
                             Do NOT think further. Execute the first reasonable approach you have.",
                            thinking_summary_len
                        ),
                    ));

                    // Reset per-turn state for the continuation
                    assistant_response.clear();
                    prune_messages(&mut messages);
                    continue;
                }
                break;
            }
            ToolUseAction::ContinueServerTools => {
                break;
            }
        }
    }

    match last_stop_reason.as_deref() {
        Some(reason @ ("content_filter" | "SAFETY" | "RECITATION" | "refusal")) => {
            send_chunk(
                &stream_tx,
                StreamChunk::Stopped {
                    stop_reason: reason.to_string(),
                },
            );
        }
        Some("max_tokens") => {
            send_chunk(
                &stream_tx,
                StreamChunk::SystemMessage(
                    "Response truncated (max tokens reached). The response will continue \
                     automatically if auto-continue is enabled."
                        .to_string(),
                ),
            );
            send_chunk(&stream_tx, StreamChunk::Done);
        }
        _ => send_chunk(&stream_tx, StreamChunk::Done),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::workspace_context::find_project_instruction_file;
    use tempfile::TempDir;

    #[test]
    fn test_load_project_instruction_file_finds_claude_md() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.md"), "Project instructions").unwrap();

        let loaded = find_project_instruction_file(temp_dir.path());
        let (filename, content) = loaded.expect("CLAUDE.md should load");

        assert_eq!(filename, "CLAUDE.md");
        assert_eq!(content, "Project instructions");
    }

    #[test]
    fn test_load_project_instruction_file_ignores_instruction_md() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("instruction.md"), "Follow the steps").unwrap();

        assert!(find_project_instruction_file(temp_dir.path()).is_none());
    }

    #[test]
    fn test_load_project_instruction_file_ignores_instructions_md() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("instructions.md"), "Use this instead").unwrap();

        let loaded = find_project_instruction_file(temp_dir.path());
        assert!(loaded.is_none());
    }

    #[test]
    fn test_load_project_instruction_file_missing() {
        let temp_dir = TempDir::new().unwrap();
        assert!(find_project_instruction_file(temp_dir.path()).is_none());
    }

    #[test]
    fn test_fix_conversation_removes_leading_assistant() {
        use super::fix_conversation_messages;
        use rustycode_llm::provider::ChatMessage;

        let mut messages = vec![
            ChatMessage::assistant("hello".to_string()),
            ChatMessage::user("hi".to_string()),
        ];
        fix_conversation_messages(&mut messages);
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].role,
            rustycode_llm::provider::MessageRole::User
        ));
    }

    #[test]
    fn test_fix_conversation_removes_trailing_assistant_without_tools() {
        use super::fix_conversation_messages;
        use rustycode_llm::provider::ChatMessage;

        let mut messages = vec![
            ChatMessage::user("hi".to_string()),
            ChatMessage::assistant("hello".to_string()),
            ChatMessage::assistant("world".to_string()),
        ];
        fix_conversation_messages(&mut messages);
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].role,
            rustycode_llm::provider::MessageRole::User
        ));
    }

    #[test]
    fn test_fix_conversation_merges_consecutive_same_role() {
        use super::fix_conversation_messages;
        use rustycode_llm::provider::ChatMessage;

        let mut messages = vec![
            ChatMessage::user("hello".to_string()),
            ChatMessage::user("world".to_string()),
            ChatMessage::assistant("response".to_string()),
            ChatMessage::user("follow-up".to_string()),
        ];
        fix_conversation_messages(&mut messages);
        // After merging consecutive users and keeping trailing assistant removal:
        // "hello" + "world" merged, "response" kept (followed by user), "follow-up" stays
        assert!(messages[0].content.as_text().contains("hello"));
        assert!(messages[0].content.as_text().contains("world"));
    }

    #[test]
    fn test_fix_conversation_adds_fallback_when_empty() {
        use super::fix_conversation_messages;
        use rustycode_llm::provider::MessageRole;

        let mut messages = vec![];
        fix_conversation_messages(&mut messages);
        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0].role, MessageRole::User));
    }

    #[test]
    fn test_fix_conversation_removes_orphaned_tool_result() {
        use super::fix_conversation_messages;
        use rustycode_llm::provider::ChatMessage;

        let mut messages = vec![
            ChatMessage::user(
                r#"{"type":"tool_result","tool_use_id":"call_123","content":"output"}"#.to_string(),
            ),
            ChatMessage::user("next message".to_string()),
        ];
        fix_conversation_messages(&mut messages);
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].role,
            rustycode_llm::provider::MessageRole::User
        ));
        assert_eq!(messages[0].content.as_text(), "next message");
    }

    #[test]
    fn test_stream_config_orchestration_fields_default() {
        use super::StreamConfig;
        use std::sync::mpsc::sync_channel;

        let (tx, _rx) = sync_channel(100);
        let config = StreamConfig::new("hello", std::path::Path::new("/tmp"), tx);

        assert!(config.orchestration_guidance.is_none());
        assert!(config.phase_context.is_none());
        assert!(config.orchestration.is_none());
    }

    #[test]
    fn test_stream_config_orchestration_builder() {
        use super::StreamConfig;
        use crate::app::orchestration_integration::OrchestrationIntegration;
        use std::sync::mpsc::sync_channel;
        use std::sync::{Arc, Mutex as StdMutex};

        let (tx, _rx) = sync_channel(100);
        let orch = Arc::new(StdMutex::new(OrchestrationIntegration::default()));

        let config = StreamConfig::new("explore", std::path::Path::new("/tmp"), tx)
            .orchestration_guidance_opt(Some("think deeply".to_string()))
            .phase_context_opt(Some("phase 2".to_string()))
            .orchestration_opt(Some(orch));

        assert_eq!(
            config.orchestration_guidance.as_deref(),
            Some("think deeply")
        );
        assert_eq!(config.phase_context.as_deref(), Some("phase 2"));
        assert!(config.orchestration.is_some());
    }
}
