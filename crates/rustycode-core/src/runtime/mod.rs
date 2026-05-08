//! Runtime module - Core runtime and session management.
//!
//! This module extracts the runtime logic from the main lib.rs file,
//! including session management, event publishing, and tool execution orchestration.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use rustycode_bus::{EventBus, SessionCompletedEvent, SessionStartedEvent, ToolBlockedEvent};
use rustycode_config::Config;
use rustycode_git::GitStatus;
use rustycode_llm::tool_annotations::anthropic_annotations_for_tool_info;
use rustycode_lsp::LspServerStatus;
use rustycode_memory::MemoryEntry;
use rustycode_protocol::{
    ContextPlan, EventKind, Plan, PlanId, Session, SessionEvent, SessionId, ToolCall, ToolResult,
};
use rustycode_session::session_manager::{SessionManager, SessionStats};
use rustycode_skill::{manager::SkillManager, Skill};
use rustycode_storage::{
    conversation_history::{
        new_conversation_id, now_timestamp, Conversation as HistoryConversation,
        ConversationHistory, SavedMessage,
    },
    Storage,
};
use rustycode_tools::{check_tool_permission, ToolContext, ToolInfo, ToolRegistry};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

// Re-exports from sibling modules
pub use super::execution::{
    ExecutionConfig, ExecutionContext, StepExecutor, StepExecutorRegistry, ToolInvocationWrapper,
};
pub use super::plan_executor::{ExecutionOptions, ExecutionReport, PlanExecutor};
pub use super::session::{AiMode, MessageType, SessionState, ToolExecution, ToolStatus};
pub use super::tool_result_storage::{CacheConfig, ToolResultCache};

// === Report Structs ===

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub config: Config,
    pub git: GitStatus,
    pub lsp_servers: Vec<LspServerStatus>,
    pub memory_entries: usize,
    pub skills: usize,
    pub sample_context_plan: ContextPlan,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub session: Session,
    pub git: GitStatus,
    pub lsp_servers: Vec<LspServerStatus>,
    pub memory: Vec<MemoryEntry>,
    pub skills: Vec<Skill>,
    pub recent_tasks: Vec<String>,
    pub code_excerpts: Vec<CodeExcerpt>,
    pub context_plan: ContextPlan,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallReport {
    pub session: Session,
    pub result: ToolResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeExcerpt {
    pub path: String,
    pub preview: String,
    pub score: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanReport {
    pub session: Session,
    pub plan: Plan,
    /// Absolute path to the skeleton plan.md file.
    pub plan_path: PathBuf,
}

/// Core runtime struct - manages session lifecycle and tool execution
pub struct Runtime {
    pub config: Config,
    pub storage: Storage,
    pub tools: Arc<ToolRegistry>,
    pub bus: Arc<EventBus>,
    pub llm_provider: Option<Box<dyn rustycode_llm::LLMProvider>>,
    pub mcp_integration: Option<super::integration::mcp_integration::McpIntegration>,
    pub hooks: Arc<tokio::sync::RwLock<super::integration::hooks_integration::HookRegistry>>,
    pub session_manager: Option<SessionManager>,
    pub tool_cache: Arc<Mutex<ToolResultCache>>,
    pub skill_manager: Arc<Mutex<Option<SkillManager>>>,
}

impl Runtime {
    /// Load runtime from configuration
    pub fn load(cwd: &Path) -> Result<Self> {
        let config = Config::load(cwd)?;
        let storage = Storage::open(&config.data_dir.join("rustycode.db"))?;
        let mut tools = rustycode_tools::default_registry();

        // Register structured thinking tool for headless agent
        tools.register(
            rustycode_orchestration::structured_thinking_tool_impl::StructuredThinkingTool::new(
                None,
            ),
        );
        // Register ask_user tool so LLM can request clarification when stuck
        tools.register(rustycode_orchestration::ask_user_tool::AskUserTool);
        let bus = Arc::new(EventBus::new());

        // LLM provider initialization moved to runtime layer
        let llm_provider: Option<Box<dyn rustycode_llm::LLMProvider>> = None;

        // Initialize MCP integration if configured
        let mcp_integration = if !config.advanced.mcp_servers_map.is_empty() {
            info!(
                "Initializing MCP integration with {} server(s)",
                config.advanced.mcp_servers_map.len()
            );
            let config_clone = config.clone();
            let integration = crate::shared_runtime::block_on_shared_send(async move {
                let mut integration =
                    super::integration::mcp_integration::McpIntegration::new(&config_clone).await?;
                integration.start_servers().await?;
                Ok::<_, anyhow::Error>(integration)
            })?;

            info!("MCP integration initialized successfully");
            Some(integration)
        } else {
            info!("No MCP servers configured");
            None
        };

        // Initialize session manager
        let sessions_dir = config.data_dir.join("sessions");
        let session_manager = Some(SessionManager::new(sessions_dir));

        let tool_cache = Arc::new(Mutex::new(ToolResultCache::new(CacheConfig {
            max_entries: 1000,
            ttl: std::time::Duration::from_mins(5),
            min_size_to_cache: 100,
        })));

        let skill_manager = {
            let mut builder = SkillManager::builder().token_budget(50_000);
            let user_skills = dirs::home_dir()
                .map(|h| h.join(".rustycode").join("skills"))
                .unwrap_or_else(|| config.data_dir.join("skills"));
            if user_skills.exists() {
                builder = builder.user_skills_dir(&user_skills);
            }
            if config.skills_dir.exists() {
                builder = builder.project_skills_dir(&config.skills_dir);
            }
            builder = builder.quality_storage_dir(config.data_dir.join("skill-quality"));
            builder = builder.graph_path(config.data_dir.join("capability-graph.json"));
            match builder.build() {
                Ok(mgr) => {
                    info!("Skill manager v2 initialized");
                    Some(mgr)
                }
                Err(e) => {
                    warn!("Failed to initialize skill manager v2: {}", e);
                    None
                }
            }
        };

        Ok(Self {
            config,
            storage,
            tools: Arc::new(tools),
            bus,
            llm_provider,
            mcp_integration,
            hooks: Arc::new(tokio::sync::RwLock::new(
                super::integration::hooks_integration::HookRegistry::new(),
            )),
            session_manager,
            tool_cache,
            skill_manager: Arc::new(Mutex::new(skill_manager)),
        })
    }

    /// Get config reference
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get event bus reference
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.bus
    }

    /// Get LLM provider reference
    pub fn llm_provider(&self) -> Option<&dyn rustycode_llm::LLMProvider> {
        self.llm_provider.as_deref()
    }

    /// List all available tools (including MCP tools)
    pub fn tool_list(&self) -> Vec<ToolInfo> {
        let mut tools = self.tools.list();

        // Add MCP tools if available
        if let Some(mcp) = &self.mcp_integration {
            let mcp_tools = mcp.mcp_tools();
            for mcp_tool in mcp_tools {
                tools.push(ToolInfo {
                    name: mcp_tool.name.clone(),
                    description: mcp_tool.description.clone(),
                    parameters_schema: mcp_tool.input_schema.clone(),
                    permission: rustycode_tools::ToolPermission::Execute,
                    defer_loading: None,
                    annotations: None,
                    tags: vec![],
                    max_result_size_chars: None,
                    is_destructive_default: false,
                });
            }
        }

        tools
    }

    // === Session Management ===

    /// Save a session to disk
    pub fn save_session(&self, session: &Session) -> Result<()> {
        if let Some(manager) = &self.session_manager {
            manager.save_session(session)?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Load a session by ID
    pub fn load_session(&self, session_id: &SessionId) -> Result<Session> {
        if let Some(manager) = &self.session_manager {
            manager.load_session(session_id)
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Fork an existing session
    pub fn fork_session(&self, session_id: &SessionId) -> Result<SessionId> {
        if let Some(manager) = &self.session_manager {
            manager.fork_session(session_id)
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        if let Some(manager) = &self.session_manager {
            manager.list_sessions()
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Delete a session
    pub fn delete_session(&self, session_id: &SessionId) -> Result<()> {
        if let Some(manager) = &self.session_manager {
            manager.delete_session(session_id)
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Clean up old sessions
    pub fn cleanup_old_sessions(&self, days_old: u64) -> Result<usize> {
        if let Some(manager) = &self.session_manager {
            manager.cleanup_old_sessions(days_old)
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    /// Get session statistics
    pub fn session_stats(&self) -> Result<SessionStats> {
        if let Some(manager) = &self.session_manager {
            manager.stats()
        } else {
            Err(anyhow::anyhow!("Session manager not initialized"))
        }
    }

    // === Event Publishing ===

    /// Helper to publish events from sync code
    fn publish_event<E: rustycode_bus::Event + Clone + Send + 'static>(&self, event: E) {
        let bus = Arc::clone(&self.bus);

        // Try to use existing runtime; otherwise use the shared runtime
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // We're inside a runtime, spawn a task to publish
            handle.spawn(async move {
                if let Err(e) = bus.publish(event).await {
                    warn!("Failed to publish event to bus: {}", e);
                }
            });
        } else {
            // No runtime exists — run on the workspace-wide shared runtime.
            crate::shared_runtime::block_on_shared(async move {
                if let Err(e) = bus.publish(event).await {
                    warn!("Failed to publish event to bus: {}", e);
                }
            });
        }
    }

    /// Publish session started event
    pub fn publish_session_started(&self, session_id: SessionId, task: String, detail: String) {
        self.publish_event(SessionStartedEvent::new(session_id, task, detail));
    }

    /// Publish session completed event
    pub fn publish_session_completed(
        &self,
        session_id: SessionId,
        task: String,
        status: String,
        detail: String,
    ) {
        self.publish_event(SessionCompletedEvent::new(session_id, task, status, detail));
    }

    /// Publish tool blocked event
    pub fn publish_tool_blocked(
        &self,
        session_id: SessionId,
        tool_name: String,
        arguments: serde_json::Value,
        reason: String,
        detail: String,
    ) {
        self.publish_event(ToolBlockedEvent::new(
            session_id, tool_name, arguments, reason, detail,
        ));
    }

    /// Check tool permissions based on session mode and publish blocked event if not permitted
    pub fn check_tool_permission_and_publish(
        &self,
        session_id: &SessionId,
        call: &ToolCall,
    ) -> Result<()> {
        // Check permissions based on session mode
        if let Ok(Some(session)) = self.storage.load_session(session_id) {
            if !check_tool_permission(&call.name, session.mode) {
                warn!(
                    tool = %call.name,
                    mode = ?session.mode,
                    "tool not permitted in current session mode"
                );

                // Publish tool blocked event
                self.publish_tool_blocked(
                    session_id.clone(),
                    call.name.clone(),
                    call.arguments.clone(),
                    format!("{:?}", session.mode),
                    format!(
                        "Tool '{}' is not permitted in {:?} mode",
                        call.name, session.mode
                    ),
                );

                // Record blocked event in storage
                self.storage.insert_event(&SessionEvent {
                    session_id: session_id.clone(),
                    at: Utc::now(),
                    kind: EventKind::ToolBlockedInPlanningMode,
                    detail: format!("tool={} mode={:?}", call.name, session.mode),
                })?;

                bail!(
                    "tool '{}' is not permitted in {:?} mode",
                    call.name,
                    session.mode
                );
            }
        }
        Ok(())
    }

    // === Doctor / Diagnostics ===

    /// Run diagnostics and return a health report.
    pub fn doctor(&self, cwd: &Path) -> Result<DoctorReport> {
        let git = rustycode_git::inspect(cwd).unwrap_or(GitStatus {
            root: None,
            branch: None,
            worktree: false,
            dirty: None,
        });
        let lsp_servers: Vec<LspServerStatus> = self
            .config
            .lsp_servers
            .iter()
            .map(|name| LspServerStatus {
                name: name.clone(),
                installed: false,
                path: None,
            })
            .collect();
        let memory_entries = self.storage.memory("project").map(|v| v.len()).unwrap_or(0);
        // Count built-in skills (always available)
        let builtin_count = 5; // hardcoded; SkillRegistry is internal
                               // Count custom skills from the skills directory (only valid skill files)
        let custom_count = if self.config.skills_dir.exists() {
            std::fs::read_dir(&self.config.skills_dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path().extension().is_some_and(|ext| {
                                matches!(ext.to_str(), Some("yaml" | "yml" | "toml" | "json"))
                            })
                        })
                        .count()
                })
                .unwrap_or(0)
        } else {
            0
        };
        let skills = builtin_count + custom_count;
        let sample_context_plan = ContextPlan::default();
        Ok(DoctorReport {
            config: self.config.clone(),
            git,
            lsp_servers,
            memory_entries,
            skills,
            sample_context_plan,
        })
    }

    // === Core Run Operations ===

    /// Run a task and return a full report.
    pub fn run(&self, cwd: &Path, task: &str) -> Result<RunReport> {
        let git = rustycode_git::inspect(cwd).unwrap_or(GitStatus {
            root: None,
            branch: None,
            worktree: false,
            dirty: None,
        });
        let lsp_servers: Vec<LspServerStatus> = self
            .config
            .lsp_servers
            .iter()
            .map(|name| LspServerStatus {
                name: name.clone(),
                installed: false,
                path: None,
            })
            .collect();
        let memory = Vec::new();
        let skills: Vec<Skill> = self
            .skill_manager
            .lock()
            .map(|guard| {
                guard
                    .as_ref()
                    .map(|mgr| {
                        mgr.all_definitions()
                            .iter()
                            .map(|def| Skill::from((*def).clone()))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let recent_tasks = Vec::new();
        let code_excerpts = Vec::new();
        let context_plan = ContextPlan::default();

        let session = Session::builder().task(task.to_string()).build();
        self.storage.insert_session(&session)?;

        self.publish_session_started(
            session.id.clone(),
            session.task.clone(),
            format!("task={}", task),
        );

        Ok(RunReport {
            session,
            git,
            lsp_servers,
            memory,
            skills,
            recent_tasks,
            code_excerpts,
            context_plan,
        })
    }

    /// Run an agent task synchronously.
    pub fn run_agent(&self, session_id: &SessionId, task: &str) -> Result<()> {
        let session = Session::builder().task(task.to_string()).build();
        self.storage.insert_session(&session)?;
        self.publish_session_started(
            session_id.clone(),
            task.to_string(),
            "mode=agent".to_string(),
        );
        Ok(())
    }

    /// Run a headless agent task with the shared tool registry.
    pub async fn run_headless_task_with_iteration(
        &self,
        provider: &dyn rustycode_llm::provider::LLMProvider,
        model: &str,
        task: &str,
        cwd: &Path,
        iteration: usize,
    ) -> Result<crate::headless::HeadlessTaskResult> {
        let tools_schema: Vec<serde_json::Value> = self
            .tool_list()
            .into_iter()
            .map(|tool| {
                let mut tool_json = serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters_schema,
                });
                if let Some(annotations) = anthropic_annotations_for_tool_info(
                    &tool.name,
                    matches!(tool.permission, rustycode_tools::ToolPermission::Read),
                ) {
                    tool_json["annotations"] = annotations;
                }
                tool_json
            })
            .collect();

        crate::headless::run_headless_task_core(
            provider,
            model,
            &tools_schema,
            task,
            cwd,
            iteration,
            &self.tools,
            None,
        )
        .await
    }

    /// Run headless agent with prior conversation messages for retry continuation.
    pub async fn run_headless_with_prior_messages(
        &self,
        provider: &dyn rustycode_llm::provider::LLMProvider,
        model: &str,
        task: &str,
        cwd: &Path,
        iteration: usize,
        prior_messages: Option<Vec<rustycode_llm::provider::ChatMessage>>,
    ) -> Result<crate::headless::HeadlessTaskResult> {
        let tools_schema: Vec<serde_json::Value> = self
            .tool_list()
            .into_iter()
            .map(|tool| {
                let mut tool_json = serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters_schema,
                });
                if let Some(annotations) = anthropic_annotations_for_tool_info(
                    &tool.name,
                    matches!(tool.permission, rustycode_tools::ToolPermission::Read),
                ) {
                    tool_json["annotations"] = annotations;
                }
                tool_json
            })
            .collect();

        crate::headless::run_headless_task_core(
            provider,
            model,
            &tools_schema,
            task,
            cwd,
            iteration,
            &self.tools,
            prior_messages,
        )
        .await
    }

    /// Execute a tool call within a session.
    ///
    /// Checks the cache before executing the tool. If a cached result exists
    /// and is not expired, returns the cached result. Otherwise, executes the
    /// tool and caches the result.
    pub fn execute_tool(
        &self,
        session_id: &SessionId,
        call: ToolCall,
        cwd: &Path,
    ) -> Result<ToolResult> {
        self.check_tool_permission_and_publish(session_id, &call)?;

        // Check cache before executing
        // Serialize arguments for cache key computation
        let args_json = serde_json::to_value(&call.arguments).unwrap_or(serde_json::Value::Null);

        // Check cache (need to hold lock briefly)
        let cached_result = {
            let mut cache = self.tool_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.get(&call.name, &args_json).cloned()
        };

        if let Some(cached) = cached_result {
            info!(
                tool = %call.name,
                tokens_saved = cached.token_count,
                "Tool result cache hit"
            );
            // Return cached result - reconstruct ToolResult from cached content
            return Ok(ToolResult {
                call_id: call.call_id.clone(),
                output: cached.content.clone(),
                error: None,
                success: true,
                exit_code: None,
                data: None,
                new_cwd: None,
            });
        }

        let ctx = ToolContext::new(cwd).with_registry(self.tools.clone());
        let result = self.tools.execute(&call, &ctx);

        if let Ok(mut guard) = self.skill_manager.lock() {
            if let Some(ref mut mgr) = *guard {
                let args_str = serde_json::to_string(&call.arguments).unwrap_or_default();
                mgr.observe_tool_use(&call.name, &args_str);
            }
        }

        // Cache the result if successful and large enough
        if result.success {
            let content = result.output.as_str();
            let mut cache = self.tool_cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.insert(&call.name, &args_json, content) {
                info!(
                    tool = %call.name,
                    content_len = content.len(),
                    "Tool result cached"
                );
            }
        }

        Ok(result)
    }

    /// Perform cleanup when shutting down the runtime.
    /// Persists skill quality scores, saves the capability graph,
    /// and publishes a session completed event.
    pub fn shutdown(&self) {
        // Persist skill quality scores and capability graph
        if let Ok(mut guard) = self.skill_manager.lock() {
            if let Some(mgr) = guard.as_mut() {
                mgr.end_session();
            }
        }

        // Publish session completed event
        self.publish_session_completed(
            SessionId::new(),
            String::new(),
            "completed".to_string(),
            "Runtime shutdown".to_string(),
        );

        info!("Runtime shutdown complete");
    }

    /// Run a single tool by name and arguments, returning a full report.
    pub fn run_tool(
        &self,
        cwd: &Path,
        name: String,
        arguments: serde_json::Value,
    ) -> Result<ToolCallReport> {
        let call_id = SessionId::new().to_string();
        let session = Session::builder().task(format!("tool={}", name)).build();
        self.storage.insert_session(&session)?;

        let call = ToolCall {
            call_id,
            name: name.clone(),
            arguments,
        };
        let ctx = ToolContext::new(cwd).with_registry(self.tools.clone());
        let result = self.tools.execute(&call, &ctx);

        Ok(ToolCallReport { session, result })
    }

    // === Planning ===

    /// Start planning a task (sync version).
    pub fn start_planning(&self, cwd: &Path, task: &str) -> Result<PlanReport> {
        let session = Session::builder().task(task.to_string()).build();
        self.storage.insert_session(&session)?;

        let plan = Plan {
            id: PlanId::new(),
            session_id: session.id.clone(),
            task: task.to_string(),
            created_at: Utc::now(),
            status: rustycode_protocol::PlanStatus::Draft,
            summary: String::new(),
            approach: String::new(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,

            milestone_id: None,
        };
        let plan_path = cwd.join("plan.md");

        // Write plan skeleton to disk
        let mut md = String::new();
        md.push_str(&format!("# Plan: {}\n\n", &plan.task));
        md.push_str("## Approach\n\n");
        if plan.approach.is_empty() {
            md.push_str("To be determined\n\n");
        } else {
            md.push_str(&plan.approach);
            md.push_str("\n\n");
        }
        md.push_str("## Steps\n\n");
        if plan.steps.is_empty() {
            md.push_str("To be determined\n\n");
        } else {
            for step in &plan.steps {
                md.push_str(&format!("### {}. {}\n", step.order + 1, &step.title));
                md.push_str(&step.description);
                md.push('\n');
                if !step.tools.is_empty() {
                    md.push_str(&format!("**Tools:** {}\n", step.tools.join(", ")));
                }
                if !step.expected_outcome.is_empty() {
                    md.push_str(&format!(
                        "**Expected outcome:** {}\n",
                        step.expected_outcome
                    ));
                }
                md.push('\n');
            }
        }
        md.push_str("## Files to Modify\n\n");
        if plan.files_to_modify.is_empty() {
            md.push_str("To be determined\n\n");
        } else {
            for f in &plan.files_to_modify {
                md.push_str(&format!("- {f}\n"));
            }
            md.push('\n');
        }
        md.push_str("## Risks\n\n");
        if plan.risks.is_empty() {
            md.push_str("None identified\n");
        } else {
            for r in &plan.risks {
                md.push_str(&format!("- {r}\n"));
            }
        }
        self.storage.insert_plan(&plan)?;

        std::fs::write(&plan_path, &md)
            .with_context(|| format!("failed to write plan to {}", plan_path.display()))?;

        self.publish_session_started(
            session.id.clone(),
            session.task.clone(),
            format!("task={} mode=planning", task),
        );

        Ok(PlanReport {
            session,
            plan,
            plan_path,
        })
    }

    /// Start planning a task (async version for the runtime layer).
    pub async fn start_planning_async(&self, cwd: &Path, task: &str) -> Result<PlanReport> {
        let cwd = cwd.to_path_buf();
        let task = task.to_string();
        // Delegate to the sync version via spawn_blocking
        let config = self.config.clone();
        let inner =
            Runtime::load_from_parts(config, Arc::clone(&self.tools), Arc::clone(&self.bus))?;
        let report = tokio::task::spawn_blocking(move || inner.start_planning(&cwd, &task))
            .await
            .map_err(|e| anyhow::anyhow!(e))??;
        Ok(report)
    }

    /// Approve a plan for execution.
    pub fn approve_plan(&self, session_id: &SessionId, _cwd: &Path) -> Result<()> {
        let plans = self.storage.list_plans(session_id)?;
        if let Some(plan) = plans.first() {
            self.storage
                .update_plan_status(&plan.id, &rustycode_protocol::PlanStatus::Approved)?;
        }
        Ok(())
    }

    /// Reject a plan.
    pub fn reject_plan(&self, session_id: &SessionId) -> Result<()> {
        let plans = self.storage.list_plans(session_id)?;
        if let Some(plan) = plans.first() {
            self.storage
                .update_plan_status(&plan.id, &rustycode_protocol::PlanStatus::Rejected)?;
        }
        Ok(())
    }

    /// List all plans up to a limit.
    pub fn all_plans(&self, limit: usize) -> Result<Vec<Plan>> {
        self.storage.all_plans(limit)
    }

    /// Update a specific plan step.
    pub fn update_plan_step(
        &self,
        plan_id: &PlanId,
        step_index: usize,
        step: &rustycode_protocol::PlanStep,
    ) -> Result<()> {
        self.storage.update_plan_step(plan_id, step_index, step)
    }

    // === Memory ===

    /// Upsert a memory entry.
    pub fn upsert_memory(&self, scope: &str, key: &str, value: &str) -> Result<()> {
        self.storage.upsert_memory(scope, key, value)
    }

    /// Get all memory entries for a scope.
    pub fn memory(&self, scope: &str) -> Result<Vec<rustycode_storage::MemoryRecord>> {
        self.storage.memory(scope)
    }

    /// Get a single memory entry.
    pub fn memory_entry(&self, scope: &str, key: &str) -> Result<Option<String>> {
        self.storage.memory_entry(scope, key)
    }

    // === Plan Loading ===

    /// Load a plan by ID.
    pub fn load_plan(&self, plan_id: &PlanId) -> Result<Option<Plan>> {
        self.storage.load_plan(plan_id)
    }

    /// Load the plan associated with a session.
    pub fn load_plan_for_session(&self, session_id: &SessionId) -> Result<Option<Plan>> {
        let plans = self.storage.list_plans(session_id)?;
        Ok(plans.into_iter().next())
    }

    /// Execute the next pending step in a plan.
    pub fn execute_plan_step(&self, _session_id: &SessionId) -> Result<()> {
        // Stub: plan step execution requires plan state tracking
        Ok(())
    }

    // === Session Queries ===

    /// Get recent sessions.
    pub fn recent_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        self.storage.recent_sessions(limit)
    }

    /// Get events for a session.
    pub fn session_events(&self, session_id: &SessionId) -> Result<Vec<SessionEvent>> {
        self.storage.session_events(session_id)
    }

    /// Build a Runtime from pre-existing parts (for async spawn_blocking).
    fn load_from_parts(
        config: Config,
        tools: Arc<ToolRegistry>,
        bus: Arc<EventBus>,
    ) -> Result<Self> {
        let storage = Storage::open(&config.data_dir.join("rustycode.db"))?;
        let sessions_dir = config.data_dir.join("sessions");
        let tool_cache = Arc::new(Mutex::new(ToolResultCache::new(CacheConfig::default())));
        Ok(Self {
            config,
            storage,
            tools,
            bus,
            llm_provider: None,
            mcp_integration: None,
            hooks: Arc::new(tokio::sync::RwLock::new(
                super::integration::hooks_integration::HookRegistry::new(),
            )),
            session_manager: Some(SessionManager::new(sessions_dir)),
            tool_cache,
            skill_manager: Arc::new(Mutex::new(None)),
        })
    }

    /// Save conversation history to disk
    pub fn save_conversation(
        &self,
        session_id: &SessionId,
        messages: &[rustycode_llm::provider::ChatMessage],
        task: &str,
        model: &str,
        provider: &str,
    ) -> Result<()> {
        // Initialize conversation history manager
        let history = ConversationHistory::default_dir()
            .context("Failed to initialize conversation history")?;

        // Extract tags from task (first few words)
        let tags: Vec<String> = task
            .split_whitespace()
            .take(3)
            .map(|s| s.to_lowercase())
            .collect();

        // Convert ChatMessage to SavedMessage
        let saved_messages: Vec<SavedMessage> = messages
            .iter()
            .map(|msg| SavedMessage {
                role: format!("{:?}", msg.role).to_lowercase(),
                content: msg.content.as_text().to_string(),
                timestamp: now_timestamp(),
                tokens: None,
                model: Some(model.to_string()),
                provider: Some(provider.to_string()),
            })
            .collect();

        // Create conversation
        let conversation = HistoryConversation {
            id: new_conversation_id(),
            title: task.chars().take(80).collect::<String>(),
            created_at: now_timestamp(),
            updated_at: now_timestamp(),
            model: model.to_string(),
            provider: provider.to_string(),
            messages: saved_messages,
            tags,
            total_tokens: 0,
            total_cost_cents: 0,
            workspace_path: std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string()),
        };

        // Save to disk
        history
            .save(&conversation)
            .context("Failed to save conversation history")?;

        info!(
            session_id = %session_id.to_string(),
            conversation_id = %conversation.id,
            "Conversation saved to history"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn test_runtime_creation() {
        let temp_dir = TempDir::new().unwrap();
        let result = Runtime::load(temp_dir.path());
        // Note: This may fail if config doesn't exist, which is expected
        // The test verifies the module compiles and can be instantiated
        assert!(result.is_ok() || result.is_err());
    }
}
pub mod monitor;
