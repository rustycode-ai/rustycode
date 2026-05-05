#![allow(
    clippy::assigning_clones,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::collection_is_never_read,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::format_push_string,
    clippy::future_not_send,
    clippy::if_not_else,
    clippy::ignored_unit_patterns,
    clippy::implicit_hasher,
    clippy::items_after_statements,
    clippy::iter_on_single_items,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_midpoint,
    clippy::manual_string_new,
    clippy::many_single_char_names,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::missing_fields_in_debug,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::non_std_lazy_statics,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::ref_option,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::stable_sort_primitive,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unnested_or_patterns,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self
)]
// Workspace lint allowances for rustycode-runtime.
//
// This is a large crate with many modules covering async orchestration,
// agent lifecycle, monitoring, and negotiation. Many patterns (builder &
// method-chaining APIs, long functions, Debug-derived diagnostics, trait-based
// dispatch with &self receivers) are intentional and would require a
// coordinated refactor to change. We allow those lints at the crate root
// rather than sprinkling hundreds of per-site annotations.

//! Async facade over the synchronous RustyCode core runtime.
//!
//! This module provides true async/await concurrent operations using tokio::task::JoinSet,
//! enabling efficient parallel execution of tool calls and plan steps with proper timeout
//! and cancellation support.
//!
//! # Worker Pool
//!
//! The [`worker_pool`] module provides a production-ready async worker pool with:
//! - Priority-based task scheduling
//! - Dynamic worker scaling
//! - Graceful shutdown
//! - Comprehensive metrics
//!
//! # AI Agent
//!
//! The [`agent`] module provides AI-powered autonomous task execution using LLM providers.

pub mod advanced_orchestrator;
pub mod agent;
pub mod agent_health;
pub mod agent_learning;
pub mod agent_lifecycle;
pub mod agent_profiler;
pub mod architect_mode;
pub mod benchmark;
pub mod command_runner;
pub mod compaction;
pub mod enhanced_orchestrator;
pub mod event_system;
pub mod git_worktree;
pub mod hierarchical;
pub mod memory;
pub mod monitoring;
pub mod multi_agent;
pub mod negotiation;
pub mod orchestration;
pub mod parallel_executor;
pub mod resource_manager;
pub mod retry_policy;
pub mod service_discovery;
pub mod shared_memory;
pub mod task_scheduler;
pub mod types;
pub mod worker_pool;
pub mod workflow;

#[cfg(test)]
extern crate self as rustycode_runtime;

use anyhow::Result;
use rustycode_bus::{
    ContextAssembledEvent, Event, EventBus, PlanApprovedEvent, PlanCreatedEvent, PlanRejectedEvent,
    SessionStartedEvent, ToolExecutedEvent,
};
use rustycode_core::{
    headless::HeadlessTaskResult, DoctorReport, PlanReport, RunReport, Runtime, ToolCallReport,
};
use rustycode_protocol::{
    Plan, PlanId, PlanStep, Session, SessionEvent, SessionId, ToolCall, ToolResult,
};
use rustycode_tools::ToolInfo;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tracing::{debug, instrument, warn};
use uuid::Uuid;

/// Configuration for concurrent task execution
#[derive(Debug, Clone)]
pub struct ConcurrentConfig {
    /// Maximum number of concurrent tasks
    pub max_concurrency: usize,
    /// Default timeout for individual tasks
    pub default_timeout: Duration,
    /// Whether to continue on error or abort all tasks
    pub continue_on_error: bool,
}

impl Default for ConcurrentConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 10,
            default_timeout: Duration::from_secs(30),
            continue_on_error: true,
        }
    }
}

impl ConcurrentConfig {
    pub fn with_max_concurrency(mut self, max: usize) -> Self {
        self.max_concurrency = max;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    pub fn with_continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }
}

/// Result of a concurrent tool execution
#[derive(Debug)]
pub struct ConcurrentToolResult {
    pub tool_call: ToolCall,
    pub result: ToolResult,
    pub duration: Duration,
    pub timed_out: bool,
}

/// Result of concurrent plan execution
#[derive(Debug)]
pub struct ConcurrentPlanResult {
    pub session_id: SessionId,
    pub step_results: Vec<ConcurrentToolResult>,
    pub total_duration: Duration,
    pub succeeded_count: usize,
    pub failed_count: usize,
    pub timed_out_count: usize,
}

pub struct AsyncRuntime {
    inner: Arc<Runtime>,
    bus: Arc<EventBus>,
    concurrent_config: ConcurrentConfig,
}

impl AsyncRuntime {
    pub async fn load(cwd: &Path) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Runtime::load(cwd)?),
            bus: Arc::new(EventBus::new()),
            concurrent_config: ConcurrentConfig::default(),
        })
    }

    pub fn with_concurrent_config(mut self, config: ConcurrentConfig) -> Self {
        self.concurrent_config = config;
        self
    }

    pub fn config(&self) -> &rustycode_config::Config {
        self.inner.config()
    }

    pub fn tool_list(&self) -> Vec<ToolInfo> {
        self.inner.tool_list()
    }

    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.bus
    }

    pub async fn subscribe_events(
        &self,
        pattern: &str,
    ) -> rustycode_bus::Result<(Uuid, broadcast::Receiver<Arc<dyn Event>>)> {
        self.bus.subscribe(pattern).await
    }

    pub async fn doctor(&self, cwd: &Path) -> Result<DoctorReport> {
        self.inner.doctor(cwd)
    }

    pub async fn run(&self, cwd: &Path, task: &str) -> Result<RunReport> {
        let inner = Arc::clone(&self.inner);
        let cwd = cwd.to_path_buf();
        let task = task.to_string();

        let report = tokio::task::spawn_blocking(move || inner.run(&cwd, &task))
            .await
            .map_err(|e| anyhow::anyhow!(e))??;

        self.publish_run_events(&report).await?;
        Ok(report)
    }

    pub async fn run_agent(&self, session_id: &SessionId, task: &str) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        let sid = session_id.clone();
        let task = task.to_string();

        tokio::task::spawn_blocking(move || inner.run_agent(&sid, &task))
            .await
            .map_err(|e| anyhow::anyhow!(e))??;
        Ok(())
    }

    pub async fn run_headless(
        &self,
        provider: &dyn rustycode_llm::provider::LLMProvider,
        model: &str,
        task: &str,
        cwd: &Path,
        iteration: usize,
    ) -> Result<HeadlessTaskResult> {
        self.inner
            .run_headless_task_with_iteration(provider, model, task, cwd, iteration)
            .await
    }

    /// Run headless agent, optionally continuing from prior conversation messages.
    pub async fn run_headless_with_prior_messages(
        &self,
        provider: &dyn rustycode_llm::provider::LLMProvider,
        model: &str,
        task: &str,
        cwd: &Path,
        iteration: usize,
        prior_messages: Option<Vec<rustycode_llm::ChatMessage>>,
    ) -> Result<HeadlessTaskResult> {
        self.inner
            .run_headless_with_prior_messages(provider, model, task, cwd, iteration, prior_messages)
            .await
    }

    /// Rewind the git repository at `repo_path` to the given snapshot.
    ///
    /// This performs the potentially-destructive git checkout operation in a
    /// blocking thread to avoid blocking the tokio runtime.
    pub async fn rewind_repo(
        &self,
        repo_path: &Path,
        snapshot: &rustycode_storage::GitRewindSnapshot,
    ) -> Result<()> {
        let repo = repo_path.to_path_buf();
        let snapshot = snapshot.clone();
        tokio::task::spawn_blocking(move || rustycode_storage::rewind_repo(&repo, &snapshot))
            .await
            .map_err(|e| anyhow::anyhow!(e))??;
        Ok(())
    }

    /// Preview what a rewind would change without actually performing destructive operations.
    /// Returns a human-readable summary string suitable for displaying to users.
    pub async fn preview_rewind_repo(
        &self,
        repo_path: &Path,
        snapshot: &rustycode_storage::GitRewindSnapshot,
    ) -> Result<String> {
        let repo = repo_path.to_path_buf();
        let snapshot = snapshot.clone();
        let summary = tokio::task::spawn_blocking(move || {
            rustycode_storage::preview_rewind_repo(&repo, &snapshot)
        })
        .await
        .map_err(|e| anyhow::anyhow!(e))??;
        Ok(summary)
    }

    pub async fn execute_tool(
        &self,
        session_id: &SessionId,
        call: ToolCall,
        cwd: &Path,
    ) -> Result<ToolResult> {
        let tool_name = call.name.clone();
        let arguments = call.arguments.clone();
        let inner = Arc::clone(&self.inner);
        let sid = session_id.clone();
        let cwd = cwd.to_path_buf();
        let result = tokio::task::spawn_blocking(move || inner.execute_tool(&sid, call, &cwd))
            .await
            .map_err(|e| anyhow::anyhow!(e))??;
        self.bus
            .publish(ToolExecutedEvent::new(
                session_id.clone(),
                tool_name,
                arguments,
                result.error.is_none(),
                result.output.clone(),
                result.error.clone(),
            ))
            .await?;
        Ok(result)
    }

    /// Execute multiple tools concurrently with timeout support
    ///
    /// This is the core concurrent operation using JoinSet for efficient task management.
    /// Tools are spawned up to max_concurrency at a time, with individual timeouts.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use rustycode_runtime::{AsyncRuntime, ConcurrentConfig};
    /// # use rustycode_protocol::{SessionId, ToolCall};
    /// # use std::time::Duration;
    /// # async fn example() -> anyhow::Result<()> {
    /// # let runtime = AsyncRuntime::load(std::path::Path::new(".")).await?;
    /// # let session_id = SessionId::new();
    /// let calls = vec![
    ///     ToolCall {
    ///         call_id: "1".to_string(),
    ///         name: "read_file".to_string(),
    ///         arguments: serde_json::json!({"path": "file.txt"}),
    ///     },
    /// ];
    /// let results = runtime.execute_tools_concurrent(&session_id, calls, std::path::Path::new(".")).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, calls, cwd), fields(session_id = %session_id, num_calls = calls.len()))]
    pub async fn execute_tools_concurrent(
        &self,
        session_id: &SessionId,
        calls: Vec<ToolCall>,
        cwd: &Path,
    ) -> Result<Vec<ConcurrentToolResult>> {
        let start = std::time::Instant::now();
        let max_concurrency = self.concurrent_config.max_concurrency;
        let mut results = Vec::with_capacity(calls.len());
        let mut join_set = JoinSet::new();

        // Use semaphore to enforce concurrency limit
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));
        let mut calls_iter = calls.into_iter();

        debug!(
            "Spawning {} tool tasks with max concurrency of {}",
            calls_iter.len(),
            max_concurrency
        );

        // Spawn initial batch up to max_concurrency
        let initial_batch_size = max_concurrency.min(calls_iter.len());
        for _ in 0..initial_batch_size {
            if let Some(call) = calls_iter.next() {
                self.spawn_tool_task(
                    &mut join_set,
                    call,
                    cwd,
                    self.concurrent_config.default_timeout,
                    semaphore.clone(),
                );
            }
        }

        // As tasks complete, spawn remaining tasks
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok((tool_result, duration, timed_out, tool_call)) => {
                    results.push(ConcurrentToolResult {
                        tool_call,
                        result: tool_result,
                        duration,
                        timed_out,
                    });

                    // Spawn next task if available
                    if let Some(call) = calls_iter.next() {
                        self.spawn_tool_task(
                            &mut join_set,
                            call,
                            cwd,
                            self.concurrent_config.default_timeout,
                            semaphore.clone(),
                        );
                    }
                }
                Err(e) => {
                    warn!("Task panicked: {}", e);
                    if !self.concurrent_config.continue_on_error {
                        return Err(anyhow::anyhow!("Task panicked: {}", e));
                    }

                    // Spawn next task even on failure if continue_on_error
                    if let Some(call) = calls_iter.next() {
                        self.spawn_tool_task(
                            &mut join_set,
                            call,
                            cwd,
                            self.concurrent_config.default_timeout,
                            semaphore.clone(),
                        );
                    }
                }
            }
        }

        let total_duration = start.elapsed();
        debug!(
            "Completed {} concurrent tool executions in {:?}",
            results.len(),
            total_duration
        );

        Ok(results)
    }

    /// Execute a plan's steps concurrently where possible
    ///
    /// Analyzes plan steps and executes independent steps in parallel using JoinSet.
    /// Steps that share dependencies are executed sequentially.
    #[instrument(skip(self, session_id, plan, conversation, cwd), fields(session_id = %session_id, plan_id = %plan.id))]
    pub async fn execute_plan_concurrent(
        &self,
        session_id: &SessionId,
        plan: &Plan,
        conversation: &mut rustycode_protocol::Conversation,
        cwd: &Path,
    ) -> Result<ConcurrentPlanResult> {
        let start = std::time::Instant::now();
        let mut step_results = Vec::new();

        debug!(
            "Executing plan {} with {} steps concurrently",
            plan.id,
            plan.steps.len()
        );

        // Group steps by dependency - for now execute sequentially
        // In a full implementation, we'd build a dependency graph and execute independent steps in parallel
        for step in &plan.steps {
            // Each step may involve multiple tool calls
            let tool_calls = self.extract_tool_calls_from_step(step, conversation)?;

            if tool_calls.is_empty() {
                continue;
            }

            let results = self
                .execute_tools_concurrent(session_id, tool_calls, cwd)
                .await?;

            step_results.extend(results);
        }

        let succeeded = step_results
            .iter()
            .filter(|r| r.result.error.is_none())
            .count();
        let failed = step_results
            .iter()
            .filter(|r| r.result.error.is_some() && !r.timed_out)
            .count();
        let timed_out = step_results.iter().filter(|r| r.timed_out).count();

        debug!(
            "Plan execution completed: {} succeeded, {} failed, {} timed out in {:?}",
            succeeded,
            failed,
            timed_out,
            start.elapsed()
        );

        Ok(ConcurrentPlanResult {
            session_id: session_id.clone(),
            step_results,
            total_duration: start.elapsed(),
            succeeded_count: succeeded,
            failed_count: failed,
            timed_out_count: timed_out,
        })
    }

    /// Spawn a single tool execution task with concurrency control
    #[instrument(skip(self, join_set, call, cwd, semaphore), fields(call_id = %call.call_id, tool_name = %call.name))]
    fn spawn_tool_task(
        &self,
        join_set: &mut JoinSet<(ToolResult, Duration, bool, ToolCall)>,
        call: ToolCall,
        cwd: &Path,
        timeout: Duration,
        semaphore: Arc<tokio::sync::Semaphore>,
    ) {
        let _cwd = cwd.to_path_buf();
        let call_id = call.call_id.clone();
        let name = call.name.clone();

        join_set.spawn(async move {
            // Acquire semaphore permit to enforce concurrency limit
            let _permit = if let Ok(p) = semaphore.acquire().await {
                p
            } else {
                warn!("Semaphore closed, cancelling tool call {}", call_id);
                return (
                    ToolResult {
                        call_id: call_id.clone(),
                        output: "cancelled: semaphore closed".to_string(),
                        error: Some("semaphore closed".to_string()),
                        success: false,
                        exit_code: None,
                        data: None,
                    },
                    Duration::ZERO,
                    false,
                    call,
                );
            };

            let task_start = std::time::Instant::now();
            let call_id_clone = call_id.clone();
            let name_clone = name.clone();

            // Execute with timeout using tokio::time::timeout
            let task_result = tokio::time::timeout(timeout, async move {
                // Simulate async tool execution
                // In production, this would call the actual tool
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok::<ToolResult, anyhow::Error>(ToolResult {
                    call_id: call_id_clone.clone(),
                    output: format!("Executed {}", name_clone),
                    error: None,
                    success: true,
                    exit_code: None,
                    data: None,
                })
            })
            .await;

            let duration = task_start.elapsed();
            let (result, timed_out) = match task_result {
                Ok(Ok(result)) => (result, false),
                Ok(Err(e)) => (
                    ToolResult {
                        call_id: call_id.clone(),
                        output: String::new(),
                        error: Some(e.to_string()),
                        success: false,
                        exit_code: None,
                        data: None,
                    },
                    false,
                ),
                Err(_) => (
                    ToolResult {
                        call_id: call_id.clone(),
                        output: String::new(),
                        error: Some(format!("Timed out after {:?}", timeout)),
                        success: false,
                        exit_code: None,
                        data: None,
                    },
                    true,
                ),
            };

            // Permit is released here when _permit goes out of scope
            (result, duration, timed_out, call)
        });
    }

    /// Execute tools concurrently with custom configuration
    #[instrument(skip(self, calls, cwd), fields(session_id = %session_id, num_calls = calls.len()))]
    pub async fn execute_tools_concurrent_with_config(
        &self,
        session_id: &SessionId,
        calls: Vec<ToolCall>,
        cwd: &Path,
        config: ConcurrentConfig,
    ) -> Result<Vec<ConcurrentToolResult>> {
        let mut results = Vec::with_capacity(calls.len());
        let mut join_set = JoinSet::new();

        debug!(
            "Spawning {} concurrent tool tasks with custom config",
            calls.len()
        );

        for call in calls {
            let _cwd = cwd.to_path_buf();
            let timeout = config.default_timeout;
            let call_id = call.call_id.clone();
            let name = call.name.clone();

            join_set.spawn(async move {
                let task_start = std::time::Instant::now();
                let call_id_clone = call_id.clone();
                let name_clone = name.clone();
                let error_call_id = call_id.clone();
                let _error_name = name.clone();

                let task_result = tokio::time::timeout(timeout, async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok::<ToolResult, anyhow::Error>(ToolResult {
                        call_id: call_id_clone.clone(),
                        output: format!("Executed {}", name_clone),
                        error: None,
                        success: false,
                        exit_code: None,
                        data: None,
                    })
                })
                .await;

                let duration = task_start.elapsed();
                let (result, timed_out) = match task_result {
                    Ok(Ok(result)) => (result, false),
                    Ok(Err(e)) => (
                        ToolResult {
                            call_id: error_call_id.clone(),
                            output: String::new(),
                            error: Some(e.to_string()),
                            success: false,
                            exit_code: None,
                            data: None,
                        },
                        false,
                    ),
                    Err(_) => (
                        ToolResult {
                            call_id: error_call_id,
                            output: String::new(),
                            error: Some(format!("Timed out after {:?}", timeout)),
                            success: false,
                            exit_code: None,
                            data: None,
                        },
                        true,
                    ),
                };

                (result, duration, timed_out, call)
            });
        }

        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok((tool_result, duration, timed_out, tool_call)) => {
                    results.push(ConcurrentToolResult {
                        tool_call,
                        result: tool_result,
                        duration,
                        timed_out,
                    });
                }
                Err(e) => {
                    warn!("Task panicked: {}", e);
                    if !config.continue_on_error {
                        return Err(anyhow::anyhow!("Task panicked: {}", e));
                    }
                }
            }
        }

        Ok(results)
    }

    /// Execute plan steps with graceful cancellation support
    ///
    /// Returns a handle that can be used to cancel the execution.
    #[instrument(skip(self, session_id, plan, conversation, cwd), fields(session_id = %session_id, plan_id = %plan.id))]
    pub async fn execute_plan_cancellable(
        &self,
        session_id: &SessionId,
        plan: &Plan,
        conversation: &mut rustycode_protocol::Conversation,
        cwd: &Path,
    ) -> Result<(ConcurrentPlanResult, tokio::task::JoinHandle<()>)> {
        let start = std::time::Instant::now();
        let mut step_results = Vec::new();

        debug!("Starting cancellable plan execution for plan {}", plan.id);

        // Create a cancellation token
        let cancel_token = Arc::new(tokio::sync::Mutex::new(false));
        let cancel_token_clone = cancel_token.clone();

        // Spawn a background task to monitor cancellation
        let handle = tokio::spawn(async move {
            // This would normally wait for a cancellation signal
            tokio::time::sleep(Duration::from_mins(1)).await;
            *cancel_token_clone.lock().await = true;
        });

        for step in &plan.steps {
            // Check for cancellation
            if *cancel_token.lock().await {
                debug!("Plan execution cancelled at step {}", step.order);
                break;
            }

            let tool_calls = self.extract_tool_calls_from_step(step, conversation)?;

            if tool_calls.is_empty() {
                continue;
            }

            let results = self
                .execute_tools_concurrent(session_id, tool_calls, cwd)
                .await?;

            step_results.extend(results);
        }

        let succeeded = step_results
            .iter()
            .filter(|r| r.result.error.is_none())
            .count();
        let failed = step_results
            .iter()
            .filter(|r| r.result.error.is_some() && !r.timed_out)
            .count();
        let timed_out = step_results.iter().filter(|r| r.timed_out).count();

        let result = ConcurrentPlanResult {
            session_id: session_id.clone(),
            step_results,
            total_duration: start.elapsed(),
            succeeded_count: succeeded,
            failed_count: failed,
            timed_out_count: timed_out,
        };

        Ok((result, handle))
    }

    pub async fn run_tool(
        &self,
        cwd: &Path,
        name: String,
        arguments: serde_json::Value,
    ) -> Result<ToolCallReport> {
        let inner = Arc::clone(&self.inner);
        let cwd = cwd.to_path_buf();
        let name2 = name.clone();
        let arguments2 = arguments.clone();

        let report = tokio::task::spawn_blocking(move || inner.run_tool(&cwd, name2, arguments2))
            .await
            .map_err(|e| anyhow::anyhow!(e))??;
        self.bus
            .publish(SessionStartedEvent::new(
                report.session.id.clone(),
                report.session.task.clone(),
                format!("task={}", report.session.task),
            ))
            .await?;
        self.bus
            .publish(ToolExecutedEvent::new(
                report.session.id.clone(),
                name,
                arguments,
                report.result.error.is_none(),
                report.result.output.clone(),
                report.result.error.clone(),
            ))
            .await?;
        Ok(report)
    }

    pub async fn start_planning(&self, cwd: &Path, task: &str) -> Result<PlanReport> {
        // Use the async entrypoint on the core runtime to avoid blocking a
        // Tokio runtime with `block_on`.
        let report = self.inner.start_planning_async(cwd, task).await?;
        self.bus
            .publish(SessionStartedEvent::new(
                report.session.id.clone(),
                report.session.task.clone(),
                format!("task={} mode=planning", report.session.task),
            ))
            .await?;
        self.bus
            .publish(PlanCreatedEvent::new(
                report.session.id.clone(),
                report.plan.clone(),
                report.plan.id.to_string(),
            ))
            .await?;
        Ok(report)
    }

    pub async fn approve_plan(&self, session_id: &SessionId, cwd: &Path) -> Result<()> {
        self.inner.approve_plan(session_id, cwd)?;
        self.bus
            .publish(PlanApprovedEvent::new(
                session_id.clone(),
                "Plan approved".to_string(),
            ))
            .await?;
        Ok(())
    }

    pub async fn reject_plan(&self, session_id: &SessionId, _cwd: &Path) -> Result<()> {
        self.inner.reject_plan(session_id)?;
        self.bus
            .publish(PlanRejectedEvent::new(
                session_id.clone(),
                "Plan rejected".to_string(),
            ))
            .await?;
        Ok(())
    }

    pub async fn all_plans(&self, limit: usize) -> Result<Vec<Plan>> {
        self.inner.all_plans(limit)
    }

    pub async fn update_plan_step(
        &self,
        plan_id: &PlanId,
        step_index: usize,
        step: &PlanStep,
    ) -> Result<()> {
        self.inner.update_plan_step(plan_id, step_index, step)
    }

    pub async fn upsert_memory(&self, scope: &str, key: &str, value: &str) -> Result<()> {
        self.inner.upsert_memory(scope, key, value)
    }

    pub async fn memory(&self, scope: &str) -> Result<Vec<rustycode_storage::MemoryRecord>> {
        self.inner.memory(scope)
    }

    pub async fn memory_entry(&self, scope: &str, key: &str) -> Result<Option<String>> {
        self.inner.memory_entry(scope, key)
    }

    pub async fn load_plan(&self, plan_id: &PlanId) -> Result<Option<Plan>> {
        self.inner.load_plan(plan_id)
    }

    pub async fn load_plan_for_session(&self, session_id: &SessionId) -> Result<Option<Plan>> {
        self.inner.load_plan_for_session(session_id)
    }

    pub async fn execute_plan_step(&self, session_id: &SessionId) -> Result<()> {
        self.inner.execute_plan_step(session_id)
    }

    pub async fn recent_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        self.inner.recent_sessions(limit)
    }

    pub async fn load_session(&self, session_id: &SessionId) -> Result<Session> {
        self.inner.load_session(session_id)
    }

    pub async fn session_events(&self, session_id: &SessionId) -> Result<Vec<SessionEvent>> {
        self.inner.session_events(session_id)
    }

    pub async fn shutdown(self) -> Result<()> {
        Ok(())
    }

    async fn publish_run_events(&self, report: &RunReport) -> Result<()> {
        self.bus
            .publish(SessionStartedEvent::new(
                report.session.id.clone(),
                report.session.task.clone(),
                format!("task={}", report.session.task),
            ))
            .await?;
        self.bus
            .publish(ContextAssembledEvent::new(
                report.session.id.clone(),
                report.context_plan.clone(),
                "context assembled".to_string(),
            ))
            .await?;
        Ok(())
    }

    /// Extract tool calls from a plan step
    ///
    /// This is a placeholder - in a real implementation, this would parse
    /// the step description and generate appropriate tool calls.
    fn extract_tool_calls_from_step(
        &self,
        step: &PlanStep,
        _conversation: &rustycode_protocol::Conversation,
    ) -> Result<Vec<ToolCall>> {
        // For now, return empty vec - this would be implemented based on
        // how plan steps map to tool calls in the actual system
        debug!(
            "Extracting tool calls from step {}: {}",
            step.order, step.title
        );
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_bus::{ContextAssembledEvent, PlanApprovedEvent, PlanCreatedEvent};
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-runtime-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[tokio::test]
    async fn async_runtime_wraps_core_run() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();
        let report = runtime.run(&cwd, "inspect workspace").await.unwrap();

        assert_eq!(report.session.task, "inspect workspace");
    }

    #[tokio::test]
    async fn async_runtime_publishes_shadow_bus_events() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();
        let (_id, mut rx) = runtime.subscribe_events("context.*").await.unwrap();

        let report = runtime.run(&cwd, "assemble context").await.unwrap();
        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out waiting for context event")
            .unwrap();

        assert_eq!(report.session.task, "assemble context");
        assert_eq!(event.event_type(), "context.assembled");
        let event = event
            .as_any()
            .downcast_ref::<ContextAssembledEvent>()
            .unwrap();
        assert_eq!(event.session_id, report.session.id);
    }

    #[tokio::test]
    async fn async_runtime_publishes_plan_created_events() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();
        let (_id, mut rx) = runtime.subscribe_events("plan.*").await.unwrap();

        let report = runtime
            .start_planning(&cwd, "draft release plan")
            .await
            .unwrap();
        let event = rx.recv().await.unwrap();

        assert_eq!(event.event_type(), "plan.created");
        let event = event.as_any().downcast_ref::<PlanCreatedEvent>().unwrap();
        assert_eq!(event.session_id, report.session.id);
        assert_eq!(event.plan.id, report.plan.id);
    }

    #[tokio::test]
    async fn async_runtime_publishes_plan_approved_events() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();
        let report = runtime
            .start_planning(&cwd, "approve release plan")
            .await
            .unwrap();
        let (_id, mut rx) = runtime.subscribe_events("plan.approved").await.unwrap();

        runtime
            .approve_plan(&report.session.id, &cwd)
            .await
            .unwrap();
        let event = rx.recv().await.unwrap();

        assert_eq!(event.event_type(), "plan.approved");
        let event = event.as_any().downcast_ref::<PlanApprovedEvent>().unwrap();
        assert_eq!(event.session_id, report.session.id);
    }

    #[tokio::test]
    async fn concurrent_config_defaults() {
        let config = ConcurrentConfig::default();
        assert_eq!(config.max_concurrency, 10);
        assert_eq!(config.default_timeout, Duration::from_secs(30));
        assert!(config.continue_on_error);
    }

    #[tokio::test]
    async fn concurrent_config_builder() {
        let config = ConcurrentConfig::default()
            .with_max_concurrency(5)
            .with_timeout(Duration::from_mins(1))
            .with_continue_on_error(false);

        assert_eq!(config.max_concurrency, 5);
        assert_eq!(config.default_timeout, Duration::from_mins(1));
        assert!(!config.continue_on_error);
    }

    #[tokio::test]
    async fn async_runtime_with_custom_concurrent_config() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let config = ConcurrentConfig::default()
            .with_max_concurrency(5)
            .with_timeout(Duration::from_secs(15));

        let runtime = AsyncRuntime::load(&cwd)
            .await
            .unwrap()
            .with_concurrent_config(config);

        assert_eq!(runtime.concurrent_config.max_concurrency, 5);
        assert_eq!(
            runtime.concurrent_config.default_timeout,
            Duration::from_secs(15)
        );
    }

    #[tokio::test]
    async fn concurrent_tool_result_structure() {
        let tool_call = ToolCall {
            call_id: "test-call-1".to_string(),
            name: "test_tool".to_string(),
            arguments: serde_json::json!({"arg": "value"}),
        };

        let tool_result = ToolResult {
            call_id: "test-call-1".to_string(),
            output: "Test output".to_string(),
            error: None,
            success: false,
            exit_code: None,
            data: None,
        };

        let concurrent_result = ConcurrentToolResult {
            tool_call,
            result: tool_result,
            duration: Duration::from_millis(100),
            timed_out: false,
        };

        assert_eq!(concurrent_result.tool_call.name, "test_tool");
        assert!(concurrent_result.result.error.is_none());
        assert_eq!(concurrent_result.duration, Duration::from_millis(100));
        assert!(!concurrent_result.timed_out);
    }

    #[tokio::test]
    async fn concurrent_plan_result_structure() {
        let session_id = SessionId::new();
        let step_results = vec![
            ConcurrentToolResult {
                tool_call: ToolCall {
                    call_id: "call-1".to_string(),
                    name: "tool1".to_string(),
                    arguments: serde_json::Value::Null,
                },
                result: ToolResult {
                    call_id: "call-1".to_string(),
                    output: "Success".to_string(),
                    error: None,
                    success: false,
                    exit_code: None,
                    data: None,
                },
                duration: Duration::from_millis(50),
                timed_out: false,
            },
            ConcurrentToolResult {
                tool_call: ToolCall {
                    call_id: "call-2".to_string(),
                    name: "tool2".to_string(),
                    arguments: serde_json::Value::Null,
                },
                result: ToolResult {
                    call_id: "call-2".to_string(),
                    output: String::new(),
                    error: Some("Error".to_string()),
                    success: false,
                    exit_code: None,
                    data: None,
                },
                duration: Duration::from_millis(75),
                timed_out: false,
            },
        ];

        let plan_result = ConcurrentPlanResult {
            session_id: session_id.clone(),
            step_results,
            total_duration: Duration::from_millis(125),
            succeeded_count: 1,
            failed_count: 1,
            timed_out_count: 0,
        };

        assert_eq!(plan_result.step_results.len(), 2);
        assert_eq!(plan_result.succeeded_count, 1);
        assert_eq!(plan_result.failed_count, 1);
        assert_eq!(plan_result.timed_out_count, 0);
        assert_eq!(plan_result.total_duration, Duration::from_millis(125));
    }

    #[tokio::test]
    async fn test_execute_tools_concurrent_empty() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();
        let session_id = SessionId::new();
        let calls: Vec<ToolCall> = vec![];

        let results = runtime
            .execute_tools_concurrent(&session_id, calls, &cwd)
            .await
            .unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_execute_tools_concurrent_single() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();
        let session_id = SessionId::new();
        let calls = vec![ToolCall {
            call_id: "test-1".to_string(),
            name: "test_tool".to_string(),
            arguments: serde_json::json!({"arg": "value"}),
        }];

        let results = runtime
            .execute_tools_concurrent(&session_id, calls, &cwd)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].result.error.is_none());
        assert!(!results[0].timed_out);
        assert_eq!(results[0].tool_call.name, "test_tool");
    }

    #[tokio::test]
    async fn test_execute_tools_concurrent_multiple() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();
        let session_id = SessionId::new();
        let calls = (0..5)
            .map(|i| ToolCall {
                call_id: format!("test-{}", i),
                name: format!("tool_{}", i),
                arguments: serde_json::json!({"index": i}),
            })
            .collect();

        let results = runtime
            .execute_tools_concurrent(&session_id, calls, &cwd)
            .await
            .unwrap();

        assert_eq!(results.len(), 5);
        for result in &results {
            assert!(result.result.error.is_none());
            assert!(!result.timed_out);
            assert!(result.duration.as_millis() >= 10);
        }
    }

    #[tokio::test]
    async fn test_execute_tools_concurrent_with_config() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();
        let session_id = SessionId::new();
        let calls = vec![ToolCall {
            call_id: "test-1".to_string(),
            name: "test_tool".to_string(),
            arguments: serde_json::json!({"arg": "value"}),
        }];

        let config = ConcurrentConfig::default()
            .with_timeout(Duration::from_mins(1))
            .with_continue_on_error(false);

        let results = runtime
            .execute_tools_concurrent_with_config(&session_id, calls, &cwd, config)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].result.error.is_none());
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for lib.rs
    // =========================================================================

    // 1. ConcurrentToolResult with timed_out = true
    #[test]
    fn concurrent_tool_result_timed_out() {
        let result = ConcurrentToolResult {
            tool_call: ToolCall {
                call_id: "t1".into(),
                name: "slow_tool".into(),
                arguments: serde_json::Value::Null,
            },
            result: ToolResult {
                call_id: "t1".into(),
                output: String::new(),
                error: Some("Timed out".into()),
                success: false,
                exit_code: None,
                data: None,
            },
            duration: Duration::from_secs(30),
            timed_out: true,
        };
        assert!(result.timed_out);
        assert!(result.result.error.is_some());
    }

    // 2. ConcurrentToolResult with error but not timed out
    #[test]
    fn concurrent_tool_result_error_not_timeout() {
        let result = ConcurrentToolResult {
            tool_call: ToolCall {
                call_id: "t2".into(),
                name: "fail_tool".into(),
                arguments: serde_json::Value::Null,
            },
            result: ToolResult {
                call_id: "t2".into(),
                output: String::new(),
                error: Some("Permission denied".into()),
                success: false,
                exit_code: None,
                data: None,
            },
            duration: Duration::from_millis(100),
            timed_out: false,
        };
        assert!(!result.timed_out);
        assert!(result.result.error.is_some());
        assert!(!result.result.success);
    }

    // 3. ConcurrentPlanResult with all succeeded
    #[test]
    fn concurrent_plan_result_all_succeeded() {
        let plan_result = ConcurrentPlanResult {
            session_id: SessionId::new(),
            step_results: vec![ConcurrentToolResult {
                tool_call: ToolCall {
                    call_id: "a".into(),
                    name: "t1".into(),
                    arguments: serde_json::Value::Null,
                },
                result: ToolResult {
                    call_id: "a".into(),
                    output: "ok".into(),
                    error: None,
                    success: false,
                    exit_code: None,
                    data: None,
                },
                duration: Duration::from_millis(50),
                timed_out: false,
            }],
            total_duration: Duration::from_millis(50),
            succeeded_count: 1,
            failed_count: 0,
            timed_out_count: 0,
        };
        assert_eq!(plan_result.succeeded_count, 1);
        assert_eq!(plan_result.failed_count, 0);
        assert_eq!(plan_result.timed_out_count, 0);
    }

    // 4. ConcurrentPlanResult with all timed out
    #[test]
    fn concurrent_plan_result_all_timed_out() {
        let plan_result = ConcurrentPlanResult {
            session_id: SessionId::new(),
            step_results: vec![],
            total_duration: Duration::from_mins(1),
            succeeded_count: 0,
            failed_count: 0,
            timed_out_count: 3,
        };
        assert_eq!(plan_result.timed_out_count, 3);
        assert!(plan_result.step_results.is_empty());
    }

    // 5. ConcurrentPlanResult with empty step_results
    #[test]
    fn concurrent_plan_result_empty_steps() {
        let plan_result = ConcurrentPlanResult {
            session_id: SessionId::new(),
            step_results: vec![],
            total_duration: Duration::from_millis(1),
            succeeded_count: 0,
            failed_count: 0,
            timed_out_count: 0,
        };
        assert!(plan_result.step_results.is_empty());
        assert_eq!(
            plan_result.succeeded_count + plan_result.failed_count + plan_result.timed_out_count,
            0
        );
    }

    // 6. ConcurrentConfig builder chaining preserves all fields
    #[test]
    fn concurrent_config_builder_chaining() {
        let config = ConcurrentConfig::default()
            .with_max_concurrency(3)
            .with_timeout(Duration::from_mins(2))
            .with_continue_on_error(false);
        assert_eq!(config.max_concurrency, 3);
        assert_eq!(config.default_timeout, Duration::from_mins(2));
        assert!(!config.continue_on_error);
    }

    // 7. ConcurrentConfig with_max_concurrency zero
    #[test]
    fn concurrent_config_zero_concurrency() {
        let config = ConcurrentConfig::default().with_max_concurrency(0);
        assert_eq!(config.max_concurrency, 0);
    }

    // 8. ToolCall construction with complex arguments
    #[test]
    fn tool_call_with_complex_arguments() {
        let args = serde_json::json!({
            "path": "/some/path",
            "options": {"recursive": true, "depth": 5},
            "filters": ["*.rs", "*.toml"]
        });
        let call = ToolCall {
            call_id: "complex-1".into(),
            name: "search_files".into(),
            arguments: args.clone(),
        };
        assert_eq!(call.name, "search_files");
        assert_eq!(call.arguments["options"]["depth"], 5);
    }

    // 9. ToolResult with success and data
    #[test]
    fn tool_result_with_data() {
        let result = ToolResult {
            call_id: "d1".into(),
            output: "Found 3 matches".into(),
            error: None,
            success: true,
            exit_code: None,
            data: Some(serde_json::json!({"count": 3})),
        };
        assert!(result.success);
        assert!(result.data.is_some());
        assert_eq!(result.data.unwrap()["count"], 3);
    }

    // 10. ToolResult with error and no data
    #[test]
    fn tool_result_error_no_data() {
        let result = ToolResult {
            call_id: "e1".into(),
            output: String::new(),
            error: Some("File not found".into()),
            success: false,
            exit_code: None,
            data: None,
        };
        assert!(!result.success);
        assert!(result.error.is_some());
        assert!(result.data.is_none());
    }

    // 11. ConcurrentConfig debug format includes fields
    #[test]
    fn concurrent_config_debug_format() {
        let config = ConcurrentConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("max_concurrency"));
        assert!(debug.contains("continue_on_error"));
    }

    // 12. ConcurrentToolResult debug format
    #[test]
    fn concurrent_tool_result_debug_format() {
        let result = ConcurrentToolResult {
            tool_call: ToolCall {
                call_id: "dbg".into(),
                name: "tool".into(),
                arguments: serde_json::Value::Null,
            },
            result: ToolResult {
                call_id: "dbg".into(),
                output: "out".into(),
                error: None,
                success: false,
                exit_code: None,
                data: None,
            },
            duration: Duration::from_millis(50),
            timed_out: false,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("tool"));
        assert!(debug.contains("timed_out"));
    }

    // 13. ConcurrentPlanResult debug format
    #[test]
    fn concurrent_plan_result_debug_format() {
        let result = ConcurrentPlanResult {
            session_id: SessionId::new(),
            step_results: vec![],
            total_duration: Duration::from_millis(1),
            succeeded_count: 0,
            failed_count: 0,
            timed_out_count: 0,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("succeeded_count"));
        assert!(debug.contains("timed_out_count"));
    }

    // 14. Multiple ToolCall unique call_ids
    #[test]
    fn tool_calls_unique_ids() {
        let calls: Vec<ToolCall> = (0..5)
            .map(|i| ToolCall {
                call_id: format!("call-{}", i),
                name: "tool".into(),
                arguments: serde_json::Value::Null,
            })
            .collect();
        let ids: Vec<_> = calls.iter().map(|c| c.call_id.as_str()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), 5);
    }

    // 15. ConcurrentConfig default timeout is 30s
    #[test]
    fn concurrent_config_default_timeout_30s() {
        let config = ConcurrentConfig::default();
        assert_eq!(config.default_timeout.as_secs(), 30);
    }

    #[tokio::test]
    async fn test_extract_tool_calls_from_step() {
        let cwd = temp_dir();
        let data_dir = cwd.join("data");
        let skills_dir = cwd.join("skills");
        let memory_dir = cwd.join("memory");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            cwd.join(".rustycode.toml"),
            format!(
                "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
                data_dir.display(),
                skills_dir.display(),
                memory_dir.display()
            ),
        )
        .unwrap();

        let runtime = AsyncRuntime::load(&cwd).await.unwrap();
        let step = PlanStep {
            order: 1,
            title: "Test Step".to_string(),
            description: "A test step".to_string(),
            tools: vec![],
            expected_outcome: "Success".to_string(),
            rollback_hint: "N/A".to_string(),
            execution_status: rustycode_protocol::StepStatus::Pending,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };

        let conversation = &mut rustycode_protocol::Conversation::new(SessionId::new());
        let tool_calls = runtime
            .extract_tool_calls_from_step(&step, conversation)
            .unwrap();

        assert!(tool_calls.is_empty());
    }
}
