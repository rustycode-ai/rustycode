//! Async channel system for TUI event loop
//!
//! This module provides bounded channels with backpressure handling for async communication
//! between background tasks and the main TUI event loop. It ensures the UI never freezes
//! even under heavy load.
//!
//! ## Architecture
//!
//! - **BoundedChannel**: Fixed-capacity channel with backpressure handling
//! - **Event Types**: StreamChunk, ToolResult, CommandResult, WorkspaceUpdate
//! - **Backpressure Handling**: try_send() and send_with_backpressure()
//! - **Non-blocking Snapshots**: StateSnapshot trait for thread-safe state access
//!
//! ## Example
//!
//! ```rust,ignore
//! use rustycode_tui::app::async_::*;
//!
//! // Create a bounded channel with capacity 100
//! let channel = BoundedChannel::<StreamChunk>::new(100);
//!
//! // Producer: Try to send (non-blocking)
//! match channel.try_send(StreamChunk::Text("Hello".to_string())) {
//!     Ok(_) => println!("Sent"),
//!     Err(e) => println!("Channel full: {:?}", e),
//! }
//!
//! // Consumer: Poll one item per frame
//! if let Some(chunk) = channel.try_recv() {
//!     match chunk {
//!         StreamChunk::Text(text) => println!("Received: {}", text),
//!         StreamChunk::Done => println!("Stream complete"),
//!         _ => {}
//!     }
//! }
//! ```

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
pub struct QuestionOption {
    pub label: String,
    pub description: String,
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
    /// Streaming encountered an error
    Error(String),
    /// The final execution trace from the orchestration pipeline
    ExecutionTrace(serde_json::Value),
    /// A system-level status message (e.g., tool started, phase changed)
    SystemMessage(String),
}

/// Result from tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Tool call identifier
    pub id: String,
    /// Tool name
    pub name: String,
    /// Execution result
    pub result: ToolOutput,
}

/// Tool execution output
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ToolOutput {
    /// Successful execution with output
    Success(String),
    /// Execution failed
    Error(String),
    /// Tool execution timeout
    Timeout,
}

/// Result from bash command execution
#[derive(Debug, Clone)]
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
    /// Create a new bounded channel with specified capacity
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0
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

    /// Get the number of dropped messages (backpressure indicator)
    pub fn dropped_count(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Reset the dropped message counter
    pub fn reset_dropped_count(&self) {
        self.dropped.store(0, Ordering::Relaxed);
    }

    /// Get channel capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clone the sender for use in another thread
    pub fn clone_sender(&self) -> mpsc::SyncSender<T> {
        self.tx.clone()
    }

    /// Take the receiver (can only be called once)
    ///
    /// Returns `None` if the receiver was already taken
    pub fn take_receiver(&mut self) -> Option<mpsc::Receiver<T>> {
        self.rx.take()
    }

    /// Check if receiver has been taken
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
pub struct Snapshot<T> {
    inner: T,
}

impl<T> Snapshot<T>
where
    T: Clone,
{
    /// Create a new snapshot
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Get a reference to the snapshot data
    pub fn get(&self) -> &T {
        &self.inner
    }

    /// Clone the snapshot data
    pub fn clone_data(&self) -> T {
        self.inner.clone()
    }

    /// Convert into the inner data
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

        let error = StreamChunk::Error("Failed".to_string());
        assert_eq!(error, StreamChunk::Error("Failed".to_string()));
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
