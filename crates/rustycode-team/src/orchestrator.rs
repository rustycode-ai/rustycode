//! TeamOrchestrator — wires LLM provider calls to the team coordination loop.
//!
//! This is the bridge that makes the team system work end-to-end:
//!
//! ```text
//! Task → TeamOrchestrator
//!         │
//!         ├── Profile → Plan (TaskProfiler + PlanManager)
//!         │
//!         ├── For each plan step:
//!         │     ├── Builder turn: system_prompt + briefing → LLM → BuilderTurn
//!         │     ├── Skeptic turn: system_prompt + briefing → LLM → SkepticTurn
//!         │     ├── Judge turn: cargo check + cargo test (LOCAL, no LLM)
//!         │     └── Feed TurnInput → Coordinator (trust + progress tracking)
//!         │
//!         └── Return OrchestratorOutcome
//! ```
//!
//! **Phase 4: Event-Driven Orchestration** — Agents react proactively to events
//! like code changes, compilation failures, and test failures through a pub/sub
//! event engine.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rustycode_llm::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, MessageRole,
};
use rustycode_llm::tool_executor::{LLMToolExecutor, ParsedToolCall};
use rustycode_protocol::team::*;
use rustycode_protocol::EscalationTarget;
use rustycode_tools::ToolExecutor;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::agent_timeline::{AgentTimeline, TaskStatus};
use super::briefing::BriefingBuilder;
use super::coordinator::{
    BuilderAction, Coordinator, SkepticReview, SkepticVerdict, TurnInput, TurnOutcome,
};
use super::event_engine::EventEngine;
use super::execution_trace::{ExecutionTrace, PatternMiner, TurnTrace};
use super::executor::{
    local_capabilities, parse_architect_turn, parse_turn, ParsedTurn, TeamExecutor,
};
use super::plan_manager::{PlanManager, StepFailureAction};
use super::profiler::TaskProfiler;
use super::prompt_optimization::{self, PromptOptimization};
use rustycode_orchestration::agent_registry::{AgentRegistry, AgentSelection};

// Team Event broadcasting

/// Events emitted by the TeamOrchestrator during execution.
///
/// Subscribe via `TeamOrchestrator::subscribe()` to receive real-time updates
/// for live status display, tmux pane visualization, or TUI integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TeamEvent {
    /// An agent was activated for a turn.
    AgentActivated {
        role: String,
        turn: u32,
        reason: String,
    },
    /// An agent changed state (e.g., Reading → Analyzing).
    AgentStateChanged {
        role: String,
        from_state: String,
        to_state: String,
    },
    /// An agent was deactivated after completing its turn.
    AgentDeactivated {
        role: String,
        turn: u32,
        reason: String,
    },
    /// A plan step completed or failed.
    StepCompleted {
        step: u32,
        success: bool,
        files: Vec<String>,
    },
    /// The entire task finished.
    TaskCompleted {
        success: bool,
        turns: u32,
        files_modified: Vec<String>,
        final_trust: f64,
    },
    /// An insight was recorded by an agent.
    Insight {
        role: String,
        message: String,
    },
    // ========================================================================
    // Phase 4: Event-Driven Orchestration Events
    // ========================================================================
    /// Code was modified (triggers Skeptic review, security scans, etc.).
    CodeChanged {
        files: Vec<String>,
        author: String,
        generation: u32,
    },
    /// Compilation failed (triggers Scalpel or Builder retry).
    CompilationFailed {
        errors: String,
        files: Vec<String>,
        severity: String, // "error" | "warning"
    },
    /// Tests failed (triggers TestDebugger agent if available).
    TestsFailed {
        failed_tests: Vec<String>,
        total_failed: u32,
        error_output: String,
    },
    /// Trust level changed (triggers escalation if exhausted).
    TrustChanged {
        old_value: f64,
        new_value: f64,
        reason: String,
    },
    /// A verification check passed.
    VerificationPassed {
        check_type: String, // "compilation" | "tests" | "security"
        details: String,
    },
    /// A pattern was discovered (saved to vector memory).
    PatternDiscovered {
        pattern: String,
        confidence: f32,
        source: String,
    },
    /// Security issue detected (triggers SecurityAuditor if available).
    SecurityIssueDetected {
        severity: String, // "critical" | "high" | "medium" | "low"
        issue_type: String,
        location: String, // file:line
        description: String,
    },
    /// Architect declaration was set/updated.
    StructuralDeclarationSet {
        modules: Vec<String>,
        interfaces: Vec<String>,
    },
    /// Plan was adapted (trigger, change, reason).
    PlanAdapted {
        trigger: String,
        change: String,
        reason: String,
        step_index: usize,
    },
    /// Specialist agent was created for a task type.
    SpecialistCreated {
        specialist_type: String,
        agent_id: String,
        task: String,
    },
    /// Request for parallel agent execution.
    ParallelExecutionRequested {
        agents: Vec<String>,
        task: String,
    },
    ToolStarted {
        role: String,
        tool_name: String,
        iteration: u32,
    },
    ToolCompleted {
        role: String,
        tool_name: String,
        success: bool,
        output_preview: String,
    },
    ToolLoopIteration {
        role: String,
        iteration: u32,
        tool_count: u32,
    },
    AdvisorGuidance {
        role: String,
        plan: String,
    },
    LLMTextChunk {
        role: String,
        chunk: String,
    },
    LLMThinkingChunk {
        role: String,
        chunk: String,
    },
}

// LLM Client abstraction

/// Events emitted during streaming LLM responses.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Text(String),
    Thinking(String),
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolCallDelta {
        index: usize,
        arguments: String,
    },
    Done,
    Error(String),
}

/// A mockable LLM client for the orchestrator.
#[async_trait]
pub trait TeamLLMClient: Send + Sync {
    async fn complete(&self, messages: Vec<ChatMessage>) -> Result<String>;

    async fn complete_with_tools(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<serde_json::Value>,
    ) -> Result<CompletionResponse> {
        let _ = tools;
        let content = self.complete(messages).await?;
        Ok(CompletionResponse {
            content,
            model: String::new(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    fn complete_stream_with_tools<'a>(
        &'a self,
        messages: Vec<ChatMessage>,
        tools: Vec<serde_json::Value>,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamEvent> + Send + 'a>> {
        let _ = (messages, tools);
        Box::pin(futures::stream::empty())
    }
}

struct RealLLMClient {
    provider: Arc<dyn LLMProvider>,
    model: String,
    max_tokens: u32,
}

#[async_trait]
impl TeamLLMClient for RealLLMClient {
    async fn complete(&self, messages: Vec<ChatMessage>) -> Result<String> {
        let request =
            CompletionRequest::new(&self.model, messages).with_max_tokens(self.max_tokens);
        let response = self
            .provider
            .complete(request)
            .await
            .context("LLM provider call failed")?;
        Ok(response.content)
    }

    async fn complete_with_tools(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<serde_json::Value>,
    ) -> Result<CompletionResponse> {
        let request = CompletionRequest::new(&self.model, messages)
            .with_max_tokens(self.max_tokens)
            .with_tools(tools);
        self.provider
            .complete(request)
            .await
            .context("LLM provider call failed with tools")
    }

    fn complete_stream_with_tools<'a>(
        &'a self,
        messages: Vec<ChatMessage>,
        tools: Vec<serde_json::Value>,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamEvent> + Send + 'a>> {
        let request = CompletionRequest::new(&self.model, messages)
            .with_max_tokens(self.max_tokens)
            .with_tools(tools);

        let provider = self.provider.clone();

        // Return a stream that collects all events from the provider's SSE stream
        // and yields them. We use an async block + unfold pattern.
        let (tx, rx) = futures::channel::mpsc::channel(64);

        tokio::spawn(async move {
            use futures::SinkExt;
            use futures::StreamExt;
            let mut tx = tx;
            match provider.complete_stream(request).await {
                Ok(mut stream) => {
                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(event) => {
                                let local = match event {
                                    rustycode_protocol::stream_event::StreamEvent::TextDelta { content } => {
                                        vec![StreamEvent::Text(content)]
                                    }
                                    rustycode_protocol::stream_event::StreamEvent::ThinkingDelta { content } => {
                                        vec![StreamEvent::Thinking(content)]
                                    }
                                    rustycode_protocol::stream_event::StreamEvent::ToolCallStarted { id, name } => {
                                        vec![StreamEvent::ToolCallStart { index: 0, id, name }]
                                    }
                                    rustycode_protocol::stream_event::StreamEvent::ToolInputDelta { chunk, .. } => {
                                        vec![StreamEvent::ToolCallDelta { index: 0, arguments: chunk }]
                                    }
                                    rustycode_protocol::stream_event::StreamEvent::TurnCompleted { .. } => {
                                        vec![StreamEvent::Done]
                                    }
                                    _ => vec![],
                                };
                                for ev in local {
                                    if tx.send(ev).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                }
            }
        });

        Box::pin(rx)
    }
}

// Configuration

/// Configuration for the orchestrator.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Maximum total turns across all plan steps.
    pub max_total_turns: u32,
    /// Maximum retries per plan step before adapting.
    pub max_retries_per_step: u32,
    /// Maximum plan adaptations before escalating.
    pub max_adaptations: u32,
    /// Maximum tokens for LLM responses.
    pub max_response_tokens: u32,
    /// Whether to use local checks (cargo check/test) for the Judge.
    pub use_local_judge: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_total_turns: 50,
            max_retries_per_step: 3,
            max_adaptations: 5,
            max_response_tokens: 16384,
            use_local_judge: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolLoopConfig {
    pub max_iterations: u32,
    pub max_advisor_tokens: u32,
    pub builder_tools: Vec<String>,
    pub scalpel_tools: Vec<String>,
}

impl Default for ToolLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 30,
            max_advisor_tokens: 1024,
            builder_tools: vec![
                "Read".into(),
                "Write".into(),
                "Bash".into(),
                "Grep".into(),
                "Glob".into(),
            ],
            scalpel_tools: vec!["Read".into(), "Write".into(), "Bash".into()],
        }
    }
}

// Step execution result

/// Result of a single plan step execution.
#[derive(Debug, Clone)]
enum StepResult {
    /// Step completed successfully.
    StepComplete { files: Vec<String> },
    /// Step failed with an error.
    StepFailed { error: String },
    /// Execution should stop.
    Stop(StopReason),
}

// Orchestrator outcome

/// The outcome of a full team orchestration run.
#[derive(Debug, Clone)]
pub struct OrchestratorOutcome {
    /// Whether the task completed successfully.
    pub success: bool,
    /// Files modified during execution.
    pub files_modified: Vec<String>,
    /// Number of turns taken.
    pub turns: u32,
    /// Final builder trust score.
    pub final_trust: f64,
    /// Human-readable message.
    pub message: String,
    /// Agent lifetime timeline (for visualization/debugging).
    pub timeline: Option<AgentTimeline>,
}

// TeamOrchestrator

/// The full team orchestration engine.
///
/// Wires the LLM provider to the team coordination loop, enabling real
/// task execution through the Builder→Skeptic→Judge cycle.
///
/// **Dynamic Agent Generation:** Creates specialist agents on-demand for
/// domain-specific tasks (database, security, testing, performance, API).
///
/// **Phase 4: Event-Driven Orchestration:** Agents react proactively to
/// events like code changes, compilation failures, and test failures.
///
/// **AutoAgent-Inspired:** Execution trace mining for automatic pattern discovery.
pub struct TeamOrchestrator {
    project_root: PathBuf,
    client: Arc<dyn TeamLLMClient>,
    config: OrchestratorConfig,
    /// Broadcast channel for live team events (visualization, status, tmux).
    event_tx: tokio::sync::broadcast::Sender<TeamEvent>,
    /// Registry of built-in and generated specialist agents.
    agent_registry: std::sync::Mutex<AgentRegistry>,
    /// Event engine for proactive agent coordination.
    event_engine: std::sync::Mutex<EventEngine>,
    /// Cooperative cancellation flag — checked each turn.
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    /// Pattern miner for automatic learning from execution traces.
    pattern_miner: std::sync::Mutex<PatternMiner>,
    /// Prompt optimizations derived from mined patterns.
    prompt_optimizations: std::sync::Mutex<Vec<PromptOptimization>>,
    tool_executor: LLMToolExecutor,
    tool_loop_config: ToolLoopConfig,
}

impl TeamOrchestrator {
    pub fn new(
        project_root: impl Into<PathBuf>,
        provider: Arc<dyn LLMProvider>,
        model: String,
    ) -> Self {
        let config = OrchestratorConfig::default();
        let client = Arc::new(RealLLMClient {
            provider,
            model,
            max_tokens: config.max_response_tokens,
        });
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let mut event_engine = EventEngine::new();
        event_engine.register_standard_team();
        let root: PathBuf = project_root.into();
        let registry = std::sync::Arc::new(rustycode_tools::default_registry());
        let context = rustycode_tools::ToolContext::new(root.clone());
        let tool_executor = LLMToolExecutor::with_executor(ToolExecutor::new(registry, context));
        Self {
            project_root: root,
            client,
            config,
            event_tx,
            agent_registry: std::sync::Mutex::new(AgentRegistry::new()),
            event_engine: std::sync::Mutex::new(event_engine),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pattern_miner: std::sync::Mutex::new(PatternMiner::new(0.5, 1)),
            prompt_optimizations: std::sync::Mutex::new(Vec::new()),
            tool_executor,
            tool_loop_config: ToolLoopConfig::default(),
        }
    }

    /// Create a new orchestrator with a custom LLM client (for testing).
    pub fn with_client(
        project_root: impl Into<PathBuf>,
        client: Arc<dyn TeamLLMClient>,
        config: OrchestratorConfig,
    ) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let mut event_engine = EventEngine::new();
        event_engine.register_standard_team();
        let root: PathBuf = project_root.into();
        let registry = std::sync::Arc::new(rustycode_tools::default_registry());
        let context = rustycode_tools::ToolContext::new(root.clone());
        let tool_executor = LLMToolExecutor::with_executor(ToolExecutor::new(registry, context));
        Self {
            project_root: root,
            client,
            config,
            event_tx,
            agent_registry: std::sync::Mutex::new(AgentRegistry::new()),
            event_engine: std::sync::Mutex::new(event_engine),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pattern_miner: std::sync::Mutex::new(PatternMiner::new(0.5, 1)),
            prompt_optimizations: std::sync::Mutex::new(Vec::new()),
            tool_executor,
            tool_loop_config: ToolLoopConfig {
                max_advisor_tokens: 0,
                ..ToolLoopConfig::default()
            },
        }
    }

    /// Subscribe to live team events for visualization.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<TeamEvent> {
        self.event_tx.subscribe()
    }

    /// Request cooperative cancellation of the running team task.
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
        info!("Team task cancellation requested");
    }

    /// Check if cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Get a shared reference to the cancellation flag.
    /// The caller can set this to `true` to request cancellation.
    pub fn cancel_token(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.cancelled.clone()
    }

    /// Emit a team event.
    fn emit(&self, event: TeamEvent) {
        if let Err(e) = self.event_tx.send(event) {
            tracing::debug!("team event broadcast failed (no receivers): {e}");
        }
    }

    /// Emit an event and dispatch to registered listeners.
    /// Returns actions from interested agents.
    pub fn emit_and_dispatch(
        &self,
        event: TeamEvent,
    ) -> Vec<(String, super::event_engine::AgentAction)> {
        self.emit(event.clone());
        let mut engine = match self.event_engine.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // Recover from poisoned mutex rather than propagating panic
                warn!("Event engine mutex poisoned, recovering");
                poisoned.into_inner()
            }
        };
        engine.dispatch(&event)
    }

    /// Get event engine statistics.
    pub fn event_engine_stats(
        &self,
    ) -> std::collections::HashMap<super::event_engine::TeamEventType, u32> {
        let engine = self.event_engine.lock().unwrap_or_else(|e| e.into_inner());
        engine.dispatch_stats().clone()
    }

    /// Record learnings from a completed task and mine patterns from the execution trace.
    ///
    /// Called at the end of each task to capture what worked, what failed,
    /// and any project-specific insights for future reference.
    async fn record_task_learnings_and_patterns(
        &self,
        task: &str,
        success: bool,
        coordinator: &Coordinator,
        timeline: &AgentTimeline,
    ) {
        use crate::team_learnings::{LearningCategory, TeamLearnings};

        // Record to markdown-based team learnings (human-readable)
        let mut learnings = match TeamLearnings::load(&self.project_root) {
            Ok(l) => l,
            Err(e) => {
                warn!("Failed to load team learnings: {}", e);
                return;
            }
        };

        let state = coordinator.state();

        if success {
            // Record the approach that worked
            if let Some(last_approach) = state.previous_approaches.last() {
                learnings.record(
                    LearningCategory::WhatWorked,
                    format!(
                        "Approach '{}' on files {:?} succeeded (trust: {:.2})",
                        last_approach.strategy,
                        last_approach.target_files,
                        state.builder_trust.value
                    ),
                    None,
                );
            }
        } else {
            // Record specific failure signals for future avoidance
            let attempts = coordinator.attempt_log();
            if let Some(last_attempt) = attempts.last() {
                learnings.record(
                    LearningCategory::WhatFailed,
                    format!(
                        "Approach '{}' failed with {:?}: {}",
                        last_attempt.approach, last_attempt.outcome, last_attempt.root_cause
                    ),
                    None,
                );
            } else {
                learnings.record(
                    LearningCategory::WhatFailed,
                    format!("Task '{}' failed with no completed attempts", task),
                    None,
                );
            }
        }

        // Record insights discovered during execution
        for insight in coordinator.insights() {
            learnings.record(LearningCategory::CodebaseQuirk, insight.clone(), None);
        }

        if let Err(e) = learnings.save() {
            warn!("Failed to save team learnings: {}", e);
        }

        // Build execution trace for pattern mining
        let trace = self.build_execution_trace(task, success, coordinator, timeline);

        // Add trace to pattern miner and regenerate prompt optimizations
        {
            let mut miner = self.pattern_miner.lock().unwrap_or_else(|e| e.into_inner());
            miner.add_trace(trace);
            info!("Added execution trace to pattern miner");

            // Log discovered patterns
            let patterns = miner.all_patterns();
            if !patterns.is_empty() {
                info!(
                    "Pattern miner has {} patterns ({} confirmed)",
                    patterns.len(),
                    miner.confirmed_patterns().len()
                );
            }

            // Regenerate prompt optimizations from confirmed patterns
            let confirmed: Vec<_> = miner.confirmed_patterns().into_iter().cloned().collect();
            let optimizations = prompt_optimization::generate_optimizations(&confirmed, 0.5);
            if !optimizations.is_empty() {
                info!(
                    "Regenerated {} prompt optimizations from {} confirmed patterns",
                    optimizations.len(),
                    confirmed.len()
                );
            }
            let mut stored = self
                .prompt_optimizations
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *stored = optimizations;
        }

        // Record to vector memory (semantic search)
        self.record_to_vector_memory(task, success, coordinator);

        info!("Recorded learnings and patterns from task: {}", task);
    }

    /// Record task outcome and discovered patterns to vector memory.
    #[cfg(feature = "vector-memory")]
    fn record_to_vector_memory(&self, task: &str, success: bool, coordinator: &Coordinator) {
        use rustycode_vector_memory::{MemoryMeta, MemoryType, VectorMemory};

        let state = coordinator.state();
        let mut memory = VectorMemory::new(&self.project_root);
        if memory.init().is_ok() {
            let outcome = if success { "SUCCESS" } else { "FAILED" };
            let trust = state.builder_trust.value;
            let turns = state.turn;
            let content = format!(
                "[{}] Task: {} (turns={}, trust={:.2})",
                outcome, task, turns, trust
            );

            // Task traces are short-term (24h TTL)
            let meta = MemoryMeta {
                confidence: if success { 0.8 } else { 0.5 },
                source_task: Some(task.to_string()),
                occurrence_count: 1,
                created_timestamp: None, // Will be set by VectorMemory::add
                ttl_hours: Some(24),     // Short-term: 24 hours
            };

            match memory.add(content, MemoryType::TaskTraces, meta) {
                Ok(id) => info!("Recorded vector memory for task: {} (id={})", task, id),
                Err(e) => warn!("Failed to save vector memory: {}", e),
            }

            // Also store discovered patterns in vector memory (long-term, no TTL)
            let miner = self.pattern_miner.lock().unwrap_or_else(|e| e.into_inner());
            let confirmed = miner.confirmed_patterns();
            for pattern in confirmed.iter() {
                let pattern_content = format!(
                    "[PATTERN] {} (confidence: {:.2}, occurrences: {})",
                    pattern.description, pattern.confidence, pattern.occurrence_count
                );
                let pattern_meta = MemoryMeta {
                    confidence: pattern.confidence,
                    source_task: pattern.source_tasks.first().cloned(),
                    occurrence_count: pattern.occurrence_count,
                    created_timestamp: None,
                    ttl_hours: None, // Long-term: permanent
                };
                if let Err(e) = memory.add(pattern_content, MemoryType::CodePatterns, pattern_meta)
                {
                    warn!("Failed to store code pattern in memory: {}", e);
                }
            }
        }
    }

    /// Stub when vector-memory feature is disabled.
    #[cfg(not(feature = "vector-memory"))]
    fn record_to_vector_memory(&self, _task: &str, _success: bool, _coordinator: &Coordinator) {
        // Vector memory not available; learning is recorded to markdown only.
    }

    /// Build an execution trace from coordinator state and timeline.
    fn build_execution_trace(
        &self,
        task: &str,
        success: bool,
        coordinator: &Coordinator,
        _timeline: &AgentTimeline,
    ) -> ExecutionTrace {
        use crate::execution_trace::{FailureCategory, TaskOutcome as TraceOutcome};

        let outcome = if success {
            TraceOutcome::Success
        } else {
            TraceOutcome::Failed
        };

        let attempt_log = coordinator.attempt_log();

        // Collect all errors for failure classification - pre-allocate
        let all_errors: Vec<String> = attempt_log
            .iter()
            .filter(|a| !a.root_cause.is_empty())
            .map(|a| a.root_cause.clone())
            .collect();

        // Classify failure (Meta-Harness: failure taxonomy)
        let failure_category = if !success {
            Some(FailureCategory::from_errors(
                &all_errors,
                coordinator.state().turn,
                coordinator.state().builder_trust.value,
                50, // default turn budget
            ))
        } else {
            None
        };

        // Build turn traces from coordinator's attempt log - pre-allocate capacity
        let mut turns = Vec::with_capacity(attempt_log.len());
        for (i, attempt) in attempt_log.iter().enumerate() {
            let mut errors = Vec::with_capacity(1);
            if !attempt.root_cause.is_empty() {
                errors.push(attempt.root_cause.clone());
            }
            turns.push(TurnTrace {
                turn_number: (i + 1) as u32,
                agent_role: "Builder".to_string(),
                action: attempt.approach.clone(),
                files_changed: attempt.files_changed.clone(),
                events: vec![],
                verification_passed: matches!(
                    attempt.outcome,
                    rustycode_protocol::team::AttemptOutcome::Success
                ),
                errors,
                tool_calls: vec![],
                trust_delta: None,
            });
        }

        // Get files modified - pre-allocate with estimated capacity
        let files_modified: Vec<String> = attempt_log
            .iter()
            .flat_map(|a| a.files_changed.iter().cloned())
            .collect();

        let root_cause = attempt_log
            .last()
            .filter(|a| !a.root_cause.is_empty())
            .map(|a| a.root_cause.clone());

        ExecutionTrace {
            task: task.to_string(),
            outcome,
            turns,
            files_modified,
            total_turns: coordinator.state().turn,
            final_trust: coordinator.state().builder_trust.value,
            root_cause,
            failure_category,
            discovered_patterns: vec![],
            tool_calls: vec![],
            duration_ms: None,
        }
    }

    /// Execute a task through the full team loop.
    #[tracing::instrument(skip(self, task))]
    pub async fn execute(&self, task: &str) -> Result<OrchestratorOutcome> {
        info!("TeamOrchestrator starting task: {}", task);

        // Create timeline for tracking agent activations
        let mut timeline = AgentTimeline::new(task);

        // 1. Profile the task
        let profiler = TaskProfiler::new();
        let profile = profiler.profile(task);

        // 2. Determine which agent(s) should handle this task
        let agent_selection = {
            let mut registry = self
                .agent_registry
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            registry.agent_for_task(task, &profile)
        };

        // Log agent selection and configure coordinator if specialist
        match &agent_selection {
            AgentSelection::NewSpecialist {
                agent_id,
                specialist_type,
                reason,
            } => {
                info!("Created specialist agent: {} ({})", agent_id, reason);
                self.emit(TeamEvent::AgentActivated {
                    role: format!("{:?}", specialist_type),
                    turn: 0,
                    reason: reason.clone(),
                });
            }
            AgentSelection::Reuse { agent_id, reason } => {
                info!("Reusing specialist agent: {} ({})", agent_id, reason);
            }
            AgentSelection::StandardTeam { reason } => {
                info!("Using standard team: {}", reason);
            }
            #[allow(unreachable_patterns)]
            _ => {}
        }

        // 3. Determine reasoning strategy from task profile
        let strategy = profile.strategy;
        info!("Using reasoning strategy: {:?}", strategy);

        // 4. Create plan
        let session_id = rustycode_protocol::SessionId::new();
        let mut plan_mgr = PlanManager::create_plan(session_id, task, &profile);
        plan_mgr.start_execution();

        // 5. Create coordinator (clone profile since we need it for risk check)
        let mut coordinator = Coordinator::new(self.project_root.clone(), profile.clone());

        // 6. Run Architect phase for high-risk tasks OR when strategy requires planning
        // This is a one-time cost that prevents expensive rework later.
        let should_run_architect = profile.risk == RiskLevel::High
            || profile.risk == RiskLevel::Critical
            || !strategy.skips_architect();

        if should_run_architect {
            info!(
                "Running Architect phase for task (strategy: {:?})",
                strategy
            );
            timeline.activate_agent(
                rustycode_protocol::agent_protocol::AgentRole::Architect,
                "High/Critical risk task or PlanFirst strategy",
            );
            self.emit(TeamEvent::AgentActivated {
                role: "Architect".into(),
                turn: 0,
                reason: "High/Critical risk task or PlanFirst strategy".into(),
            });
            let architect_briefing = self
                .build_briefing_for_role(TeamRole::Architect, &plan_mgr, &coordinator)
                .await?;
            let architect_turn = self.execute_architect(task, &architect_briefing).await?;
            coordinator.set_structural_declaration(architect_turn.declaration);
            timeline.deactivate_agent(
                rustycode_protocol::agent_protocol::AgentRole::Architect,
                "Declaration produced",
            );
            self.emit(TeamEvent::AgentDeactivated {
                role: "Architect".into(),
                turn: 0,
                reason: "Declaration produced".into(),
            });
            info!("Architect phase completed, declaration stored");
        } else {
            info!("Skipping Architect phase (strategy: {:?})", strategy);
        }

        // 6. Execute plan steps
        let mut total_turns = 0u32;
        let mut files_modified = Vec::new();

        while !plan_mgr.is_complete() && total_turns < self.config.max_total_turns {
            // Check for cooperative cancellation
            if self.is_cancelled() {
                info!("Team task cancelled by user after {} turns", total_turns);
                timeline.set_status(TaskStatus::Cancelled);
                self.emit(TeamEvent::TaskCompleted {
                    success: false,
                    turns: total_turns,
                    files_modified: files_modified.clone(),
                    final_trust: coordinator.state().builder_trust.value,
                });
                return Ok(OrchestratorOutcome {
                    success: false,
                    files_modified,
                    turns: total_turns,
                    final_trust: coordinator.state().builder_trust.value,
                    message: "Task cancelled by user".to_string(),
                    timeline: Some(timeline),
                });
            }
            let result = self
                .execute_plan_step(&mut plan_mgr, &mut coordinator, &mut timeline)
                .await?;

            total_turns += 1;
            timeline.next_turn();

            match result {
                StepResult::StepComplete { files } => {
                    for f in &files {
                        if !files_modified.contains(f) {
                            files_modified.push(f.clone());
                        }
                    }
                    plan_mgr.complete_current_step(files);
                    plan_mgr.advance_to_next_step();
                }
                StepResult::StepFailed { error } => {
                    let action = plan_mgr.handle_step_failure(&error);
                    match action {
                        StepFailureAction::Retry {
                            step_index,
                            attempt,
                            max_attempts,
                        } => {
                            debug!(
                                "Retrying step {} (attempt {}/{})",
                                step_index, attempt, max_attempts
                            );
                            continue;
                        }
                        StepFailureAction::Adapt {
                            step_index,
                            adaptation,
                        } => {
                            plan_mgr.adapt_plan(
                                adaptation.trigger,
                                adaptation.change,
                                adaptation.reason,
                            )?;
                            debug!("Adapted plan at step {}", step_index);
                        }
                        StepFailureAction::Escalate(msg) => {
                            timeline.set_status(TaskStatus::Failed);
                            return Ok(OrchestratorOutcome {
                                success: false,
                                files_modified,
                                turns: total_turns,
                                final_trust: coordinator.state().builder_trust.value,
                                message: format!("Escalated: {}", msg),
                                timeline: Some(timeline),
                            });
                        }
                    }
                }
                StepResult::Stop(reason) => {
                    timeline.set_status(TaskStatus::Failed);
                    return Ok(OrchestratorOutcome {
                        success: false,
                        files_modified,
                        turns: total_turns,
                        final_trust: coordinator.state().builder_trust.value,
                        message: format!("Stopped: {:?}", reason),
                        timeline: Some(timeline),
                    });
                }
            }
        }

        let success = plan_mgr.is_complete();
        timeline.set_status(if success {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        });

        self.emit(TeamEvent::TaskCompleted {
            success,
            turns: total_turns,
            files_modified: files_modified.clone(),
            final_trust: coordinator.state().builder_trust.value,
        });

        // Record task outcome with agent registry
        {
            let mut registry = self
                .agent_registry
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match &agent_selection {
                AgentSelection::NewSpecialist { agent_id, .. }
                | AgentSelection::Reuse { agent_id, .. } => {
                    registry.record_task_outcome(task, agent_id, success);
                    info!("Recorded task outcome for agent {}", agent_id);
                }
                AgentSelection::StandardTeam { .. } => {
                    // Standard team doesn't need outcome tracking
                }
                #[allow(unreachable_patterns)]
                _ => {}
            }
        }

        // Record task learnings and mine patterns from execution trace
        self.record_task_learnings_and_patterns(task, success, &coordinator, &timeline)
            .await;

        Ok(OrchestratorOutcome {
            success,
            files_modified,
            turns: total_turns,
            final_trust: coordinator.state().builder_trust.value,
            message: if success {
                "Task completed successfully".to_string()
            } else {
                "Budget exhausted before completion".to_string()
            },
            timeline: Some(timeline),
        })
    }

    /// Execute a single plan step: Builder → Skeptic → Judge.
    async fn execute_plan_step(
        &self,
        plan_mgr: &mut PlanManager,
        coordinator: &mut Coordinator,
        timeline: &mut AgentTimeline,
    ) -> Result<StepResult> {
        use crate::AgentState;
        use rustycode_protocol::agent_protocol::AgentRole;

        let step_context = plan_mgr.step_context_for_role(TeamRole::Builder);

        // --- Builder Turn ---
        timeline.activate_agent(AgentRole::Builder, "Implementing plan step");
        self.emit(TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: timeline.current_turn,
            reason: "Implementing plan step".into(),
        });
        timeline.record_state_change(AgentRole::Builder, AgentState::Idle, AgentState::Reasoning);
        self.emit(TeamEvent::AgentStateChanged {
            role: "Builder".into(),
            from_state: "Idle".into(),
            to_state: "Reasoning".into(),
        });

        let builder_briefing = self
            .build_briefing_for_role(TeamRole::Builder, plan_mgr, coordinator)
            .await?;

        timeline.record_state_change(
            AgentRole::Builder,
            AgentState::Reasoning,
            AgentState::Implementing,
        );
        let builder_response = self
            .call_llm_for_role(TeamRole::Builder, &builder_briefing, &step_context)
            .await?;

        let builder_turn = match parse_turn(&builder_response, TeamRole::Builder) {
            Ok(ParsedTurn::Builder(t)) => t,
            Ok(_) => {
                timeline.deactivate_agent(AgentRole::Builder, "Parse failed");
                return Ok(StepResult::StepFailed {
                    error: "Expected Builder turn, got different role".to_string(),
                });
            }
            Err(e) => {
                timeline.deactivate_agent(AgentRole::Builder, "Parse failed");
                return Ok(StepResult::StepFailed {
                    error: format!("Builder parse failed: {}", e),
                });
            }
        };

        timeline.deactivate_agent(AgentRole::Builder, "Step complete");

        let files_changed: Vec<String> = builder_turn
            .changes
            .iter()
            .map(|c| c.path.clone())
            .collect();

        // Post-check: verify claimed files exist
        let executor = TeamExecutor::new(&self.project_root);
        let post_check = executor.run_post_checks(&files_changed);
        if !post_check.files_missing.is_empty() {
            return Ok(StepResult::StepFailed {
                error: format!(
                    "Builder claimed files that don't exist: {:?}",
                    post_check.files_missing
                ),
            });
        }

        // Emit event for code changes (triggers proactive agent reactions)
        if !files_changed.is_empty() {
            let _actions = self.emit_and_dispatch(TeamEvent::CodeChanged {
                files: files_changed.clone(), // Clone required for event broadcast
                author: "Builder".to_string(),
                generation: coordinator.state().team_config.builder_generation,
            });
            debug!(
                "Dispatched CodeChanged event, {} actions returned",
                _actions.len()
            );
        }

        // Check for Builder-requested escalation
        if let Some(ref escalation) = builder_turn.escalation {
            info!(
                "Builder requested escalation to {:?}: {}",
                escalation.target, escalation.reason
            );
            match escalation.target {
                EscalationTarget::Architect => {
                    // Run Architect phase mid-flight
                    let architect_briefing = self
                        .build_briefing_for_role(TeamRole::Architect, plan_mgr, coordinator)
                        .await?;
                    let architect_turn = self
                        .execute_architect(plan_mgr.task(), &architect_briefing)
                        .await?;
                    coordinator.set_structural_declaration(architect_turn.declaration);
                    debug!("Architect phase completed mid-flight, declaration stored");
                }
                other => {
                    debug!(
                        "Unknown escalation target: {:?}, continuing normally",
                        other
                    );
                }
            }
        }

        // Feed builder action to coordinator - use references where possible
        let builder_action = BuilderAction {
            approach: builder_turn.approach.clone(), // Needs clone: String -> String
            files_changed: files_changed.clone(),    // Clone needed - used multiple times below
            claims_done: builder_turn.done,
        };
        let turn_input = TurnInput {
            builder_action: Some(builder_action),
            skeptic_review: None,
            judge_results: None,
        };
        let outcome = coordinator.process_turn(turn_input);

        match outcome {
            TurnOutcome::Stop(reason) => return Ok(StepResult::Stop(reason)),
            TurnOutcome::Escalate(_) => {
                return Ok(StepResult::Stop(StopReason::TrustExhausted));
            }
            _ => {}
        }

        // --- Skeptic Turn ---
        timeline.activate_agent(AgentRole::Skeptic, "Reviewing builder output");
        self.emit(TeamEvent::AgentActivated {
            role: "Skeptic".into(),
            turn: timeline.current_turn,
            reason: "Reviewing builder output".into(),
        });
        timeline.record_state_change(AgentRole::Skeptic, AgentState::Idle, AgentState::Reviewing);
        self.emit(TeamEvent::AgentStateChanged {
            role: "Skeptic".into(),
            from_state: "Idle".into(),
            to_state: "Reviewing".into(),
        });

        let skeptic_context = plan_mgr.step_context_for_role(TeamRole::Skeptic);
        let skeptic_briefing = self
            .build_briefing_for_role(TeamRole::Skeptic, plan_mgr, coordinator)
            .await?;

        timeline.record_state_change(
            AgentRole::Skeptic,
            AgentState::Reviewing,
            AgentState::Verifying,
        );
        let skeptic_response = self
            .call_llm_for_role_with_declaration(
                TeamRole::Skeptic,
                &skeptic_briefing,
                &skeptic_context,
                coordinator.structural_declaration(),
            )
            .await?;

        let skeptic_turn = match parse_turn(&skeptic_response, TeamRole::Skeptic) {
            Ok(ParsedTurn::Skeptic(t)) => t,
            _ => {
                // Skeptic parse failed — skip review, assume approve
                warn!("Skeptic parse failed; skipping review");
                timeline
                    .deactivate_agent(AgentRole::Skeptic, "Parse failed, defaulting to approve");
                SkepticTurn {
                    verdict: rustycode_protocol::team::SkepticVerdict::Approve,
                    verified: vec![],
                    refuted: vec![],
                    insights: vec![],
                }
            }
        };

        timeline.deactivate_agent(AgentRole::Skeptic, "Review complete");

        // Convert protocol SkepticVerdict to coordinator SkepticVerdict
        let coord_verdict = match skeptic_turn.verdict {
            rustycode_protocol::team::SkepticVerdict::Approve => SkepticVerdict::Approve,
            rustycode_protocol::team::SkepticVerdict::NeedsWork => SkepticVerdict::RevisionNeeded,
            rustycode_protocol::team::SkepticVerdict::Veto => SkepticVerdict::Stop,
            #[allow(unreachable_patterns)]
            _ => SkepticVerdict::RevisionNeeded,
        };

        let skeptic_review = SkepticReview {
            verdict: coord_verdict,
            issues: skeptic_turn
                .refuted
                .iter()
                .map(|r| (r.claim.clone(), r.evidence.clone()))
                .collect(),
            hallucination_detected: matches!(
                skeptic_turn.verdict,
                rustycode_protocol::team::SkepticVerdict::Veto
            ),
        };

        let turn_input = TurnInput {
            builder_action: None,
            skeptic_review: Some(skeptic_review),
            judge_results: None,
        };
        let outcome = coordinator.process_turn(turn_input);

        match outcome {
            TurnOutcome::Vetoed {
                reason,
                evidence: _,
            } => {
                coordinator.record_attempt(AttemptSummary {
                    approach: builder_turn.approach.clone(),
                    files_changed: files_changed.clone(),
                    outcome: AttemptOutcome::Vetoed(reason.clone()),
                    root_cause: reason,
                    builder_generation: coordinator.team_config().builder_generation,
                });
                return Ok(StepResult::StepFailed {
                    error: "Skeptic vetoed the changes".to_string(),
                });
            }
            TurnOutcome::Stop(reason) => return Ok(StepResult::Stop(reason)),
            TurnOutcome::Escalate(_) => {
                return Ok(StepResult::Stop(StopReason::TrustExhausted));
            }
            _ => {
                // Skeptic approved or needs-work
                for insight in &skeptic_turn.insights {
                    coordinator.add_insight(insight.clone());
                }
            }
        }

        // --- Judge Turn (LOCAL — no LLM call) ---
        timeline.activate_agent(AgentRole::Judge, "Verifying with cargo check/test");
        self.emit(TeamEvent::AgentActivated {
            role: "Judge".into(),
            turn: timeline.current_turn,
            reason: "Verifying with cargo check/test".into(),
        });
        timeline.record_state_change(AgentRole::Judge, AgentState::Idle, AgentState::Compiling);
        self.emit(TeamEvent::AgentStateChanged {
            role: "Judge".into(),
            from_state: "Idle".into(),
            to_state: "Compiling".into(),
        });

        if self.config.use_local_judge {
            let (compiles, compile_errors) =
                local_capabilities::check_compilation(&self.project_root);

            if !compiles {
                timeline.record_state_change(
                    AgentRole::Judge,
                    AgentState::Compiling,
                    AgentState::Testing,
                );

                // Emit compilation failed event (triggers Scalpel)
                let _actions = self.emit_and_dispatch(TeamEvent::CompilationFailed {
                    errors: compile_errors.clone(),
                    files: files_changed.clone(),
                    severity: "error".to_string(),
                });
                debug!(
                    "Dispatched CompilationFailed event, {} actions returned",
                    _actions.len()
                );
            } else {
                timeline.record_insight(AgentRole::Judge, "Compilation successful");

                // Emit verification passed event
                let _actions = self.emit_and_dispatch(TeamEvent::VerificationPassed {
                    check_type: "compilation".to_string(),
                    details: "Compilation successful".to_string(),
                });
            }

            let (passed, failed, total, failed_names) =
                local_capabilities::run_tests(&self.project_root);

            if failed > 0 {
                timeline.record_insight(
                    AgentRole::Judge,
                    format!("{} tests failed: {}", failed, failed_names.join(", ")),
                );

                // Emit tests failed event (triggers TestDebugger)
                let _actions = self.emit_and_dispatch(TeamEvent::TestsFailed {
                    failed_tests: failed_names.clone(),
                    total_failed: failed.min(u32::MAX as usize) as u32,
                    error_output: format!("{} tests failed", failed),
                });
                debug!(
                    "Dispatched TestsFailed event, {} actions returned",
                    _actions.len()
                );
            } else if compiles {
                // Emit verification passed event for tests
                let _actions = self.emit_and_dispatch(TeamEvent::VerificationPassed {
                    check_type: "tests".to_string(),
                    details: format!("All {} tests passed", total),
                });
            }

            let verification = VerificationState {
                compiles,
                tests: TestSummary {
                    total,
                    passed,
                    failed,
                    failed_names: failed_names.clone(),
                },
                dirty_files: files_changed.clone(),
            };

            let turn_input = TurnInput {
                builder_action: None,
                skeptic_review: None,
                judge_results: Some(verification),
            };
            let outcome = coordinator.process_turn(turn_input);

            match outcome {
                TurnOutcome::Complete => {
                    timeline.deactivate_agent(AgentRole::Judge, "All checks passed");
                    return Ok(StepResult::StepComplete {
                        files: files_changed,
                    });
                }
                TurnOutcome::Stop(reason) => {
                    timeline.deactivate_agent(AgentRole::Judge, format!("Stopped: {:?}", reason));
                    return Ok(StepResult::Stop(reason));
                }
                TurnOutcome::Escalate(_) => {
                    timeline.deactivate_agent(AgentRole::Judge, "Trust exhausted");
                    return Ok(StepResult::Stop(StopReason::TrustExhausted));
                }
                _ => {
                    // Verification didn't pass
                    timeline.deactivate_agent(AgentRole::Judge, "Verification failed");
                    let error = if !compiles {
                        let first_lines: Vec<&str> = compile_errors.lines().take(3).collect();
                        format!("Compilation failed: {}", first_lines.join("\n"))
                    } else if failed > 0 {
                        format!("{} tests failed: {}", failed, failed_names.join(", "))
                    } else {
                        "Verification did not pass".to_string()
                    };

                    // Try Scalpel for compile errors
                    if !compiles && is_scalpel_appropriate(&compile_errors) {
                        debug!("Attempting Scalpel fix for compilation errors");
                        if let Ok(Some(scalpel_files)) =
                            self.execute_scalpel(&compile_errors, coordinator).await
                        {
                            return Ok(StepResult::StepComplete {
                                files: scalpel_files,
                            });
                        }
                    }

                    coordinator.record_attempt(AttemptSummary {
                        approach: builder_turn.approach.clone(),
                        files_changed: files_changed.clone(),
                        outcome: if !compiles {
                            AttemptOutcome::CompilationError
                        } else {
                            AttemptOutcome::TestFailure
                        },
                        root_cause: error.clone(),
                        builder_generation: coordinator.team_config().builder_generation,
                    });

                    return Ok(StepResult::StepFailed { error });
                }
            }
        }

        // No local judge — just mark step complete based on builder + skeptic
        Ok(StepResult::StepComplete {
            files: files_changed,
        })
    }

    /// Run the Architect phase: read-only analysis producing a StructuralDeclaration.
    ///
    /// This is called once before any Builder turn. The Architect analyzes the task
    /// and codebase structure, then produces a binding StructuralDeclaration that
    /// constrains all subsequent Builder work.
    ///
    /// Returns the ArchitectTurn containing the declaration and rationale.
    #[tracing::instrument(skip(self, task))]
    pub async fn execute_architect(
        &self,
        task: &str,
        briefing: &RoleBriefing,
    ) -> Result<ArchitectTurn> {
        // Build messages using the standard briefing + step context pattern
        let messages = TeamExecutor::build_messages_for_role(briefing, task, &[]);

        debug!("Calling Architect LLM ({} messages)", messages.len());
        let response = self.client.complete(messages).await?;

        parse_architect_turn(&response).context("Failed to parse Architect turn from LLM response")
    }

    /// Run the Scalpel agent for targeted compilation fixes.
    ///
    /// This is called when Judge reports specific, targeted failures
    /// (compile errors, type errors) that don't require redesign.
    ///
    /// Returns Ok(Some(files_changed)) if the Scalpel successfully fixed
    /// the issues and verification passed. Returns Ok(None) if Scalpel
    /// couldn't fix the issues or verification failed.
    #[tracing::instrument(skip(self))]
    pub async fn execute_scalpel(
        &self,
        errors: &str,
        coordinator: &mut Coordinator,
    ) -> Result<Option<Vec<String>>> {
        let scalpel_context = format!("## Compilation Errors\n{}", errors);
        let step_context = &scalpel_context;

        // Build a simple briefing for scalpel with current attempt context
        let last_attempt_files = coordinator
            .attempt_log()
            .last()
            .map(|a| a.files_changed.clone())
            .unwrap_or_default();
        let briefing_text = format!(
            "Fix compilation errors. Files modified: {:?}",
            last_attempt_files
        );

        let system_prompt = TeamExecutor::system_prompt_for_role(TeamRole::Scalpel, step_context);
        let messages = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(briefing_text),
        ];

        debug!("Calling Scalpel LLM for targeted fix");
        let response = self.client.complete(messages).await?;
        let scalpel_turn = match parse_turn(&response, TeamRole::Scalpel) {
            Ok(ParsedTurn::Scalpel(t)) => t,
            _ => return Ok(None),
        };

        let files_changed: Vec<String> = scalpel_turn
            .changes
            .iter()
            .map(|c| c.path.clone())
            .collect();

        // Check files exist
        let executor = TeamExecutor::new(&self.project_root);
        let post_check = executor.run_post_checks(&files_changed);
        if !post_check.files_missing.is_empty() {
            return Ok(None);
        }

        // Verify locally
        let (compiles, _) = local_capabilities::check_compilation(&self.project_root);
        if compiles {
            let (_, failed, _, _) = local_capabilities::run_tests(&self.project_root);
            if failed == 0 {
                return Ok(Some(files_changed));
            }
        }

        Ok(None)
    }

    /// Build a role-filtered briefing.
    async fn build_briefing_for_role(
        &self,
        role: TeamRole,
        plan_mgr: &PlanManager,
        coordinator: &Coordinator,
    ) -> Result<RoleBriefing> {
        let builder = BriefingBuilder::new(&self.project_root);
        let briefing = builder
            .build(
                plan_mgr.task(),
                &[],
                coordinator.attempt_log(),
                coordinator.insights(),
                None, // No verification state tracked in coordinator
            )
            .await
            .context("failed to build briefing")?;

        // Get the task profile from the plan
        let profile = plan_mgr.profile();

        // Inject prompt optimization hints into insights for Builder/Scalpel roles
        let mut insights = briefing.insights.clone();
        if matches!(role, TeamRole::Builder | TeamRole::Scalpel) {
            let opt_guard = self
                .prompt_optimizations
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if !opt_guard.is_empty() {
                let hints = prompt_optimization::format_for_briefing(&opt_guard, plan_mgr.task());
                if !hints.is_empty() {
                    insights.push(hints);
                }
            }
        }

        Ok(RoleBriefing::for_role(
            role,
            &briefing.task,
            &briefing.relevant_code,
            &briefing.attempts,
            &briefing.constraints,
            &insights,
            None, // verification state
            Some(&coordinator.state().builder_trust),
            Some(coordinator.state()),
            &profile,
        ))
    }

    /// Call the LLM for a specific role.
    async fn call_llm_for_role(
        &self,
        role: TeamRole,
        briefing: &RoleBriefing,
        step_context: &str,
    ) -> Result<String> {
        self.call_llm_for_role_with_declaration(role, briefing, step_context, None)
            .await
    }

    async fn call_llm_for_role_with_declaration(
        &self,
        role: TeamRole,
        briefing: &RoleBriefing,
        step_context: &str,
        structural_declaration: Option<&StructuralDeclaration>,
    ) -> Result<String> {
        let messages = TeamExecutor::build_messages_for_role_with_declaration(
            briefing,
            step_context,
            &[],
            structural_declaration,
        );
        debug!(
            "Calling LLM for role: {} ({} messages)",
            role,
            messages.len()
        );

        match role {
            TeamRole::Builder | TeamRole::Scalpel => self.run_tool_loop(role, messages).await,
            _ => self.client.complete(messages).await,
        }
    }

    async fn run_advisor_pass(&self, role: TeamRole, messages: &[ChatMessage]) -> Option<String> {
        let advisor_prompt = format!(
            "You are an expert advisor reviewing the task plan for the {} agent.\n\x4e
             Analyze the context and produce a concise action plan (max 5 steps).\n\x4e
             Focus on: what to read first, what to write, what to verify.\n\x4e
             Be specific about file paths and commands.\n\x4e
             Output ONLY the action plan, nothing else.",
            match role {
                TeamRole::Builder => "Builder (implementation)",
                TeamRole::Scalpel => "Scalpel (targeted fix)",
                _ => "agent",
            }
        );
        let advisor_messages = vec![
            ChatMessage::system(advisor_prompt),
            ChatMessage::user(
                messages
                    .iter()
                    .map(|m| m.text())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        ];
        match self.client.complete(advisor_messages).await {
            Ok(plan) => {
                self.emit(TeamEvent::AdvisorGuidance {
                    role: format!("{:?}", role),
                    plan: plan.clone(),
                });
                Some(plan)
            }
            Err(e) => {
                warn!("Advisor pass failed: {}", e);
                None
            }
        }
    }

    fn validate_tool_call(&self, tc: &ParsedToolCall) -> Option<String> {
        match tc.name.as_str() {
            "Bash" => {
                let raw = tc.arguments.get("command");
                match raw.and_then(|v| v.as_str()) {
                    None => Some(format!(
                        "bash requires a non-null 'command' string. Got: {}",
                        raw.map(|v| v.to_string())
                            .unwrap_or_else(|| "null".to_string())
                    )),
                    Some(s) if s.trim().is_empty() => {
                        Some("bash requires a non-empty 'command' string.".to_string())
                    }
                    _ => None,
                }
            }
            "Write" => {
                let path = tc.arguments.get("path").and_then(|v| v.as_str());
                if path.is_none_or(|s| s.trim().is_empty()) {
                    return Some("write_file requires a non-null 'path' string.".to_string());
                }
                let content = tc.arguments.get("content").and_then(|v| v.as_str());
                if content.is_none() {
                    return Some("write_file requires a 'content' parameter.".to_string());
                }
                None
            }
            "Read" => {
                let path = tc.arguments.get("path").and_then(|v| v.as_str());
                if path.is_none_or(|s| s.trim().is_empty()) {
                    return Some("read_file requires a non-null 'path' string.".to_string());
                }
                None
            }
            "Grep" => {
                let pattern = tc.arguments.get("pattern").and_then(|v| v.as_str());
                if pattern.is_none_or(|s| s.trim().is_empty()) {
                    return Some("grep requires a non-null 'pattern' string.".to_string());
                }
                None
            }
            "Glob" => {
                let pattern = tc.arguments.get("pattern").and_then(|v| v.as_str());
                if pattern.is_none_or(|s| s.trim().is_empty()) {
                    return Some("glob requires a non-null 'pattern' string.".to_string());
                }
                None
            }
            "LspReferences" | "LspHover" => {
                let path = tc.arguments.get("path").and_then(|v| v.as_str());
                if path.is_none_or(|s| s.trim().is_empty()) {
                    return Some(format!("{} requires a non-null 'path' string.", tc.name));
                }
                None
            }
            _ => None,
        }
    }

    async fn run_tool_loop(
        &self,
        role: TeamRole,
        initial_messages: Vec<ChatMessage>,
    ) -> Result<String> {
        let role_name = format!("{:?}", role);
        let tool_names = match role {
            TeamRole::Builder => &self.tool_loop_config.builder_tools,
            TeamRole::Scalpel => &self.tool_loop_config.scalpel_tools,
            _ => &self.tool_loop_config.builder_tools,
        };
        let all_tool_defs = self.tool_executor.openai_tool_definitions();
        let filtered_tools: Vec<serde_json::Value> = all_tool_defs
            .into_iter()
            .filter(|t| {
                t.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .is_some_and(|name| tool_names.iter().any(|tn| tn == name))
            })
            .collect();
        if filtered_tools.is_empty() {
            warn!(
                "No tools available for {} role, falling back to single-shot",
                role_name
            );
            return self.client.complete(initial_messages).await;
        }

        let mut messages = initial_messages;
        if self.tool_loop_config.max_advisor_tokens > 0 {
            if let Some(advice) = self.run_advisor_pass(role, &messages).await {
                messages.push(ChatMessage::system(format!("## Advisor Plan\n{}", advice)));
            }
        }

        for iteration in 0..self.tool_loop_config.max_iterations {
            if self.is_cancelled() {
                return Err(anyhow::anyhow!("Tool loop cancelled"));
            }
            debug!(
                "Tool loop iteration {}/{} for {}",
                iteration + 1,
                self.tool_loop_config.max_iterations,
                role_name
            );

            let content = {
                let response = self
                    .client
                    .complete_with_tools(messages.clone(), filtered_tools.clone())
                    .await
                    .with_context(|| {
                        format!(
                            "Tool loop LLM call failed for {} at iteration {}",
                            role_name, iteration
                        )
                    })?;
                response.content
            };

            debug!(
                "Tool loop {} iter {} response ({} bytes): {:.200}",
                role_name,
                iteration,
                content.len(),
                content
            );

            let tool_calls_openai = self.tool_executor.parse_openai_tool_calls(&content)?;
            let used_openai = !tool_calls_openai.is_empty();
            let tool_calls = if tool_calls_openai.is_empty() {
                self.tool_executor.parse_anthropic_tool_calls(&content)?
            } else {
                tool_calls_openai
            };
            if tool_calls.is_empty() {
                debug!(
                    "No tool calls parsed for {} from {} bytes (has ```tool block: {})",
                    role_name,
                    content.len(),
                    content.contains("```tool")
                );
            } else {
                let names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();
                debug!(
                    "Parsed {} tool calls for {}: {:?} (openai={}, anthropic fallback={})",
                    tool_calls.len(),
                    role_name,
                    names,
                    used_openai,
                    !used_openai
                );
            }
            if tool_calls.is_empty() {
                debug!(
                    "No tool calls in response, returning final content for {}",
                    role_name
                );
                return Ok(content);
            }
            self.emit(TeamEvent::ToolLoopIteration {
                role: role_name.clone(),
                iteration: iteration + 1,
                tool_count: tool_calls.len().min(u32::MAX as usize) as u32,
            });
            messages.push(ChatMessage::assistant(content));

            for tc in &tool_calls {
                // Validate tool call arguments before execution
                if let Some(rejection) = self.validate_tool_call(tc) {
                    warn!("Tool {} rejected (invalid args): {}", tc.name, rejection);
                    let error_msg = ChatMessage {
                        role: MessageRole::Tool(tc.name.clone()),
                        content: rustycode_protocol::MessageContent::simple(
                            serde_json::json!({
                                "tool_call_id": tc.id.clone().unwrap_or_default(),
                                "error": rejection,
                                "hint": "Ensure all required parameters are non-null strings."
                            })
                            .to_string(),
                        ),
                    };
                    messages.push(error_msg);
                    continue;
                }

                self.emit(TeamEvent::ToolStarted {
                    role: role_name.clone(),
                    tool_name: tc.name.clone(),
                    iteration: iteration + 1,
                });
                match self.tool_executor.execute_tool_call(tc).await {
                    Ok(result) => {
                        if result.success {
                            info!(
                                "Tool {} executed: success={}",
                                result.tool_name, result.success
                            );
                        } else {
                            warn!(
                                "Tool {} executed: success={}, error={}",
                                result.tool_name,
                                result.success,
                                result.error.as_deref().unwrap_or("unknown")
                            );
                        }
                        self.emit(TeamEvent::ToolCompleted {
                            role: role_name.clone(),
                            tool_name: result.tool_name.clone(),
                            success: result.success,
                            output_preview: result.output.chars().take(200).collect(),
                        });
                        let tool_msg = self
                            .tool_executor
                            .result_to_openai_message(&result, tc.id.clone());
                        messages.push(tool_msg);
                    }
                    Err(e) => {
                        warn!("Tool {} execution failed: {}", tc.name, e);
                        let error_msg = ChatMessage {
                            role: MessageRole::Tool(tc.name.clone()),
                            content: rustycode_protocol::MessageContent::simple(
                                serde_json::json!({
                                    "tool_call_id": tc.id.clone().unwrap_or_default(),
                                    "error": e.to_string()
                                })
                                .to_string(),
                            ),
                        };
                        messages.push(error_msg);
                    }
                }
            }
        }
        warn!(
            "Tool loop exhausted max iterations ({}) for {}",
            self.tool_loop_config.max_iterations, role_name
        );
        let final_response = self.client.complete(messages).await?;
        Ok(final_response)
    }
}

/// Check if errors are appropriate for the Scalpel (compile/type errors only).
pub fn is_scalpel_appropriate(errors: &str) -> bool {
    let redesign_signals = [
        "logic",
        "approach",
        "redesign",
        "wrong output",
        "wrong result",
    ];
    !errors.lines().any(|line| {
        let lower = line.to_lowercase();
        redesign_signals.iter().any(|s| lower.contains(s))
    })
}

// Mock client for testing

/// A mock LLM client for testing. Returns scripted responses in order.
pub struct MockLLMClient {
    responses: std::sync::Mutex<Vec<String>>,
}

impl MockLLMClient {
    /// Create a new mock client with the given responses.
    ///
    /// Responses are consumed in FIFO order (first in, first out).
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses),
        }
    }
}

#[async_trait]
impl TeamLLMClient for MockLLMClient {
    async fn complete(&self, _messages: Vec<ChatMessage>) -> Result<String> {
        let mut responses = self.responses.lock().unwrap_or_else(|e| e.into_inner());
        match responses.pop() {
            Some(response) => Ok(response),
            None => Err(anyhow::anyhow!("no more mock responses")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orchestrator_config_has_defaults() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.max_total_turns, 50);
        assert_eq!(config.max_retries_per_step, 3);
        assert!(config.use_local_judge);
    }

    #[test]
    fn scalpel_appropriate_for_compile_errors() {
        assert!(is_scalpel_appropriate("error[E0308]: mismatched types"));
        assert!(is_scalpel_appropriate("error[E0425]: cannot find value"));
    }

    #[test]
    fn scalpel_not_appropriate_for_logic_errors() {
        assert!(!is_scalpel_appropriate("wrong output: expected 42 got 0"));
        assert!(!is_scalpel_appropriate("logic error in calculation"));
    }

    #[tokio::test]
    async fn mock_client_returns_scripted_responses() {
        let client = MockLLMClient::new(vec![
            "first response".to_string(),
            "second response".to_string(),
        ]);

        // Responses are popped from the end (LIFO)
        let result1 = client.complete(vec![]).await.unwrap();
        assert_eq!(result1, "second response");

        let result2 = client.complete(vec![]).await.unwrap();
        assert_eq!(result2, "first response");
    }

    #[tokio::test]
    async fn mock_client_exhausts_responses() {
        let client = MockLLMClient::new(vec!["only one".to_string()]);
        let _ = client.complete(vec![]).await;
        let result = client.complete(vec![]).await;
        assert!(result.is_err());
    }

    #[test]
    fn orchestrator_outcome_tracks_progress() {
        let timeline = AgentTimeline::new("test");
        let outcome = OrchestratorOutcome {
            success: true,
            files_modified: vec!["src/main.rs".to_string()],
            turns: 3,
            final_trust: 0.85,
            message: "Task completed successfully".to_string(),
            timeline: Some(timeline),
        };
        assert!(outcome.success);
        assert_eq!(outcome.turns, 3);
    }

    // --- validate_tool_call tests ---

    /// Helper to build a TeamOrchestrator with a no-op provider for unit tests.
    fn make_orchestrator() -> TeamOrchestrator {
        use rustycode_llm::tool_executor::LLMToolExecutor;

        // Create a minimal orchestrator by constructing fields directly.
        // We use the real constructor with a mock-ish provider isn't feasible here,
        // so test validate_tool_call as a standalone method.
        let config = OrchestratorConfig::default();
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let mut event_engine = EventEngine::new();
        event_engine.register_standard_team();
        let root = PathBuf::from("/tmp/rustycode-test");
        let registry = std::sync::Arc::new(rustycode_tools::default_registry());
        let context = rustycode_tools::ToolContext::new(root.clone());
        let tool_executor = LLMToolExecutor::with_executor(ToolExecutor::new(registry, context));

        TeamOrchestrator {
            project_root: root,
            client: Arc::new(MockLLMClient::new(vec![])),
            config,
            event_tx,
            agent_registry: std::sync::Mutex::new(AgentRegistry::new()),
            event_engine: std::sync::Mutex::new(event_engine),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pattern_miner: std::sync::Mutex::new(PatternMiner::new(0.5, 1)),
            prompt_optimizations: std::sync::Mutex::new(Vec::new()),
            tool_executor,
            tool_loop_config: ToolLoopConfig::default(),
        }
    }

    fn tool_call(name: &str, args: serde_json::Value) -> ParsedToolCall {
        ParsedToolCall {
            name: name.to_string(),
            arguments: args,
            id: Some("tc_1".to_string()),
        }
    }

    #[test]
    fn validate_bash_valid() {
        let orch = make_orchestrator();
        let tc = tool_call("Bash", serde_json::json!({"command": "cargo test"}));
        assert!(orch.validate_tool_call(&tc).is_none());
    }

    #[test]
    fn validate_bash_null_command() {
        let orch = make_orchestrator();
        let tc = tool_call("Bash", serde_json::json!({"command": null}));
        let err = orch.validate_tool_call(&tc).unwrap();
        assert!(
            err.contains("non-null"),
            "Expected non-null error, got: {err}"
        );
    }

    #[test]
    fn validate_bash_missing_command() {
        let orch = make_orchestrator();
        let tc = tool_call("Bash", serde_json::json!({}));
        let err = orch.validate_tool_call(&tc).unwrap();
        assert!(
            err.contains("non-null"),
            "Expected non-null error, got: {err}"
        );
    }

    #[test]
    fn validate_bash_empty_command() {
        let orch = make_orchestrator();
        let tc = tool_call("Bash", serde_json::json!({"command": "   "}));
        let err = orch.validate_tool_call(&tc).unwrap();
        assert!(
            err.contains("non-empty"),
            "Expected non-empty error, got: {err}"
        );
    }

    #[test]
    fn validate_write_file_valid() {
        let orch = make_orchestrator();
        let tc = tool_call(
            "Write",
            serde_json::json!({"path": "src/main.rs", "content": "fn main() {}"}),
        );
        assert!(orch.validate_tool_call(&tc).is_none());
    }

    #[test]
    fn validate_write_file_missing_path() {
        let orch = make_orchestrator();
        let tc = tool_call("Write", serde_json::json!({"content": "hello"}));
        let err = orch.validate_tool_call(&tc).unwrap();
        assert!(err.contains("path"), "Expected path error, got: {err}");
    }

    #[test]
    fn validate_write_file_missing_content() {
        let orch = make_orchestrator();
        let tc = tool_call("Write", serde_json::json!({"path": "src/main.rs"}));
        let err = orch.validate_tool_call(&tc).unwrap();
        assert!(
            err.contains("content"),
            "Expected content error, got: {err}"
        );
    }

    #[test]
    fn validate_read_file_valid() {
        let orch = make_orchestrator();
        let tc = tool_call("Read", serde_json::json!({"path": "src/main.rs"}));
        assert!(orch.validate_tool_call(&tc).is_none());
    }

    #[test]
    fn validate_read_file_empty_path() {
        let orch = make_orchestrator();
        let tc = tool_call("Read", serde_json::json!({"path": ""}));
        assert!(orch.validate_tool_call(&tc).is_some());
    }

    #[test]
    fn validate_grep_valid() {
        let orch = make_orchestrator();
        let tc = tool_call(
            "Grep",
            serde_json::json!({"pattern": "fn main", "path": "src/"}),
        );
        assert!(orch.validate_tool_call(&tc).is_none());
    }

    #[test]
    fn validate_grep_missing_pattern() {
        let orch = make_orchestrator();
        let tc = tool_call("Grep", serde_json::json!({"path": "src/"}));
        assert!(orch.validate_tool_call(&tc).is_some());
    }

    #[test]
    fn validate_glob_valid() {
        let orch = make_orchestrator();
        let tc = tool_call("Glob", serde_json::json!({"pattern": "**/*.rs"}));
        assert!(orch.validate_tool_call(&tc).is_none());
    }

    #[test]
    fn validate_glob_empty_pattern() {
        let orch = make_orchestrator();
        let tc = tool_call("Glob", serde_json::json!({"pattern": ""}));
        assert!(orch.validate_tool_call(&tc).is_some());
    }

    #[test]
    fn validate_lsp_references_valid() {
        let orch = make_orchestrator();
        let tc = tool_call(
            "LspReferences",
            serde_json::json!({"path": "src/main.rs", "line": 10, "character": 5}),
        );
        assert!(orch.validate_tool_call(&tc).is_none());
    }

    #[test]
    fn validate_lsp_hover_missing_path() {
        let orch = make_orchestrator();
        let tc = tool_call("LspHover", serde_json::json!({"line": 10, "character": 5}));
        assert!(orch.validate_tool_call(&tc).is_some());
    }

    #[test]
    fn validate_unknown_tool_passes() {
        let orch = make_orchestrator();
        // Unknown tools should pass through without validation
        let tc = tool_call("custom_tool", serde_json::json!({}));
        assert!(orch.validate_tool_call(&tc).is_none());
    }

    #[test]
    fn validate_bash_command_integer_arg() {
        let orch = make_orchestrator();
        // Some LLMs might send non-string args
        let tc = tool_call("Bash", serde_json::json!({"command": 42}));
        let err = orch.validate_tool_call(&tc).unwrap();
        assert!(
            err.contains("non-null"),
            "Integer should be treated as non-string: {err}"
        );
    }
}
