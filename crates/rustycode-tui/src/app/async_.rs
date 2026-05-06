//! Bounded channels, event types (StreamChunk, ToolResult, WorkspaceUpdate), and

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

// ── Event Types ─────────────────────────────────────────────────────────────

/// Receive status for non-blocking channel receive operations
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RecvStatus<T> {
    /// Received an item
    Item(T),
    /// No item available (empty)
    Empty,
    /// Channel disconnected (all senders dropped)
    Disconnected,
}

/// Option for a question (multiple choice)
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

/// Structured error type for streaming failures.
///
/// Replaces the previous `StreamChunk::Error(String)` with typed variants
/// so the error handler can match on enum variants instead of fragile
/// string matching (`.contains("401")`, `.contains("Rate limit")`, etc.).
///
/// The `Provider` variant wraps `ProviderError` from `rustycode-llm`,
/// which is already a structured enum with variants for auth, rate limit,
/// network, context length, etc. The TUI-specific variants cover errors
/// that originate inside the TUI layer itself (config, timeouts, orchestration).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StreamError {
    // --- Provider errors (passthrough from rustycode-llm) ---
    /// Error from the LLM provider (auth, rate limit, network, context, etc.)
    Provider(rustycode_llm::provider::ProviderError),

    // --- Config / validation errors ---
    /// No API key configured for the selected provider
    NoApiKey { provider: String },
    /// API key present but invalid format (e.g., too short)
    InvalidApiKey { details: String },

    // --- Stream limit errors ---
    /// Exceeded maximum tool-use turns (infinite loop guard)
    MaxToolTurns { limit: usize },
    /// Stream exceeded maximum wall-clock duration
    StreamDurationExceeded,
    /// No data received from provider for too long
    StreamIdleTimeout { seconds: u64 },

    // --- Orchestration errors ---
    /// Context / token budget exceeded during orchestration
    ContextBudgetExceeded,
    /// An orchestration pipeline step failed
    OrchestrationStepFailed { message: String },

    // --- Infrastructure errors ---
    /// Pipeline task failed with a reason string
    PipelineFailed { reason: String },
    /// Failed to create async runtime (thread resource issue)
    RuntimeError { message: String },
    /// Streaming thread panicked (internal error)
    InternalError { message: String },

    // --- Channel errors ---
    /// Approval channel not available for tool confirmation
    ApprovalChannelUnavailable,
    /// Question channel not available for user prompts
    QuestionChannelUnavailable,
}

impl StreamError {
    /// Whether the error is transient and worth retrying with backoff.
    ///
    /// Non-retryable errors indicate a fundamental problem (bad credentials,
    /// wrong model, exhausted context) that retrying won't fix.
    pub fn is_retryable(&self) -> bool {
        match self {
            // Provider errors: delegate to ProviderError's retryability
            StreamError::Provider(e) => matches!(
                e,
                rustycode_llm::provider::ProviderError::RateLimited { .. }
                    | rustycode_llm::provider::ProviderError::Network(_)
                    | rustycode_llm::provider::ProviderError::Timeout(_)
                    | rustycode_llm::provider::ProviderError::Api(_)
            ),

            // TUI errors that are transient
            StreamError::StreamDurationExceeded
            | StreamError::StreamIdleTimeout { .. }
            | StreamError::PipelineFailed { .. }
            | StreamError::RuntimeError { .. }
            | StreamError::ContextBudgetExceeded
            | StreamError::OrchestrationStepFailed { .. } => true,

            // TUI errors that are NOT transient
            StreamError::NoApiKey { .. }
            | StreamError::InvalidApiKey { .. }
            | StreamError::MaxToolTurns { .. }
            | StreamError::ApprovalChannelUnavailable
            | StreamError::QuestionChannelUnavailable
            | StreamError::InternalError { .. } => false,
        }
    }

    /// Short category label for display in the retry countdown message.
    ///
    /// Returns strings like "Rate limited", "Connection issue", "Auth error",
    /// "Context too long", "Temporary issue" — matching the previous
    /// string-matching logic in `handle_error_chunk`.
    pub fn display_category(&self) -> &'static str {
        match self {
            StreamError::Provider(e) => match e {
                rustycode_llm::provider::ProviderError::RateLimited { .. } => "Rate limited",
                rustycode_llm::provider::ProviderError::Network(_) => "Connection issue",
                rustycode_llm::provider::ProviderError::Auth(_) => "Auth error",
                rustycode_llm::provider::ProviderError::ContextLengthExceeded(_) => {
                    "Context too long"
                }
                rustycode_llm::provider::ProviderError::CreditsExhausted { .. } => {
                    "Credits exhausted"
                }
                rustycode_llm::provider::ProviderError::InvalidModel(_) => "Invalid model",
                rustycode_llm::provider::ProviderError::Timeout(_) => "Connection issue",
                _ => "Temporary issue",
            },
            StreamError::StreamIdleTimeout { .. } | StreamError::StreamDurationExceeded => {
                "Connection issue"
            }
            StreamError::ContextBudgetExceeded => "Context too long",
            StreamError::NoApiKey { .. } | StreamError::InvalidApiKey { .. } => "Auth error",
            _ => "Temporary issue",
        }
    }

    /// Whether this error should cause auto-continue to be disabled.
    ///
    /// Non-retryable errors disable auto-continue to prevent infinite loops.
    pub fn should_disable_auto_continue(&self) -> bool {
        !self.is_retryable()
    }

    /// Whether the queued user message should be preserved for retry.
    ///
    /// For auth/context errors the message is cleared since retrying won't help.
    /// For transient errors the message is kept so the user can retry after backoff.
    pub fn should_preserve_queued_message(&self) -> bool {
        self.is_retryable()
    }
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Provider(e) => write!(f, "{}", e),
            StreamError::NoApiKey { provider } => write!(
                f,
                "No API key configured for provider '{}'. Please set the appropriate environment variable or add it to your config.json.",
                provider
            ),
            StreamError::InvalidApiKey { details } => write!(f, "Invalid API key: {}", details),
            StreamError::MaxToolTurns { limit } => write!(
                f,
                "Reached maximum tool-use turns ({}). Stopping to prevent infinite loop.",
                limit
            ),
            StreamError::StreamDurationExceeded => write!(
                f,
                "Stream exceeded maximum duration (10 minutes). Task may be too complex for a single session."
            ),
            StreamError::StreamIdleTimeout { seconds } => write!(
                f,
                "Stream timed out ({}s without data). The provider may be overloaded.",
                seconds
            ),
            StreamError::ContextBudgetExceeded => write!(
                f,
                "Context limit reached — response may be incomplete"
            ),
            StreamError::OrchestrationStepFailed { message } => {
                write!(f, "Step failed: {}", message)
            }
            StreamError::PipelineFailed { reason } => write!(f, "{}", reason),
            StreamError::RuntimeError { message } => {
                write!(f, "Failed to create async runtime: {}", message)
            }
            StreamError::InternalError { message } => write!(f, "Internal error: {}", message),
            StreamError::ApprovalChannelUnavailable => {
                write!(f, "Error: approval channel not available")
            }
            StreamError::QuestionChannelUnavailable => {
                write!(f, "Error: question channel not available")
            }
        }
    }
}

impl std::error::Error for StreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StreamError::Provider(e) => Some(e),
            _ => None,
        }
    }
}

/// Chunk of streamed LLM response
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StreamChunk {
    /// Text chunk from LLM
    Text(String),
    /// Thinking/reasoning chunk from LLM (displayed separately from response text)
    Thinking(String),
    /// Tool execution started
    ToolStart {
        tool_name: String,
        tool_id: String,
        input_json: String,
    },
    /// Tool execution progress update
    ToolProgress {
        tool_id: Option<String>,
        tool_name: String,
        stage: String,
        elapsed_ms: u64,
        output_preview: Option<String>,
    },
    /// Tool execution completed
    ToolComplete {
        tool_name: String,
        tool_id: String,
        duration_ms: u64,
        success: bool,
        output_size: usize,
        output: Option<String>,
    },
    /// Request user approval for a tool execution
    ApprovalRequest {
        tool_name: String,
        tool_id: String,
        description: String,
        diff: Option<String>,
    },
    /// User approved tool execution
    ApprovalApproved { tool_id: String },
    /// User rejected tool execution
    ApprovalRejected { tool_id: String },
    /// Request user answer to a question (multiple choice or free text)
    QuestionRequest {
        question_id: String,
        question_text: String,
        header: String,
        options: Vec<QuestionOption>,
        multi_select: bool,
    },
    /// User answered a question
    QuestionAnswered { question_id: String, answer: String },
    /// Extract tasks/todos from this text
    ExtractTasks { text: String },
    /// Tasks/todos extracted from response
    TasksExtracted {
        todos_count: usize,
        tasks_count: usize,
    },
    /// File snapshot before a write operation (for /undo)
    FileSnapshot { batch: Vec<(String, String)> },
    /// Token usage from LLM response (input + output + cache tokens for this turn)
    TokenUsage {
        input_tokens: usize,
        output_tokens: usize,
        cache_read_tokens: usize,
        cache_creation_tokens: usize,
    },
    /// Streaming completed successfully
    Done,
    /// Streaming stopped with a non-normal stop reason (e.g., content_filter, SAFETY)
    Stopped { stop_reason: String },
    /// Streaming encountered an error (structured — see [`StreamError`])
    Error(StreamError),
    /// The final execution trace from the orchestration pipeline
    ExecutionTrace(serde_json::Value),
    /// A system-level status message (e.g., tool started, phase changed)
    SystemMessage(String),
    /// Milestone progress update from autonomous sequencing.
    MilestoneProgress {
        milestone_id: String,
        milestone_title: String,
        status: rustycode_protocol::MilestoneStatus,
        plans_total: usize,
        plans_completed: usize,
        current_plan_summary: String,
        action_hint: String,
        plan_rows: Vec<rustycode_orchestration::bus::MilestonePlanProgress>,
    },
    /// Sync LLM todo state into persisted workspace tasks
    TodoSync,
}

/// Result from tool execution
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToolResult {
    /// Tool call identifier
    pub id: String,
    /// Tool name
    pub name: String,
    /// Execution result
    pub result: ToolOutput,
}

impl ToolResult {
    /// Create a new tool result.
    pub fn new(id: impl Into<String>, name: impl Into<String>, result: ToolOutput) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            result,
        }
    }
}

/// Tool execution output
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ToolOutput {
    /// Successful execution with output
    Success(String),
    /// Execution failed with structured error
    Error(ToolExecutionError),
    /// Tool execution timeout
    Timeout,
}

/// Structured error for tool execution failures.
///
/// Replaces `ToolOutput::Error(String)` to enable pattern matching on error
/// categories instead of fragile string comparisons.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ToolExecutionError {
    /// Tool rejected by security policy
    PermissionDenied { tool: String, reason: String },
    /// Tool arguments failed validation
    InvalidInput { tool: String, message: String },
    /// Tool ran but returned a non-zero exit or error response
    ExecutionFailed { tool: String, output: String },
    /// File or resource not found
    NotFound { path: String },
    /// Catch-all for unstructured errors (migration shim)
    Other(String),
}

impl ToolExecutionError {
    /// Create from a plain error string (migration helper)
    pub fn from_message(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    /// The primary error message for display
    pub fn display_message(&self) -> &str {
        match self {
            Self::PermissionDenied { reason, .. } => reason,
            Self::InvalidInput { message, .. } => message,
            Self::ExecutionFailed { output, .. } => output,
            Self::NotFound { path } => path,
            Self::Other(msg) => msg,
        }
    }

    /// The tool name involved, if known
    pub fn tool_name(&self) -> Option<&str> {
        match self {
            Self::PermissionDenied { tool, .. } => Some(tool),
            Self::InvalidInput { tool, .. } => Some(tool),
            Self::ExecutionFailed { tool, .. } => Some(tool),
            Self::NotFound { .. } | Self::Other(_) => None,
        }
    }
}

impl std::fmt::Display for ToolExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionDenied { tool, reason } => {
                write!(f, "{tool}: permission denied — {reason}")
            }
            Self::InvalidInput { tool, message } => write!(f, "{tool}: invalid input — {message}"),
            Self::ExecutionFailed { tool, output } => write!(f, "{tool}: {output}"),
            Self::NotFound { path } => write!(f, "not found: {path}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl From<String> for ToolExecutionError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

/// Result from bash command execution
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CommandResult {
    /// Command that was executed
    pub command: String,
    /// Exit code (None if still running)
    pub exit_code: Option<i32>,
    /// stdout output
    pub stdout: String,
    /// stderr output
    pub stderr: String,
}

impl CommandResult {
    /// Create a new command result.
    pub fn new(
        command: impl Into<String>,
        exit_code: Option<i32>,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
    ) -> Self {
        Self { command: command.into(), exit_code, stdout: stdout.into(), stderr: stderr.into() }
    }
}

/// Workspace context update
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WorkspaceUpdate {
    /// Workspace scan progress
    ScanProgress {
        /// Files scanned so far
        scanned: usize,
        /// Total files to scan
        total: usize,
    },
    /// Workspace scan complete
    ScanComplete {
        /// Total files found
        file_count: usize,
        /// Total directory count
        dir_count: usize,
    },
    /// Workspace context loaded
    ContextLoaded(String),
    /// Workspace notice for the user
    Notice(String),
    /// Workspace scan error
    Error(String),
}

/// Result from slash command execution
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SlashCommandResult {
    /// Command succeeded with message
    Success(String),
    /// Command failed with error
    Error(String),
    /// Session loaded — replace TUI messages with loaded ones
    LoadedSession {
        messages: Vec<crate::ui::message::Message>,
        name: String,
    },
}

// ── Channel Implementation ───────────────────────────────────────────────────

/// Error type for bounded channel operations
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ChannelError {
    /// Channel is full (backpressure)
    Full,
    /// Channel is closed (receiver dropped)
    Closed,
}

/// Bounded channel with fixed capacity and backpressure handling
///
/// This channel prevents memory bloat by dropping messages when full,
/// ensuring the event loop never gets overwhelmed by fast producers.
///
/// ## Backpressure Strategy
///
/// - **try_send()**: Returns immediately with `ChannelError::Full` if channel is full.
/// - **send_with_backpressure()**: Waits with timeout for space to become available.
/// - **Dropped messages**: Tracked in `dropped` counter for monitoring.
///
/// ## Thread Safety
///
/// The channel is thread-safe and can be used from background threads.
/// Use `clone_sender()` to get additional senders.
#[non_exhaustive]
pub struct BoundedChannel<T> {
    /// Channel sender (sync sender for bounded channel)
    tx: mpsc::SyncSender<T>,
    /// Channel receiver (only one receiver supported)
    rx: Option<mpsc::Receiver<T>>,
    /// Channel capacity
    capacity: usize,
    /// Counter for dropped messages
    dropped: Arc<AtomicUsize>,
}

impl<T> BoundedChannel<T>
where
    T: Send + 'static,
{
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Channel capacity must be > 0");
        let (tx, rx) = mpsc::sync_channel(capacity);
        Self {
            tx,
            rx: Some(rx),
            capacity,
            dropped: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Try to send a message without blocking
    ///
    /// Returns `Ok(())` if sent successfully, or `ChannelError::Full` if the channel is full.
    /// This is the preferred method for the event loop to maintain responsiveness.
    pub fn try_send(&self, item: T) -> Result<(), ChannelError> {
        match self.tx.try_send(item) {
            Ok(_) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Err(ChannelError::Full)
            }
            Err(mpsc::TrySendError::Disconnected(_)) => Err(ChannelError::Closed),
        }
    }

    /// Send a message with backpressure (wait with timeout)
    ///
    /// This method will wait up to the specified timeout for space to become available.
    /// Returns `ChannelError::Full` if timeout expires before space is available.
    pub fn send_with_backpressure(&self, item: T, timeout: Duration) -> Result<(), ChannelError> {
        let start = std::time::Instant::now();
        let mut item = Some(item);

        while start.elapsed() < timeout {
            let item_to_send = match item.take() {
                Some(i) => i,
                None => {
                    tracing::error!("send_with_backpressure: item is unexpectedly None");
                    return Err(ChannelError::Closed);
                }
            };

            match self.tx.try_send(item_to_send) {
                Ok(_) => return Ok(()),
                Err(mpsc::TrySendError::Full(owned_item)) => {
                    item = Some(owned_item);
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(mpsc::TrySendError::Disconnected(_)) => return Err(ChannelError::Closed),
            }
        }

        // Timeout expired
        self.dropped.fetch_add(1, Ordering::Relaxed);
        Err(ChannelError::Full)
    }

    /// Try to receive a message without blocking
    ///
    /// Returns `Some(item)` if a message is available, or `None` if the channel is empty.
    /// This is the preferred method for the event loop - call once per frame.
    pub fn try_recv(&mut self) -> Option<T> {
        match &mut self.rx {
            Some(rx) => match rx.try_recv() {
                Ok(item) => Some(item),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => None,
            },
            None => None,
        }
    }

    /// Try to receive a message, reporting if the channel is disconnected.
    ///
    /// Returns `RecvStatus::Item(item)` on success, `RecvStatus::Empty` if no message available,
    /// `RecvStatus::Disconnected` if all senders have been dropped.
    pub fn try_recv_ex(&mut self) -> RecvStatus<T> {
        match &mut self.rx {
            Some(rx) => match rx.try_recv() {
                Ok(item) => RecvStatus::Item(item),
                Err(mpsc::TryRecvError::Empty) => RecvStatus::Empty,
                Err(mpsc::TryRecvError::Disconnected) => RecvStatus::Disconnected,
            },
            None => RecvStatus::Empty,
        }
    }

    pub fn dropped_count(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn reset_dropped_count(&self) {
        self.dropped.store(0, Ordering::Relaxed);
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn clone_sender(&self) -> mpsc::SyncSender<T> {
        self.tx.clone()
    }

    /// Take the receiver (can only be called once)
    ///
    /// Returns `None` if the receiver was already taken
    pub fn take_receiver(&mut self) -> Option<mpsc::Receiver<T>> {
        self.rx.take()
    }

    pub fn has_receiver(&self) -> bool {
        self.rx.is_some()
    }
}

// ── State Snapshot System ─────────────────────────────────────────────────────

/// Trait for non-blocking state snapshots
///
/// This trait allows the event loop to access state without blocking,
/// preventing UI freezes even if state is locked by another thread.
pub trait StateSnapshot: Clone + Send + 'static {
    /// Try to create a snapshot immediately (non-blocking)
    ///
    /// Returns `None` if state is locked, allowing the event loop to continue
    fn try_snapshot(state: &std::sync::Mutex<Self>) -> Option<Self>;
}

/// Blanket implementation for all types that satisfy the constraints
impl<T> StateSnapshot for T
where
    T: Clone + Send + 'static,
{
    fn try_snapshot(state: &std::sync::Mutex<Self>) -> Option<Self> {
        state.try_lock().ok().map(|guard| guard.clone())
    }
}

/// Helper for creating async state snapshots
///
/// This provides a convenient way to capture state snapshots from background
/// threads for use in the event loop.
#[non_exhaustive]
pub struct Snapshot<T> {
    inner: T,
}

impl<T> Snapshot<T>
where
    T: Clone,
{
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    pub fn get(&self) -> &T {
        &self.inner
    }

    pub fn clone_data(&self) -> T {
        self.inner.clone()
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> Clone for Snapshot<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_bounded_channel_basic() {
        let mut channel: BoundedChannel<StreamChunk> = BoundedChannel::new(10);

        // Send and receive
        channel
            .try_send(StreamChunk::Text("Hello".to_string()))
            .unwrap();
        let received = channel.try_recv();
        assert_eq!(received, Some(StreamChunk::Text("Hello".to_string())));
    }

    #[test]
    fn test_bounded_channel_full() {
        let channel: BoundedChannel<StreamChunk> = BoundedChannel::new(2);

        // Fill the channel
        channel
            .try_send(StreamChunk::Text("1".to_string()))
            .unwrap();
        channel
            .try_send(StreamChunk::Text("2".to_string()))
            .unwrap();

        // Channel should be full now
        let result = channel.try_send(StreamChunk::Text("3".to_string()));
        assert_eq!(result, Err(ChannelError::Full));
        assert_eq!(channel.dropped_count(), 1);
    }

    #[test]
    fn test_bounded_channel_backpressure() {
        let channel: BoundedChannel<StreamChunk> = BoundedChannel::new(1);

        // Fill the channel
        channel
            .try_send(StreamChunk::Text("1".to_string()))
            .unwrap();

        // Try to send with backpressure (will timeout)
        let result = channel.send_with_backpressure(
            StreamChunk::Text("2".to_string()),
            Duration::from_millis(10),
        );
        assert_eq!(result, Err(ChannelError::Full));
        assert_eq!(channel.dropped_count(), 1);
    }

    #[test]
    fn test_bounded_channel_clone_sender() {
        let mut channel: BoundedChannel<StreamChunk> = BoundedChannel::new(10);

        // Clone sender for another thread
        let tx = channel.clone_sender();
        let tx2 = channel.clone_sender();

        // Send from cloned senders
        tx.send(StreamChunk::Text("From tx".to_string())).unwrap();
        tx2.send(StreamChunk::Text("From tx2".to_string())).unwrap();

        // Receive in main thread
        assert_eq!(
            channel.try_recv(),
            Some(StreamChunk::Text("From tx".to_string()))
        );
        assert_eq!(
            channel.try_recv(),
            Some(StreamChunk::Text("From tx2".to_string()))
        );
    }

    #[test]
    fn test_bounded_channel_take_receiver() {
        let mut channel: BoundedChannel<StreamChunk> = BoundedChannel::new(10);

        // Send before taking receiver
        channel
            .try_send(StreamChunk::Text("Before".to_string()))
            .unwrap();

        // Take receiver
        let rx = channel.take_receiver().unwrap();
        assert!(!channel.has_receiver());

        // Receive using taken receiver
        let received = rx.try_recv().unwrap();
        assert_eq!(received, StreamChunk::Text("Before".to_string()));

        // Channel's try_recv should return None (no receiver)
        assert_eq!(channel.try_recv(), None);
    }

    #[test]
    fn test_bounded_channel_threaded() {
        let mut channel: BoundedChannel<StreamChunk> = BoundedChannel::new(100);

        // Spawn producer thread
        let tx = channel.clone_sender();
        thread::spawn(move || {
            for i in 0..10 {
                tx.send(StreamChunk::Text(format!("Chunk {}", i))).unwrap();
            }
            tx.send(StreamChunk::Done).unwrap();
        });

        // Consume in main thread
        let mut count = 0;
        loop {
            match channel.try_recv() {
                Some(StreamChunk::Text(_)) => count += 1,
                Some(StreamChunk::Done) => break,
                None => thread::sleep(Duration::from_millis(1)),
                _ => {}
            }
        }

        assert_eq!(count, 10);
    }

    #[test]
    fn test_stream_chunk_types() {
        let text = StreamChunk::Text("Hello".to_string());
        assert_eq!(text, StreamChunk::Text("Hello".to_string()));

        let done = StreamChunk::Done;
        assert_eq!(done, StreamChunk::Done);

        let error = StreamChunk::Error(StreamError::InternalError {
            message: "Failed".to_string(),
        });
        assert_eq!(
            error,
            StreamChunk::Error(StreamError::InternalError {
                message: "Failed".to_string(),
            })
        );
    }

    #[test]
    fn test_tool_result() {
        let result = ToolResult {
            id: "call-1".to_string(),
            name: "read_file".to_string(),
            result: ToolOutput::Success("File contents".to_string()),
        };

        match result.result {
            ToolOutput::Success(output) => assert_eq!(output, "File contents"),
            _ => panic!("Expected Success"),
        }
    }

    #[test]
    fn test_command_result() {
        let result = CommandResult {
            command: "echo hello".to_string(),
            exit_code: Some(0),
            stdout: "hello\n".to_string(),
            stderr: "".to_string(),
        };

        assert_eq!(result.command, "echo hello");
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout, "hello\n");
    }

    #[test]
    fn test_workspace_update() {
        let progress = WorkspaceUpdate::ScanProgress {
            scanned: 50,
            total: 100,
        };
        match progress {
            WorkspaceUpdate::ScanProgress { scanned, total } => {
                assert_eq!(scanned, 50);
                assert_eq!(total, 100);
            }
            _ => panic!("Expected ScanProgress"),
        }

        let complete = WorkspaceUpdate::ScanComplete {
            file_count: 200,
            dir_count: 50,
        };
        match complete {
            WorkspaceUpdate::ScanComplete {
                file_count,
                dir_count,
            } => {
                assert_eq!(file_count, 200);
                assert_eq!(dir_count, 50);
            }
            _ => panic!("Expected ScanComplete"),
        }
    }

    #[test]
    fn test_state_snapshot() {
        use std::sync::Mutex;

        let state = Mutex::new(String::from("Hello"));
        let snapshot = String::try_snapshot(&state);

        assert_eq!(snapshot, Some(String::from("Hello")));

        // Test with locked state
        let _lock = state.lock().unwrap_or_else(|e| e.into_inner());
        let snapshot = String::try_snapshot(&state);
        assert_eq!(snapshot, None);
    }

    #[test]
    fn test_snapshot_wrapper() {
        let snapshot = Snapshot::new(String::from("Test"));
        assert_eq!(snapshot.get(), &"Test".to_string());
        assert_eq!(snapshot.clone_data(), "Test".to_string());
        assert_eq!(snapshot.into_inner(), "Test".to_string());
    }

    #[test]
    fn test_reset_dropped_count() {
        let channel: BoundedChannel<StreamChunk> = BoundedChannel::new(1);

        // Fill and drop
        channel
            .try_send(StreamChunk::Text("1".to_string()))
            .unwrap();
        channel
            .try_send(StreamChunk::Text("2".to_string()))
            .unwrap_err();
        assert_eq!(channel.dropped_count(), 1);

        // Reset
        channel.reset_dropped_count();
        assert_eq!(channel.dropped_count(), 0);
    }

    #[test]
    #[should_panic(expected = "Channel capacity must be > 0")]
    fn test_bounded_channel_zero_capacity() {
        let _channel: BoundedChannel<StreamChunk> = BoundedChannel::new(0);
    }

    #[test]
    fn test_channel_capacity() {
        let channel: BoundedChannel<StreamChunk> = BoundedChannel::new(50);
        assert_eq!(channel.capacity(), 50);
    }
}
