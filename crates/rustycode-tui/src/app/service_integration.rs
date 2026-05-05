//! Integration between the TUI event loop and background services (LLM streaming,

use crate::agent_mode::AiMode;
use crate::app::async_::*;
use crate::app::orchestration_integration::OrchestrationIntegration;
use crate::app::streaming::stream_llm_response;
use crate::conversation_service::{ConversationConfig, ConversationService};
// sessions_dir import used by auto-session feature
use crate::workspace_context;
use crate::{ErrorTracker, FileReadCache};
use anyhow::{Context, Result};
use rustycode_llm::provider::LLMProvider;
use rustycode_protocol::QueryGuard;
use rustycode_tools::ToolRegistry;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex as StdMutex};
use std::thread;
use std::time::Duration;

fn send_chunk<T: std::fmt::Debug>(tx: &SyncSender<T>, value: T) {
    if let Err(e) = tx.send(value) {
        tracing::debug!("Stream send failed (channel closed): {:?}", e.0);
    }
}

// ── Service Manager ───────────────────────────────────────────────────────────

/// Manages all background services for the TUI
///
/// The service manager owns all service channels and handles the lifecycle
/// of background tasks. It provides one-item-per-frame polling methods
/// for integration with the event loop.
pub struct ServiceManager {
    /// Conversation service (LLM streaming)
    conversation: Option<ConversationService>,

    /// Channel for LLM stream chunks
    stream_channel: Option<BoundedChannel<StreamChunk>>,

    /// Channel for tool execution results
    tool_channel: Option<BoundedChannel<ToolResult>>,

    /// Channel for workspace loading updates
    workspace_channel: Option<BoundedChannel<WorkspaceUpdate>>,

    /// Channel for slash command results
    command_channel: Option<BoundedChannel<SlashCommandResult>>,

    /// Channel for approval responses (TUI → streaming thread)
    approval_tx: Option<std::sync::mpsc::Sender<bool>>,

    /// Channel for question responses (TUI → streaming thread)
    question_tx: Option<std::sync::mpsc::Sender<String>>,

    ai_mode: AiMode,

    /// Current specialized agent mode
    agent_mode: crate::agent_mode::AgentMode,

    /// Current working directory
    cwd: PathBuf,

    /// Cooperative stop flag for active streaming requests
    stream_stop_requested: Arc<AtomicBool>,

    /// Signal to stop the persistent forwarding thread (used when updating model/pipeline)
    forwarding_thread_stop: Arc<AtomicBool>,

    /// File read deduplication cache
    file_read_cache: Arc<StdMutex<FileReadCache>>,

    /// Tool error tracker
    error_tracker: Arc<StdMutex<ErrorTracker>>,

    /// Guard ensuring only one LLM query runs at a time
    query_guard: QueryGuard,

    /// Shared todo state for LLM todo tools (todo_read, todo_write, todo_update)
    todo_state: Option<rustycode_tools::todo::TodoState>,

    /// Shared tool registry for executing tools (including skill tools)
    tool_registry: Option<Arc<ToolRegistry>>,

    orchestration: Arc<StdMutex<OrchestrationIntegration>>,

    /// Unified orchestration pipeline
    pub(crate) orchestration_pipeline:
        Option<Arc<rustycode_orchestration::pipeline::OrchestrationPipeline>>,

    /// Current reasoning effort level (low/medium/high/xhigh/max)
    effort: String,

    /// Hook manager for lifecycle hooks (PermissionRequest, UserPromptSubmit, etc.)
    hook_manager: Option<rustycode_tools::hooks::HookManager>,
}

/// Context passed to background streaming threads.
///
/// Bundles all necessary clones of service state and TUI configuration to avoid
/// repetitive cloning in the main message-sending path.
struct StreamingContext {
    content: String,
    cwd: PathBuf,
    stop_flag: Arc<AtomicBool>,
    agent_mode: crate::agent_mode::AgentMode,
    ai_mode: AiMode,
    orchestration: Arc<StdMutex<OrchestrationIntegration>>,
    file_read_cache: Arc<StdMutex<FileReadCache>>,
    error_tracker: Arc<StdMutex<ErrorTracker>>,
    todo_state: Option<rustycode_tools::todo::TodoState>,
    tool_registry: Arc<ToolRegistry>,
    history: Option<Vec<rustycode_llm::provider::ChatMessage>>,
    orchestration_guidance: Option<String>,
    phase_context: Option<String>,
    /// Base64-encoded image blocks from clipboard paste, threaded through to the LLM.
    image_blocks: Option<Vec<rustycode_llm::provider::ContentBlock>>,
    /// Reasoning effort level for LLM requests.
    effort: String,
    /// Hook manager for PermissionRequest and other lifecycle hooks.
    hook_manager: Option<rustycode_tools::hooks::HookManager>,
}

impl ServiceManager {
    pub fn new(cwd: PathBuf, ai_mode: AiMode) -> Self {
        Self {
            conversation: None,
            stream_channel: None,
            tool_channel: None,
            workspace_channel: None,
            command_channel: Some(BoundedChannel::new(100)),
            approval_tx: None,
            question_tx: None,
            ai_mode,
            agent_mode: crate::agent_mode::AgentMode::Code,
            cwd,
            stream_stop_requested: Arc::new(AtomicBool::new(false)),
            forwarding_thread_stop: Arc::new(AtomicBool::new(false)),
            file_read_cache: Arc::new(StdMutex::new(FileReadCache::new())),
            error_tracker: Arc::new(StdMutex::new(ErrorTracker::new())),
            query_guard: QueryGuard::new(),
            todo_state: None,
            tool_registry: None,
            orchestration: Arc::new(StdMutex::new(OrchestrationIntegration::default())),
            orchestration_pipeline: None,
            effort: "medium".to_string(),
            hook_manager: None,
        }
    }

    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    /// Switch the active model and update the orchestration pipeline.
    pub fn switch_model(&mut self, model_id: String) -> Result<()> {
        let (provider_type, _, v2_config) = rustycode_llm::load_provider_config_from_env()
            .unwrap_or_else(|_| {
                (
                    "anthropic".to_string(),
                    "claude-3-5-sonnet-20241022".to_string(),
                    Default::default(),
                )
            });

        let provider =
            rustycode_llm::create_provider_with_config(&provider_type, &model_id, v2_config)
                .context("Failed to create provider for new model")?;

        let stream_tx = self
            .stream_channel
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Stream channel not created"))?
            .clone_sender();

        self.init_orchestration(provider, &model_id, stream_tx)?;

        tracing::info!(model = %model_id, "Switched model and updated orchestration pipeline");
        Ok(())
    }

    pub fn file_read_cache(&self) -> Arc<StdMutex<FileReadCache>> {
        Arc::clone(&self.file_read_cache)
    }

    pub fn error_tracker(&self) -> Arc<StdMutex<ErrorTracker>> {
        Arc::clone(&self.error_tracker)
    }

    /// Set the shared todo state for LLM todo tools
    pub fn set_todo_state(&mut self, state: rustycode_tools::todo::TodoState) {
        self.todo_state = Some(state);
    }

    /// Create an LLM provider based on environment configuration with a fallback.
    fn create_llm_provider(&self) -> Result<(Arc<dyn LLMProvider>, String)> {
        let (provider_type, model, v2_config) = rustycode_llm::load_provider_config_from_env()
            .unwrap_or_else(|_| {
                (
                    "anthropic".to_string(),
                    "claude-3-5-sonnet-20241022".to_string(),
                    Default::default(),
                )
            });

        let provider =
            rustycode_llm::create_provider_with_config(&provider_type, &model, v2_config)
                .or_else(|_| {
                    rustycode_llm::create_provider("anthropic", "claude-3-5-sonnet-20241022")
                })
                .context("No LLM provider available. Set ANTHROPIC_API_KEY or configure a provider with /provider.")?;

        Ok((provider, model))
    }

    /// Start the conversation service
    ///
    /// Initializes the conversation service and creates the stream channel.
    /// Call this once at startup before sending messages.
    pub fn start_conversation(
        &mut self,
        config: ConversationConfig,
        tool_registry: ToolRegistry,
    ) -> Result<()> {
        crate::info_log!(
            "start_conversation called with {} tools",
            tool_registry.list().len()
        );
        // Store Arc reference to tool registry for tool execution
        let tool_registry_arc = Arc::new(tool_registry);
        self.tool_registry = Some(Arc::clone(&tool_registry_arc));

        // Create conversation service - pass the Arc'd registry
        let service = ConversationService::new(config, tool_registry_arc);

        // Create bounded channel for stream chunks (capacity 100)
        let stream_channel = BoundedChannel::new(100);
        let forward_tx = stream_channel.clone_sender();

        // Create channel for tool results (capacity 50)
        let tool_channel = BoundedChannel::new(50);

        self.conversation = Some(service);
        self.stream_channel = Some(stream_channel);
        self.tool_channel = Some(tool_channel);

        // Initialize unified orchestration pipeline
        let (provider, model) = self.create_llm_provider()?;
        self.init_orchestration(provider, &model, forward_tx)?;

        tracing::info!("Conversation service and orchestration pipeline started");

        Ok(())
    }

    /// Initialize the orchestration pipeline and its persistent event forwarding thread.
    fn init_orchestration(
        &mut self,
        provider: Arc<dyn LLMProvider>,
        model: &str,
        stream_tx: SyncSender<StreamChunk>,
    ) -> Result<()> {
        let tool_registry_arc = self
            .tool_registry
            .clone()
            .unwrap_or_else(|| Arc::new(ToolRegistry::new()));

        let pipeline =
            rustycode_orchestration::pipeline::OrchestrationPipeline::with_provider_model_and_tools(
                rustycode_orchestration::config::OrchestrationConfig::default(),
                provider,
                model,
                tool_registry_arc,
            );

        let bus = pipeline.bus_handle();
        self.orchestration_pipeline = Some(Arc::new(pipeline));
        crate::info_log!(
            "Pipeline initialized successfully, tool_count={}",
            self.orchestration_pipeline
                .as_ref()
                .map(|p| p.tool_count())
                .unwrap_or(0)
        );

        // Signal any existing forwarding thread to stop
        self.forwarding_thread_stop.store(true, Ordering::SeqCst);
        // Reset the stop flag for the new thread
        let stop_flag = Arc::new(AtomicBool::new(false));
        self.forwarding_thread_stop = Arc::clone(&stop_flag);

        // Spawn persistent orchestration event forwarding thread (single instance)
        let mut rx = bus.subscribe();
        thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("Failed to create runtime for forwarding thread: {}", e);
                    return;
                }
            };

            rt.block_on(async {
                let mut adapter =
                    crate::app::streaming::adapter::StreamEventAdapter::new(stream_tx);
                loop {
                    if stop_flag.load(Ordering::SeqCst) {
                        // Drain remaining events before exiting to prevent
                        // chunk loss during state transitions.
                        while let Ok(event) = rx.try_recv() {
                            adapter.on_orchestration_event(event);
                        }
                        tracing::info!("Forwarding thread stopping due to reset signal");
                        break;
                    }

                    match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                        Ok(Ok(event)) => {
                            adapter.on_orchestration_event(event);
                        }
                        Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(count))) => {
                            tracing::warn!("Orchestration bus lagged by {} events", count);
                        }
                        Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                            tracing::info!("Orchestration bus closed, exiting forwarding thread");
                            break;
                        }
                        Err(_) => {
                            // Timeout reached, just loop and check stop_flag
                        }
                    }
                }
            });
        });

        Ok(())
    }

    /// Send a user message and start streaming response
    ///
    /// This method is called from the event loop when the user sends a message.
    /// It spawns a background task that streams the LLM response through the channel.
    pub fn send_message(&mut self, content: String) -> Result<()> {
        self.send_message_with_history(content, None, None)
    }

    /// Send a message with conversation history for multi-turn context
    pub fn send_message_with_history(
        &mut self,
        content: String,
        conversation_history: Option<Vec<rustycode_llm::provider::ChatMessage>>,
        image_blocks: Option<Vec<rustycode_llm::provider::ContentBlock>>,
    ) -> Result<()> {
        use rustycode_prompt::ModelProvider;

        // 1. Guarding and Initialization
        if self.query_guard.is_active() {
            anyhow::bail!("A query is already in progress.");
        }

        let _generation = self
            .query_guard
            .try_start()
            .context("Failed to start query guard")?;

        let service = self.conversation.as_ref().ok_or_else(|| {
            self.query_guard.force_end();
            anyhow::anyhow!("Conversation service not started")
        })?;

        let (provider_type, model) = rustycode_llm::load_provider_config_from_env()
            .map(|(pt, model, _)| (pt, model))
            .unwrap_or_else(|_| ("anthropic".to_string(), "claude-3-5-sonnet".to_string()));

        let stream_tx = self
            .stream_channel
            .as_ref()
            .ok_or_else(|| {
                self.query_guard.force_end();
                anyhow::anyhow!("Stream channel not created")
            })?
            .clone_sender();

        // 2. Orchestration Analysis
        let (analysis, orchestration_guidance, phase_context) = {
            let orch = self.orchestration.lock().unwrap_or_else(|e| e.into_inner());
            let analysis = orch.analyze_message(&content, Some(&model));
            let guidance = if analysis.enable_structured_thinking {
                let mut g = OrchestrationIntegration::structured_thinking_guidance().to_string();
                g.push_str("\n\n");
                g.push_str(rustycode_orchestration::ask_user_tool::AskUserToolSchema::system_prompt_guidance());
                if let Some(ref report) = analysis.clarity_report {
                    if !report.questions.is_empty() {
                        g.push_str("\n\n## Clarity Assessment\n");
                        g.push_str(&format!(
                            "Ambiguity: {:.0}% — address these gaps in your reasoning:\n",
                            report.ambiguity * 100.0
                        ));
                        for q in &report.questions {
                            g.push_str(&format!(
                                "- **{}** ({}): {}\n",
                                q.dimension, q.rationale, q.question
                            ));
                        }
                    }
                }
                // Inject local decomposition plan for complex tasks
                if analysis.complexity >= 2.0 {
                    g.push_str("\n\n");
                    g.push_str(&rustycode_orchestration::decompose_local(
                        &content,
                        analysis.clarity_report.as_ref(),
                        analysis.complexity,
                    ));
                }
                Some(g)
            } else if analysis.complexity >= 2.5 {
                // Non-trivial tasks that don't trigger structured thinking still get a plan
                let plan = rustycode_orchestration::decompose_local(
                    &content,
                    analysis.clarity_report.as_ref(),
                    analysis.complexity,
                );
                Some(plan)
            } else {
                None
            };
            let ctx = orch.phase_context().map(|v| v.to_string());
            (analysis, guidance, ctx)
        };

        // 3. Tool Preparation
        let provider = ModelProvider::from_model_id(&provider_type);
        let mut tools_schema = service.generate_tool_schema_for_provider(provider, &model);
        if analysis.enable_structured_thinking {
            if let Some(tools) = tools_schema.get_mut("tools").and_then(|t| t.as_array_mut()) {
                for schema_fn in [
                    OrchestrationIntegration::structured_thinking_tool_schema,
                    || rustycode_orchestration::ask_user_tool::AskUserToolSchema::schema(),
                ] {
                    let raw = schema_fn();
                    let func = raw.get("function").cloned().unwrap_or_else(|| raw.clone());
                    let name = func
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let desc = func
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let params = func
                        .get("parameters")
                        .cloned()
                        .unwrap_or(serde_json::json!({"type":"object","properties":{}}));
                    tools.push(serde_json::json!({
                        "name": name,
                        "description": desc,
                        "input_schema": params,
                    }));
                }
            }
        }

        let tools = tools_schema
            .get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        // 4. Thread Context Preparation
        let (approval_tx, approval_rx) = std::sync::mpsc::channel();
        let (question_tx, question_rx) = std::sync::mpsc::channel();
        self.approval_tx = Some(approval_tx);
        self.question_tx = Some(question_tx);
        self.stream_stop_requested = Arc::new(AtomicBool::new(false));

        let ctx = StreamingContext {
            content,
            cwd: self.cwd.clone(),
            stop_flag: Arc::clone(&self.stream_stop_requested),
            agent_mode: self.agent_mode,
            ai_mode: self.ai_mode,
            orchestration: Arc::clone(&self.orchestration),
            file_read_cache: Arc::clone(&self.file_read_cache),
            error_tracker: Arc::clone(&self.error_tracker),
            todo_state: self.todo_state.clone(),
            tool_registry: self
                .tool_registry
                .clone()
                .unwrap_or_else(|| Arc::new(ToolRegistry::new())),
            history: conversation_history,
            orchestration_guidance,
            phase_context,
            image_blocks,
            effort: self.effort.clone(),
            hook_manager: self.hook_manager.clone(),
        };

        // 5. Dispatch
        let has_pipeline = self.orchestration_pipeline.is_some();
        crate::info_log!(
            "Dispatch: pipeline={}, tool_schema_count={}",
            has_pipeline,
            tools.len()
        );
        // Always use legacy streaming for interactive chat — the pipeline path
        // (conduct_with_history) is for autonomous orchestration loops and doesn't
        // stream tokens back to the TUI.
        self.execute_legacy_streaming(ctx, stream_tx, tools, approval_rx, question_rx);

        Ok(())
    }

    /// Request cooperative cancellation of an active stream.
    pub fn request_stop_stream(&mut self) {
        self.stream_stop_requested.store(true, Ordering::SeqCst);
        self.query_guard.force_end();
    }


    /// Execute legacy streaming as a fallback when the pipeline is unavailable.
    fn execute_legacy_streaming(
        &self,
        ctx: StreamingContext,
        stream_tx: SyncSender<StreamChunk>,
        tools_schema: Vec<serde_json::Value>,
        approval_rx: std::sync::mpsc::Receiver<bool>,
        question_rx: std::sync::mpsc::Receiver<String>,
    ) {
        let stream_tx_panic = stream_tx.clone();

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        send_chunk(&stream_tx, StreamChunk::Error(StreamError::RuntimeError {
                            message: e.to_string(),
                        }));
                        send_chunk(&stream_tx, StreamChunk::Done);
                        return;
                    }
                };

                let result = rt.block_on(async {
                    let config = crate::app::streaming::StreamConfig::new(
                        &ctx.content,
                        &ctx.cwd,
                        stream_tx.clone(),
                    )
                    .stop_signal_opt(Some(ctx.stop_flag))
                    .tools_schema_opt(Some(tools_schema))
                    .approval_rx_opt(Some(approval_rx))
                    .question_rx_opt(Some(question_rx))
                    .agent_mode_opt(Some(ctx.agent_mode))
                    .ai_mode_opt(Some(ctx.ai_mode))
                    .file_read_cache_opt(Some(ctx.file_read_cache))
                    .error_tracker_opt(Some(ctx.error_tracker))
                    .todo_state_opt(ctx.todo_state)
                    .conversation_history_opt(ctx.history)
                    .tool_registry_opt(Some(ctx.tool_registry))
                    .orchestration_guidance_opt(ctx.orchestration_guidance)
                    .phase_context_opt(ctx.phase_context)
                    .orchestration_opt(Some(ctx.orchestration))
                    .image_blocks_opt(ctx.image_blocks)
                    .effort_opt(Some(ctx.effort.clone()))
                    .hook_manager_opt(ctx.hook_manager);

                    stream_llm_response(config).await
                });

                if let Err(e) = result {
                    send_chunk(&stream_tx, StreamChunk::Error(StreamError::Provider(
                        rustycode_llm::provider::ProviderError::Api(e.to_string()),
                    )));
                    send_chunk(&stream_tx, StreamChunk::Done);
                }
            }));

            if result.is_err() {
                send_chunk(&stream_tx_panic, StreamChunk::Error(StreamError::InternalError {
                    message: "streaming thread panicked".to_string(),
                }));
                send_chunk(&stream_tx_panic, StreamChunk::Done);
            }
        });
    }

    pub fn has_pipeline(&self) -> bool {
        self.orchestration_pipeline.is_some()
    }

    pub fn complete_query(&mut self) {
        self.query_guard.force_end();
    }

    pub fn is_query_active(&self) -> bool {
        self.query_guard.is_active()
    }

    /// Send approval response to the streaming thread
    ///
    /// Called by TUI when user responds to an approval request.
    /// `true` = approve, `false` = reject
    pub fn send_approval_response(&self, approved: bool) {
        if let Some(ref tx) = self.approval_tx {
            if let Err(e) = tx.send(approved) {
                tracing::warn!("Failed to send approval response: {}", e);
            }
        } else {
            tracing::warn!("No approval channel available — response dropped");
        }
    }

    /// Send question response to the streaming thread
    ///
    /// Called by TUI when user answers a question.
    pub fn send_question_response(&self, answer: String) {
        if let Some(ref tx) = self.question_tx {
            if let Err(e) = tx.send(answer) {
                tracing::warn!("Failed to send question answer (channel closed): {:?}", e);
            }
        }
    }

    pub fn has_approval_channel(&self) -> bool {
        self.approval_tx.is_some()
    }

    pub fn agent_mode(&self) -> crate::agent_mode::AgentMode {
        self.agent_mode
    }

    pub fn set_agent_mode(&mut self, mode: crate::agent_mode::AgentMode) {
        self.agent_mode = mode;
        if let Some(conv) = &mut self.conversation {
            conv.set_agent_mode(mode);
        }
    }

    pub fn set_effort(&mut self, effort: String) {
        std::env::set_var("RUSTYCODE_EFFORT_OVERRIDE", &effort);
        self.effort = effort;
    }

    pub fn set_hook_manager(&mut self, hm: rustycode_tools::hooks::HookManager) {
        self.hook_manager = Some(hm);
    }

    pub fn next_agent_mode(&mut self) -> crate::agent_mode::AgentMode {
        self.agent_mode = self.agent_mode.next_mode();
        self.agent_mode
    }

    pub fn prev_agent_mode(&mut self) -> crate::agent_mode::AgentMode {
        self.agent_mode = self.agent_mode.prev();
        self.agent_mode
    }

    pub fn allows_tool(&self, tool_name: &str) -> bool {
        self.agent_mode.allows_tool(tool_name)
    }

    /// Spawns a background thread that loads workspace info with progress tracking.
    pub fn start_workspace_loading(&mut self) -> Result<()> {
        // Create bounded channel for workspace updates (capacity 20)
        let workspace_channel = BoundedChannel::new(20);

        let cwd = self.cwd.clone();
        let tx = workspace_channel.clone_sender();
        let last_reported_pct = std::sync::Arc::new(std::sync::Mutex::new(0u16));

        // Spawn background task for workspace loading with progress tracking
        let tx_final = workspace_channel.clone_sender();
        let last_reported_pct_for_thread = last_reported_pct.clone();
        thread::spawn(move || {
            // Create progress callback that sends updates through the channel
            let progress_callback: workspace_context::ScanProgressCallback =
                Box::new(move |scanned: usize, total: usize| {
                    let pct = if total > 0 {
                        ((scanned as f64 / total as f64) * 100.0).round() as u16
                    } else {
                        0
                    }
                    .clamp(0, 100);

                    let mut last_pct = last_reported_pct_for_thread
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let should_report =
                        *last_pct == 0 || pct == 100 || pct >= last_pct.saturating_add(10);

                    if should_report {
                        *last_pct = pct;
                        send_chunk(&tx, WorkspaceUpdate::ScanProgress { scanned, total });
                    }
                });

            // Load workspace context with progress tracking
            let context = workspace_context::load_workspace_context_with_progress(
                &cwd,
                10,
                20,
                Some(progress_callback),
            );

            // Send final context loaded message
            send_chunk(&tx_final, WorkspaceUpdate::ContextLoaded(context));

            if let Some((filename, _)) = workspace_context::find_project_instruction_file(&cwd) {
                send_chunk(&tx_final, WorkspaceUpdate::Notice(format!(
                    "Loaded {} from the workspace root",
                    filename
                )));
            }
        });

        self.workspace_channel = Some(workspace_channel);

        tracing::info!("Workspace loading started with progress tracking");

        Ok(())
    }

    /// Processes at most ONE chunk per frame, ensuring responsiveness.
    pub fn poll_stream_one<F>(&mut self, callback: F) -> Result<bool>
    where
        F: FnOnce(StreamChunk),
    {
        let channel = self
            .stream_channel
            .as_mut()
            .context("Stream channel not created")?;

        match channel.try_recv() {
            Some(chunk) => {
                callback(chunk);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Processes at most ONE result per frame, ensuring responsiveness.
    pub fn poll_tools_one<F>(&mut self, callback: F) -> Result<bool>
    where
        F: FnOnce(ToolResult),
    {
        let channel = self
            .tool_channel
            .as_mut()
            .context("Tool channel not created")?;

        match channel.try_recv() {
            Some(result) => {
                callback(result);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Processes at most ONE update per frame, ensuring responsiveness.
    pub fn poll_workspace_one<F>(&mut self, callback: F) -> Result<bool>
    where
        F: FnOnce(WorkspaceUpdate),
    {
        let channel = self
            .workspace_channel
            .as_mut()
            .context("Workspace channel not created")?;

        match channel.try_recv() {
            Some(update) => {
                callback(update);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn ai_mode(&self) -> AiMode {
        self.ai_mode
    }

    pub fn set_ai_mode(&mut self, mode: AiMode) {
        self.ai_mode = mode;
        if let Some(ref mut service) = self.conversation {
            service.set_ai_mode(mode);
        }
    }

    pub fn channel_stats(&self) -> ServiceStats {
        ServiceStats {
            stream_dropped: self
                .stream_channel
                .as_ref()
                .map(|c| c.dropped_count())
                .unwrap_or(0),
            tool_dropped: self
                .tool_channel
                .as_ref()
                .map(|c| c.dropped_count())
                .unwrap_or(0),
            workspace_dropped: self
                .workspace_channel
                .as_ref()
                .map(|c| c.dropped_count())
                .unwrap_or(0),
        }
    }

    pub fn stream_channel_mut(&mut self) -> Option<&mut BoundedChannel<StreamChunk>> {
        self.stream_channel.as_mut()
    }

    pub fn tool_channel_mut(&mut self) -> Option<&mut BoundedChannel<ToolResult>> {
        self.tool_channel.as_mut()
    }

    pub fn workspace_channel_mut(&mut self) -> Option<&mut BoundedChannel<WorkspaceUpdate>> {
        self.workspace_channel.as_mut()
    }

    pub fn command_channel_mut(&mut self) -> Option<&mut BoundedChannel<SlashCommandResult>> {
        self.command_channel.as_mut()
    }

    pub fn command_sender(&self) -> Option<std::sync::mpsc::SyncSender<SlashCommandResult>> {
        self.command_channel.as_ref().map(|c| c.clone_sender())
    }
}

// ── Statistics ────────────────────────────────────────────────────────────────

/// Statistics about service channel health
#[derive(Debug, Clone)]
pub struct ServiceStats {
    /// Number of dropped stream chunks (backpressure)
    pub stream_dropped: usize,
    /// Number of dropped tool results (backpressure)
    pub tool_dropped: usize,
    /// Number of dropped workspace updates (backpressure)
    pub workspace_dropped: usize,
}

impl ServiceStats {
    pub fn has_backpressure(&self) -> bool {
        self.stream_dropped > 0 || self.tool_dropped > 0 || self.workspace_dropped > 0
    }

    /// Get total dropped events
    pub fn total_dropped(&self) -> usize {
        self.stream_dropped + self.tool_dropped + self.workspace_dropped
    }
}

// ── Integration with TUI ───────────────────────────────────────────────────────


// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_manager_creation() {
        let cwd = PathBuf::from("/tmp");
        let manager = ServiceManager::new(cwd, AiMode::Act);
        assert_eq!(manager.ai_mode(), AiMode::Act);
    }

    #[test]
    fn test_ai_mode_get_set() {
        let cwd = PathBuf::from("/tmp");
        let mut manager = ServiceManager::new(cwd, AiMode::Act);

        manager.set_ai_mode(AiMode::Plan);
        assert_eq!(manager.ai_mode(), AiMode::Plan);
    }

    #[test]
    fn test_channel_stats() {
        let stats = ServiceStats {
            stream_dropped: 5,
            tool_dropped: 2,
            workspace_dropped: 0,
        };

        assert!(stats.has_backpressure());
        assert_eq!(stats.total_dropped(), 7);
    }

    #[test]
    fn test_channel_stats_no_backpressure() {
        let stats = ServiceStats {
            stream_dropped: 0,
            tool_dropped: 0,
            workspace_dropped: 0,
        };

        assert!(!stats.has_backpressure());
        assert_eq!(stats.total_dropped(), 0);
    }
}
