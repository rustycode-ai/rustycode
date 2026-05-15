//! Main LLM response streaming function

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use crate::app::async_::{StreamChunk, StreamError};

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
/// permanently stuck state.
///
/// The guard can be defused (via `defuse()`) to prevent the drop-time Done send.
/// This is critical when Done has already been sent through the agent bridge
/// (e.g., `run_loop` emits `StreamEvent::Done` which the adapter converts).
/// Without defusing, the second Done would race with queued-message streams:
/// the first Done triggers `send_queued_message` which starts a new stream,
/// then the second Done calls `complete_stream_cleanup` on the NEW stream,
/// killing it before any chunks arrive.
struct DoneGuard {
    stream_tx: Option<SyncSender<StreamChunk>>,
}

impl DoneGuard {
    fn new(stream_tx: SyncSender<StreamChunk>) -> Self {
        Self {
            stream_tx: Some(stream_tx),
        }
    }

    /// Prevent the drop handler from sending Done.
    /// Call this when Done has already been sent via another path
    /// (e.g., `run_loop` → `StreamEvent::Done` → adapter).
    fn defuse(mut self) {
        self.stream_tx.take();
    }
}

impl Drop for DoneGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.stream_tx.take() {
            send_chunk(&tx, StreamChunk::Done);
        }
    }
}
use crate::app::tool_errors::ErrorTracker;
use crate::services::file_read_cache::FileReadCache;

use rustycode_llm::provider::ChatMessage;
#[cfg(test)]
use rustycode_protocol::{ContentBlock, MessageContent};

/// Configuration for streaming LLM responses
///
/// Builder pattern to handle the many parameters needed for `run_agent_session_stream`.
pub struct StreamConfig {
    pub content: String,
    pub cwd: std::path::PathBuf,
    pub stream_tx: SyncSender<StreamChunk>,
    pub workspace_context: Option<String>,
    pub stop_signal: Option<Arc<AtomicBool>>,
    pub tools_schema: Option<Vec<serde_json::Value>>,
    pub approval_rx: Option<std::sync::mpsc::Receiver<(String, bool)>>,
    pub question_rx: Option<std::sync::mpsc::Receiver<String>>,
    pub agent_mode: Option<crate::services::agent_mode::AgentMode>,
    pub file_read_cache: Option<Arc<StdMutex<FileReadCache>>>,
    pub error_tracker: Option<Arc<StdMutex<ErrorTracker>>>,
    pub todo_state: Option<rustycode_tools::todo::TodoState>,
    pub conversation_history: Option<Vec<ChatMessage>>,
    pub tool_registry: Option<Arc<rustycode_tools::ToolRegistry>>,
    pub plan_mode: Option<rustycode_orchestration::plan_mode::PlanMode>,
    pub ai_mode: Option<crate::services::agent_mode::AiMode>,
    pub orchestration_guidance: Option<String>,
    pub phase_context: Option<String>,
    pub orchestration:
        Option<Arc<StdMutex<crate::app::orchestration_integration::OrchestrationIntegration>>>,
    pub image_blocks: Option<Vec<rustycode_llm::provider::ContentBlock>>,
    pub effort: Option<String>,
    pub hook_manager: Option<rustycode_tools::hooks::HookManager>,
    pub permission_mode: Option<rustycode_protocol::permission_modes::PermissionMode>,
}

impl StreamConfig {
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
            image_blocks: None,
            effort: None,
            hook_manager: None,
            permission_mode: None,
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

    pub fn approval_rx_opt(
        mut self,
        rx: Option<std::sync::mpsc::Receiver<(String, bool)>>,
    ) -> Self {
        self.approval_rx = rx;
        self
    }

    pub fn question_rx_opt(mut self, rx: Option<std::sync::mpsc::Receiver<String>>) -> Self {
        self.question_rx = rx;
        self
    }

    pub fn agent_mode_opt(mut self, mode: Option<crate::services::agent_mode::AgentMode>) -> Self {
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

    pub fn ai_mode_opt(mut self, mode: Option<crate::services::agent_mode::AiMode>) -> Self {
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

    pub fn image_blocks_opt(
        mut self,
        blocks: Option<Vec<rustycode_llm::provider::ContentBlock>>,
    ) -> Self {
        self.image_blocks = blocks;
        self
    }

    pub fn effort_opt(mut self, effort: Option<String>) -> Self {
        self.effort = effort;
        self
    }

    pub fn hook_manager_opt(mut self, hm: Option<rustycode_tools::hooks::HookManager>) -> Self {
        self.hook_manager = hm;
        self
    }

    pub fn permission_mode_opt(
        mut self,
        mode: Option<rustycode_protocol::permission_modes::PermissionMode>,
    ) -> Self {
        self.permission_mode = mode;
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
    use rustycode_protocol::message::{ContentBlock, MessageContent};

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
        image_blocks,
        // effort is read via RUSTYCODE_EFFORT_OVERRIDE env var inside AgentConfig::from_env()
        effort: _,
        hook_manager: _,
        permission_mode,
    } = config;

    let _done_guard = DoneGuard::new(stream_tx.clone());

    if stop_signal
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire))
    {
        return Ok(());
    }

    let (provider_type, model, v2_config) =
        rustycode_llm::load_provider_config_from_env().context("Failed to load provider config")?;

    let needs_api_key = !matches!(
        provider_type.to_lowercase().as_str(),
        "ollama" | "local" | "lmstudio" | "litert-lm" | "litert_lm" | "litert"
    );
    if needs_api_key && v2_config.api_key.is_none() {
        send_chunk(
            &stream_tx,
            StreamChunk::Error(StreamError::NoApiKey {
                provider: provider_type.clone(),
            }),
        );
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

    // Classify intent from user's message to inject per-request guidance
    let intent_category = rustycode_protocol::intent::classify_intent(&content);
    let intent_suffix = intent_category.prompt_suffix();

    let system_message = super::system_prompt::build_system_prompt(
        &cwd,
        workspace_context.as_deref(),
        agent_mode.as_ref(),
        orchestration_guidance.as_deref(),
        phase_context.as_deref(),
        Some(intent_suffix),
    )
    .await;
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
    if let Some(ref img_blocks) = image_blocks {
        if !img_blocks.is_empty() {
            let mut blocks = vec![rustycode_llm::provider::ContentBlock::text(content)];
            blocks.extend(img_blocks.iter().cloned());
            messages.push(rustycode_llm::provider::ChatMessage {
                role: rustycode_llm::provider::MessageRole::User,
                content: rustycode_protocol::MessageContent::blocks(blocks),
            });
        } else {
            messages.push(rustycode_llm::provider::ChatMessage::user(content));
        }
    } else {
        messages.push(rustycode_llm::provider::ChatMessage::user(content));
    }

    let tool_registry =
        tool_registry.unwrap_or_else(|| std::sync::Arc::new(rustycode_tools::ToolRegistry::new()));
    let tools_schema = tools_schema.unwrap_or_default();
    let agent_config = rustycode_agent_runtime::AgentConfig::from_env();

    // Phase 1A: Setup unified channels
    let (op_tx, op_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut session =
        rustycode_agent_runtime::AgentSession::new(agent_config, cwd).with_op_receiver(op_rx);

    // Subscribe to EventMsg broadcast BEFORE running
    let mut event_rx = session.subscribe();

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
        }))
        .with_permission_mode(
            permission_mode
                .unwrap_or(rustycode_protocol::permission_modes::PermissionMode::Default),
        );

    fix_conversation_messages(&mut messages);

    // Phase 1D: Spawn broadcast handler task
    // This task converts broadcast EventMsgs into TUI StreamChunks
    let _stream_tx_clone = stream_tx.clone();
    let mut adapter = bridge.take_adapter();
    let mut _stop_flag_clone = stop_signal.clone();

    let _broadcast_handler = tokio::spawn(async move {
        while let Ok(msg) = event_rx.recv().await {
            adapter.on_event_msg(msg);

            // If Done is received, we're finished
            // Note: Done is also sent by session.run() ending
        }
        adapter
    });

    let run_future = session.run(
        provider.as_ref(),
        &model,
        &system_message,
        messages,
        &tools_schema,
        tool_registry.as_ref(),
        &mut bridge,
    );

    let result = match stop_signal {
        Some(stop_flag) => {
            tokio::select! {
                res = run_future => res.map_err(|e| {
                    tracing::error!("AgentSession streaming failed: {e:#}");
                    e
                }).context("AgentSession streaming failed"),
                _ = async {
                    loop {
                        if stop_flag.load(std::sync::atomic::Ordering::Acquire) {
                            // Phase 1C: Dispatch StopStream Op to the core
                            let _ = op_tx.send(rustycode_protocol::Op::StopStream);
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                } => {
                    tracing::info!("Streaming cancelled by user via Op::StopStream");
                    // After sending StopStream, we still need to wait for the future
                    // But since it was already moved into the select!, we need a different approach
                    // For now, return an error to indicate cancellation
                    Err(anyhow::anyhow!("Streaming cancelled by user"))
                }
            }
        }
        None => run_future
            .await
            .map_err(|e| {
                tracing::error!("AgentSession streaming failed: {e:#}");
                e
            })
            .context("AgentSession streaming failed"),
    };

    if let Err(err) = result {
        send_chunk(
            &stream_tx,
            StreamChunk::Error(StreamError::Provider(
                rustycode_llm::provider::ProviderError::Api(err.to_string()),
            )),
        );
        return Err(err);
    }

    // session.run() emits StreamEvent::Done internally, which the adapter
    // converts to StreamChunk::Done. Defuse the guard to prevent a second Done.
    _done_guard.defuse();

    Ok(())
}

pub async fn run_agent_session_stream(config: StreamConfig) -> Result<()> {
    stream_llm_response_agent(config).await
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    use crate::workspace::workspace_context::find_project_instruction_file;
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
