//! Core session logic for RustyCode.
//! This module contains the business logic that is independent of UI implementation.
//! It can be used by TUI, web UI, or any other frontend.

use anyhow::Result;
use chrono::Utc;
use rustycode_llm::ConversationManager;
use rustycode_memory::MemoryEntry;
use rustycode_protocol::{
    ContentBlock, Conversation, Message, MessageContent, MessageMetadata, MessageRole, SessionId,
    ToolCall, ToolResult as ProtocolToolResult,
};
use rustycode_tools_api::{new_todo_state, TodoItem};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::checkpoint_recovery::Recovery;
use crate::checkpoint_store::CheckpointStore;

/// Maximum messages retained in memory before oldest are evicted.
const MAX_SESSION_MESSAGES: usize = 1000;

/// Todo state - shared list of todo items
pub type TodoState = Arc<Mutex<Vec<TodoItem>>>;

/// AI behavior mode - determines how autonomous the AI should be
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Hash, Default)]
pub enum AiMode {
    /// Default mode - ask before destructive actions
    #[default]
    Ask,
    /// Plan mode - only describe what would be done, don't execute
    Plan,
    /// Act mode - execute but summarize before destructive actions
    Act,
    /// Yolo mode - fully autonomous, no confirmation
    Yolo,
}

/// Tool execution status
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum ToolStatus {
    Running,
    Complete,
}

/// Track tool execution with timing and metadata
#[derive(Clone, Debug)]
pub struct ToolExecution {
    pub name: String,
    pub status: ToolStatus,
    pub start_time: Instant,
    pub output_preview: String,
}

/// Checkpoint recovery state - tracks crash recovery and checkpointing
pub struct CheckpointState {
    pub counter: u32,
    pub last_checkpoint_time: Instant,
    pub last_checkpoint_id: Option<String>,
    pub recovery_handler: Option<Recovery>,
}

/// Edit preview state - tracks file edit previews
#[derive(Clone, Debug)]
pub struct EditPreviewState {
    pub file_path: Option<String>,
    pub original_content: String,
    pub new_content: String,
}

/// Code panel state - tracks code display in the UI
#[derive(Clone, Debug)]
pub struct CodePanelState {
    pub file: Option<String>,
    pub content: String,
    pub language: String,
}

/// Core session state - independent of UI implementation
pub struct SessionState {
    // Conversation state
    pub messages: Vec<Message>,
    pub conversation_manager: ConversationManager,
    pub llm_provider: Option<Box<dyn rustycode_llm::LLMProvider>>,
    pub pending_llm_request: Arc<Mutex<bool>>,

    // Input state
    pub input: String,
    pub scroll_offset: usize,
    pub selected_message: usize,

    // Session metadata
    pub cwd: PathBuf,
    pub workspace_context: String,
    pub session_title: String,
    pub tokens_used: usize,
    pub last_response_tokens: usize,
    pub total_requests: usize,

    // Tool execution
    pub active_tools: Vec<ToolExecution>,
    pub current_session_tools: Vec<String>,
    pub tool_iteration_count: u32,
    pub pending_tool_call: Option<ToolCall>,

    // AI behavior mode
    pub ai_mode: AiMode,

    // Persistent memory
    pub memory_entries: Vec<MemoryEntry>,

    // First run detection
    pub is_first_run: bool,

    // Model selection
    pub available_models: Vec<String>,
    pub current_model: String,
    pub provider_configured: bool,

    // Performance monitoring
    pub request_start_time: Option<Instant>,
    pub request_latencies: Vec<u128>, // Store last 100 request latencies in ms
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub current_request_input_tokens: usize,
    pub error_count: usize,
    pub last_request_latency: Option<u128>,

    // Token budget
    pub token_budget: Option<usize>,

    // Streaming state
    pub is_streaming: bool,
    pub current_response: String,

    // Edit preview state
    pub edit_preview: EditPreviewState,

    // Regeneration
    pub last_user_prompt: Option<String>,

    // System prompt cache
    pub cached_system_prompt: String,

    // Checkpointing for crash recovery
    pub checkpoint: CheckpointState,

    // Todo state for task planning
    pub todo_state: TodoState,

    /// Tool executor for running tools
    pub tool_executor: rustycode_tools::ToolExecutor,

    // Code panel state (for showing file contents)
    pub code_panel: CodePanelState,
}

/// Message type for role mapping
/// Maps MessageType to protocol Message role strings
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum MessageType {
    User,
    AI,
    System,
    Tool,
    #[allow(dead_code)] // Kept for future use
    Thinking,
    Error,
}

impl MessageType {
    /// Convert MessageType to protocol MessageRole
    pub fn as_message_role(&self) -> MessageRole {
        match self {
            Self::User => MessageRole::User,
            Self::AI => MessageRole::Assistant,
            Self::System => MessageRole::System,
            Self::Tool => MessageRole::Tool("tool".to_string()),
            Self::Thinking => MessageRole::Tool("thinking".to_string()),
            Self::Error => MessageRole::Tool("error".to_string()),
        }
    }

    /// Convert protocol MessageRole to MessageType
    pub fn from_message_role(role: &MessageRole) -> Self {
        match role {
            MessageRole::User => Self::User,
            MessageRole::Assistant => Self::AI,
            MessageRole::System => Self::System,
            MessageRole::Tool(name) => match name.as_str() {
                "thinking" => Self::Thinking,
                "error" => Self::Error,
                _ => Self::Tool,
            },
            _ => Self::System, // Unknown role, default to System for future-proofing
        }
    }

    /// Convert MessageType to protocol message role string (deprecated, use as_message_role)
    pub fn as_role(&self) -> String {
        self.as_message_role().to_string()
    }

    /// Convert protocol message role string to MessageType (deprecated, use from_message_role)
    pub fn from_role(role: &str) -> Self {
        Self::from_message_role(&MessageRole::from(role))
    }
}

/// Token budget check result.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenBudgetStatus {
    /// Over 90% consumed but not yet exceeded.
    Warning {
        used: usize,
        budget: usize,
        remaining: usize,
    },
    /// Budget has been exceeded.
    Exceeded {
        used: usize,
        budget: usize,
        over_by: usize,
    },
}

impl std::fmt::Display for TokenBudgetStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Warning {
                used,
                budget,
                remaining,
            } => {
                write!(
                    f,
                    "token budget warning: {used}/{budget} used ({remaining} remaining)"
                )
            }
            Self::Exceeded {
                used,
                budget,
                over_by,
            } => {
                write!(
                    f,
                    "token budget exceeded: {used}/{budget} (over by {over_by})"
                )
            }
        }
    }
}

impl SessionState {
    /// Create a new session state
    pub fn new(cwd: PathBuf) -> Self {
        let session_id = SessionId::new();
        let conversation = Conversation::new(session_id);
        Self {
            messages: Vec::new(),
            conversation_manager: ConversationManager::new(conversation),
            llm_provider: None,
            pending_llm_request: Arc::new(Mutex::new(false)),
            input: String::new(),
            scroll_offset: 0,
            selected_message: 0,
            cwd: cwd.clone(),
            workspace_context: String::new(),
            session_title: "New Session".to_string(),
            tokens_used: 0,
            last_response_tokens: 0,
            total_requests: 0,
            active_tools: Vec::new(),
            current_session_tools: Vec::new(),
            tool_iteration_count: 0,
            pending_tool_call: None,
            ai_mode: AiMode::Act,
            memory_entries: Vec::new(),
            is_first_run: false,
            available_models: Vec::new(),
            current_model: String::new(),
            provider_configured: false,
            request_start_time: None,
            request_latencies: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            current_request_input_tokens: 0,
            error_count: 0,
            last_request_latency: None,
            token_budget: None,
            is_streaming: false,
            current_response: String::new(),
            edit_preview: EditPreviewState {
                file_path: None,
                original_content: String::new(),
                new_content: String::new(),
            },
            last_user_prompt: None,
            cached_system_prompt: String::new(),
            checkpoint: CheckpointState {
                counter: 0,
                last_checkpoint_time: Instant::now(),
                last_checkpoint_id: None,
                recovery_handler: None,
            },
            todo_state: new_todo_state(),
            tool_executor: rustycode_tools::ToolExecutor::from_cwd(cwd.clone()),
            code_panel: CodePanelState {
                file: None,
                content: String::new(),
                language: String::new(),
            },
        }
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, content: String, message_type: MessageType) {
        let message = Message {
            role: message_type.as_message_role(),
            content: MessageContent::simple(content),
            timestamp: Utc::now(),
            metadata: MessageMetadata::default(),
        };
        self.messages.push(message);
        // Evict oldest non-system messages when cap exceeded (batch O(n))
        if self.messages.len() > MAX_SESSION_MESSAGES {
            let excess = self.messages.len() - MAX_SESSION_MESSAGES;
            let mut removed = 0usize;
            self.messages.retain(|m| {
                if removed >= excess {
                    return true;
                }
                if MessageType::from_message_role(&m.role) != MessageType::System {
                    removed += 1;
                    false
                } else {
                    true
                }
            });
        }
        self.scroll_offset = self.messages.len().saturating_sub(1);
    }

    /// Add tool call blocks to the last AI message
    pub fn add_tool_calls(&mut self, tool_calls: Vec<ToolCall>) {
        if let Some(msg) = self.messages.last_mut() {
            if MessageType::from_message_role(&msg.role) == MessageType::AI {
                // Convert ToolCall to ContentBlock::ToolUse and add to message content
                let tool_use_blocks: Vec<ContentBlock> = tool_calls
                    .into_iter()
                    .map(|tc| ContentBlock::ToolUse {
                        id: tc.call_id,
                        name: tc.name,
                        input: tc.arguments,
                    })
                    .collect();

                // Get existing text content if any, then append tool use blocks
                match &msg.content {
                    MessageContent::Simple(_) => {
                        // Convert simple content to blocks with text + tool use
                        if let MessageContent::Simple(text) = &msg.content {
                            let mut blocks = vec![ContentBlock::Text {
                                text: text.clone(),
                                cache_control: None,
                            }];
                            blocks.extend(tool_use_blocks);
                            msg.content = MessageContent::Blocks(blocks);
                        }
                    }
                    MessageContent::Blocks(blocks) => {
                        // Append to existing blocks
                        let mut new_blocks = blocks.clone();
                        new_blocks.extend(tool_use_blocks);
                        msg.content = MessageContent::Blocks(new_blocks);
                    }
                    _ => {
                        // Other variants of MessageContent handled here for future-proofing
                    }
                }
            }
        }
    }

    /// Add tool results to the conversation
    pub fn add_tool_results(&mut self, tool_results: Vec<ProtocolToolResult>) {
        // Convert ToolResult to ContentBlock::ToolResult
        let tool_result_blocks: Vec<ContentBlock> = tool_results
            .iter()
            .map(|tr| ContentBlock::ToolResult {
                tool_use_id: tr.call_id.clone(),
                content: tr.output.clone(),
                is_error: tr.error.is_some(),
            })
            .collect();

        // Create message with tool result blocks
        let message = Message {
            role: MessageRole::User, // Tool results come from user (system) perspective
            content: MessageContent::Blocks(tool_result_blocks),
            timestamp: Utc::now(),
            metadata: MessageMetadata::default(),
        };
        self.messages.push(message);
    }

    /// Format tool results for display
    #[allow(dead_code)]
    fn format_tool_results(results: &[rustycode_protocol::ToolResult]) -> String {
        let mut output = String::new();
        for result in results {
            output.push_str(&format!("Tool Call: {}\n", result.call_id));
            if result.error.is_none() {
                output.push_str(&format!("Output: {}\n", result.output));
            } else if let Some(ref error_text) = result.error {
                output.push_str(&format!("Error: {}\n", error_text));
            }
            output.push('\n');
        }
        output
    }

    /// Update token usage statistics
    pub fn update_token_usage(&mut self, input_tokens: usize, output_tokens: usize) {
        self.total_input_tokens = self.total_input_tokens.saturating_add(input_tokens);
        self.total_output_tokens = self.total_output_tokens.saturating_add(output_tokens);
        self.tokens_used = self
            .total_input_tokens
            .saturating_add(self.total_output_tokens);
        self.current_request_input_tokens = input_tokens;
        self.last_response_tokens = output_tokens;
    }

    /// Set a token budget for this session. When `tokens_used` exceeds this,
    /// `check_token_budget()` returns a warning.
    pub fn set_token_budget(&mut self, budget: usize) {
        self.token_budget = Some(budget);
    }

    /// Check if the session is within its token budget.
    /// Returns `Ok(())` if within budget, or `Err` with remaining tokens info.
    pub fn check_token_budget(&self) -> Result<(), TokenBudgetStatus> {
        let Some(budget) = self.token_budget else {
            return Ok(());
        };
        if self.tokens_used > budget {
            Err(TokenBudgetStatus::Exceeded {
                used: self.tokens_used,
                budget,
                over_by: self.tokens_used.saturating_sub(budget),
            })
        } else if self.tokens_used > budget / 10 * 9 {
            Err(TokenBudgetStatus::Warning {
                used: self.tokens_used,
                budget,
                remaining: budget.saturating_sub(self.tokens_used),
            })
        } else {
            Ok(())
        }
    }

    /// Fraction of token budget consumed (0.0 to 1.0+).
    pub fn token_budget_fraction(&self) -> f64 {
        let budget = self.token_budget.unwrap_or(usize::MAX);
        if budget == 0 {
            return 1.0;
        }
        self.tokens_used as f64 / budget as f64
    }

    /// Record request latency
    pub fn record_latency(&mut self, latency_ms: u128) {
        self.request_latencies.push(latency_ms);
        if self.request_latencies.len() > 100 {
            self.request_latencies
                .drain(0..self.request_latencies.len() - 100);
        }
        self.last_request_latency = Some(latency_ms);
    }

    /// Increment error count
    pub fn increment_error_count(&mut self) {
        self.error_count = self.error_count.saturating_add(1);
    }

    /// Start a tool execution
    pub fn start_tool_execution(&mut self, name: String) {
        self.active_tools.push(ToolExecution {
            name: name.clone(),
            status: ToolStatus::Running,
            start_time: Instant::now(),
            output_preview: String::new(),
        });
        self.current_session_tools.push(name);
    }

    /// Complete a tool execution
    pub fn complete_tool_execution(&mut self, name: &str, output: String) {
        if let Some(tool) = self.active_tools.iter_mut().find(|t| t.name == name) {
            tool.output_preview = output.chars().take(100).collect();
        }
        // Remove completed tool to prevent unbounded growth
        self.active_tools.retain(|t| t.name != name);
    }

    /// Set streaming state
    pub fn set_streaming(&mut self, is_streaming: bool) {
        self.is_streaming = is_streaming;
        if !is_streaming {
            self.current_response.clear();
        }
    }

    /// Append to current streaming response
    pub fn append_streaming_response(&mut self, text: &str) {
        self.current_response.push_str(text);
    }

    /// Complete streaming response and add to messages
    pub fn complete_streaming_response(&mut self) {
        if !self.current_response.is_empty() {
            let message = Message {
                role: MessageRole::Assistant,
                content: MessageContent::simple(self.current_response.clone()),
                timestamp: Utc::now(),
                metadata: MessageMetadata::default(),
            };
            self.messages.push(message);
            self.current_response.clear();
        }
        self.is_streaming = false;
    }

    /// Safely set pending LLM request flag
    pub fn set_pending_request(&self, value: bool) {
        let mut guard = self.pending_llm_request.lock().unwrap_or_else(|e| {
            tracing::warn!("pending_llm_request mutex poisoned, recovering");
            e.into_inner()
        });
        *guard = value;
    }

    /// Safely get pending LLM request flag
    pub fn get_pending_request(&self) -> bool {
        self.pending_llm_request
            .lock()
            .map(|guard| *guard)
            .unwrap_or(false)
    }

    /// Update workspace context
    pub fn update_workspace_context(&mut self, context: String) {
        self.workspace_context = context;
    }

    /// Set the current model
    pub fn set_model(&mut self, model: String) {
        self.current_model = model;
    }

    /// Set available models
    pub fn set_available_models(&mut self, models: Vec<String>) {
        self.available_models = models;
    }

    /// Set provider configured status
    pub fn set_provider_configured(&mut self, configured: bool) {
        self.provider_configured = configured;
    }

    /// Update edit preview
    pub fn update_edit_preview(
        &mut self,
        file_path: Option<String>,
        original: String,
        new: String,
    ) {
        self.edit_preview.file_path = file_path;
        self.edit_preview.original_content = original;
        self.edit_preview.new_content = new;
    }

    /// Clear edit preview
    pub fn clear_edit_preview(&mut self) {
        self.edit_preview.file_path = None;
        self.edit_preview.original_content.clear();
        self.edit_preview.new_content.clear();
    }

    /// Update code panel
    pub fn update_code_panel(&mut self, file: Option<String>, content: String, language: String) {
        self.code_panel.file = file;
        self.code_panel.content = content;
        self.code_panel.language = language;
    }

    /// Clear code panel
    pub fn clear_code_panel(&mut self) {
        self.code_panel.file = None;
        self.code_panel.content.clear();
        self.code_panel.language.clear();
    }

    /// Execute a tool call
    pub fn execute_tool(&mut self, call: &ToolCall) -> rustycode_protocol::ToolResult {
        self.start_tool_execution(call.name.clone());
        let result = self.tool_executor.execute(call);
        self.complete_tool_execution(&call.name, result.output.clone());
        result
    }

    /// Restore session state from a stored checkpoint.
    pub fn restore_from_checkpoint(
        &mut self,
        checkpoint_id: &str,
        store: &CheckpointStore,
    ) -> Result<()> {
        let checkpoint = store.load(checkpoint_id)?;
        let mut recovery = Recovery::from_checkpoint(checkpoint);
        recovery.validate()?;
        self.checkpoint.recovery_handler = Some(recovery);
        self.checkpoint.last_checkpoint_id = Some(checkpoint_id.to_string());
        Ok(())
    }

    /// Get list of pending effects from the recovery handler.
    pub fn pending_effects(&self) -> Vec<String> {
        self.checkpoint
            .recovery_handler
            .as_ref()
            .map(Recovery::remaining_effects)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- AiMode tests ---

    #[test]
    fn ai_mode_default_is_ask() {
        assert_eq!(AiMode::default(), AiMode::Ask);
    }

    #[test]
    fn ai_mode_variants_distinct() {
        let modes = [AiMode::Ask, AiMode::Plan, AiMode::Act, AiMode::Yolo];
        for (i, m) in modes.iter().enumerate() {
            for (j, n) in modes.iter().enumerate() {
                if i != j {
                    assert_ne!(m, n);
                }
            }
        }
    }

    // --- MessageType tests ---

    #[test]
    fn message_type_variants() {
        let types = [
            MessageType::User,
            MessageType::AI,
            MessageType::System,
            MessageType::Tool,
            MessageType::Thinking,
            MessageType::Error,
        ];
        for (i, t) in types.iter().enumerate() {
            for (j, u) in types.iter().enumerate() {
                if i != j {
                    assert_ne!(t, u);
                }
            }
        }
    }

    // --- ToolStatus tests ---

    #[test]
    fn tool_status_variants() {
        assert_ne!(ToolStatus::Running, ToolStatus::Complete);
    }

    // --- SessionState tests ---

    fn make_session() -> SessionState {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
        SessionState::new(cwd)
    }

    #[test]
    fn session_state_new_defaults() {
        let s = make_session();
        assert!(s.messages.is_empty());
        assert!(s.input.is_empty());
        assert_eq!(s.scroll_offset, 0);
        assert_eq!(s.total_requests, 0);
        assert_eq!(s.tokens_used, 0);
        assert!(!s.is_streaming);
        assert!(s.current_response.is_empty());
        assert!(s.active_tools.is_empty());
        assert!(!s.provider_configured);
        assert_eq!(s.error_count, 0);
    }

    #[test]
    fn add_message_user() {
        let mut s = make_session();
        s.add_message("Hello".to_string(), MessageType::User);
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].content.as_text(), "Hello");
        assert_eq!(
            MessageType::from_role(s.messages[0].role.as_str()),
            MessageType::User
        );
    }

    #[test]
    fn add_message_multiple() {
        let mut s = make_session();
        s.add_message("Hi".to_string(), MessageType::User);
        s.add_message("Response".to_string(), MessageType::AI);
        s.add_message("Error occurred".to_string(), MessageType::Error);
        assert_eq!(s.messages.len(), 3);
    }

    #[test]
    fn update_token_usage_accumulates() {
        let mut s = make_session();
        s.update_token_usage(100, 50);
        assert_eq!(s.total_input_tokens, 100);
        assert_eq!(s.total_output_tokens, 50);
        assert_eq!(s.tokens_used, 150);

        s.update_token_usage(200, 100);
        assert_eq!(s.total_input_tokens, 300);
        assert_eq!(s.total_output_tokens, 150);
        assert_eq!(s.tokens_used, 450);
    }

    #[test]
    fn record_latency_caps_at_100() {
        let mut s = make_session();
        for i in 0..105 {
            s.record_latency(i as u128);
        }
        assert_eq!(s.request_latencies.len(), 100);
        // Oldest entries should have been removed
        assert_eq!(s.request_latencies[0], 5);
    }

    #[test]
    fn record_latency_tracks_last() {
        let mut s = make_session();
        s.record_latency(42);
        assert_eq!(s.last_request_latency, Some(42));
    }

    #[test]
    fn increment_error_count() {
        let mut s = make_session();
        assert_eq!(s.error_count, 0);
        s.increment_error_count();
        s.increment_error_count();
        assert_eq!(s.error_count, 2);
    }

    #[test]
    fn start_and_complete_tool_execution() {
        let mut s = make_session();
        s.start_tool_execution("bash".to_string());
        assert_eq!(s.active_tools.len(), 1);
        assert_eq!(s.active_tools[0].name, "bash");
        assert_eq!(s.active_tools[0].status, ToolStatus::Running);
        assert!(s.current_session_tools.contains(&"bash".to_string()));

        s.complete_tool_execution("bash", "output text".to_string());
        // Completed tools are removed from active_tools to prevent unbounded growth
        assert!(s.active_tools.is_empty());
        assert!(s.current_session_tools.contains(&"bash".to_string()));
    }

    #[test]
    fn set_streaming_state() {
        let mut s = make_session();
        assert!(!s.is_streaming);

        s.set_streaming(true);
        assert!(s.is_streaming);

        s.set_streaming(false);
        assert!(!s.is_streaming);
        assert!(s.current_response.is_empty());
    }

    #[test]
    fn append_and_complete_streaming() {
        let mut s = make_session();
        s.set_streaming(true);
        s.append_streaming_response("Hello ");
        s.append_streaming_response("World");
        assert_eq!(s.current_response, "Hello World");

        s.complete_streaming_response();
        assert!(!s.is_streaming);
        assert!(s.current_response.is_empty());
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].content.as_text(), "Hello World");
        assert_eq!(MessageType::from_role(s.messages[0].role.as_str()), MessageType::AI);
    }

    #[test]
    fn complete_streaming_empty_no_message() {
        let mut s = make_session();
        s.set_streaming(true);
        s.complete_streaming_response();
        assert!(s.messages.is_empty());
    }

    #[test]
    fn pending_request_flag() {
        let s = make_session();
        assert!(!s.get_pending_request());

        s.set_pending_request(true);
        assert!(s.get_pending_request());

        s.set_pending_request(false);
        assert!(!s.get_pending_request());
    }

    #[test]
    fn update_workspace_context() {
        let mut s = make_session();
        assert!(s.workspace_context.is_empty());
        s.update_workspace_context("Rust project".to_string());
        assert_eq!(s.workspace_context, "Rust project");
    }

    #[test]
    fn set_model() {
        let mut s = make_session();
        assert!(s.current_model.is_empty());
        s.set_model("claude-3-opus".to_string());
        assert_eq!(s.current_model, "claude-3-opus");
    }

    #[test]
    fn set_available_models() {
        let mut s = make_session();
        s.set_available_models(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(s.available_models.len(), 2);
    }

    #[test]
    fn set_provider_configured() {
        let mut s = make_session();
        assert!(!s.provider_configured);
        s.set_provider_configured(true);
        assert!(s.provider_configured);
    }

    #[test]
    fn update_and_clear_edit_preview() {
        let mut s = make_session();
        s.update_edit_preview(
            Some("file.rs".to_string()),
            "old content".to_string(),
            "new content".to_string(),
        );
        assert_eq!(s.edit_preview.file_path, Some("file.rs".to_string()));
        assert_eq!(s.edit_preview.original_content, "old content");

        s.clear_edit_preview();
        assert!(s.edit_preview.file_path.is_none());
        assert!(s.edit_preview.original_content.is_empty());
    }

    #[test]
    fn update_and_clear_code_panel() {
        let mut s = make_session();
        s.update_code_panel(
            Some("main.rs".to_string()),
            "fn main() {}".to_string(),
            "rust".to_string(),
        );
        assert_eq!(s.code_panel.file, Some("main.rs".to_string()));

        s.clear_code_panel();
        assert!(s.code_panel.file.is_none());
        assert!(s.code_panel.content.is_empty());
    }

    #[test]
    fn add_tool_calls_to_ai_message() {
        let mut s = make_session();
        s.add_message("AI response".to_string(), MessageType::AI);

        let tc = ToolCall {
            call_id: "call_1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "ls"}),
        };
        s.add_tool_calls(vec![tc]);

        // Check that ToolUse block was added to content
        match &s.messages[0].content {
            MessageContent::Blocks(blocks) => {
                let tool_use_blocks: Vec<_> = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
                    .collect();
                assert_eq!(tool_use_blocks.len(), 1);
            }
            _ => panic!("Expected content to be Blocks variant"),
        }
    }

    #[test]
    fn add_tool_calls_not_added_to_non_ai() {
        let mut s = make_session();
        s.add_message("User message".to_string(), MessageType::User);

        let tc = ToolCall {
            call_id: "call_1".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({}),
        };
        s.add_tool_calls(vec![tc]);
        // Should not be added to User messages - content should remain unchanged
        assert_eq!(s.messages[0].content.as_text(), "User message");
    }

    #[test]
    fn tool_execution_completed_and_removed() {
        let mut s = make_session();
        s.start_tool_execution("bash".to_string());
        assert_eq!(s.active_tools.len(), 1);
        let long_output = "x".repeat(200);
        s.complete_tool_execution("bash", long_output);
        // Completed tools are removed from active_tools
        assert!(s.active_tools.is_empty());
    }

    // --- Checkpoint recovery integration tests ---

    use crate::checkpoint::ExecutionPhase;

    fn valid_checkpoint_with_effects(
        effects: Vec<String>,
    ) -> crate::checkpoint::CheckpointSnapshot {
        let mut cp =
            crate::checkpoint::CheckpointSnapshot::generate("test-session", ExecutionPhase::Act);
        cp.pending_effects = effects;
        cp
    }

    #[test]
    fn session_state_initializes_with_none() {
        let s = make_session();
        assert!(s.checkpoint.last_checkpoint_id.is_none());
        assert!(s.checkpoint.recovery_handler.is_none());
        assert!(s.pending_effects().is_empty());
    }

    #[test]
    fn restore_from_checkpoint_loads_checkpoint() {
        let mut s = make_session();
        let cp = valid_checkpoint_with_effects(vec!["e1".into(), "e2".into()]);
        let id = cp.id.clone();
        let mut store = CheckpointStore::new();
        store.save(cp);

        let result = s.restore_from_checkpoint(&id, &store);
        assert!(result.is_ok());
        assert_eq!(s.checkpoint.last_checkpoint_id, Some(id));
        assert!(s.checkpoint.recovery_handler.is_some());
    }

    #[test]
    fn restore_from_checkpoint_fails_for_invalid() {
        let mut s = make_session();
        let mut cp =
            crate::checkpoint::CheckpointSnapshot::generate("test-session", ExecutionPhase::Act);
        cp.memory_state = Vec::new();
        let id = cp.id.clone();
        let mut store = CheckpointStore::new();
        store.save(cp);

        let result = s.restore_from_checkpoint(&id, &store);
        assert!(result.is_err());
        assert!(s.checkpoint.last_checkpoint_id.is_none());
    }

    #[test]
    fn pending_effects_returns_effects() {
        let mut s = make_session();
        let cp = valid_checkpoint_with_effects(vec!["a".into(), "b".into(), "c".into()]);
        let id = cp.id.clone();
        let mut store = CheckpointStore::new();
        store.save(cp);

        s.restore_from_checkpoint(&id, &store).unwrap();
        let mut effects = s.pending_effects();
        effects.sort();
        assert_eq!(effects, vec!["a", "b", "c"]);
    }

    #[test]
    fn pending_effects_returns_empty_when_no_recovery() {
        let s = make_session();
        assert!(s.pending_effects().is_empty());
    }

    // --- Token budget tests ---

    #[test]
    fn token_budget_defaults_to_none() {
        let s = make_session();
        assert!(s.token_budget.is_none());
        assert!(s.check_token_budget().is_ok());
    }

    #[test]
    fn token_budget_within_limits() {
        let mut s = make_session();
        s.set_token_budget(1000);
        s.update_token_usage(100, 50);
        assert!(s.check_token_budget().is_ok());
        assert!(s.token_budget_fraction() < 0.9);
    }

    #[test]
    fn token_budget_warning_at_90_percent() {
        let mut s = make_session();
        s.set_token_budget(1000);
        s.update_token_usage(920, 0);
        let result = s.check_token_budget();
        assert!(matches!(result, Err(TokenBudgetStatus::Warning { .. })));
    }

    #[test]
    fn token_budget_exceeded() {
        let mut s = make_session();
        s.set_token_budget(100);
        s.update_token_usage(150, 0);
        let result = s.check_token_budget();
        assert!(matches!(
            result,
            Err(TokenBudgetStatus::Exceeded { over_by: 50, .. })
        ));
    }

    #[test]
    fn token_budget_fraction_tracks_usage() {
        let mut s = make_session();
        s.set_token_budget(1000);
        s.update_token_usage(250, 250);
        let frac = s.token_budget_fraction();
        assert!((frac - 0.5).abs() < 0.01, "Expected ~0.5, got {frac}");
    }

    #[test]
    fn token_budget_status_display() {
        let warn = TokenBudgetStatus::Warning {
            used: 920,
            budget: 1000,
            remaining: 80,
        };
        assert!(warn.to_string().contains("warning"));
        let exceeded = TokenBudgetStatus::Exceeded {
            used: 1100,
            budget: 1000,
            over_by: 100,
        };
        assert!(exceeded.to_string().contains("exceeded"));
    }

    #[test]
    fn message_eviction_preserves_system_messages() {
        let mut s = make_session();
        s.add_message("system prompt".to_string(), MessageType::System);
        for i in 0..=MAX_SESSION_MESSAGES {
            s.add_message(format!("user_{i}"), MessageType::User);
        }
        assert!(
            s.messages.len() <= MAX_SESSION_MESSAGES,
            "should be at or below cap: got {}",
            s.messages.len()
        );
        assert!(
            s.messages
                .iter()
                .any(|m| m.content.as_text() == "system prompt"),
            "system message should be preserved"
        );
    }
}
