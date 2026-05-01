//! Core session logic for RustyCode.
//! This module contains the business logic that is independent of UI implementation.
//! It can be used by TUI, web UI, or any other frontend.

use anyhow::Result;
use rustycode_llm::ConversationManager;
use rustycode_memory::MemoryEntry;
use rustycode_protocol::{Conversation, SessionId, ToolCall};
use rustycode_tools_api::{new_todo_state, TodoItem};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::checkpoint_recovery::Recovery;
use crate::checkpoint_store::CheckpointStore;

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

/// Core session state - independent of UI implementation
pub struct SessionState {
    // Conversation state
    pub messages: Vec<ChatMessage>,
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

    // Streaming state
    pub is_streaming: bool,
    pub current_response: String,

    // Edit preview state
    pub edit_file_path: Option<String>,
    pub edit_original_content: String,
    pub edit_new_content: String,

    // Regeneration
    pub last_user_prompt: Option<String>,

    // System prompt cache
    pub cached_system_prompt: String,

    // Checkpointing for crash recovery
    pub checkpoint_counter: u32,
    pub last_checkpoint_time: Instant,
    pub last_checkpoint_id: Option<String>,
    pub recovery_handler: Option<Recovery>,

    // Todo state for task planning
    pub todo_state: TodoState,

    /// Tool executor for running tools
    pub tool_executor: rustycode_tools::ToolExecutor,

    // Code panel state (for showing file contents)
    pub code_panel_file: Option<String>,
    pub code_panel_content: String,
    pub code_panel_language: String,
}

/// Chat message representation
#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub content: String,
    pub message_type: MessageType,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_results: Option<Vec<rustycode_protocol::ToolResult>>,
}

/// Message type for color coding
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
            is_streaming: false,
            current_response: String::new(),
            edit_file_path: None,
            edit_original_content: String::new(),
            edit_new_content: String::new(),
            last_user_prompt: None,
            cached_system_prompt: String::new(),
            checkpoint_counter: 0,
            last_checkpoint_time: Instant::now(),
            last_checkpoint_id: None,
            recovery_handler: None,
            todo_state: new_todo_state(),
            tool_executor: rustycode_tools::ToolExecutor::from_cwd(cwd.clone()),
            code_panel_file: None,
            code_panel_content: String::new(),
            code_panel_language: String::new(),
        }
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, content: String, message_type: MessageType) {
        let message = ChatMessage {
            content,
            message_type,
            tool_calls: None,
            tool_results: None,
        };
        self.messages.push(message);
        self.scroll_offset = self.messages.len().saturating_sub(1);
    }

    /// Add a tool call to the last AI message
    pub fn add_tool_calls(&mut self, tool_calls: Vec<ToolCall>) {
        if let Some(msg) = self.messages.last_mut() {
            if msg.message_type == MessageType::AI {
                msg.tool_calls = Some(tool_calls);
            }
        }
    }

    /// Add tool results to the conversation
    pub fn add_tool_results(&mut self, tool_results: Vec<rustycode_protocol::ToolResult>) {
        let content = Self::format_tool_results(&tool_results);
        self.messages.push(ChatMessage {
            content,
            message_type: MessageType::Tool,
            tool_calls: None,
            tool_results: Some(tool_results),
        });
    }

    /// Format tool results for display
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
            self.add_message(self.current_response.clone(), MessageType::AI);
            self.current_response.clear();
        }
        self.is_streaming = false;
    }

    /// Safely set pending LLM request flag
    pub fn set_pending_request(&self, value: bool) {
        if let Ok(mut guard) = self.pending_llm_request.lock() {
            *guard = value;
        }
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
        self.edit_file_path = file_path;
        self.edit_original_content = original;
        self.edit_new_content = new;
    }

    /// Clear edit preview
    pub fn clear_edit_preview(&mut self) {
        self.edit_file_path = None;
        self.edit_original_content.clear();
        self.edit_new_content.clear();
    }

    /// Update code panel
    pub fn update_code_panel(&mut self, file: Option<String>, content: String, language: String) {
        self.code_panel_file = file;
        self.code_panel_content = content;
        self.code_panel_language = language;
    }

    /// Clear code panel
    pub fn clear_code_panel(&mut self) {
        self.code_panel_file = None;
        self.code_panel_content.clear();
        self.code_panel_language.clear();
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
        self.recovery_handler = Some(recovery);
        self.last_checkpoint_id = Some(checkpoint_id.to_string());
        Ok(())
    }

    /// Get list of pending effects from the recovery handler.
    pub fn pending_effects(&self) -> Vec<String> {
        self.recovery_handler
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
        assert_eq!(s.messages[0].content, "Hello");
        assert_eq!(s.messages[0].message_type, MessageType::User);
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
        assert_eq!(s.messages[0].content, "Hello World");
        assert_eq!(s.messages[0].message_type, MessageType::AI);
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
        assert_eq!(s.edit_file_path, Some("file.rs".to_string()));
        assert_eq!(s.edit_original_content, "old content");

        s.clear_edit_preview();
        assert!(s.edit_file_path.is_none());
        assert!(s.edit_original_content.is_empty());
    }

    #[test]
    fn update_and_clear_code_panel() {
        let mut s = make_session();
        s.update_code_panel(
            Some("main.rs".to_string()),
            "fn main() {}".to_string(),
            "rust".to_string(),
        );
        assert_eq!(s.code_panel_file, Some("main.rs".to_string()));

        s.clear_code_panel();
        assert!(s.code_panel_file.is_none());
        assert!(s.code_panel_content.is_empty());
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
        assert!(s.messages[0].tool_calls.is_some());
        assert_eq!(s.messages[0].tool_calls.as_ref().unwrap().len(), 1);
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
        // Should not be added to User messages
        assert!(s.messages[0].tool_calls.is_none());
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
        assert!(s.last_checkpoint_id.is_none());
        assert!(s.recovery_handler.is_none());
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
        assert_eq!(s.last_checkpoint_id, Some(id));
        assert!(s.recovery_handler.is_some());
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
        assert!(s.last_checkpoint_id.is_none());
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
}
