//! Responsive Event Loop
//!
//! Coordinates all UI components via one-item-per-frame processing.
//! Guarantees <50ms input latency and 60 FPS (16ms frame budget).

use crate::agent_mode::AiMode;
use crate::agents::AgentManager;
use crate::app::event_loop_commands::{
    dispatch_registered_slash_command, CommandContext, CommandEffect,
};
use crate::app::keyboard_shortcuts::KeyboardShortcutHandler;
use crate::app::rate_limit_handler::RateLimitHandler;
use crate::app::renderer::RendererMode;
use crate::app::team_mode_handler::TeamModeHandler;
use crate::app::wizard_handler::WizardHandler;
use crate::app::{service_integration::*, FRAME_BUDGET_60FPS};
use crate::compaction::{CompactionConfig, ContextMonitor};
use crate::config::load_config;
use crate::config::TUIConfig;
use crate::conversation_service::ConversationConfig;
use crate::help::HelpState;
use crate::memory_auto::ThreadSafeAutoMemory;
use crate::memory_injection::InjectionConfig;
use crate::plugin::PluginManager;
use crate::plugin::PluginManagerUI;
use crate::providers::get_all_available_models;
use crate::session::load_command_history;
use crate::skills::{SkillLoader, SkillStateManager};
use crate::tasks::{load_tasks, WorkspaceTasks};
use crate::theme::{Theme, ThemeColors};
use crate::tool_approval::ToolApprovalManager;
use crate::ui::animator::Animator;
use crate::ui::command_palette::CommandPalette;
use crate::ui::file_selector::FileSelector;
use crate::ui::input::{InputHandler, InputMode, InputState};
use crate::ui::message::{Message, MessageRenderer, ToolExecution};
use crate::ui::message_search::SearchState;
use crate::ui::message_tags::TagFilter;
use crate::ui::model_selector::ModelSelector;
use crate::ui::session_sidebar::{McpServerState, McpServerStatus, SessionSidebar};
use crate::ui::skill_palette::SkillPalette;
use rustycode_llm::tool_annotations::anthropic_annotations_for_tool_info;
use rustycode_lsp::{default_servers as default_lsp_servers, discover as discover_lsp_servers};

use crate::ui::theme_preview::{ThemePreview, ThemeSwitcher};
use crate::ui::toast::ToastManager;
use anyhow::{Context, Result};
use crossterm::event;
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use rustycode_core::integration::HookRegistry;
use rustycode_tools::ToolRegistry;
use std::collections::{HashSet, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tracing::info;

/// Terminal cleanup guard - ensures terminal is restored even on panic
struct TerminalCleanupGuard;

impl Drop for TerminalCleanupGuard {
    fn drop(&mut self) {
        // Restore terminal state - ignore errors since we're in a panic handler
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show, // Ensure cursor is visible
        );
        // Flush to ensure all commands are executed
        let _ = std::io::stdout().flush();

        // Print session summary to terminal after leaving alternate screen
        // (Goose pattern: users see cost/duration after exiting)
        // Note: We can't access TUI state here, so the summary is printed
        // by the event_loop before this guard drops.
    }
}

/// Install panic hook to ensure terminal cleanup on panic
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show,
        );
        let _ = std::io::stdout().flush();
        eprintln!("\nRustyCode TUI panicked:");
        eprintln!("{}", panic_info);
        eprintln!("\nPlease report this bug at https://github.com/luengnat/rustycode/issues");
    }));
}

/// Main TUI application
///
/// Wires together all UI components:
/// - Message rendering (hierarchical display)
/// - Input handling (multi-line, clipboard, images)
/// - Markdown rendering (syntax highlighting, diffs)
/// - Status bar (progress, animations)
/// - Animation system (smooth updates)
/// - Service integration (LLM streaming, tool execution, workspace loading)
pub struct TUI {
    // UI Components (plugins)
    pub(crate) message_renderer: MessageRenderer,
    pub(crate) input_handler: InputHandler,
    pub(crate) animator: Animator,
    pub(crate) event_receiver: tokio::sync::broadcast::Receiver<rustycode_mcp::protocol::McpEvent>,
    #[allow(dead_code)] // Wired to UI in future milestone
    pub(crate) marketplace_browser: crate::ui::marketplace_browser::MarketplaceBrowser,

    // Service Manager (background tasks)
    pub(crate) services: ServiceManager,

    // State
    pub(crate) messages: Vec<Message>,
    pub(crate) _input_state: InputState,
    pub(crate) input_mode: InputMode,
    pub(crate) running: bool,

    // Message list state
    pub(crate) scroll_offset_line: usize, // Line-based scroll (for actual rendering)
    pub(crate) selected_message: usize,
    pub(crate) viewport_height: usize,
    pub(crate) last_total_lines: std::cell::Cell<usize>, // Track total lines from last render pass
    pub(crate) messages_area: std::cell::Cell<Rect>,     // Store messages area for click detection
    pub(crate) sidebar_area: std::cell::Cell<Rect>,      // Store sidebar area for mouse routing
    pub(crate) mouse_selection_start: std::cell::Cell<Option<(u16, u16)>>,
    pub(crate) mouse_selection_dragged: std::cell::Cell<bool>,
    pub(crate) user_scrolled: bool, // Track if user manually scrolled up
    pub(crate) last_user_scroll_time: Instant, // Debounce: prevent auto-scroll for 2s after user scrolls

    // Streaming state
    pub(crate) current_stream_content: String,
    pub(crate) streaming_render_buffer: crate::app::streaming_render_buffer::StreamingRenderBuffer,
    pub(crate) is_streaming: bool,
    pub(crate) stream_cancelled: bool, // Set by Esc/Ctrl+C, checked by Done handler
    pub(crate) chunks_received: usize,
    pub(crate) thinking_chunks_received: usize,
    pub(crate) queued_message: Option<String>, // Queued while streaming (goose pattern)
    pub(crate) pending_bash_result: Arc<std::sync::Mutex<Option<String>>>,
    pub(crate) stream_start_time: Option<Instant>, // Goose pattern: response timing
    pub(crate) last_response_duration: Option<Duration>, // Shows in status bar after completion
    pub(crate) plan_mode_banner: Option<crate::app::plan_mode_ops::PlanModeBanner>,
    pub(crate) execution_trace: Option<serde_json::Value>,

    // Tool execution tracking
    pub(crate) active_tools: std::collections::HashMap<String, ToolExecution>,

    // Workspace state
    pub(crate) workspace_loaded: bool,
    pub(crate) workspace_context: Option<String>, // Store workspace context for LLM
    pub(crate) workspace_tasks: WorkspaceTasks,
    pub(crate) pipeline: crate::app::pipeline::registry::PipelineRegistry,
    pub(crate) pipeline_ctx: crate::app::pipeline::registry::PipelineContext,
    pub(crate) pipeline_guardian: crate::app::pipeline::guardian::PipelineGuardian,
    // Pipeline cron scheduler (std::sync::mpsc channels, NOT tokio)
    pub(crate) scheduler_rx: Option<mpsc::Receiver<crate::app::pipeline::ScheduledPhaseEvent>>,
    #[allow(dead_code)]
    pub(crate) scheduler_tx: Option<mpsc::Sender<crate::app::pipeline::ScheduledPhaseEvent>>,
    pub(crate) active_scheduled_phases: std::collections::HashSet<String>,
    pub(crate) max_concurrent_phases: usize,
    pub(crate) last_extraction: Option<(Vec<crate::tasks::Task>, Vec<crate::tasks::Todo>)>,
    pub(crate) workspace_scan_progress: Option<(usize, usize)>, // (scanned, total)
    pub(crate) git_branch: Option<String>,                      // Current git branch for status bar

    // Rate limit handler
    pub(crate) rate_limit: RateLimitHandler,

    // Auto-continue mode - automatically continue working on pending tasks
    pub(crate) auto_continue_enabled: bool, // Whether auto-continue is active
    pub(crate) auto_continue_pending: bool, // Whether a continuation is pending
    pub(crate) auto_continue_iterations: usize, // Number of auto-continue iterations

    // Turn-level verification (snapshot before agent turn, diff after)
    pub(crate) turn_snapshot: Option<crate::app::turn_snapshot::TurnSnapshot>,
    // Doom loop detector — tracks repetitive tool-call patterns
    pub(crate) doom_loop: crate::app::doom_loop::DoomLoopDetector,

    // Active Reasoning Engine budget tracking
    pub(crate) reasoning_budget:
        std::sync::Mutex<rustycode_tools::providers::reasoning_types::BudgetState>,

    // Performance: dirty flag - only render when state changes
    pub(crate) dirty: bool,
    // Set after external editor returns to force terminal.clear() + full redraw
    pub(crate) needs_full_redraw: bool,

    // Token compaction
    pub(crate) context_monitor: ContextMonitor,
    pub(crate) compaction_config: CompactionConfig,
    pub(crate) showing_compaction_preview: bool,
    pub(crate) pending_compaction: bool,

    // Auto-memory system
    pub(crate) auto_memory: Option<Arc<ThreadSafeAutoMemory>>,
    pub(crate) memory_injection_config: InjectionConfig,

    // Skill palette
    pub(crate) skill_palette: SkillPalette,
    pub(crate) skill_manager: Arc<RwLock<SkillStateManager>>,
    pub(crate) plugin_manager: Arc<RwLock<PluginManager>>,
    pub(crate) plugin_manager_ui: PluginManagerUI,
    pub(crate) showing_plugin_manager: bool,
    pub(crate) showing_marketplace_browser: bool,

    // Round 2 Features: Help system
    pub(crate) help_state: HelpState,

    // Round 2 Features: Tool approval
    pub(crate) tool_approval: ToolApprovalManager,
    pub(crate) pending_approval_request: VecDeque<crate::tool_approval::ApprovalRequest>,
    pub(crate) awaiting_approval: bool, // Whether we're waiting for user response

    // Session start time (for elapsed time display)
    pub(crate) start_time: Instant,
    // Last time we refreshed LSP status
    pub(crate) last_lsp_refresh: Instant,
    // Cache of last-known LSP state so we can detect changes and mark dirty
    pub(crate) last_lsp_servers: Vec<String>,
    pub(crate) last_lsp_connected: bool,
    // Last time we refreshed MCP/server status
    pub(crate) last_mcp_refresh: Instant,
    // Cache of last-known MCP state so we can detect changes and mark dirty
    pub(crate) last_mcp_servers: Vec<McpServerStatus>,
    pub(crate) last_mcp_connected: bool,

    // Theme colors for live switching
    pub(crate) theme_colors: Arc<std::sync::Mutex<ThemeColors>>,

    // Theme preview for live theme switching
    pub(crate) theme_preview: ThemePreview,

    // Quick theme switcher
    pub(crate) theme_switcher: ThemeSwitcher,

    // Toast notifications for theme change feedback
    pub(crate) toast_manager: ToastManager,

    // Error display manager for prominent error messages with suggestions
    pub(crate) error_manager: crate::ui::errors::ErrorManager,
    pub(crate) showing_error: bool,

    // Tool panel visibility (independent of sidebar)
    pub(crate) showing_tool_panel: bool,
    pub(crate) tool_panel_history: Vec<ToolExecution>, // Recent tool executions
    pub(crate) tool_panel_selected_index: Option<usize>, // Selected tool for inspection
    pub(crate) showing_tool_result: bool,              // Showing detailed tool result
    pub(crate) tool_result_show_full: bool,            // Toggle full output in tool detail
    pub(crate) tool_result_scroll_offset: usize,       // Scroll offset for tool result overlay

    // Team agent timeline panel
    pub(crate) team_panel: crate::ui::team_panel::TeamPanel,
    /// Team mode handler
    pub(crate) team_handler: TeamModeHandler,

    // Worker status panel
    pub(crate) worker_panel: crate::ui::worker_panel::WorkerPanel,

    // AST pipeline phase progress
    pub(crate) ast_phase_state: crate::ui::ast_progress::AstPhaseState,

    // Clarification questions panel
    pub(crate) clarification_panel: crate::ui::clarification::ClarificationPanel,
    pub(crate) awaiting_clarification: bool, // Whether we're waiting for user answers

    // Command palette for slash commands
    pub(crate) command_palette: CommandPalette,
    pub(crate) showing_command_palette: bool,
    pub(crate) showing_skill_palette: bool,

    // Collapsible sections (Phase 3 polish)
    pub(crate) status_bar_collapsed: bool,
    pub(crate) footer_collapsed: bool,

    // Double-Esc to clear input
    pub(crate) last_esc_press: Option<Instant>,

    // Stashed prompt (Ctrl+S)
    pub(crate) stashed_prompt: Option<String>,

    // Model/Provider selector screens
    pub(crate) model_selector: ModelSelector,
    pub(crate) file_selector: FileSelector,
    pub(crate) showing_provider_selector: bool,
    pub(crate) current_model: String,

    // Session sidebar
    pub(crate) session_sidebar: SessionSidebar,

    // Session recovery (crash detection + auto-save)
    pub(crate) session_recovery:
        Option<crate::app::session_recovery_integration::SessionRecoveryManager>,

    // Message click detection (for collapse/expand)
    pub(crate) message_areas: std::cell::RefCell<Vec<(usize, Rect)>>, // (message_index, area)

    // Per-message line offsets from last render (for accurate search scroll)
    pub(crate) message_line_offsets: std::cell::RefCell<Vec<usize>>, // msg_idx -> start line

    // First-run configuration wizard handler
    pub(crate) wizard: WizardHandler,

    // Agent lifecycle management
    pub(crate) agent_manager: AgentManager,

    // TUI Configuration (mouse scroll speed, behavior settings, etc.)
    pub(crate) tui_config: TUIConfig,

    // Keyboard shortcut handler for Vim mode and chord detection
    pub(crate) keyboard_handler: KeyboardShortcutHandler,

    // Undo stack for scroll positions (last 5 positions: (selected_message, scroll_offset_line))
    pub(crate) undo_stack: VecDeque<(usize, usize)>,

    /// File undo stack for `/undo` command — each entry is a batch of (path, old_content) pairs
    pub(crate) file_undo_stack: Vec<Vec<(String, String)>>,

    // File finder (Ctrl+O fuzzy file search)
    pub(crate) file_finder: crate::ui::file_finder::FileFinder,

    // Message search state
    pub(crate) search_state: SearchState,

    // Message tag filter state
    pub(crate) tag_filter: TagFilter,

    // Active frame renderer backend
    pub(crate) renderer_mode: RendererMode,

    /// Session-long MCP proxy cache - owns live MCP connections for loaded tools
    pub(crate) mcp_proxies: Option<
        Arc<
            tokio::sync::RwLock<std::collections::HashMap<String, rustycode_mcp::proxy::ToolProxy>>,
        >,
    >,
    /// Shared todo state for LLM todo tools (todo_read, todo_write, todo_update)
    pub(crate) todo_state: rustycode_tools::todo::TodoState,

    // Session token usage and cost tracking
    pub(crate) session_input_tokens: usize,
    pub(crate) session_output_tokens: usize,
    pub(crate) session_cache_read_tokens: usize,
    pub(crate) session_cache_creation_tokens: usize,
    pub(crate) last_turn_input_tokens: usize,
    pub(crate) session_cost_usd: f64,
    pub(crate) cost_tracker: rustycode_llm::cost_tracker::CostTracker,

    // Hook manager for lifecycle extensibility
    pub(crate) hook_manager: rustycode_tools::hooks::HookManager,

    // Plan mode for plan-first execution gates
    pub(crate) plan_mode: rustycode_orchestration::plan_mode::PlanMode,

    // Cached API key warning (computed once, not per-frame)
    pub(crate) api_key_warning: String,
}

impl TUI {
    /// Evaluate if a task might benefit from team mode.
    /// Returns a suggestion message if team mode is recommended.
    ///
    /// Only suggests for genuinely high-risk operations to avoid noise.
    /// Common coding words like "build", "create", "service" are excluded
    /// since they appear in almost every request.
    pub fn evaluate_team_mode_suggestion(content: &str) -> Option<String> {
        let lower = content.to_lowercase();

        // Skip if already a slash command
        if content.trim().starts_with('/') {
            return None;
        }

        // Only suggest for genuinely high-risk security/production keywords
        let high_risk_keywords = [
            "authentication system",
            "authorization system",
            "password",
            "credential store",
            "api key rotation",
            "production deployment",
            "database migration",
            "payment processing",
            "encryption key",
        ];

        let has_high_risk = high_risk_keywords.iter().any(|kw| lower.contains(kw));

        if has_high_risk {
            Some(format!(
                "💡 High-risk task detected. Consider using team mode for built-in review:\n   /team {}",
                content.chars().take(50).collect::<String>()
            ))
        } else {
            None
        }
    }

    pub fn poll_mcp_events(&mut self) -> anyhow::Result<()> {
        while let Ok(event) = self.event_receiver.try_recv() {
            match event {
                rustycode_mcp::protocol::McpEvent::ProgressNotification { progress, message } => {
                    info!("MCP progress: {}% - {:?}", progress * 100.0, message);
                    self.dirty = true;
                }
                rustycode_mcp::protocol::McpEvent::ToolsListChanged { .. } => {
                    self.dirty = true;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Reset all streaming-related state to defaults.
    ///
    /// Call this when a stream ends (success, error, or cancellation),
    /// when send fails, or when loading/resuming a session that may have
    /// been saved mid-stream.
    pub(crate) fn reset_streaming_state(&mut self) {
        self.is_streaming = false;
        self.stream_cancelled = false;
        self.chunks_received = 0;
        self.thinking_chunks_received = 0;
        self.current_stream_content.clear();
        self.streaming_render_buffer =
            crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
        self.stream_start_time = None;
        self.ast_phase_state.deactivate();
    }

    /// Create a new TUI instance with service integration
    #[allow(clippy::await_holding_lock)]
    pub fn new(
        cwd: PathBuf,
        ai_mode: AiMode,
        reconfigure: bool,
        event_receiver: tokio::sync::broadcast::Receiver<rustycode_mcp::protocol::McpEvent>,
    ) -> Result<Self> {
        let services = ServiceManager::new(cwd.clone(), ai_mode);

        // Load TUI configuration
        let tui_config = load_config();
        let renderer_mode = RendererMode::from_brutalist(tui_config.ui.brutalist_mode);
        let reduced_motion = tui_config.behavior.reduced_motion; // Extract before move

        let model_id = rustycode_llm::load_model_from_config().unwrap_or_else(|e| {
            tracing::warn!("Failed to load model config, using default: {}", e);
            String::new()
        });
        let compaction_config = CompactionConfig {
            model_id: if model_id.is_empty() {
                None
            } else {
                Some(model_id.clone())
            },
            ..Default::default()
        };
        let context_monitor = ContextMonitor::new(
            compaction_config.effective_max_tokens(),
            compaction_config.warning_threshold,
        );

        // Initialize auto-memory system
        let auto_memory = ThreadSafeAutoMemory::new(&cwd)
            .map_err(|e| tracing::warn!("Auto-memory initialization failed, disabled: {}", e))
            .ok()
            .map(Arc::new);

        // Initialize memory injection configuration
        let memory_injection_config = InjectionConfig::default();

        // Initialize theme colors with default theme
        let theme_colors = Arc::new(std::sync::Mutex::new(ThemeColors::from(&Theme::default())));

        // Initialize theme preview and switcher
        let theme_preview = ThemePreview::new(theme_colors.clone());
        let theme_switcher = ThemeSwitcher::new(theme_colors.clone());
        let toast_manager = ToastManager::new();
        let error_manager = crate::ui::errors::ErrorManager::new();
        let registry = crate::marketplace::registry::RegistryManager::new(vec![]);
        let marketplace_browser = crate::ui::marketplace_browser::MarketplaceBrowser::new(registry);

        // Initialize command palette
        let command_palette = CommandPalette::new();

        // Load available skills
        let skill_loader = SkillLoader::new();
        let available_skills = skill_loader.load_all().unwrap_or_else(|e| {
            tracing::warn!("Failed to load skills: {}", e);
            Vec::new()
        });
        let skill_palette = SkillPalette::new(available_skills.clone());

        // Initialize skill state manager
        let skill_manager = Arc::new(RwLock::new(SkillStateManager::new()));
        let plugin_manager = Arc::new(RwLock::new(PluginManager::default()));
        let plugin_manager_ui = PluginManagerUI::new();
        // Load skills asynchronously in background
        let skill_manager_clone = skill_manager.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("Failed to create tokio runtime for skill loading: {}", e);
                    return;
                }
            };
            rt.block_on(async {
                let load_result = {
                    #[allow(clippy::await_holding_lock)]
                    let mut manager = skill_manager_clone
                        .write()
                        .unwrap_or_else(|e| e.into_inner());
                    manager.load_skills().await
                };
                if let Err(e) = load_result {
                    tracing::error!("Failed to load skills: {}", e);
                }
            });
        });

        // Initialize hooks registry
        let _hook_registry = Arc::new(RwLock::new(HookRegistry::new()));

        // Initialize input handler and load command history
        let mut input_handler = InputHandler::new();
        let history = load_command_history();
        input_handler.set_history(history);

        Ok(Self {
            message_renderer: MessageRenderer::new(),
            input_handler,
            animator: Animator::new(4, reduced_motion),
            event_receiver,
            marketplace_browser,
            services,
            messages: Vec::new(),
            _input_state: InputState::new(),
            input_mode: InputMode::SingleLine,
            running: true,
            scroll_offset_line: 0,
            last_total_lines: std::cell::Cell::new(0),
            selected_message: 0,
            viewport_height: 20,
            messages_area: std::cell::Cell::new(Rect::default()),
            sidebar_area: std::cell::Cell::new(Rect::default()),
            mouse_selection_start: std::cell::Cell::new(None),
            mouse_selection_dragged: std::cell::Cell::new(false),
            user_scrolled: false,
            last_user_scroll_time: Instant::now(),
            current_stream_content: String::new(),
            streaming_render_buffer:
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new(),
            is_streaming: false,
            stream_cancelled: false,
            queued_message: None,
            pending_bash_result: Arc::new(std::sync::Mutex::new(None)),
            chunks_received: 0,
            thinking_chunks_received: 0,
            stream_start_time: None,
            last_response_duration: None,
            plan_mode_banner: None,
            active_tools: std::collections::HashMap::new(),
            workspace_loaded: false,
            workspace_context: None, // Initialize workspace context as None
            workspace_tasks: load_tasks(),
            pipeline: {
                let mut p = crate::app::pipeline::registry::PipelineRegistry::new();
                let browser_manager =
                    Arc::new(crate::app::pipeline::browser_manager::BrowserManager::new());
                p.tool_registry.register(
                    "browser",
                    "goto",
                    Arc::new(
                        crate::app::pipeline::tools::browser_tools::BrowserGotoTool::new(
                            browser_manager.clone(),
                        ),
                    ),
                );
                p.tool_registry.register(
                    "browser",
                    "extract",
                    Arc::new(
                        crate::app::pipeline::tools::browser_extract::BrowserExtractTool::new(
                            browser_manager,
                        ),
                    ),
                );

                p.register_factory(
                    "rustycode::steps::DataGateStep",
                    Box::new(crate::app::pipeline::steps::data_gate_factory::DataGateFactory),
                );
                p.register_factory(
                    "rustycode::steps::AgentStep",
                    Box::new(crate::app::pipeline::steps::agent_factory::AgentStepFactory),
                );
                p
            },
            pipeline_ctx: {
                let (pt, mdl, _) =
                    rustycode_llm::load_provider_config_from_env().unwrap_or_else(|_| {
                        (
                            "anthropic".into(),
                            "claude-haiku-4-5-20251001".into(),
                            Default::default(),
                        )
                    });
                let pipeline_provider =
                    rustycode_llm::create_provider(&pt, &mdl).unwrap_or_else(|_| {
                        std::sync::Arc::new(rustycode_llm::mock::MockProvider::from_text(
                            "mock result",
                        ))
                    });
                crate::app::pipeline::registry::PipelineContext::new(
                    pipeline_provider,
                    rustycode_agent::AgentConfig::default(),
                    mdl,
                    crate::app::pipeline::tool_registry::ToolRegistry::new(),
                )
            },
            pipeline_guardian: crate::app::pipeline::guardian::PipelineGuardian::new(),
            scheduler_rx: None,
            scheduler_tx: None,
            active_scheduled_phases: std::collections::HashSet::new(),
            max_concurrent_phases: 3,
            last_extraction: None,
            workspace_scan_progress: None,
            git_branch: None,
            rate_limit: RateLimitHandler::new(),
            auto_continue_enabled: std::env::var("RUSTYCODE_AUTO_CONTINUE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            auto_continue_pending: false,
            auto_continue_iterations: 0,
            turn_snapshot: None,
            doom_loop: crate::app::doom_loop::DoomLoopDetector::new(),
            execution_trace: None,
            reasoning_budget: std::sync::Mutex::new(
                rustycode_tools::providers::reasoning_types::BudgetState::default(),
            ),
            dirty: true,
            needs_full_redraw: false,
            context_monitor,
            theme_colors,
            compaction_config,
            showing_compaction_preview: false,
            pending_compaction: false,
            auto_memory,
            memory_injection_config,
            skill_palette,
            skill_manager,
            plugin_manager,
            plugin_manager_ui,
            showing_plugin_manager: false,
            showing_marketplace_browser: false,
            help_state: HelpState::new(),
            tool_approval: ToolApprovalManager::new(),
            pending_approval_request: VecDeque::new(),
            awaiting_approval: false,
            start_time: Instant::now(),
            last_lsp_refresh: Instant::now() - Duration::from_mins(1), // force immediate refresh
            last_lsp_servers: Vec::new(),
            last_lsp_connected: false,
            last_mcp_refresh: Instant::now() - Duration::from_mins(1), // force immediate refresh
            last_mcp_servers: Vec::new(),
            last_mcp_connected: false,
            theme_preview,
            theme_switcher,
            toast_manager,
            error_manager,
            showing_error: false,
            showing_tool_panel: false,
            tool_panel_history: Vec::new(),
            tool_panel_selected_index: None,
            showing_tool_result: false,
            tool_result_show_full: false,
            tool_result_scroll_offset: 0,
            command_palette,
            showing_command_palette: false,
            showing_skill_palette: false,
            status_bar_collapsed: false,
            footer_collapsed: false,
            last_esc_press: None,
            stashed_prompt: None,
            model_selector: ModelSelector::with_models(get_all_available_models()),
            file_selector: FileSelector::new(Vec::new()),
            showing_provider_selector: false,
            current_model: rustycode_llm::load_model_from_config().unwrap_or_else(|e| {
                tracing::warn!("Failed to load model config: {}", e);
                String::new()
            }),
            session_sidebar: SessionSidebar::new(),
            session_recovery:
                crate::app::session_recovery_integration::SessionRecoveryManager::new(
                    crate::app::session_recovery_integration::SessionRecoveryConfig::default(),
                )
                .ok(),
            message_areas: std::cell::RefCell::new(Vec::new()), // Track message areas for click detection
            message_line_offsets: std::cell::RefCell::new(Vec::new()), // Per-message line offsets
            agent_manager: AgentManager::new(),
            // First-run wizard initialization
            wizard: WizardHandler::new(&cwd, reconfigure),
            // Keyboard shortcut handler for Vim mode (gg chord detection)
            keyboard_handler: KeyboardShortcutHandler::new(tui_config.behavior.vim_enabled),
            // Undo stack for scroll positions (max 5 entries)
            undo_stack: VecDeque::with_capacity(5),
            file_undo_stack: Vec::new(),
            // Message search state
            search_state: SearchState::new(),
            // File finder (Ctrl+O)
            file_finder: crate::ui::file_finder::FileFinder::new(cwd.clone()),
            // Message tag filter state
            tag_filter: TagFilter::new(),
            // TUI configuration
            tui_config,
            // Brutalist mode from config (new distinctive look)
            renderer_mode,
            // MCP proxy cache (initialized in init_services)
            mcp_proxies: None,
            // Shared todo state for LLM todo tools
            todo_state: rustycode_tools::todo::new_todo_state(),
            // Team agent timeline panel
            team_panel: crate::ui::team_panel::TeamPanel::new(),
            team_handler: TeamModeHandler::new(),
            // Worker status panel
            worker_panel: crate::ui::worker_panel::WorkerPanel::new(),
            // AST pipeline phase progress
            ast_phase_state: crate::ui::ast_progress::AstPhaseState::new(),
            // Clarification questions panel
            clarification_panel: crate::ui::clarification::ClarificationPanel::hidden(),
            awaiting_clarification: false,
            // Session token usage and cost tracking
            session_input_tokens: 0,
            session_output_tokens: 0,
            session_cache_read_tokens: 0,
            session_cache_creation_tokens: 0,
            last_turn_input_tokens: 0,
            session_cost_usd: 0.0,
            cost_tracker: rustycode_llm::cost_tracker::CostTracker::new(None),
            hook_manager: rustycode_tools::hooks::HookManager::new(
                PathBuf::from(".rustycode/hooks"),
                rustycode_tools::hooks::HookProfile::Standard,
                String::new(),
            ),
            plan_mode: {
                use rustycode_orchestration::plan_mode::{PlanMode, PlanModeConfig};
                use rustycode_protocol::AgentRole;

                let mut plan_mode = PlanMode::new(PlanModeConfig::default());
                plan_mode.set_role(AgentRole::Worker);
                plan_mode
            },
            // Cached API key warning (computed once)
            api_key_warning: Self::compute_api_key_warning(),
        })
    }

    /// Compute API key warning string once at startup (not per-frame)
    fn compute_api_key_warning() -> String {
        if let Ok((provider_type, _, v2_config)) = rustycode_llm::load_provider_config_from_env() {
            let needs_api_key = !matches!(
                provider_type.to_lowercase().as_str(),
                "ollama" | "local" | "lmstudio" | ""
            );
            if needs_api_key && v2_config.api_key.is_none() {
                return format!(
                    "⚠ No API key — set {} to get started",
                    rustycode_config::api_key_env_name(&provider_type)
                );
            }
        }
        String::new()
    }

    #[cfg(test)]
    /// Create a TUI instance for testing (minimal setup)
    pub fn new_for_test() -> Self {
        use std::path::PathBuf;

        let cwd = PathBuf::from(".");
        let services = ServiceManager::new(cwd.clone(), AiMode::Ask);

        let compaction_config = CompactionConfig::default();
        let context_monitor = ContextMonitor::new(
            compaction_config.effective_max_tokens(),
            compaction_config.warning_threshold,
        );

        let auto_memory = None;
        let memory_injection_config = InjectionConfig::default();
        let theme_colors = Arc::new(std::sync::Mutex::new(ThemeColors::from(&Theme::default())));
        let theme_preview = ThemePreview::new(theme_colors.clone());
        let theme_switcher = ThemeSwitcher::new(theme_colors.clone());
        let toast_manager = ToastManager::new();
        let error_manager = crate::ui::errors::ErrorManager::new();
        let registry = crate::marketplace::registry::RegistryManager::new(vec![]);
        let marketplace_browser = crate::ui::marketplace_browser::MarketplaceBrowser::new(registry);
        let command_palette = CommandPalette::new();

        let skill_loader = SkillLoader::new();
        let available_skills = skill_loader.load_all().unwrap_or_else(|e| {
            tracing::warn!("Failed to reload skills: {}", e);
            Vec::new()
        });
        let skill_palette = SkillPalette::new(available_skills.clone());
        let plugin_manager = Arc::new(RwLock::new(PluginManager::default()));
        let plugin_manager_ui = PluginManagerUI::new();

        let input_handler = InputHandler::new();

        // Brutalist mode for tests (use default config value)
        let renderer_mode = RendererMode::from_brutalist(TUIConfig::default().ui.brutalist_mode);

        Self {
            message_renderer: MessageRenderer::new(),
            execution_trace: None,
            input_handler,
            animator: Animator::new(4, false),
            services,
            messages: Vec::new(),
            _input_state: InputState::new(),
            input_mode: InputMode::SingleLine,
            running: true,
            scroll_offset_line: 0,
            selected_message: 0,
            viewport_height: 20,
            last_total_lines: std::cell::Cell::new(0),
            messages_area: std::cell::Cell::new(Rect::default()),
            sidebar_area: std::cell::Cell::new(Rect::default()),
            mouse_selection_start: std::cell::Cell::new(None),
            mouse_selection_dragged: std::cell::Cell::new(false),
            user_scrolled: false,
            last_user_scroll_time: Instant::now(),
            current_stream_content: String::new(),
            streaming_render_buffer:
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new(),
            is_streaming: false,
            stream_cancelled: false,
            queued_message: None,
            pending_bash_result: Arc::new(std::sync::Mutex::new(None)),
            chunks_received: 0,
            thinking_chunks_received: 0,
            stream_start_time: None,
            last_response_duration: None,
            plan_mode_banner: None,
            active_tools: std::collections::HashMap::new(),
            workspace_loaded: false,
            workspace_context: None,
            workspace_tasks: load_tasks(),
            pipeline: {
                let mut p = crate::app::pipeline::registry::PipelineRegistry::new();
                let browser_manager =
                    Arc::new(crate::app::pipeline::browser_manager::BrowserManager::new());
                p.tool_registry.register(
                    "browser",
                    "goto",
                    Arc::new(
                        crate::app::pipeline::tools::browser_tools::BrowserGotoTool::new(
                            browser_manager.clone(),
                        ),
                    ),
                );
                p.tool_registry.register(
                    "browser",
                    "extract",
                    Arc::new(
                        crate::app::pipeline::tools::browser_extract::BrowserExtractTool::new(
                            browser_manager,
                        ),
                    ),
                );

                p.register_factory(
                    "rustycode::steps::DataGateStep",
                    Box::new(crate::app::pipeline::steps::data_gate_factory::DataGateFactory),
                );
                p.register_factory(
                    "rustycode::steps::AgentStep",
                    Box::new(crate::app::pipeline::steps::agent_factory::AgentStepFactory),
                );
                p
            },
            pipeline_ctx: {
                let (pt, mdl, _) =
                    rustycode_llm::load_provider_config_from_env().unwrap_or_else(|_| {
                        (
                            "anthropic".into(),
                            "claude-haiku-4-5-20251001".into(),
                            Default::default(),
                        )
                    });
                let pipeline_provider =
                    rustycode_llm::create_provider(&pt, &mdl).unwrap_or_else(|_| {
                        std::sync::Arc::new(rustycode_llm::mock::MockProvider::from_text(
                            "mock result",
                        ))
                    });
                crate::app::pipeline::registry::PipelineContext::new(
                    pipeline_provider,
                    rustycode_agent::AgentConfig::default(),
                    mdl,
                    crate::app::pipeline::tool_registry::ToolRegistry::new(),
                )
            },
            pipeline_guardian: crate::app::pipeline::guardian::PipelineGuardian::new(),
            scheduler_rx: None,
            scheduler_tx: None,
            active_scheduled_phases: std::collections::HashSet::new(),
            max_concurrent_phases: 3,
            last_extraction: None,
            workspace_scan_progress: None,
            git_branch: None,
            rate_limit: RateLimitHandler::new(),
            auto_continue_enabled: std::env::var("RUSTYCODE_AUTO_CONTINUE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            auto_continue_pending: false,
            auto_continue_iterations: 0,
            turn_snapshot: None,
            doom_loop: crate::app::doom_loop::DoomLoopDetector::new(),
            reasoning_budget: std::sync::Mutex::new(
                rustycode_tools::providers::reasoning_types::BudgetState::default(),
            ),
            dirty: true,
            needs_full_redraw: false,
            context_monitor,
            theme_colors,
            compaction_config,
            showing_compaction_preview: false,
            pending_compaction: false,
            auto_memory,
            memory_injection_config,
            skill_palette,
            skill_manager: Arc::new(RwLock::new(SkillStateManager::new())),
            plugin_manager,
            plugin_manager_ui,
            showing_plugin_manager: false,
            showing_marketplace_browser: false,
            help_state: HelpState::new(),
            tool_approval: ToolApprovalManager::new(),
            pending_approval_request: VecDeque::new(),
            awaiting_approval: false,
            start_time: Instant::now(),
            last_lsp_refresh: Instant::now() - Duration::from_mins(1),
            last_lsp_servers: Vec::new(),
            last_lsp_connected: false,
            last_mcp_refresh: Instant::now() - Duration::from_mins(1),
            last_mcp_servers: Vec::new(),
            last_mcp_connected: false,
            theme_preview,
            theme_switcher,
            toast_manager,
            error_manager,
            showing_error: false,
            showing_tool_panel: false,
            tool_panel_history: Vec::new(),
            tool_panel_selected_index: None,
            showing_tool_result: false,
            tool_result_show_full: false,
            tool_result_scroll_offset: 0,
            last_esc_press: None,
            stashed_prompt: None,
            command_palette,
            showing_command_palette: false,
            showing_skill_palette: false,
            status_bar_collapsed: false,
            footer_collapsed: false,
            model_selector: ModelSelector::with_models(get_all_available_models()),
            file_selector: FileSelector::new(Vec::new()),
            showing_provider_selector: false,
            current_model: rustycode_llm::load_model_from_config().unwrap_or_else(|e| {
                tracing::warn!("Failed to load model config: {}", e);
                String::new()
            }),
            session_sidebar: SessionSidebar::new(),
            session_recovery:
                crate::app::session_recovery_integration::SessionRecoveryManager::new(
                    crate::app::session_recovery_integration::SessionRecoveryConfig::default(),
                )
                .ok(),
            message_areas: std::cell::RefCell::new(Vec::new()),
            message_line_offsets: std::cell::RefCell::new(Vec::new()),
            agent_manager: AgentManager::new(),
            wizard: WizardHandler::new(&PathBuf::from("."), false),
            tui_config: TUIConfig::default(),
            keyboard_handler: KeyboardShortcutHandler::new(false),
            undo_stack: VecDeque::with_capacity(5),
            file_undo_stack: Vec::new(),
            file_finder: crate::ui::file_finder::FileFinder::new(PathBuf::from(".")),
            search_state: SearchState::new(),
            tag_filter: TagFilter::new(),
            renderer_mode,
            mcp_proxies: None,
            todo_state: rustycode_tools::todo::new_todo_state(),
            team_panel: crate::ui::team_panel::TeamPanel::new(),
            team_handler: TeamModeHandler::new(),
            clarification_panel: crate::ui::clarification::ClarificationPanel::hidden(),
            awaiting_clarification: false,
            // Worker panel (sub-agent orchestration)
            worker_panel: crate::ui::worker_panel::WorkerPanel::new(),
            // AST pipeline phase progress
            ast_phase_state: crate::ui::ast_progress::AstPhaseState::new(),
            // Session token usage and cost tracking
            session_input_tokens: 0,
            session_output_tokens: 0,
            session_cache_read_tokens: 0,
            session_cache_creation_tokens: 0,
            last_turn_input_tokens: 0,
            session_cost_usd: 0.0,
            cost_tracker: rustycode_llm::cost_tracker::CostTracker::new(None),
            hook_manager: rustycode_tools::hooks::HookManager::new(
                PathBuf::from(".rustycode/hooks"),
                rustycode_tools::hooks::HookProfile::Standard,
                String::new(),
            ),
            plan_mode: {
                use rustycode_orchestration::plan_mode::{PlanMode, PlanModeConfig};
                use rustycode_protocol::AgentRole;

                let mut plan_mode = PlanMode::new(PlanModeConfig::default());
                plan_mode.set_role(AgentRole::Worker);
                plan_mode
            },
            // Cached API key warning
            api_key_warning: String::new(),
            event_receiver: tokio::sync::broadcast::channel(16).1,
            marketplace_browser,
        }
    }

    /// Initialize all background services
    pub fn init_services(&mut self) -> Result<()> {
        crate::info_log!("init_services starting");
        let config = ConversationConfig::default();
        let mut tool_registry = ToolRegistry::new();

        // Register built-in tools - these are essential for AI coding assistant functionality
        self.register_builtin_tools(&mut tool_registry);

        // Register structured thinking tool for AgentSession path
        tool_registry.register(
            rustycode_orchestration::structured_thinking_tool_impl::StructuredThinkingTool::new(
                None,
            ),
        );

        // Load MCP tools if MCP servers are configured
        self.load_mcp_tools(&mut tool_registry);

        // Count tools before moving registry
        let tool_count = tool_registry.list().len();

        self.services.start_conversation(config, tool_registry)?;
        crate::info_log!(
            "start_conversation OK, pipeline={}",
            self.services.has_pipeline()
        );
        self.services.start_workspace_loading()?;

        self.refresh_mcp_status(true);

        // Wire shared todo state into service manager so LLM can use todo tools
        self.services.set_todo_state(self.todo_state.clone());

        tracing::info!("Services initialized with {} tools", tool_count);

        Ok(())
    }

    /// Refresh sidebar LSP status from discovery.
    fn refresh_lsp_status(&mut self, force: bool) -> bool {
        if !force && self.last_lsp_refresh.elapsed() < Duration::from_secs(30) {
            return false;
        }

        let candidate_servers = default_lsp_servers();
        let statuses = discover_lsp_servers(&candidate_servers);
        let lsp_connected = statuses.iter().any(|status| status.installed);

        let active_clients = rustycode_tools::providers::lsp::active_clients_status();
        let any_running = active_clients.iter().any(|(_, state)| state == "running");

        let mut lsp_names: Vec<String> = if active_clients.is_empty() {
            statuses
                .into_iter()
                .filter(|status| status.installed)
                .map(|status| status.name)
                .collect()
        } else {
            active_clients
                .iter()
                .map(|(name, state)| {
                    if state == "running" {
                        format!("✓ {name}")
                    } else {
                        format!("○ {name} ({state})")
                    }
                })
                .collect()
        };

        if lsp_names.is_empty() {
            lsp_names.push("No LSP servers detected".to_string());
        }

        let display_connected = any_running || lsp_connected;

        let changed = force
            || display_connected != self.last_lsp_connected
            || lsp_names != self.last_lsp_servers;

        if changed {
            self.session_sidebar.update_lsp_status(
                display_connected,
                lsp_names.clone(),
                std::collections::HashMap::new(),
            );
            self.last_lsp_connected = display_connected;
            self.last_lsp_servers = lsp_names;
            self.dirty = true;
        }

        self.last_lsp_refresh = Instant::now();
        changed
    }

    /// Refresh sidebar MCP status from the live proxy cache and config discovery.
    fn refresh_mcp_status(&mut self, force: bool) -> bool {
        if !force && self.last_mcp_refresh.elapsed() < Duration::from_secs(30) {
            return false;
        }

        let connected_servers: HashSet<String> = if let Some(mcp_proxies) = &self.mcp_proxies {
            let proxies = mcp_proxies.clone();
            rustycode_shared_runtime::SHARED_RUNTIME.block_on(async move {
                let proxies = proxies.read().await;
                let snapshot: Vec<(String, rustycode_mcp::proxy::ToolProxy)> = proxies
                    .iter()
                    .map(|(server_id, proxy)| (server_id.clone(), proxy.clone()))
                    .collect();

                let mut connected = HashSet::new();
                for (server_id, proxy) in snapshot {
                    if proxy.is_connected().await {
                        connected.insert(server_id);
                    }
                }
                connected
            })
        } else {
            HashSet::new()
        };

        let mut seen = HashSet::new();
        let mut mcp_servers = Vec::new();

        for (config_path, config_file) in
            rustycode_mcp::McpConfigFile::load_from_standard_locations()
        {
            for (server_id, server_config) in config_file.servers {
                if !seen.insert(server_id.clone()) {
                    continue;
                }

                let (state, detail) = if !server_config.enabled {
                    (
                        McpServerState::Disabled,
                        Some("disabled in config".to_string()),
                    )
                } else if server_config.command.is_some() {
                    if connected_servers.contains(&server_id) {
                        (McpServerState::Connected, Some("stdio".to_string()))
                    } else {
                        (McpServerState::Disconnected, Some("stdio".to_string()))
                    }
                } else if server_config.url.is_some() {
                    (
                        McpServerState::Remote,
                        server_config.url.clone().or(Some("remote".to_string())),
                    )
                } else {
                    (
                        McpServerState::Configured,
                        Some(format!("from {}", config_path.display())),
                    )
                };

                mcp_servers.push(McpServerStatus {
                    name: server_id,
                    state,
                    detail,
                });
            }
        }

        if mcp_servers.is_empty() && self.mcp_proxies.is_some() {
            mcp_servers.push(McpServerStatus {
                name: "No MCP servers configured".to_string(),
                state: McpServerState::Configured,
                detail: None,
            });
        }

        let mcp_connected = mcp_servers
            .iter()
            .any(|server| matches!(server.state, McpServerState::Connected));

        let changed = force
            || mcp_connected != self.last_mcp_connected
            || mcp_servers != self.last_mcp_servers;

        if changed {
            self.session_sidebar
                .update_mcp_status(mcp_connected, mcp_servers.clone());
            self.last_mcp_connected = mcp_connected;
            self.last_mcp_servers = mcp_servers;
            self.dirty = true;
        }

        self.last_mcp_refresh = Instant::now();
        changed
    }

    /// Refresh sidebar tool call summary from the current tool history.
    fn refresh_tool_call_summary(&mut self) {
        let recent = self
            .tool_panel_history
            .last()
            .map(|tool| format!("{} {}", tool.status.icon(), tool.result_summary));
        self.session_sidebar
            .update_tool_call_summary(self.active_tools.len(), recent);
    }

    /// Resume the most recent session from disk.
    ///
    /// Called when `--resume` flag is passed on the CLI. Finds the most
    /// recently saved session and loads its messages/scroll state.
    pub fn resume_most_recent_session(&mut self) {
        if let Some(ref recovery) = self.session_recovery {
            match recovery.list_recoverable_sessions() {
                Ok(sessions) => {
                    if sessions.is_empty() {
                        self.add_system_message("No previous sessions found to resume".to_string());
                        return;
                    }

                    // Try sessions in order, load the first one that works
                    for session_id in &sessions {
                        if let Ok(state) = recovery.load_state(session_id) {
                            let msg_count = state.messages.len();
                            if msg_count == 0 {
                                continue;
                            }

                            let age = chrono::Utc::now()
                                .signed_duration_since(state.last_saved)
                                .num_minutes();

                            // Reset session state for clean load
                            self.selected_message = 0;
                            self.scroll_offset_line = state.scroll_position;
                            self.user_scrolled = false;
                            self.active_tools.clear();
                            self.tool_panel_history.clear();
                            self.tool_panel_selected_index = None;
                            self.showing_tool_result = false;
                            // Reset streaming state (session could have been saved mid-stream)
                            self.reset_streaming_state();
                            self.queued_message = None;
                            // Restore messages
                            self.messages = state.messages;
                            // Recompute token context based on restored messages so the
                            // context usage bar reflects the loaded session.
                            self.context_monitor.update(&self.messages);
                            if !self.messages.is_empty() {
                                self.selected_message = self.messages.len().saturating_sub(1);
                            }

                            self.add_system_message(format!(
                                "Resumed session '{}' ({} messages, {} min ago)",
                                session_id.split('-').next().unwrap_or(session_id),
                                msg_count,
                                age
                            ));
                            self.dirty = true;
                            tracing::info!(
                                "Resumed session {} ({} messages)",
                                session_id,
                                msg_count
                            );
                            return;
                        }
                    }

                    self.add_system_message("Could not load any saved sessions".to_string());
                }
                Err(e) => {
                    tracing::warn!("Failed to list sessions for resume: {}", e);
                    self.add_system_message("Could not find saved sessions".to_string());
                }
            }
        } else {
            self.add_system_message("Session persistence not available".to_string());
        }
    }

    /// Register all built-in tools for AI coding assistant functionality
    fn register_builtin_tools(&self, tool_registry: &mut ToolRegistry) {
        use crate::skills::as_tool::{CreateCronTool, CreateTeamTool, SkillToolRegistry};
        use rustycode_tools::todo::{TodoWriteTool, TodoUpdateTool};
        use rustycode_tools::todo_read::TodoReadTool;
        #[cfg(feature = "vector-memory")]
        use rustycode_tools::SemanticSearchTool;

        // Register all zero-config built-in tools from default registry
        *tool_registry = rustycode_tools::default_registry();

        // Register stateful tools that require runtime state
        // Todo tools (shared state with TUI sidebar)
        tool_registry.register(TodoReadTool::new(self.todo_state.clone()));
        tool_registry.register(TodoWriteTool::new(self.todo_state.clone()));
        tool_registry.register(TodoUpdateTool::new(self.todo_state.clone()));

        // Semantic search tool (conditional feature)
        #[cfg(feature = "vector-memory")]
        tool_registry.register(SemanticSearchTool::new(
            &std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        ));

        // Agent tool - functional sub-agent backed by AgentSession.
        // Build full tool definitions (name + description + input_schema) for sub-agent.
        let tools_schema: Vec<serde_json::Value> = tool_registry
            .list()
            .iter()
            .map(|t| {
                let mut schema = serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters_schema,
                });
                if let Some(annotations) = anthropic_annotations_for_tool_info(
                    &t.name,
                    matches!(t.permission, rustycode_tools::ToolPermission::Read),
                ) {
                    schema["annotations"] = annotations;
                }
                schema
            })
            .collect();
        let tools_schema_clone = tools_schema.clone();
        let agent_tool = crate::agents::agent_tool::AgentTool::new(
            Arc::clone(&self.pipeline_ctx.provider),
            self.pipeline_ctx.current_model.clone(),
            self.services.cwd().clone(),
            tools_schema,
        );
        tool_registry.register(agent_tool);

        // Delegation executor — real sub-agent execution for delegate_task tool.
        let delegation_executor = crate::agents::delegation_executor::DelegationExecutor::new(
            Arc::clone(&self.pipeline_ctx.provider),
            self.pipeline_ctx.current_model.clone(),
            self.services.cwd().clone(),
            tools_schema_clone,
        );
        tool_registry.register(delegation_executor);

        // Team management tool - allows LLM to create agent teams
        tool_registry.register(CreateTeamTool::new());

        // Cron scheduling tool - allows LLM to create scheduled tasks
        tool_registry.register(CreateCronTool::new());

        // Register skill-as-tool wrappers for active skills
        let skill_tool_registry = SkillToolRegistry::new(self.skill_manager.clone());
        let skill_tools = skill_tool_registry.build_tools();
        for skill_tool in skill_tools {
            tool_registry.register_boxed(skill_tool);
        }

        tracing::info!("Registered {} built-in tools", tool_registry.list().len());
    }

    /// Load tools from configured MCP servers
    fn load_mcp_tools(&mut self, tool_registry: &mut ToolRegistry) {
        use rustycode_mcp::proxy::{ProxyConfig, ToolProxy};
        use rustycode_mcp::McpConfigFile;
        use std::sync::Arc;

        // Load MCP config from all standard locations
        let configs = McpConfigFile::load_from_standard_locations();

        if configs.is_empty() {
            tracing::debug!("No MCP server configs found in standard locations");
            return;
        }

        // Collect existing built-in tool names for overlap detection
        let builtin_names: std::collections::HashSet<String> = tool_registry
            .list()
            .iter()
            .map(|t| t.name.clone())
            .collect();

        // Known semantic equivalents: MCP tool name → built-in tool it duplicates.
        // These are common filesystem MCP server tools that overlap with built-in tools.
        let overlap_map: std::collections::HashMap<&str, &str> = [
            // @modelcontextprotocol/server-filesystem equivalents
            ("read_text_file", "read_file"),
            ("write_file", "write_file"),
            ("list_directory", "list_dir"),
            ("list_allowed_directories", "__skip__"), // no built-in equivalent, wastes turns
            ("search_files", "grep"),
            ("get_file_info", "__skip__"), // no useful equivalent
            ("create_directory", "bash"),  // mkdir via bash
            ("move_file", "bash"),         // mv via bash
            ("read_multiple_files", "read_file"), // can read files individually
            // Other common MCP servers
            ("directory_tree", "list_dir"),
            ("read_file", "read_file"), // exact overlap
        ]
        .into_iter()
        .collect();

        // Create a shared proxy cache for the session so MCP connections stay
        // alive for the full TUI lifecycle and can be shut down explicitly.
        let proxy_cache = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::<
            String,
            ToolProxy,
        >::new()));
        self.mcp_proxies = Some(proxy_cache.clone());

        // Use the shared persistent runtime so MCP child process handles
        // (ChildStdin/ChildStdout) remain valid for the entire session.
        // A short-lived local runtime would drop its I/O driver, killing
        // the MCP transport — see Bug #11.
        use rustycode_shared_runtime::SHARED_RUNTIME;

        // Load and start servers from all config files
        let mut started_count = 0;
        let mut tools_registered = 0;
        let mut tools_skipped = 0;
        for (config_path, config_file) in configs {
            tracing::info!("Loading MCP servers from {:?}", config_path);

            for (server_id, server_config) in config_file.servers {
                tracing::info!("Starting MCP server '{}'", server_id);

                // Create a tool proxy for this server (stdio only)
                let command = match server_config.command.clone() {
                    Some(cmd) => cmd,
                    None => {
                        tracing::debug!(
                            "Skipping MCP server '{}': no command (remote transport)",
                            server_id
                        );
                        continue;
                    }
                };
                let proxy_config = ProxyConfig {
                    server_name: server_id.clone(),
                    command,
                    args: server_config.args.clone(),
                    tool_prefix: None,
                    cache_tools: true,
                };

                match SHARED_RUNTIME.block_on(ToolProxy::with_discovery(proxy_config)) {
                    Ok(proxy) => {
                        tracing::info!("MCP server '{}' connected successfully", server_id);
                        started_count += 1;

                        // Keep the live proxy around for the rest of the session.
                        let proxy_for_cache = proxy.clone();
                        let proxy_cache_clone = proxy_cache.clone();
                        let server_id_for_cache = server_id.clone();
                        SHARED_RUNTIME.block_on(async move {
                            let mut cache = proxy_cache_clone.write().await;
                            cache.insert(server_id_for_cache, proxy_for_cache);
                        });

                        // Get all tools from the proxy and register them
                        let proxied_tools = SHARED_RUNTIME.block_on(proxy.get_tools());
                        for proxied_tool in proxied_tools {
                            let tool_name = proxied_tool.name.clone();

                            // Skip MCP tools that duplicate built-in functionality
                            if let Some(equivalent) = overlap_map.get(tool_name.as_str()) {
                                if *equivalent == "__skip__" {
                                    tracing::warn!(
                                        "Skipping MCP tool '{}' (no useful equivalent, wastes LLM turns)",
                                        tool_name
                                    );
                                    tools_skipped += 1;
                                    continue;
                                }
                                if builtin_names.contains(*equivalent) {
                                    tracing::warn!(
                                        "Skipping MCP tool '{}' — built-in '{}' already registered (semantic overlap)",
                                        tool_name,
                                        equivalent
                                    );
                                    tools_skipped += 1;
                                    continue;
                                }
                            }

                            // Also skip exact name collisions
                            if builtin_names.contains(&tool_name) {
                                tracing::warn!(
                                    "Skipping MCP tool '{}' — already registered as built-in",
                                    tool_name
                                );
                                tools_skipped += 1;
                                continue;
                            }

                            tool_registry.register(proxied_tool);
                            tracing::debug!("  Registered MCP tool: {}", tool_name);
                            tools_registered += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to connect to MCP server '{}': {}", server_id, e);
                    }
                }
            }
        }

        if started_count > 0 {
            tracing::info!(
                "Started {} MCP server(s): {} tools registered, {} skipped (overlap with built-in tools)",
                started_count,
                tools_registered,
                tools_skipped
            );
        }
    }

    /// Check for tmux compatibility and add warning messages if needed
    #[allow(dead_code)]
    pub(crate) fn check_tmux_compatibility(&mut self) {
        if std::env::var("TMUX").is_err() {
            return;
        }

        use std::process::Command;

        // Check escape-time
        let escape_time = Command::new("tmux")
            .args(["show-options", "-gv", "escape-time"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u32>().ok());

        if let Some(et) = escape_time {
            if et > 50 {
                self.add_system_message(format!(
                    "⚠️ High tmux escape-time detected ({}ms). ESC key may feel sluggish. Recommend: set -sg escape-time 0",
                    et
                ));
            }
        }

        let mouse = Command::new("tmux")
            .args(["show-options", "-gv", "mouse"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim() == "on");

        if let Some(false) = mouse {
            let enabled = Command::new("tmux")
                .args(["set-option", "-g", "mouse", "on"])
                .output()
                .is_ok();
            if enabled {
                self.add_system_message(
                    "🖱️ Enabled tmux mouse support for scroll wheel. (Changed: set -g mouse on)"
                        .to_string(),
                );
            } else {
                self.add_system_message(
                    "⚠️ Tmux mouse support is off. Scrolling may not work. Recommend: set -g mouse on"
                        .to_string(),
                );
            }
        }

        // Check focus-events
        let focus_events = Command::new("tmux")
            .args(["show-options", "-gv", "focus-events"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim() == "on");

        if let Some(false) = focus_events {
            self.add_system_message(
                "⚠️ Tmux focus-events is off. TUI may not detect when you switch windows. Recommend: set -g focus-events on"
                    .to_string(),
            );
        }

        // Check for Ctrl+B clash
        self.add_system_message(
            "💡 Inside tmux: Use Ctrl+L as an alternative to Ctrl+B for toggling the sidebar."
                .to_string(),
        );

        self.dirty = true;
    }

    /// Run the TUI main loop
    pub fn run(&mut self) -> Result<()> {
        // Install panic hook FIRST - before any terminal operations
        install_panic_hook();

        tracing::info!("TUI run() — setting up terminal");

        // Setup terminal with automatic cleanup guard
        let stdout = std::io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).map_err(|e| {
            tracing::error!("Failed to create terminal backend: {}", e);
            e
        })?;

        tracing::info!("TUI run() — entering alternate screen");

        // Clear screen and setup terminal
        //
        // Mouse capture ENABLED so we can handle scroll wheel events and app-owned
        // drag selection for panel-aware clipboard copy.
        //
        // For text selection: click and drag copies the active panel text.
        // For scroll: Mouse wheel/trackpad works via captured events.
        //
        // Setup terminal:
        // - EnterAlternateScreen: isolates the TUI from the shell history.
        // - EnableBracketedPaste: ensures pasted text is handled as a single block.
        // - EnableMouseCapture: Enables scroll wheel support.
        //
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableBracketedPaste,
            crossterm::event::EnableMouseCapture,
        )
        .map_err(|e| {
            tracing::error!("Failed to enter alternate screen: {}", e);
            e
        })?;
        terminal.clear().map_err(|e| {
            tracing::error!("Failed to clear terminal: {}", e);
            e
        })?;
        crossterm::terminal::enable_raw_mode().map_err(|e| {
            tracing::error!("Failed to enable raw mode: {}", e);
            e
        })?;

        // Set terminal title to project name (Goose pattern for tab identification)
        if let Some(dir_name) = self.services.cwd().file_name().and_then(|n| n.to_str()) {
            // Sanitize: strip control characters to prevent terminal escape injection
            let sanitized: String = dir_name.chars().filter(|c| !c.is_control()).collect();
            // OSC 0 sets the terminal window/tab title
            print!("\x1b]0;rustycode: {}\x07", sanitized);
            let _ = std::io::stdout().flush();
        }

        // Create cleanup guard that runs on drop (even on panic)
        let _cleanup_guard = TerminalCleanupGuard;

        // Setup signal handlers for graceful shutdown
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let shutdown_tx_clone = shutdown_tx.clone();

        tracing::info!("TUI run() — setting up signal handler");

        ctrlc::set_handler(move || {
            let _ = shutdown_tx_clone.send(());
        })
        .map_err(|e| {
            tracing::error!("Failed to set Ctrl+C handler: {}", e);
            e
        })?;

        tracing::info!("TUI run() — entering event loop");

        // Cleanup happens automatically when _cleanup_guard goes out of scope

        let mut startup_notes: Vec<String> = Vec::new();

        if let Some(ref recovery) = self.session_recovery {
            if let Err(e) = recovery.init_session() {
                tracing::warn!("Session recovery init failed: {}", e);
            }

            if let Ok(Some(state)) = recovery.check_crash_recovery() {
                let msg_count = state.messages.len();
                let age = chrono::Utc::now()
                    .signed_duration_since(state.last_saved)
                    .num_minutes();
                startup_notes.push(format!(
                    "session recoverable ({} msgs, {}m ago) — /resume to load",
                    msg_count, age
                ));
            }
        }

        if std::env::var("TMUX").is_ok() {
            use std::process::Command;

            let escape_time = Command::new("tmux")
                .args(["show-options", "-gv", "escape-time"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| s.trim().parse::<u32>().ok());

            if let Some(et) = escape_time {
                if et > 50 {
                    startup_notes
                        .push(format!("tmux escape-time {}ms — set -sg escape-time 0", et));
                }
            }

            let focus_events = Command::new("tmux")
                .args(["show-options", "-gv", "focus-events"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim() == "on");

            if focus_events.as_ref() == Some(&false) {
                startup_notes.push("focus-events off — set -g focus-events on".to_string());
            }
        }

        if !startup_notes.is_empty() {
            self.add_system_message(startup_notes.join(" · "));
        }

        self.event_loop(&mut terminal, shutdown_rx)?;

        tracing::info!("Event loop exited normally");

        // Print session summary to terminal (after event loop exits, before cleanup guard drops)
        self.print_session_summary();

        Ok(())
    }

    /// Main event loop with one-item-per-frame processing
    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Result<()> {
        let mut last_frame_time = Instant::now();
        let mut frame_count: u32 = 0;
        let mut loop_iterations: u64 = 0;

        while self.running {
            loop_iterations += 1;

            // Check for shutdown signal (Ctrl+C)
            if shutdown_rx.try_recv().is_ok() {
                // User requested shutdown
                self.running = false;
                break;
            }

            let frame_start = Instant::now();
            let debug_enabled = crate::logging::is_debug_enabled();

            // Calculate delta time for animations (in milliseconds)
            let delta_ms = last_frame_time.elapsed().as_millis() as u64;
            last_frame_time = frame_start;

            // Phase 1: Update animations (only marks dirty when frame actually advances)
            let animation_start = Instant::now();
            if self.animator.update() {
                // Only mark dirty if an animation is visible (streaming or active tools)
                if self.is_streaming || !self.active_tools.is_empty() {
                    self.dirty = true;
                }
            }
            let animation_elapsed = animation_start.elapsed();

            // Update session sidebar info
            let sidebar_start = Instant::now();
            self.session_sidebar
                .update_session_info(self.messages.len(), self.active_tools.len());
            self.session_sidebar
                .set_rate_limited(self.rate_limit.until.is_some());
            self.refresh_tool_call_summary();
            self.refresh_lsp_status(false);
            self.refresh_mcp_status(false);
            let sidebar_elapsed = sidebar_start.elapsed();

            // Update toast animations
            let toast_start = Instant::now();
            let has_active_toasts = self.toast_manager.tick(delta_ms);
            if has_active_toasts {
                self.dirty = true; // Mark dirty for animation updates
            }
            let toast_elapsed = toast_start.elapsed();

            // Error auto-dismiss: If error_manager is showing, mark dirty so
            // the next render can check is_showing() and clear the error overlay
            // after the auto-dismiss timeout (10s). Without this, the error
            // indicator persists indefinitely when no other state changes occur.
            if self.error_manager.is_showing() {
                self.dirty = true;
            }

            // Phase 2: Poll async sources (ONE item each)
            let service_poll_start = Instant::now();
            self.poll_services()?;
            self.poll_mcp_events()?;
            {
                use rustycode_shared_runtime::SHARED_RUNTIME;
                let pipeline_tick_start = Instant::now();
                SHARED_RUNTIME.block_on(self.tick_pipeline())?;
                let pipeline_tick_elapsed = pipeline_tick_start.elapsed();
                if debug_enabled && pipeline_tick_elapsed > Duration::from_millis(2) {
                    crate::debug_log!(
                        "Pipeline tick ran long: {} ms",
                        pipeline_tick_elapsed.as_millis()
                    );
                }
            }
            let pipeline_monitor_start = Instant::now();
            self.pipeline_guardian
                .monitor(&self.pipeline, &self.pipeline_ctx)?;
            let pipeline_monitor_elapsed = pipeline_monitor_start.elapsed();
            let service_poll_elapsed = service_poll_start.elapsed();

            // Phase 2.5: Update countdowns (rate limit, agents, etc.)
            let countdown_start = Instant::now();
            if self.update_rate_limit_countdown() {
                self.dirty = true; // Mark dirty if countdown updated
            }
            let countdown_elapsed = countdown_start.elapsed();

            // Update running agents
            let agents_start = Instant::now();
            self.agent_manager.update_running_agents();

            // Periodic cleanup: remove completed/failed agents older than 1 hour
            // and cap total terminal agents at 50
            self.agent_manager.cleanup_old_agents(3600);
            self.agent_manager.cleanup_excess_agents(50);
            let agents_elapsed = agents_start.elapsed();

            // Session auto-save (every 30s when dirty)
            let autosave_start = Instant::now();
            if let Some(ref mut recovery) = self.session_recovery {
                if recovery.should_auto_save() {
                    let state = recovery.create_state(
                        &self.messages,
                        self.scroll_offset_line,
                        self.execution_trace.clone(),
                    );
                    if let Err(e) = recovery.save_state(&state) {
                        tracing::warn!("Session auto-save failed: {}", e);
                    }
                }
            }
            let autosave_elapsed = autosave_start.elapsed();

            // Phase 3: Check frame budget
            let elapsed = frame_start.elapsed();

            let mut rendered = false;
            let mut render_elapsed = Duration::ZERO;
            let mut input_handle_elapsed = Duration::ZERO;
            let mut input_polled = false;
            let mut input_handled = false;

            let input_poll_elapsed = if elapsed < FRAME_BUDGET_60FPS {
                // Phase 4: Render (only if dirty)
                // dirty is set to true when new content arrives, so no need to check is_streaming
                let should_render = self.dirty || frame_count < 3;

                if should_render {
                    let render_start = Instant::now();
                    if self.needs_full_redraw {
                        terminal.clear()?;
                        self.needs_full_redraw = false;
                    }
                    terminal.draw(|f| self.render(f))?;
                    frame_count += 1;
                    self.dirty = false;
                    render_elapsed = render_start.elapsed();
                    rendered = true;
                }

                // Phase 5: Handle input with remaining time
                // poll() blocks for up to `timeout`, consuming the remaining budget.
                // No additional sleep needed after this — poll handles the yield.
                let timeout = FRAME_BUDGET_60FPS.saturating_sub(frame_start.elapsed());

                let input_poll_start = Instant::now();
                if event::poll(timeout)? {
                    input_polled = true;
                    let input_handle_start = Instant::now();
                    self.handle_input()?;
                    input_handle_elapsed = input_handle_start.elapsed();
                    input_handled = true;
                }
                input_poll_start.elapsed()
            } else {
                // Frame over budget, skip render, handle input with small timeout
                // to prevent CPU spin when consistently over budget
                let input_poll_start = Instant::now();
                if event::poll(Duration::from_millis(1))? {
                    input_polled = true;
                    let input_handle_start = Instant::now();
                    self.handle_input()?;
                    input_handle_elapsed = input_handle_start.elapsed();
                    input_handled = true;
                }
                input_poll_start.elapsed()
            };

            let frame_elapsed = frame_start.elapsed();
            if debug_enabled
                && (rendered
                    || input_polled
                    || frame_elapsed >= FRAME_BUDGET_60FPS
                    || loop_iterations.is_multiple_of(120))
            {
                crate::debug_log!(
                    "TUI frame diagnostics frame={} loop_iter={} total_ms={} anim_ms={} sidebar_ms={} toast_ms={} services_ms={} pipeline_monitor_ms={} countdown_ms={} agents_ms={} autosave_ms={} render_ms={} input_poll_ms={} input_handle_ms={} dirty={} rendered={} input_polled={} input_handled={} messages={} tools={} streaming={} user_scrolled={} viewport_height={}",
                    frame_count,
                    loop_iterations,
                    frame_elapsed.as_millis(),
                    animation_elapsed.as_millis(),
                    sidebar_elapsed.as_millis(),
                    toast_elapsed.as_millis(),
                    service_poll_elapsed.as_millis(),
                    pipeline_monitor_elapsed.as_millis(),
                    countdown_elapsed.as_millis(),
                    agents_elapsed.as_millis(),
                    autosave_elapsed.as_millis(),
                    render_elapsed.as_millis(),
                    input_poll_elapsed.as_millis(),
                    input_handle_elapsed.as_millis(),
                    self.dirty,
                    rendered,
                    input_polled,
                    input_handled,
                    self.messages.len(),
                    self.active_tools.len(),
                    self.is_streaming,
                    self.user_scrolled,
                    self.viewport_height
                );
            }
        }

        // Cleanup: stop any active stream
        if self.is_streaming {
            self.services.request_stop_stream();
            self.stream_cancelled = true;
            // Don't set is_streaming=false here — let the async stream task's
            // Done handler clean up to avoid racing with channel receivers.
        }

        // Shutdown MCP servers to prevent orphaned child processes
        if let Some(mcp_proxies) = &self.mcp_proxies {
            let proxies = mcp_proxies.clone();
            // Spawn a small tokio runtime for async cleanup since we're in sync context.
            let _ = std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                if let Ok(rt) = rt {
                    rt.block_on(async move {
                        let snapshot: Vec<rustycode_mcp::proxy::ToolProxy> = {
                            let proxies = proxies.read().await;
                            proxies.values().cloned().collect()
                        };

                        for proxy in snapshot {
                            if let Err(e) = proxy.disconnect().await {
                                tracing::warn!("Error disconnecting MCP proxy: {}", e);
                            }
                        }
                    });
                }
            })
            .join();
        }

        // Reset terminal title on exit so it doesn't show stale rustycode state
        print!("\x1b]0;\x07");
        let _ = std::io::stdout().flush();

        // Save history on exit
        self.save_history();

        // Session recovery shutdown: save state and release lock
        if let Some(ref mut recovery) = self.session_recovery {
            let state = recovery.create_state(
                &self.messages,
                self.scroll_offset_line,
                self.execution_trace.clone(),
            );
            if let Err(e) = recovery.shutdown(&state) {
                tracing::warn!("Session recovery shutdown failed: {}", e);
            }
        }

        Ok(())
    }

    /// Handle bracketed paste event
    ///
    /// This handles paste from the terminal's native paste (Cmd+V, Ctrl+Shift+V).
    /// The entire pasted content is received at once, preventing multiple sends.
    pub(crate) fn handle_bracketed_paste(&mut self, content: &str) -> Result<()> {
        use crate::ui::input_state::InputMode;

        if content.is_empty() {
            return Ok(());
        }

        const MAX_BRACKETED_PASTE_BYTES: usize = 10 * 1024 * 1024;
        if content.len() > MAX_BRACKETED_PASTE_BYTES {
            self.add_system_message(format!(
                "Paste too large ({} bytes). Maximum is {} bytes.",
                content.len(),
                MAX_BRACKETED_PASTE_BYTES
            ));
            return Ok(());
        }

        // Check if content has newlines - if so, ensure we are in multiline mode
        // (but don't force it if it's already multiline)
        if content.contains('\n') && self.input_handler.state.mode == InputMode::SingleLine {
            self.input_handler.state.mode = InputMode::MultiLine;
            self.input_mode = InputMode::MultiLine;
        }

        let state = &mut self.input_handler.state;

        let normalized = content.replace("\r\n", "\n").replace('\r', "");
        let lines: Vec<&str> = normalized.split('\n').collect();

        if lines.len() == 1 {
            // Single line paste - just insert the string
            let text = lines[0];
            if state.cursor_row < state.lines.len() {
                let current_line = &mut state.lines[state.cursor_row];
                let cursor_col =
                    current_line.floor_char_boundary(state.cursor_col.min(current_line.len()));
                current_line.insert_str(cursor_col, text);
                state.cursor_col = cursor_col + text.len();
            }
        } else {
            // Multiline paste
            if state.cursor_row < state.lines.len() {
                let current_line = &state.lines[state.cursor_row];
                let cursor_col =
                    current_line.floor_char_boundary(state.cursor_col.min(current_line.len()));
                let before = current_line[..cursor_col].to_string();
                let after = current_line[cursor_col..].to_string();

                // Replace current line with "before" + first pasted line
                state.lines[state.cursor_row] = format!("{}{}", before, lines[0]);

                // Insert middle lines
                #[allow(clippy::needless_range_loop)]
                for i in 1..lines.len() - 1 {
                    state
                        .lines
                        .insert(state.cursor_row + i, lines[i].to_string());
                }

                // Last line: last pasted part + "after"
                let last_idx = lines.len() - 1;
                let last_pasted_part = lines[last_idx];
                state.lines.insert(
                    state.cursor_row + last_idx,
                    format!("{}{}", last_pasted_part, after),
                );

                // Move cursor to end of pasted content
                state.cursor_row += last_idx;
                state.cursor_col = last_pasted_part.len();
            }
        }

        self.dirty = true;
        Ok(())
    }

    /// Handle a slash command
    pub(crate) fn handle_slash_command(&mut self, input: &str) -> Result<()> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        // Handle /cost locally (needs TUI state not in CommandContext)
        if matches!(parts[0], "/cost" | "/usage") {
            self.handle_cost_command();
            self.dirty = true;
            self.auto_scroll();
            return Ok(());
        }

        if parts[0] == "/plan" {
            let task_text = if parts.len() > 1 {
                Some(parts[1..].join(" "))
            } else {
                None
            };

            if task_text.is_none() {
                let current = self.plan_mode.current_phase();
                match current {
                    "planning" => {
                        if let Err(e) = self.plan_mode.approve() {
                            tracing::warn!("Plan approval failed: {}", e);
                            self.add_system_message(format!("Plan approval failed: {}", e));
                            return Ok(());
                        }
                        self.plan_mode
                            .set_role(rustycode_protocol::AgentRole::Worker);
                        self.services
                            .set_ai_mode(crate::services::agent_mode::AiMode::Ask);
                        self.clear_plan_mode_banner();
                        self.add_system_message(
                            "Plan mode: switched to implementation phase".to_string(),
                        );
                    }
                    _ => {
                        self.plan_mode.reset();
                        self.plan_mode
                            .set_role(rustycode_protocol::AgentRole::Planner);
                        self.services
                            .set_ai_mode(crate::services::agent_mode::AiMode::Plan);
                        self.show_planning_banner("Manual");
                        self.add_system_message(
                            "Plan mode: switched to planning phase".to_string(),
                        );
                    }
                }
                self.dirty = true;
                return Ok(());
            }

            let task = task_text.unwrap_or_default();
            let current = self.plan_mode.current_phase();
            if current != "planning" {
                self.plan_mode.reset();
                self.plan_mode
                    .set_role(rustycode_protocol::AgentRole::Planner);
                self.services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Plan);
                self.show_planning_banner("Manual");
                self.add_system_message("Plan mode: switched to planning phase".to_string());
            }
            self.dirty = true;
            self.auto_scroll();
            self.services.send_message(task)?;
            return Ok(());
        }

        // Handle /r locally as regenerate alias (Goose pattern)
        if matches!(parts[0], "/r" | "/regen" | "/regenerate") {
            self.regenerate_last_response()?;
            return Ok(());
        }

        if let Some(command_tx) = self.services.command_sender() {
            let cwd = self.services.cwd().clone();
            let effect = dispatch_registered_slash_command(
                input,
                CommandContext {
                    cwd: &cwd,
                    command_tx,
                    workspace_tasks: &mut self.workspace_tasks,
                    messages: &mut self.messages,
                    current_stream_content: &mut self.current_stream_content,
                    is_streaming: &mut self.is_streaming,
                    last_extraction: &mut self.last_extraction,
                    services: &mut self.services,
                    agent_manager: &mut self.agent_manager,
                    memory_injection_config: &mut self.memory_injection_config,
                    theme_colors: &self.theme_colors,
                    skill_manager: &self.skill_manager,
                    plugin_manager: &self.plugin_manager,
                    running: &mut self.running,
                    context_monitor: &mut self.context_monitor,
                    compaction_config: &mut self.compaction_config,
                    showing_compaction_preview: &mut self.showing_compaction_preview,
                    pending_compaction: &mut self.pending_compaction,
                    file_undo_stack: &mut self.file_undo_stack,
                    session_input_tokens: self.session_input_tokens,
                    session_output_tokens: self.session_output_tokens,
                    session_cost_usd: self.session_cost_usd,
                    current_model: self.current_model.clone(),
                    session_start: self.start_time,
                },
            )?;

            if let Some(effect) = effect {
                self.apply_slash_command_effect(effect)?;
                return Ok(());
            }
        }

        let cmd = parts[0];

        {
            self.add_system_message(format!(
                "Unknown command: {}. Type /help for available commands.",
                cmd
            ));
        }

        self.dirty = true;
        self.auto_scroll();
        Ok(())
    }

    /// Render the active frame backend.
    pub(crate) fn render(&mut self, frame: &mut ratatui::Frame) {
        let debug_enabled = crate::logging::is_debug_enabled();
        let render_start = Instant::now();
        let renderer = self.renderer_mode;
        crate::app::renderer::FrameRenderer::render(renderer, self, frame);
        if debug_enabled {
            let elapsed = render_start.elapsed();
            if elapsed > Duration::from_millis(2) {
                crate::debug_log!(
                    "Frame renderer ran long: mode={} elapsed_ms={} size={}x{} messages={} streaming={} dirty={}",
                    renderer.label(),
                    elapsed.as_millis(),
                    frame.area().width,
                    frame.area().height,
                    self.messages.len(),
                    self.is_streaming,
                    self.dirty
                );
            }
        }
    }

    /// Render the polished layout.
    #[allow(unreachable_code)]
    pub(crate) fn render_polished(&mut self, frame: &mut ratatui::Frame) {
        let polished = crate::app::renderer::PolishedRenderer::from_tui(self, frame.area());
        polished.render(self, frame);
    }

    /// Render using brutalist renderer (complete UI)
    pub(crate) fn render_brutalist(&mut self, frame: &mut ratatui::Frame) {
        let debug_enabled = crate::logging::is_debug_enabled();
        let render_start = Instant::now();
        let input_text = self.input_handler.state.all_text();

        // Compute dynamic input area height based on content lines
        let input_line_count = if input_text.is_empty() {
            1
        } else {
            input_text.lines().count().max(1)
        };
        let input_rows: u16 = if input_line_count > 1 {
            2u16.saturating_add(input_line_count.min(6) as u16)
        } else {
            2
        };

        // Update viewport height from brutalist layout accounting for collapsed sections
        let size = frame.area();
        let header_rows: u16 = if self.status_bar_collapsed { 0 } else { 1 };
        let footer_rows: u16 = if self.footer_collapsed { 0 } else { 1 };
        let fixed_rows = header_rows + footer_rows + input_rows;
        let main_height = size.height.saturating_sub(fixed_rows);
        self.viewport_height = main_height.max(1) as usize;

        let mut message_area = Rect {
            x: size.x,
            y: header_rows, // After header (0 if collapsed)
            width: size.width,
            height: main_height,
        };
        let mut sidebar_area = None;
        if self.session_sidebar.is_visible() && message_area.width > 100 {
            let sidebar_width = (message_area.width / 3).clamp(24, 34);
            if message_area.width > sidebar_width {
                let content_width = message_area.width - sidebar_width;
                message_area.width = content_width;
                sidebar_area = Some(Rect {
                    x: size.x + content_width,
                    y: header_rows,
                    width: sidebar_width,
                    height: main_height,
                });
            }
        }
        self.sidebar_area.set(sidebar_area.unwrap_or_default());

        // Set messages area for mouse click detection (scroll-to-bottom indicator)
        self.messages_area.set(message_area);

        let renderer = self.create_brutalist_renderer(&input_text);

        // Compute message layout once — reused for rendering and click areas.
        // Using render_with_heights avoids recomputing heights inside render_messages.
        let layout_start = Instant::now();
        let width = message_area.width as usize;
        let (total_lines, heights, chain_map) = renderer.compute_message_layout(width);
        let layout_elapsed = layout_start.elapsed();

        // Render the complete brutalist UI with precomputed heights
        let draw_start = Instant::now();
        renderer.render_with_heights(frame, &heights, &chain_map, message_area);
        let draw_elapsed = draw_start.elapsed();

        if debug_enabled
            && (layout_elapsed > std::time::Duration::from_millis(1)
                || draw_elapsed > std::time::Duration::from_millis(2))
        {
            crate::debug_log!(
                "Brutalist breakdown: layout_ms={} draw_ms={} messages={} total_lines={} streaming={}",
                layout_elapsed.as_millis(),
                draw_elapsed.as_millis(),
                self.messages.len(),
                total_lines,
                self.is_streaming
            );
        }

        // Register message click areas for mouse interaction.
        // Uses the pre-computed heights to avoid redundant estimation.
        self.clear_message_areas();
        let main_height_click = size.height.saturating_sub(fixed_rows) as usize;
        let main_y = header_rows;
        let safe_viewport = main_height_click.max(1);

        // Save total lines for scroll operations (scroll_down_by, page_up, etc.)
        self.last_total_lines.set(total_lines);

        // Populate message_line_offsets from pre-computed heights so turn-based
        // navigation (Shift+Up/Down) can scroll to the correct position.
        // Without this, navigate_to_prev_turn/next_turn falls back to i*3 estimate.
        {
            let mut offsets = self.message_line_offsets.borrow_mut();
            offsets.clear();
            offsets.resize(self.messages.len(), 0);
            let mut acc = 0usize;
            for (msg_idx, &h) in heights.iter().enumerate() {
                offsets[msg_idx] = acc;
                acc += h;
            }
        }

        let max_scroll = total_lines.saturating_sub(safe_viewport);
        let effective_offset = if self.user_scrolled {
            self.scroll_offset_line.min(max_scroll)
        } else {
            max_scroll
        };

        let mut cum_line = 0usize;
        for (msg_idx, &h) in heights.iter().enumerate() {
            let end_line = cum_line + h;
            if end_line <= effective_offset {
                cum_line += h;
                continue;
            }
            if cum_line >= effective_offset + safe_viewport {
                break;
            }
            let vis_start = cum_line.saturating_sub(effective_offset);
            let vis_end = end_line.saturating_sub(effective_offset).min(safe_viewport);
            let vis_height = vis_end.saturating_sub(vis_start) as u16;
            if vis_height > 0 {
                let area = Rect {
                    x: message_area.x,
                    y: main_y + vis_start as u16,
                    width: message_area.width,
                    height: vis_height,
                };
                self.register_message_area(msg_idx, area);
            }
            cum_line += h;
        }

        if let Some(sidebar_area) = sidebar_area {
            self.session_sidebar.render(frame, sidebar_area);
        }

        // Overlay panels on top of brutalist UI

        // Worker status panel overlay (Ctrl+W)
        if self.worker_panel.visible {
            let panel_width = 50u16.min(size.width.saturating_sub(10));
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let x = size.width.saturating_sub(panel_width);
            let y = 2u16;
            let panel_area = Rect::new(x, y, panel_width, panel_height);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            self.worker_panel.render(panel_area, frame.buffer_mut());
        }

        // Team agent timeline overlay (Ctrl+G)
        if self.team_panel.visible {
            let panel_width = 60u16.min(size.width.saturating_sub(10));
            let panel_height = 20u16.min(size.height.saturating_sub(4));
            let x = size.width.saturating_sub(panel_width);
            let y = 2u16;
            let panel_area = Rect::new(x, y, panel_width, panel_height);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            frame.render_widget(self.team_panel.clone(), panel_area);
        }

        // Overlay: clarification panel (when AI asks a question)
        if self.awaiting_clarification && self.clarification_panel.visible {
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let panel_width = (size.width * 3 / 4).min(60);
            let panel_area =
                crate::app::render::shared::centered_rect(panel_width, panel_height, size);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            frame.render_widget(self.clarification_panel.clone(), panel_area);
        }

        // Overlay: search box (position at bottom of message area, not over footer)
        if self.search_state.visible {
            let search_area = Rect {
                x: size.x,
                y: header_rows,
                width: size.width,
                height: main_height,
            };
            crate::app::renderer::render_search_box(self, frame, search_area);
        }

        // Tool panel overlay (Ctrl+P) - over message area
        if self.showing_tool_panel {
            let tool_area = Rect {
                x: size.x,
                y: header_rows,
                width: size.width,
                height: main_height,
            };
            crate::app::renderer::render_tool_panel(self, frame, tool_area);
        }

        // Overlay: provider selector
        if self.showing_provider_selector {
            crate::app::renderer::render_provider_selector(frame);
        }

        // Overlay: file finder
        if self.file_finder.is_visible() {
            self.file_finder.render(frame, size);
        }

        // Overlay: model selector (Alt+P)
        if self.model_selector.is_visible() {
            self.model_selector.render(frame, size);
        }

        // Overlay: file selector (@)
        if self.file_selector.is_visible() {
            self.file_selector.render(frame, size);
        }

        // Overlay: skill palette
        if self.skill_palette.is_visible() {
            self.skill_palette.render(frame, size);
        }

        if self.showing_plugin_manager {
            let mut manager = self
                .plugin_manager
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let _ = manager.reload_from_disk();
            self.plugin_manager_ui.render(frame, size, &manager);
        }

        if self.showing_marketplace_browser {
            self.marketplace_browser.render(frame, size);
        }

        // Overlay: theme preview
        if self.theme_preview.is_visible() {
            self.theme_preview.render(frame, size);
        }

        // Overlay: command palette (Ctrl+K / Ctrl+Shift+P)
        if self.command_palette.is_visible() {
            let input = self.input_handler.state.all_text();
            self.command_palette.sync_query_from_input(&input);
            self.command_palette.render(frame, size);
        }

        // Overlay: help panel (?)
        if self.help_state.visible {
            crate::help::render_help(frame, size, &self.help_state);
        }

        // Overlay: approval dialog (before error display so errors can appear on top)
        if self.awaiting_approval {
            if let Some(req) = self.pending_approval_request.front() {
                let panel_height = 7u16.min(size.height.saturating_sub(4));
                let panel_width = 70u16.min(size.width.saturating_sub(4));
                let panel_area =
                    crate::app::render::shared::centered_rect(panel_width, panel_height, size);
                crate::tool_approval::render_approval_prompt(frame, panel_area, req);
            }
        }

        // Overlay: error display
        if self.error_manager.is_showing() {
            self.error_manager.render(frame, size);
        }

        // Overlay: compaction preview (while pending)
        if self.showing_compaction_preview {
            self.render_compaction_preview(frame, size);
        }

        // Overlay: first-run wizard (covers entire screen)
        if self.wizard.showing_wizard {
            if let Some(ref mut wizard) = self.wizard.wizard {
                frame.render_widget(ratatui::widgets::Clear, size);
                wizard.render(frame, size);
            }
        }

        // Overlay: toast notifications (topmost — always visible)
        self.toast_manager.render(
            frame,
            size,
            Some(&self.theme_colors.lock().unwrap_or_else(|e| e.into_inner())),
        );

        if debug_enabled {
            let total_elapsed = render_start.elapsed();
            if total_elapsed > Duration::from_millis(2) {
                crate::debug_log!(
                    "Brutalist render ran long: width={} height={} messages={} total_lines={} heights={} layout_ms={} draw_ms={} total_ms={} streaming={} user_scrolled={} selected_message={}",
                    size.width,
                    size.height,
                    self.messages.len(),
                    total_lines,
                    heights.len(),
                    layout_elapsed.as_millis(),
                    draw_elapsed.as_millis(),
                    total_elapsed.as_millis(),
                    self.is_streaming,
                    self.user_scrolled,
                    self.selected_message
                );
            }
        }
    }

    /// Render compaction preview overlay
    pub(crate) fn render_compaction_preview(&self, frame: &mut ratatui::Frame, size: Rect) {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

        let width = 50u16.min(size.width.saturating_sub(4));
        let height = 5u16;
        let area = crate::app::render::shared::centered_rect(width, height, size);

        frame.render_widget(Clear, area);

        let text = ratatui::text::Line::from(vec![ratatui::text::Span::styled(
            "💾 Compacting context",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]);

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title("Auto-Compact")
                    .title_style(Style::default().fg(Color::Cyan)),
            )
            .alignment(ratatui::layout::Alignment::Center)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    /// Save command history on exit
    pub(crate) fn save_history(&mut self) {
        let history = self.input_handler.get_history();
        if let Err(e) = crate::session::save_command_history(history) {
            tracing::warn!("Failed to save command history: {}", e);
        }
    }
}

impl Default for TUI {
    fn default() -> Self {
        #[cfg(test)]
        {
            // Use the lightweight test constructor when running tests to avoid
            // terminal/IO dependencies in `Default::default()` during test runs.
            Self::new_for_test()
        }

        #[cfg(not(test))]
        {
            let (tx, _) = tokio::sync::broadcast::channel(1);
            Self::new(PathBuf::from("."), AiMode::default(), false, tx.subscribe())
                .expect("Failed to create TUI")
        }
    }
}

// Slash command handlers and post-command utilities
include!("event_loop_slash_commands.rs");

// ============================================================================
// CHANNEL TYPES
// Note: These types are defined here but wiring to actual services
// will be done as part of future async service integration
// ============================================================================

/// Event from async services
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AsyncEvent {
    /// Stream chunk from LLM
    StreamChunk { delta: String, finished: bool },
    /// Tool execution result
    ToolResult {
        tool_name: String,
        success: bool,
        output: String,
    },
    /// Command result
    CommandResult { success: bool, output: String },
    /// Workspace update
    WorkspaceUpdate { file_count: usize },
}

/// Sender for async events
pub type AsyncEventSender = mpsc::Sender<AsyncEvent>;

/// Receiver for async events
pub type AsyncEventReceiver = mpsc::Receiver<AsyncEvent>;
