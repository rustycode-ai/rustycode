//! Session recording for replay and debugging.
//!
//! Records all LLM interactions, tool calls, and compaction events as JSONL
//! for session replay and post-mortem analysis.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const CHANNEL_CAPACITY: usize = 1024;

/// Configuration for the rollout recorder.
#[derive(Debug, Clone)]
pub struct RolloutConfig {
    /// Directory to store session files. Defaults to `.rustycode/sessions/`.
    pub sessions_dir: PathBuf,
    /// Whether recording is enabled.
    pub enabled: bool,
    /// Flush to disk on every event (vs buffered).
    pub flush_on_every_event: bool,
}

impl Default for RolloutConfig {
    fn default() -> Self {
        Self {
            sessions_dir: PathBuf::from(".rustycode/sessions"),
            enabled: true,
            flush_on_every_event: false,
        }
    }
}

/// Token usage snapshot for an event.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

/// Events that can be recorded during a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RolloutEvent {
    #[serde(rename = "session_start")]
    SessionStart {
        session_id: String,
        model: String,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "user_message")]
    UserMessage {
        content: String,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "assistant_message")]
    AssistantMessage {
        content: String,
        model: String,
        tokens: TokenUsage,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "tool_call")]
    ToolCall {
        tool_name: String,
        input: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_name: String,
        output: String,
        success: bool,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "compaction")]
    Compaction {
        tokens_before: usize,
        tokens_after: usize,
        strategy: String,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "session_end")]
    SessionEnd {
        reason: String,
        total_tokens: u64,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "op_submitted")]
    OpSubmitted {
        op: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "event_emitted")]
    EventEmitted {
        event: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
}

impl RolloutEvent {
    pub fn timestamp(&self) -> &DateTime<Utc> {
        match self {
            Self::SessionStart { timestamp, .. }
            | Self::UserMessage { timestamp, .. }
            | Self::AssistantMessage { timestamp, .. }
            | Self::ToolCall { timestamp, .. }
            | Self::ToolResult { timestamp, .. }
            | Self::Compaction { timestamp, .. }
            | Self::SessionEnd { timestamp, .. }
            | Self::OpSubmitted { timestamp, .. }
            | Self::EventEmitted { timestamp, .. } => timestamp,
        }
    }
}

/// Records all session events to a JSONL file for replay/inspection.
/// Also fires analytics events to GA4 when configured.
///
/// Uses an async writer via an mpsc channel so the caller never blocks
/// on disk I/O.
#[derive(Debug)]
pub struct RolloutRecorder {
    sender: mpsc::Sender<RolloutEvent>,
    session_id: String,
    enabled: bool,
    analytics: Option<rustycode_observability::AnalyticsClient>,
    analytics_ctx: Option<rustycode_observability::EventContext>,
}

impl RolloutRecorder {
    pub async fn new(session_id: &str, config: &RolloutConfig) -> Result<Self> {
        if !config.enabled {
            return Ok(Self {
                sender: mpsc::channel(1).0,
                session_id: session_id.to_string(),
                enabled: false,
                analytics: None,
                analytics_ctx: None,
            });
        }

        let path = config.sessions_dir.join(format!("{session_id}.jsonl"));
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
        let flush_every = config.flush_on_every_event;

        tokio::spawn(async move {
            Self::writer_loop(file, receiver, flush_every).await;
        });

        Ok(Self {
            sender,
            session_id: session_id.to_string(),
            enabled: true,
            analytics: None,
            analytics_ctx: None,
        })
    }

    /// Create a no-op recorder (recording disabled).
    pub fn disabled(session_id: &str) -> Self {
        Self {
            sender: mpsc::channel(1).0,
            session_id: session_id.to_string(),
            enabled: false,
            analytics: None,
            analytics_ctx: None,
        }
    }

    /// Attach an analytics client for firing GA4 events alongside rollout recording.
    pub fn set_analytics(
        &mut self,
        client: rustycode_observability::AnalyticsClient,
        ctx: rustycode_observability::EventContext,
    ) {
        self.analytics = Some(client);
        self.analytics_ctx = Some(ctx);
    }

    /// Shut down the background writer, flushing any buffered events.
    pub async fn shutdown(&mut self) {
        if !self.enabled {
            return;
        }
        drop(std::mem::replace(&mut self.sender, mpsc::channel(1).0));
    }
}

impl Drop for RolloutRecorder {
    fn drop(&mut self) {
        drop(std::mem::replace(&mut self.sender, mpsc::channel(1).0));
    }
}

impl RolloutRecorder {
    /// Record an event. Returns immediately; actual I/O is async.
    /// Logs a warning if the channel is full and the event is dropped.
    pub fn record(&self, event: RolloutEvent) {
        if !self.enabled {
            return;
        }
        if self.sender.try_send(event).is_err() {
            tracing::warn!(
                session_id = %self.session_id,
                "rollout: event dropped (channel full)"
            );
        }
    }

    /// Convenience: record a session start event.
    pub fn session_start(&self, model: &str) {
        self.record(RolloutEvent::SessionStart {
            session_id: self.session_id.clone(),
            model: model.to_string(),
            timestamp: Utc::now(),
        });
        if let Some(ctx) = &self.analytics_ctx {
            rustycode_observability::AnalyticsClient::send_enriched(
                &self.analytics,
                ctx,
                rustycode_observability::analytics::session_start(),
            );
        }
    }

    /// Convenience: record a user message.
    pub fn user_message(&self, content: &str) {
        self.record(RolloutEvent::UserMessage {
            content: content.to_string(),
            timestamp: Utc::now(),
        });
    }

    /// Convenience: record an assistant message.
    pub fn assistant_message(
        &self,
        content: &str,
        model: &str,
        tokens: TokenUsage,
        duration_ms: u64,
    ) {
        self.record(RolloutEvent::AssistantMessage {
            content: content.to_string(),
            model: model.to_string(),
            tokens,
            duration_ms,
            timestamp: Utc::now(),
        });
        if let Some(ctx) = &self.analytics_ctx {
            rustycode_observability::AnalyticsClient::send_enriched(
                &self.analytics,
                ctx,
                rustycode_observability::analytics::llm_request(model, &ctx.provider, true),
            );
        }
    }

    /// Convenience: record a tool call.
    pub fn tool_call(&self, tool_name: &str, input: serde_json::Value) {
        self.record(RolloutEvent::ToolCall {
            tool_name: tool_name.to_string(),
            input,
            timestamp: Utc::now(),
        });
    }

    /// Convenience: record a tool result.
    pub fn tool_result(&self, tool_name: &str, output: &str, success: bool, duration_ms: u64) {
        self.record(RolloutEvent::ToolResult {
            tool_name: tool_name.to_string(),
            output: output.to_string(),
            success,
            duration_ms,
            timestamp: Utc::now(),
        });
        if let Some(ctx) = &self.analytics_ctx {
            rustycode_observability::AnalyticsClient::send_enriched(
                &self.analytics,
                ctx,
                rustycode_observability::analytics::tool_use(tool_name, success, duration_ms),
            );
        }
    }

    /// Convenience: record a compaction event.
    pub fn compaction(&self, tokens_before: usize, tokens_after: usize, strategy: &str) {
        self.record(RolloutEvent::Compaction {
            tokens_before,
            tokens_after,
            strategy: strategy.to_string(),
            timestamp: Utc::now(),
        });
        if let Some(ctx) = &self.analytics_ctx {
            rustycode_observability::AnalyticsClient::send_enriched(
                &self.analytics,
                ctx,
                rustycode_observability::analytics::compaction(tokens_before, tokens_after),
            );
        }
    }

    /// Convenience: record session end.
    pub fn session_end(&self, reason: &str, total_tokens: u64) {
        self.record(RolloutEvent::SessionEnd {
            reason: reason.to_string(),
            total_tokens,
            timestamp: Utc::now(),
        });
        if let Some(ctx) = &self.analytics_ctx {
            rustycode_observability::AnalyticsClient::send_enriched(
                &self.analytics,
                ctx,
                rustycode_observability::analytics::session_end(0.0, 0, 0, total_tokens),
            );
        }
    }

    /// Record a submitted `Op` command (TUI → Core).
    pub fn op_submitted(&self, op: &rustycode_protocol::Op) {
        let value = serde_json::to_value(op).unwrap_or_else(|e| {
            tracing::debug!("rollout: failed to serialize Op: {e}");
            serde_json::Value::Null
        });
        self.record(RolloutEvent::OpSubmitted {
            op: value,
            timestamp: Utc::now(),
        });
    }

    /// Record an emitted `EventMsg` event (Core → TUI).
    pub fn event_emitted(&self, event: &rustycode_protocol::EventMsg) {
        let value = serde_json::to_value(event).unwrap_or_else(|e| {
            tracing::debug!("rollout: failed to serialize EventMsg: {e}");
            serde_json::Value::Null
        });
        self.record(RolloutEvent::EventEmitted {
            event: value,
            timestamp: Utc::now(),
        });
    }

    /// Fire an LLM error analytics event.
    pub fn llm_error(&self, error_type: &str, status_code: Option<u16>, provider: &str) {
        if let Some(ctx) = &self.analytics_ctx {
            rustycode_observability::AnalyticsClient::send_enriched(
                &self.analytics,
                ctx,
                rustycode_observability::analytics::llm_error(error_type, status_code, provider),
            );
        }
    }

    /// Fire a tool error analytics event.
    pub fn tool_error(&self, tool_name: &str, error_type: &str) {
        if let Some(ctx) = &self.analytics_ctx {
            rustycode_observability::AnalyticsClient::send_enriched(
                &self.analytics,
                ctx,
                rustycode_observability::analytics::tool_error(tool_name, error_type),
            );
        }
    }

    /// Fire an app error analytics event.
    pub fn app_error(&self, error_type: &str, error_message: &str) {
        if let Some(ctx) = &self.analytics_ctx {
            rustycode_observability::AnalyticsClient::send_enriched(
                &self.analytics,
                ctx,
                rustycode_observability::analytics::app_error(error_type, error_message),
            );
        }
    }

    /// Return the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Return whether recording is active.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Spawn a background task that records EventMsg from a broadcast channel.
    /// Returns a JoinHandle for the background task.
    ///
    /// The task consumes EventMsg from the broadcast receiver, converts each to
    /// a RolloutEvent, and writes it to the rollout file. The task exits when
    /// the channel closes or a fatal write error occurs.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let recorder = RolloutRecorder::new("session-123", &config).await?;
    /// let rx = session.subscribe().await?;
    /// let handle = recorder.spawn_recorder(rx);
    /// // Handle is cancelled on drop; or await `handle` for clean shutdown
    /// ```
    pub fn spawn_recorder(
        &self,
        mut rx: tokio::sync::broadcast::Receiver<rustycode_protocol::EventMsg>,
    ) -> JoinHandle<()> {
        let session_id = self.session_id.clone();
        let sender = self.sender.clone();
        let enabled = self.enabled;

        tokio::spawn(async move {
            if !enabled {
                return;
            }
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        if let Some(event) = event_msg_to_rollout_event(msg) {
                            if sender.send(event).await.is_err() {
                                tracing::warn!(
                                    session_id = %session_id,
                                    "rollout: event dropped (channel full or closed)"
                                );
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            session_id = %session_id,
                            count = n,
                            "rollout recorder lagged, skipped events"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!(
                            session_id = %session_id,
                            "rollout recorder: channel closed"
                        );
                        break;
                    }
                }
            }
        })
    }

    /// Background writer loop.
    async fn writer_loop(
        mut file: File,
        mut receiver: mpsc::Receiver<RolloutEvent>,
        flush_every: bool,
    ) {
        while let Some(event) = receiver.recv().await {
            match serde_json::to_string(&event) {
                Ok(line) => {
                    // Append newline-terminated JSON
                    if let Err(e) = file.write_all(line.as_bytes()).await {
                        tracing::warn!("rollout: write failed: {e}");
                    }
                    if let Err(e) = file.write_all(b"\n").await {
                        tracing::warn!("rollout: newline write failed: {e}");
                    }
                    if flush_every {
                        if let Err(e) = file.flush().await {
                            tracing::warn!("rollout: flush failed: {e}");
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("rollout: failed to serialize event: {e}");
                }
            }
        }
        // Flush on channel close (session end)
        if let Err(e) = file.flush().await {
            tracing::warn!("rollout: final flush failed: {e}");
        }
    }
}

/// Convert EventMsg to RolloutEvent for persistence.
///
/// Returns None for events that should not be persisted (e.g., transient
/// progress updates). The goal is to record business-logic events for
/// replay and debugging, not every UI frame.
fn event_msg_to_rollout_event(msg: rustycode_protocol::EventMsg) -> Option<RolloutEvent> {
    match msg {
        // Session lifecycle — always record
        rustycode_protocol::EventMsg::Done => Some(RolloutEvent::SessionEnd {
            reason: "done".to_string(),
            total_tokens: 0,
            timestamp: Utc::now(),
        }),
        rustycode_protocol::EventMsg::Stopped { stop_reason } => Some(RolloutEvent::SessionEnd {
            reason: format!("stopped: {stop_reason}"),
            total_tokens: 0,
            timestamp: Utc::now(),
        }),
        rustycode_protocol::EventMsg::Error {
            kind,
            message,
            retryable: _,
        } => Some(RolloutEvent::SessionEnd {
            reason: format!("error: {:?}: {message}", kind),
            total_tokens: 0,
            timestamp: Utc::now(),
        }),

        // Tool execution — record calls and results
        rustycode_protocol::EventMsg::ToolCallStarted {
            tool_name,
            tool_id: _,
            input,
        } => Some(RolloutEvent::ToolCall {
            tool_name,
            input,
            timestamp: Utc::now(),
        }),
        rustycode_protocol::EventMsg::ToolExecCompleted {
            tool_id: _,
            tool_name,
            success,
            output,
            output_size: _,
            duration_ms,
            ..
        } => Some(RolloutEvent::ToolResult {
            tool_name,
            output,
            success,
            duration_ms,
            timestamp: Utc::now(),
        }),

        // Approval events — record user decisions
        rustycode_protocol::EventMsg::ApprovalApproved { tool_id } => {
            Some(RolloutEvent::EventEmitted {
                event: serde_json::json!({"approval_approved": tool_id}),
                timestamp: Utc::now(),
            })
        }
        rustycode_protocol::EventMsg::ApprovalRejected { tool_id } => {
            Some(RolloutEvent::EventEmitted {
                event: serde_json::json!({"approval_rejected": tool_id}),
                timestamp: Utc::now(),
            })
        }

        // Question/answer events — record user interactions
        rustycode_protocol::EventMsg::QuestionAnswered {
            question_id,
            answer,
        } => Some(RolloutEvent::EventEmitted {
            event: serde_json::json!({"question_answered": {"question_id": question_id, "answer": answer}}),
            timestamp: Utc::now(),
        }),

        // Plan events — record plan lifecycle
        rustycode_protocol::EventMsg::PlanCreated {
            plan_id,
            title,
            steps,
        } => Some(RolloutEvent::EventEmitted {
            event: serde_json::json!({"plan_created": {"plan_id": plan_id, "title": title, "steps": steps}}),
            timestamp: Utc::now(),
        }),
        rustycode_protocol::EventMsg::PlanCompleted {
            plan_id,
            success,
            summary,
        } => Some(RolloutEvent::EventEmitted {
            event: serde_json::json!({"plan_completed": {"plan_id": plan_id, "success": success, "summary": summary}}),
            timestamp: Utc::now(),
        }),

        // Workspace events — record significant state changes
        rustycode_protocol::EventMsg::Workspace(
            rustycode_protocol::WorkspaceEvent::ContextLoaded(s),
        ) => Some(RolloutEvent::EventEmitted {
            event: serde_json::json!({"workspace_context_loaded": s}),
            timestamp: Utc::now(),
        }),
        rustycode_protocol::EventMsg::Workspace(rustycode_protocol::WorkspaceEvent::Error(e)) => {
            Some(RolloutEvent::EventEmitted {
                event: serde_json::json!({"workspace_error": e}),
                timestamp: Utc::now(),
            })
        }

        // Command events — record slash command results
        rustycode_protocol::EventMsg::Command(rustycode_protocol::CommandEvent::Success(msg)) => {
            Some(RolloutEvent::EventEmitted {
                event: serde_json::json!({"command_success": msg}),
                timestamp: Utc::now(),
            })
        }
        rustycode_protocol::EventMsg::Command(rustycode_protocol::CommandEvent::Error(msg)) => {
            Some(RolloutEvent::EventEmitted {
                event: serde_json::json!({"command_error": msg}),
                timestamp: Utc::now(),
            })
        }

        // Milestone progress — record autonomous sequencing updates
        rustycode_protocol::EventMsg::MilestoneProgress(progress) => {
            Some(RolloutEvent::EventEmitted {
                event: serde_json::to_value(progress).unwrap_or(serde_json::Value::Null),
                timestamp: Utc::now(),
            })
        }

        // Token usage — record for cost tracking
        rustycode_protocol::EventMsg::TokenUsage {
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
        } => Some(RolloutEvent::EventEmitted {
            event: serde_json::json!({
                "token_usage": {
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "cache_read_tokens": cache_read_tokens,
                    "cache_creation_tokens": cache_creation_tokens,
                }
            }),
            timestamp: Utc::now(),
        }),

        // Execution trace — record for debugging
        rustycode_protocol::EventMsg::ExecutionTrace(trace) => Some(RolloutEvent::EventEmitted {
            event: trace,
            timestamp: Utc::now(),
        }),

        // Skip transient events — these are high-frequency UI updates
        rustycode_protocol::EventMsg::TextDelta { .. }
        | rustycode_protocol::EventMsg::ThinkingDelta { .. }
        | rustycode_protocol::EventMsg::ThinkingBlockCompleted { .. }
        | rustycode_protocol::EventMsg::TurnStarted { .. }
        | rustycode_protocol::EventMsg::TurnCompleted { .. }
        | rustycode_protocol::EventMsg::ToolInputDelta { .. }
        | rustycode_protocol::EventMsg::ToolExecStarted { .. }
        | rustycode_protocol::EventMsg::ToolExecProgress { .. }
        | rustycode_protocol::EventMsg::FileSnapshot { .. }
        | rustycode_protocol::EventMsg::ApprovalRequired { .. }
        | rustycode_protocol::EventMsg::QuestionRequired { .. }
        | rustycode_protocol::EventMsg::ExtractTasks { .. }
        | rustycode_protocol::EventMsg::TasksExtracted { .. }
        | rustycode_protocol::EventMsg::PlanStepStarted { .. }
        | rustycode_protocol::EventMsg::PlanStepCompleted { .. }
        | rustycode_protocol::EventMsg::PlanApprovalRequested { .. }
        | rustycode_protocol::EventMsg::Workspace(
            rustycode_protocol::WorkspaceEvent::ScanProgress { .. },
        )
        | rustycode_protocol::EventMsg::Workspace(
            rustycode_protocol::WorkspaceEvent::ScanComplete { .. },
        )
        | rustycode_protocol::EventMsg::Workspace(rustycode_protocol::WorkspaceEvent::Notice(_))
        | rustycode_protocol::EventMsg::SystemMessage(_) => None,
        // Catch-all for future EventMsg variants (non-exhaustive enum)
        _ => None,
    }
}

/// Read events from a recorded session JSONL file.
pub async fn read_rollout(path: &Path) -> Result<Vec<RolloutEvent>> {
    let content = tokio::fs::read_to_string(path).await?;
    let mut events = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(line) {
            Ok(event) => events.push(event),
            Err(e) => tracing::debug!("rollout: skipping malformed line: {e}"),
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as StdWrite;

    #[tokio::test]
    async fn test_disabled_recorder_does_nothing() {
        let recorder = RolloutRecorder::disabled("test-session");
        assert!(!recorder.is_enabled());
        // These should be no-ops
        recorder.session_start("test-model");
        recorder.user_message("hello");
        recorder.session_end("done", 0);
    }

    #[tokio::test]
    async fn test_recorder_writes_events() {
        let dir = std::env::temp_dir().join(format!("rollout-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let config = RolloutConfig {
            sessions_dir: dir.clone(),
            enabled: true,
            flush_on_every_event: true,
        };

        let recorder = RolloutRecorder::new("test-session", &config).await.unwrap();

        assert!(recorder.is_enabled());

        recorder.session_start("test-model");
        recorder.user_message("write a hello world");
        recorder.assistant_message(
            "Here is hello world",
            "test-model",
            TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                ..Default::default()
            },
            500,
        );
        recorder.tool_call("Write", serde_json::json!({"path": "hello.rs"}));
        recorder.tool_result("Write", "File written", true, 100);
        recorder.session_end("completed", 30);

        // Drop to close the channel and flush
        drop(recorder);

        // Give the writer task time to finish
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let file_path = dir.join("test-session.jsonl");
        let events = read_rollout(&file_path).await.unwrap();

        assert_eq!(events.len(), 6);

        // Verify ordering and types
        assert!(
            matches!(&events[0], RolloutEvent::SessionStart { session_id, .. } if session_id == "test-session")
        );
        assert!(
            matches!(&events[1], RolloutEvent::UserMessage { content, .. } if content == "write a hello world")
        );
        assert!(matches!(&events[2], RolloutEvent::AssistantMessage { .. }));
        assert!(
            matches!(&events[3], RolloutEvent::ToolCall { tool_name, .. } if tool_name == "Write")
        );
        assert!(matches!(&events[4], RolloutEvent::ToolResult { .. }));
        assert!(
            matches!(&events[5], RolloutEvent::SessionEnd { reason, .. } if reason == "completed")
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_malformed_jsonl() {
        let dir = std::env::temp_dir().join(format!("rollout-malformed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"type":"session_start","session_id":"s1","model":"m","timestamp":"2026-01-01T00:00:00Z"}}"#).unwrap();
        writeln!(f, "not-json").unwrap();
        writeln!(f).unwrap();
        writeln!(f, r#"{{"type":"session_end","reason":"ok","total_tokens":0,"timestamp":"2026-01-01T00:00:00Z"}}"#).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let events = rt.block_on(read_rollout(&path)).unwrap();
        assert_eq!(events.len(), 2); // malformed line skipped

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_op_and_event_recording() {
        let dir = std::env::temp_dir().join(format!("rollout-op-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let config = RolloutConfig {
            sessions_dir: dir.clone(),
            enabled: true,
            flush_on_every_event: true,
        };

        let recorder = RolloutRecorder::new("op-test-session", &config)
            .await
            .unwrap();

        recorder.op_submitted(&rustycode_protocol::Op::StopStream);
        recorder.event_emitted(&rustycode_protocol::EventMsg::Done);

        drop(recorder);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let file_path = dir.join("op-test-session.jsonl");
        let events = read_rollout(&file_path).await.unwrap();

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], RolloutEvent::OpSubmitted { .. }));
        assert!(matches!(&events[1], RolloutEvent::EventEmitted { .. }));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
