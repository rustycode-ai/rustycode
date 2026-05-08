//! Responsive Event Loop

use crate::agents::AgentManager;
use crate::app::auto_continue_state::AutoContinueState;
use crate::app::commands::{dispatch_registered_slash_command, CommandContext, CommandEffect};
use crate::app::keyboard_shortcuts::KeyboardShortcutHandler;
use crate::app::lsp_status::LspStatus;
use crate::app::mcp_status::McpStatus;
use crate::app::rate_limit_handler::RateLimitHandler;
use crate::app::renderer::RendererMode;
use crate::app::tasks::{load_tasks, WorkspaceTasks};
use crate::app::team_mode_handler::TeamModeHandler;
use crate::app::tool_panel_state::ToolPanelState;
use crate::app::wizard_handler::WizardHandler;
use crate::app::{
    service_integration::*, DEBUG_SLOW_THRESHOLD, EVENT_POLL_TIMEOUT, FRAME_BUDGET_60FPS,
    REFRESH_COOLDOWN,
};
use crate::help::HelpState;
use crate::memory::compaction::{CompactionConfig, ContextMonitor};
use crate::memory::memory_auto::ThreadSafeAutoMemory;
use crate::memory::memory_injection::InjectionConfig;
use crate::plugin::PluginManager;
use crate::plugin::PluginManagerUI;
use crate::services::agent_mode::AiMode;
use crate::services::config::load_config;
use crate::services::config::TUIConfig;
use crate::services::conversation_service::ConversationConfig;
use crate::services::providers::all_available_models;
use crate::services::session::load_command_history;
use crate::skills::{SkillLoader, SkillStateManager};
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

use crate::app::render::layout::FrameLayoutSnapshot;
use crate::ui::theme_preview::{ThemePreview, ThemeSwitcher};
use crate::ui::toast::ToastManager;
use anyhow::{Context, Result};
use crossterm::event;
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use rustycode_core::integration::HookRegistry;
use rustycode_tools::ToolRegistry;
use std::collections::HashSet;
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
        // (users see cost/duration after exiting)
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
    pub(crate) marketplace_browser: crate::ui::marketplace_browser::MarketplaceBrowser,

    // Service Manager (background tasks)
    pub(crate) services: ServiceManager,

    // State
    pub(crate) messages: Vec<Message>,
    pub(crate) _input_state: InputState,
    pub(crate) input_mode: InputMode,
    pub(crate) running: bool,

    // Viewport and scroll state (grouped in sub-struct)
    pub(crate) view: crate::app::view_state::ViewState,
    pub(crate) sidebar_area: std::cell::Cell<Rect>, // store sidebar area for mouse routing

    // Streaming state (grouped in sub-struct)
    pub(crate) streaming: crate::app::streaming_state::StreamingState,
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
    // Pipeline cron scheduler (std::sync::mpsc channels, NOT tokio)
    pub(crate) scheduler_rx: Option<mpsc::Receiver<crate::app::pipeline::ScheduledPhaseEvent>>,
    pub(crate) active_scheduled_phases: std::collections::HashSet<String>,
    pub(crate) max_concurrent_phases: usize,
    pub(crate) last_extraction:
        Option<(Vec<crate::app::tasks::Task>, Vec<crate::app::tasks::Todo>)>,
    pub(crate) workspace_scan_progress: Option<(usize, usize)>, // (scanned, total)
    pub(crate) git_branch: Option<String>,                      // Current git branch for status bar

    // Rate limit handler
    pub(crate) rate_limit: RateLimitHandler,
    // Rate limit status tracker (from response headers)
    pub(crate) rate_limit_tracker: crate::services::rate_limit_tracker::RateLimitTracker,

    // Auto-continue mode - automatically continue working on pending tasks
    pub(crate) auto_continue: AutoContinueState,

    // Turn-level verification (snapshot before agent turn, diff after)
    pub(crate) turn_snapshot: Option<crate::app::turn_snapshot::TurnSnapshot>,
    // Doom loop detector — tracks repetitive tool-call patterns
    pub(crate) doom_loop: crate::app::doom_loop::DoomLoopDetector,
    // Carries doom loop context into the next turn's conversation so the model
    // sees it (system messages are filtered from history; this becomes a user note).
    pub(crate) pending_doom_note: Option<String>,

    // Active Reasoning Engine budget tracking
    pub(crate) reasoning_budget:
        std::sync::Mutex<rustycode_tools::providers::reasoning_types::BudgetState>,

    // Performance: dirty flag - only render when state changes
    pub(crate) dirty: bool,
    // Set after external editor returns to force terminal.clear() + full redraw
    pub(crate) needs_full_redraw: bool,

    // Token compaction (grouped in sub-struct)
    pub(crate) compaction: crate::app::compaction_state::CompactionState,

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

    // Tool approval (grouped in sub-struct)
    pub(crate) tool_approval: crate::app::tool_approval_state::ToolApprovalState,

    // Session start time (for elapsed time display)
    pub(crate) start_time: Instant,
    // Language server protocol status (LSP)
    pub(crate) lsp: LspStatus,
    // Message Control Protocol status (MCP)
    pub(crate) mcp: McpStatus,

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

    // Tool panel state
    pub(crate) tool_panel: ToolPanelState,

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
    pub(crate) current_effort: String,

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

    /// Undo stacks for message positions and file edits.
    pub(crate) undo: crate::app::undo_state::UndoState,

    // File finder (Ctrl+O fuzzy file search)
    pub(crate) file_finder: crate::ui::file_finder::FileFinder,

    // Message search state
    pub(crate) search_state: SearchState,

    // Message tag filter state
    pub(crate) tag_filter: TagFilter,

    // Active frame renderer backend
    pub(crate) renderer_mode: RendererMode,

    /// Shared todo state for LLM todo tools (todo_read, todo_write, todo_update)
    pub(crate) todo_state: rustycode_tools::todo::TodoState,

    pub(crate) tool_manager: crate::services::tool_manager::ToolManager,
    pub(crate) session_manager: crate::services::session_manager::SessionManager,

    // Session token usage and cost tracking (grouped in sub-struct)
    pub(crate) token_budget: crate::app::token_budget::TokenBudget,

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
        self.streaming.reset();
        self.ast_phase_state.deactivate();
    }

    /// Reset the conversational state that should not survive a load/resume
    /// or a full conversation clear.
    pub(crate) fn reset_conversation_state(&mut self) {
        self.view.selected_message = 0;
        self.view.scroll_offset_line = 0;
        self.view.user_scrolled = false;
        self.active_tools.clear();
        self.session_sidebar.clear_milestone_progress();
        self.tool_panel.reset();
        self.dismiss_any_overlay();
        self.reset_streaming_state();
        self.streaming.queued_message = None;
        self.stashed_prompt = None;
        self.clear_plan_mode_banner();
        self.rate_limit.clear();
        self.auto_continue.reset();
        self.token_budget.reset();
        self.view.last_total_lines.set(0);
        self.compaction.context_monitor.current_tokens = 0;
        self.compaction.context_monitor.needs_compaction = false;

        self.doom_loop.reset();
        self.undo.clear();
        self.search_state.visible = false;
        self.search_state.matches.clear();
        self.search_state.query.clear();
        self.search_state.current_match_index = 0;
        self.message_line_offsets.borrow_mut().clear();
        self.message_areas.borrow_mut().clear();
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

        // Create hook manager early so we can share with services
        let hook_manager = rustycode_tools::hooks::HookManager::new(
            PathBuf::from(".rustycode/hooks"),
            rustycode_tools::hooks::HookProfile::Standard,
            String::new(),
        );
        let mut services = services;
        services.set_hook_manager(hook_manager.clone());

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

        // Build pipeline and extract tool registry for agent context
        let (pipeline, agent_tool_registry) = {
            let mut p = crate::app::pipeline::registry::PipelineRegistry::new();
            #[cfg(feature = "browser")]
            {
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
            }

            let reg = p.tool_registry.clone();

            p.register_factory(
                "rustycode::steps::AgentStep",
                Box::new(crate::app::pipeline::steps::agent_factory::AgentStepFactory),
            );
            (p, reg)
        };

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
            view: crate::app::view_state::ViewState::new(),
            sidebar_area: std::cell::Cell::new(Rect::default()),
            streaming: crate::app::streaming_state::StreamingState::new(),
            plan_mode_banner: None,
            active_tools: std::collections::HashMap::new(),
            workspace_loaded: false,
            workspace_context: None,
            workspace_tasks: load_tasks(),
            pipeline,
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
                    rustycode_agent_runtime::AgentConfig::default(),
                    mdl,
                    agent_tool_registry,
                )
            },
            scheduler_rx: None,
            active_scheduled_phases: std::collections::HashSet::new(),
            max_concurrent_phases: 3,
            last_extraction: None,
            workspace_scan_progress: None,
            git_branch: None,
            rate_limit: RateLimitHandler::new(),
            rate_limit_tracker: crate::services::rate_limit_tracker::RateLimitTracker::default(),
            auto_continue: AutoContinueState::from_env(),
            turn_snapshot: None,
            doom_loop: crate::app::doom_loop::DoomLoopDetector::new(),
            pending_doom_note: None,
            execution_trace: None,
            reasoning_budget: std::sync::Mutex::new(
                rustycode_tools::providers::reasoning_types::BudgetState::default(),
            ),
            dirty: true,
            needs_full_redraw: false,
            compaction: crate::app::compaction_state::CompactionState::new(
                context_monitor,
                compaction_config,
            ),
            theme_colors,
            auto_memory,
            memory_injection_config,
            skill_palette,
            skill_manager,
            plugin_manager,
            plugin_manager_ui,
            showing_plugin_manager: false,
            showing_marketplace_browser: false,
            help_state: HelpState::new(),
            tool_approval: crate::app::tool_approval_state::ToolApprovalState::new(
                ToolApprovalManager::new(),
            ),
            start_time: Instant::now(),
            lsp: LspStatus::new_forced_refresh(),
            mcp: McpStatus::new_forced_refresh(),
            theme_preview,
            theme_switcher,
            toast_manager,
            error_manager,
            showing_error: false,
            tool_panel: ToolPanelState::new(),
            command_palette,
            showing_command_palette: false,
            showing_skill_palette: false,
            status_bar_collapsed: false,
            footer_collapsed: false,
            last_esc_press: None,
            stashed_prompt: None,
            model_selector: ModelSelector::with_models(all_available_models()),
            file_selector: FileSelector::new(Vec::new()),
            showing_provider_selector: false,
            current_model: rustycode_llm::load_model_from_config().unwrap_or_else(|e| {
                tracing::warn!("Failed to load model config: {}", e);
                String::new()
            }),
            current_effort: "medium".to_string(),
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
            undo: crate::app::undo_state::UndoState::new(),
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
            // MCP proxy cache (managed by tool_manager)
            todo_state: rustycode_tools::todo::new_todo_state(),
            tool_manager: crate::services::tool_manager::ToolManager::new(),
            session_manager: crate::services::session_manager::SessionManager::new(
                crate::app::session_recovery_integration::SessionRecoveryManager::new(
                    crate::app::session_recovery_integration::SessionRecoveryConfig::default(),
                )
                .ok(),
            ),
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
            token_budget: crate::app::token_budget::TokenBudget::new(),
            hook_manager,
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
        crate::services::provider_manager::compute_api_key_warning()
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
            view: crate::app::view_state::ViewState::new(),
            sidebar_area: std::cell::Cell::new(Rect::default()),
            streaming: crate::app::streaming_state::StreamingState::new(),
            plan_mode_banner: None,
            active_tools: std::collections::HashMap::new(),
            workspace_loaded: false,
            workspace_context: None,
            workspace_tasks: load_tasks(),
            pipeline: {
                let mut p = crate::app::pipeline::registry::PipelineRegistry::new();
                #[cfg(feature = "browser")]
                {
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
                }

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
                    rustycode_agent_runtime::AgentConfig::default(),
                    mdl,
                    crate::app::pipeline::tool_registry::ToolRegistry::new(),
                )
            },
            scheduler_rx: None,
            active_scheduled_phases: std::collections::HashSet::new(),
            max_concurrent_phases: 3,
            last_extraction: None,
            workspace_scan_progress: None,
            git_branch: None,
            rate_limit: RateLimitHandler::new(),
            rate_limit_tracker: crate::services::rate_limit_tracker::RateLimitTracker::default(),
            auto_continue: AutoContinueState::from_env(),
            turn_snapshot: None,
            doom_loop: crate::app::doom_loop::DoomLoopDetector::new(),
            pending_doom_note: None,
            reasoning_budget: std::sync::Mutex::new(
                rustycode_tools::providers::reasoning_types::BudgetState::default(),
            ),
            dirty: true,
            needs_full_redraw: false,
            compaction: crate::app::compaction_state::CompactionState::new(
                context_monitor,
                compaction_config,
            ),
            theme_colors,
            auto_memory,
            memory_injection_config,
            skill_palette,
            skill_manager: Arc::new(RwLock::new(SkillStateManager::new())),
            plugin_manager,
            plugin_manager_ui,
            showing_plugin_manager: false,
            showing_marketplace_browser: false,
            help_state: HelpState::new(),
            tool_approval: crate::app::tool_approval_state::ToolApprovalState::new(
                ToolApprovalManager::new(),
            ),
            start_time: Instant::now(),
            lsp: LspStatus::new_forced_refresh(),
            mcp: McpStatus::new_forced_refresh(),
            theme_preview,
            theme_switcher,
            toast_manager,
            error_manager,
            showing_error: false,
            tool_panel: ToolPanelState::new(),
            last_esc_press: None,
            stashed_prompt: None,
            command_palette,
            showing_command_palette: false,
            showing_skill_palette: false,
            status_bar_collapsed: false,
            footer_collapsed: false,
            model_selector: ModelSelector::with_models(all_available_models()),
            file_selector: FileSelector::new(Vec::new()),
            showing_provider_selector: false,
            current_model: rustycode_llm::load_model_from_config().unwrap_or_else(|e| {
                tracing::warn!("Failed to load model config: {}", e);
                String::new()
            }),
            current_effort: "medium".to_string(),
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
            undo: crate::app::undo_state::UndoState::new(),
            file_finder: crate::ui::file_finder::FileFinder::new(PathBuf::from(".")),
            search_state: SearchState::new(),
            tag_filter: TagFilter::new(),
            renderer_mode,
            todo_state: rustycode_tools::todo::new_todo_state(),
            tool_manager: crate::services::tool_manager::ToolManager::new(),
            session_manager: crate::services::session_manager::SessionManager::new(None),
            team_panel: crate::ui::team_panel::TeamPanel::new(),
            team_handler: TeamModeHandler::new(),
            clarification_panel: crate::ui::clarification::ClarificationPanel::hidden(),
            awaiting_clarification: false,
            // Worker panel (sub-agent orchestration)
            worker_panel: crate::ui::worker_panel::WorkerPanel::new(),
            // AST pipeline phase progress
            ast_phase_state: crate::ui::ast_progress::AstPhaseState::new(),
            // Session token usage and cost tracking
            token_budget: crate::app::token_budget::TokenBudget::new(),
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
            event_receiver: tokio::sync::broadcast::channel(crate::app::EVENT_CHANNEL_CAPACITY).1,
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

        self.services
            .start_conversation(config, tool_registry)
            .context("failed to start conversation service")?;
        crate::info_log!(
            "start_conversation OK, pipeline={}",
            self.services.has_pipeline()
        );
        self.services
            .start_workspace_loading()
            .context("failed to start workspace loading")?;

        self.refresh_mcp_status(true);

        // Wire shared todo state into service manager so LLM can use todo tools
        self.services.set_todo_state(self.todo_state.clone());

        tracing::info!("Services initialized with {} tools", tool_count);

        Ok(())
    }

    /// Refresh sidebar LSP status from discovery.
    fn refresh_lsp_status(&mut self, force: bool) -> bool {
        if !force && self.lsp.last_lsp_refresh.elapsed() < REFRESH_COOLDOWN {
            return false;
        }

        let active_clients = rustycode_tools::providers::lsp::active_clients_status();
        let any_running = active_clients.iter().any(|(_, state)| state == "running");

        let lsp_names: Vec<String> = active_clients
            .iter()
            .map(|(name, state)| {
                if state == "running" {
                    format!("✓ {name}")
                } else {
                    format!("○ {name} ({state})")
                }
            })
            .collect();

        // Only show the LSP section when servers are actually running.
        // Showing installed-but-not-started servers was confusing — users
        // couldn't tell which LSPs were active vs merely available.
        let display_connected = any_running;

        let changed = force
            || display_connected != self.lsp.last_lsp_connected
            || lsp_names != self.lsp.last_lsp_servers;

        if changed {
            self.session_sidebar.update_lsp_status(
                display_connected,
                lsp_names.clone(),
                std::collections::HashMap::new(),
            );
            self.lsp.last_lsp_connected = display_connected;
            self.lsp.last_lsp_servers = lsp_names;
            self.dirty = true;
        }

        self.lsp.last_lsp_refresh = Instant::now();
        changed
    }

    /// Refresh sidebar MCP status from the live proxy cache and config discovery.
    fn refresh_mcp_status(&mut self, force: bool) -> bool {
        if !force && self.mcp.last_mcp_refresh.elapsed() < REFRESH_COOLDOWN {
            return false;
        }

        let connected_servers: HashSet<String> =
            if let Some(mcp_proxies) = self.tool_manager.mcp_proxies() {
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

        if mcp_servers.is_empty() && self.tool_manager.mcp_proxies().is_some() {
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
            || mcp_connected != self.mcp.last_mcp_connected
            || mcp_servers != self.mcp.last_mcp_servers;

        if changed {
            self.session_sidebar
                .update_mcp_status(mcp_connected, mcp_servers.clone());
            self.mcp.last_mcp_connected = mcp_connected;
            self.mcp.last_mcp_servers = mcp_servers;
            self.dirty = true;
        }

        self.mcp.last_mcp_refresh = Instant::now();
        changed
    }

    /// Refresh sidebar tool call summary from the current tool history.
    fn refresh_tool_call_summary(&mut self) {
        let recent = self
            .tool_panel
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
        match self.session_manager.find_most_recent_session() {
            Ok(Some(session)) => {
                self.reset_conversation_state();
                self.view.scroll_offset_line = session.scroll_position;
                self.messages = session.messages;
                self.compaction.context_monitor.update(&self.messages);
                if !self.messages.is_empty() {
                    self.view.selected_message = self.messages.len().saturating_sub(1);
                }

                let display_id = session
                    .session_id
                    .split('-')
                    .next()
                    .unwrap_or(&session.session_id);
                self.add_system_message(format!(
                    "Resumed session '{}' ({} messages, {} min ago)",
                    display_id, session.message_count, session.age_minutes
                ));
                self.dirty = true;
                tracing::info!(
                    "Resumed session {} ({} messages)",
                    session.session_id,
                    session.message_count
                );
            }
            Ok(None) => {
                self.add_system_message("No previous sessions found to resume".to_string());
            }
            Err(e) => {
                tracing::warn!("Failed to list sessions for resume: {}", e);
                self.add_system_message("Could not find saved sessions".to_string());
            }
        }
    }

    /// Register all built-in tools for AI coding assistant functionality
    fn register_builtin_tools(&self, tool_registry: &mut ToolRegistry) {
        self.tool_manager.register_builtin_tools(
            tool_registry,
            &self.pipeline_ctx.provider,
            &self.pipeline_ctx.current_model,
            self.services.cwd(),
            &self.skill_manager,
            &self.todo_state,
        );
    }

    /// Load tools from configured MCP servers
    fn load_mcp_tools(&mut self, tool_registry: &mut ToolRegistry) {
        self.tool_manager.load_mcp_tools(tool_registry);
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

        // Set terminal title to project name (for tab identification)
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
                if self.streaming.is_streaming || !self.active_tools.is_empty() {
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
            self.poll_services()
                .context("failed to poll background services")?;
            self.poll_mcp_events()?;
            {
                use rustycode_shared_runtime::SHARED_RUNTIME;
                let pipeline_tick_start = Instant::now();
                SHARED_RUNTIME
                    .block_on(self.tick_pipeline())
                    .context("failed to tick orchestration pipeline")?;
                let pipeline_tick_elapsed = pipeline_tick_start.elapsed();
                if debug_enabled && pipeline_tick_elapsed > DEBUG_SLOW_THRESHOLD {
                    crate::debug_log!(
                        "Pipeline tick ran long: {} ms",
                        pipeline_tick_elapsed.as_millis()
                    );
                }
            }
            let pipeline_monitor_elapsed = Duration::ZERO;
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
                        self.view.scroll_offset_line,
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
                        terminal
                            .clear()
                            .context("failed to clear terminal for full redraw")?;
                        self.needs_full_redraw = false;
                    }
                    terminal
                        .draw(|f| self.render(f))
                        .context("failed to draw TUI frame")?;
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
                if event::poll(timeout).context("failed to poll for input events")? {
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
                if event::poll(EVENT_POLL_TIMEOUT)
                    .context("failed to poll for input events (over budget)")?
                {
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
                    || loop_iterations.is_multiple_of(crate::app::DIAGNOSTIC_LOG_INTERVAL as u64))
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
                    self.streaming.is_streaming,
                    self.view.user_scrolled,
                    self.view.viewport_height
                );
            }
        }

        // Cleanup: stop any active stream
        if self.streaming.is_streaming {
            self.services.request_stop_stream();
            self.streaming.stream_cancelled = true;
            // Don't set is_streaming=false here — let the async stream task's
            // Done handler clean up to avoid racing with channel receivers.
        }

        // Shutdown MCP servers to prevent orphaned child processes
        if let Some(mcp_proxies) = self.tool_manager.mcp_proxies() {
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
                self.view.scroll_offset_line,
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

        if content.contains('\n') {
            self.input_handler.state.mode = crate::ui::input_state::InputMode::MultiLine;
        }
        self.input_handler.state.insert_text_at_cursor(content);

        // Switch to multi-line mode when pasting content with newlines,
        // matching the behavior of the Ctrl+V paste handler.
        if content.contains('\n') {
            self.input_handler.state.mode = crate::ui::input_state::InputMode::MultiLine;
            self.input_mode = crate::ui::input_state::InputMode::MultiLine;
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

        // Handle /r locally as regenerate alias
        if matches!(parts[0], "/r" | "/regen" | "/regenerate") {
            self.regenerate_last_response()?;
            return Ok(());
        }

        // Handle AI mode switching commands
        if matches!(parts[0], "/yolo" | "/auto") {
            let current_mode = self.services.ai_mode();
            if matches!(current_mode, crate::services::agent_mode::AiMode::Yolo) {
                self.exit_plan_mode();
                self.services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Ask);
                self.add_system_message(
                    "🔒 YOLO mode deactivated — tools require approval again.".to_string(),
                );
            } else {
                self.exit_plan_mode();
                self.services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Yolo);
                self.add_system_message(
                    "🚀 YOLO mode activated — tools auto-approved, fully autonomous.\n\
                     Use /yolo again or /ask to return to approval mode."
                        .to_string(),
                );
            }
            self.dirty = true;
            self.auto_scroll();
            if parts.len() > 1 {
                let task = parts[1..].join(" ");
                self.services.send_message(task)?;
            }
            return Ok(());
        }

        if matches!(parts[0], "/act") {
            let current_mode = self.services.ai_mode();
            if matches!(current_mode, crate::services::agent_mode::AiMode::Act) {
                self.exit_plan_mode();
                self.services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Ask);
                self.add_system_message(
                    "💬 ACT mode deactivated — tools require approval again.".to_string(),
                );
            } else {
                self.exit_plan_mode();
                self.services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Act);
                self.add_system_message(
                    "⚡ ACT mode — execute with brief summaries, minimal approval.\n\
                     Use /act again or /ask to return to full approval mode."
                        .to_string(),
                );
            }
            self.dirty = true;
            self.auto_scroll();
            if parts.len() > 1 {
                let task = parts[1..].join(" ");
                self.services.send_message(task)?;
            }
            return Ok(());
        }

        if matches!(parts[0], "/ask") {
            self.exit_plan_mode();
            self.services
                .set_ai_mode(crate::services::agent_mode::AiMode::Ask);
            self.add_system_message(
                "💬 ASK mode — tools require approval, full summaries.".to_string(),
            );
            self.dirty = true;
            self.auto_scroll();
            if parts.len() > 1 {
                let task = parts[1..].join(" ");
                self.services.send_message(task)?;
            }
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
                    current_stream_content: &mut self.streaming.current_stream_content,
                    is_streaming: &mut self.streaming.is_streaming,
                    last_extraction: &mut self.last_extraction,
                    services: &mut self.services,
                    agent_manager: &mut self.agent_manager,
                    memory_injection_config: &mut self.memory_injection_config,
                    theme_colors: &self.theme_colors,
                    skill_manager: &self.skill_manager,
                    plugin_manager: &self.plugin_manager,
                    running: &mut self.running,
                    context_monitor: &mut self.compaction.context_monitor,
                    compaction_config: &mut self.compaction.compaction_config,
                    showing_compaction_preview: &mut self.compaction.showing_preview,
                    pending_compaction: &mut self.compaction.pending,
                    file_undo_stack: &mut self.undo,
                    session_input_tokens: self.token_budget.session_input_tokens,
                    session_output_tokens: self.token_budget.session_output_tokens,
                    session_cost_usd: self.token_budget.session_cost_usd,
                    current_model: self.current_model.clone(),
                    session_start: self.start_time,
                },
            )?;

            if let Some(effect) = effect {
                self.apply_slash_command_effect(effect)?;
                self.view.user_scrolled = false;
                self.dirty = true;
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
            if elapsed > DEBUG_SLOW_THRESHOLD {
                crate::debug_log!(
                    "Frame renderer ran long: mode={} elapsed_ms={} size={}x{} messages={} streaming={} dirty={}",
                    renderer.label(),
                    elapsed.as_millis(),
                    frame.area().width,
                    frame.area().height,
                    self.messages.len(),
                    self.streaming.is_streaming,
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
        let input_line_count = input_text.lines().count().max(1);
        let input_rows: u16 = if input_line_count > 1 {
            2u16.saturating_add(input_line_count.min(6) as u16)
        } else {
            2
        };

        let size = frame.area();
        let header_rows: u16 = if self.status_bar_collapsed { 0 } else { 1 };
        let footer_rows: u16 = if self.footer_collapsed { 0 } else { 1 };
        let fixed_rows = header_rows + footer_rows + input_rows;
        let main_height = size.height.saturating_sub(fixed_rows);
        let message_area = Rect {
            x: size.x,
            y: header_rows,
            width: size.width,
            height: main_height,
        };

        // Compute message layout once — reused for rendering, click areas,
        // and scroll bookkeeping.
        let layout_start = Instant::now();
        let width = message_area.width as usize;
        let (layout, total_lines) = {
            let renderer = self.create_brutalist_renderer(&input_text);
            let (total_lines, heights, chain_map) = renderer.compute_message_layout(width);
            (
                FrameLayoutSnapshot::from_message_layout(
                    message_area,
                    self.session_sidebar.is_visible(),
                    self.view.scroll_offset_line,
                    self.view.user_scrolled,
                    total_lines,
                    heights,
                    chain_map,
                ),
                total_lines,
            )
        };
        layout.apply(self);
        let layout_elapsed = layout_start.elapsed();

        // Render the complete brutalist UI with precomputed heights.
        let renderer = self.create_brutalist_renderer(&input_text);
        let draw_start = Instant::now();
        renderer.render_with_heights(
            frame,
            &layout.heights,
            &layout.chain_map,
            layout.message_area,
        );
        let draw_elapsed = draw_start.elapsed();

        if debug_enabled
            && (layout_elapsed > EVENT_POLL_TIMEOUT || draw_elapsed > DEBUG_SLOW_THRESHOLD)
        {
            crate::debug_log!(
                "Brutalist breakdown: layout_ms={} draw_ms={} messages={} total_lines={} streaming={}",
                layout_elapsed.as_millis(),
                draw_elapsed.as_millis(),
                self.messages.len(),
                total_lines,
                self.streaming.is_streaming
            );
        }

        if let Some(sidebar_area) = layout.sidebar_area {
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
        if self.tool_panel.showing_tool_panel {
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
        if self.tool_approval.awaiting {
            if let Some(req) = self.tool_approval.pending_requests.front() {
                let (panel_height, panel_width) =
                    crate::tool_approval::approval_panel_size(req, size);
                let panel_area =
                    crate::app::render::shared::centered_rect(panel_width, panel_height, size);
                crate::tool_approval::render_approval_prompt(frame, panel_area, req, size);
            }
        }

        // Overlay: error display
        if self.error_manager.is_showing() {
            self.error_manager.render(frame, size);
        }

        // Overlay: compaction preview (while pending)
        if self.compaction.showing_preview {
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
            if total_elapsed > DEBUG_SLOW_THRESHOLD {
                crate::debug_log!(
                    "Brutalist render ran long: width={} height={} messages={} total_lines={} heights={} layout_ms={} draw_ms={} total_ms={} streaming={} user_scrolled={} selected_message={}",
                    size.width,
                    size.height,
                    self.messages.len(),
                    total_lines,
                    layout.heights.len(),
                    layout_elapsed.as_millis(),
                    draw_elapsed.as_millis(),
                    total_elapsed.as_millis(),
                    self.streaming.is_streaming,
                    self.view.user_scrolled,
                    self.view.selected_message
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

    pub(crate) fn cycle_effort_level(&mut self) -> String {
        self.current_effort = crate::services::provider_manager::cycle_effort_level(
            &self.current_effort,
            &self.current_model,
        );
        self.services.set_effort(self.current_effort.clone());
        self.current_effort.clone()
    }

    /// Save command history on exit
    pub(crate) fn save_history(&mut self) {
        let history = self.input_handler.history();
        crate::services::session_manager::SessionManager::save_history(history);
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

// CHANNEL TYPES
// Note: These types are defined here but wiring to actual services
// will be done as part of future async service integration

/// Event from async services
#[derive(Debug, Clone)]
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
