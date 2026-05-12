//! Responsive Event Loop

use crate::agents::AgentManager;
use crate::app::auto_continue_state::AutoContinueState;
use crate::app::commands::{dispatch_registered_slash_command, CommandContext, CommandEffect};
use crate::app::keyboard_shortcuts::KeyboardShortcutHandler;
use crate::app::lsp_status::LspStatus;
use crate::app::mcp_status::McpStatus;
use crate::app::rate_limit_handler::RateLimitHandler;
use crate::app::renderer::RendererMode;
use crate::app::tasks::{
    load_tasks, load_tasks_from_storage, save_tasks_with_storage, WorkspaceTasks,
};
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
use crate::ui::input::{InputHandler, InputMode};
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
use rustycode_protocol::Op;
use rustycode_tools::ToolRegistry;
use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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
    // Grouped state sub-structs
    pub(crate) ui: crate::app::state_model::UIComponents,
    pub(crate) integration: crate::app::state_model::ServiceIntegrationState,
    pub(crate) workspace: crate::app::state_model::TaskWorkspaceState,
    pub(crate) session: crate::app::state_model::InteractionSessionState,
    pub(crate) sys: crate::app::state_model::SystemState,
    pub(crate) overlays: crate::app::state_model::OverlayState,
    pub(crate) panels: crate::app::state_model::ToolExecutionPanel,
    pub(crate) theme: crate::app::state_model::ThemeNotificationState,
    pub(crate) team: crate::app::state_model::TeamModeState,
    pub(crate) search: crate::app::state_model::MessageSearchState,
    pub(crate) model: crate::app::state_model::ProviderModelState,
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
        while let Ok(event) = self.integration.event_receiver.try_recv() {
            match event {
                rustycode_mcp::protocol::McpEvent::ProgressNotification { progress, message } => {
                    info!("MCP progress: {}% - {:?}", progress * 100.0, message);
                    self.sys.dirty = true;
                }
                rustycode_mcp::protocol::McpEvent::ToolsListChanged { .. } => {
                    self.sys.dirty = true;
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
        self.session.streaming.reset();
        self.panels.ast_phase_state.deactivate();
    }

    /// Reset the conversational state that should not survive a load/resume
    /// or a full conversation clear.
    pub(crate) fn reset_conversation_state(&mut self) {
        self.ui.view.selected_message = 0;
        self.ui.view.scroll_offset_line = 0;
        self.ui.view.user_scrolled = false;
        self.session.active_tools.clear();
        self.session.session_sidebar.clear_milestone_progress();
        self.panels.tool_panel.reset();
        self.dismiss_any_overlay();
        self.reset_streaming_state();
        self.session.streaming.queued_message = None;
        self.ui.stashed_prompt = None;
        self.clear_plan_mode_banner();
        self.integration.rate_limit.clear();
        self.session.auto_continue.reset();
        self.model.token_budget.reset();
        self.ui.view.last_total_lines.set(0);
        self.sys.compaction.context_monitor.current_tokens = 0;
        self.sys.compaction.context_monitor.needs_compaction = false;

        self.session.doom_loop.reset();
        self.session.undo.clear();
        self.search.search_state.visible = false;
        self.search.search_state.matches.clear();
        self.search.search_state.query.clear();
        self.search.search_state.current_match_index = 0;
        self.search.message_line_offsets.borrow_mut().clear();
        self.search.message_areas.borrow_mut().clear();
    }

    /// Create a new TUI instance with service integration
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
        // Load skills in background thread — I/O outside the lock, apply under short write lock
        let skill_manager_clone = skill_manager.clone();
        std::thread::spawn(move || {
            let loader = SkillLoader::new();
            let base_skills = loader.load_all().unwrap_or_else(|e| {
                tracing::error!("Failed to load skills: {}", e);
                Vec::new()
            });
            let skill_states: Vec<_> = base_skills
                .into_iter()
                .map(crate::skills::manager::SkillState::from_base)
                .collect();
            {
                let mut manager = skill_manager_clone
                    .write()
                    .unwrap_or_else(|e| e.into_inner());
                manager.skills = skill_states;
            }
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
            ui: crate::app::state_model::UIComponents {
                message_renderer: MessageRenderer::new(),
                input_handler,
                animator: Animator::new(4, reduced_motion),
                marketplace_browser,
                skill_palette,
                plugin_manager_ui,
                help_state: HelpState::new(),
                sidebar_area: std::cell::Cell::new(Rect::default()),
                view: crate::app::view_state::ViewState::new(),
                keyboard_handler: KeyboardShortcutHandler::new(tui_config.behavior.vim_enabled),
                tui_config,
                stashed_prompt: None,
                status_bar_collapsed: false,
                footer_collapsed: false,
            },
            integration: crate::app::state_model::ServiceIntegrationState {
                services,
                pipeline,
                pipeline_ctx: {
                    let (pt, mdl, _) = rustycode_llm::load_provider_config_from_env()
                        .unwrap_or_else(|_| {
                            (
                                "anthropic".into(),
                                "claude-haiku-4-5-20251001".into(),
                                Default::default(),
                            )
                        });
                    let pipeline_provider = rustycode_llm::create_provider(&pt, &mdl)
                        .unwrap_or_else(|_| {
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
                rate_limit: RateLimitHandler::new(),
                rate_limit_tracker: crate::services::rate_limit_tracker::RateLimitTracker::default(
                ),
                lsp: LspStatus::new_forced_refresh(),
                mcp: McpStatus::new_forced_refresh(),
                mcp_manager: std::sync::Arc::new(tokio::sync::RwLock::new(
                    rustycode_mcp::McpServerManager::new(rustycode_mcp::ManagerConfig::default()),
                )),
                start_time: Instant::now(),
                event_receiver,
                todo_state: rustycode_tools::todo::new_todo_state(),
                todo_event_bus: Some(std::sync::Arc::new(rustycode_bus::EventBus::new())),
                todo_dirty: Arc::new(AtomicBool::new(false)),
                storage: None,
                tool_manager: crate::services::tool_manager::ToolManager::new(),
                session_manager: crate::services::session_manager::SessionManager::new(
                    crate::app::session_recovery_integration::SessionRecoveryManager::new(
                        crate::app::session_recovery_integration::SessionRecoveryConfig::default(),
                    )
                    .ok(),
                ),
                hook_manager,
                skill_manager,
            },
            workspace: crate::app::state_model::TaskWorkspaceState {
                workspace_loaded: false,
                workspace_context: None,
                workspace_tasks: load_tasks(),
                last_extraction: None,
                workspace_scan_progress: None,
                git_branch: None,
            },
            session: crate::app::state_model::InteractionSessionState {
                messages: Vec::new(),
                streaming: crate::app::streaming_state::StreamingState::new(),
                plan_mode_banner: None,
                execution_trace: None,
                active_tools: std::collections::HashMap::new(),
                auto_continue: AutoContinueState::from_env(),
                turn_snapshot: None,
                doom_loop: crate::app::doom_loop::DoomLoopDetector::new(),
                pending_doom_note: None,
                reasoning_budget: std::sync::Mutex::new(
                    rustycode_tools::providers::reasoning_types::BudgetState::default(),
                ),
                session_recovery:
                    crate::app::session_recovery_integration::SessionRecoveryManager::new(
                        crate::app::session_recovery_integration::SessionRecoveryConfig::default(),
                    )
                    .ok(),
                session_sidebar: SessionSidebar::new(),
                wizard: WizardHandler::new(&cwd, reconfigure),
                undo: crate::app::undo_state::UndoState::new(),
            },
            sys: crate::app::state_model::SystemState {
                running: true,
                dirty: true,
                needs_full_redraw: false,
                compaction: crate::app::compaction_state::CompactionState::new(
                    context_monitor,
                    compaction_config,
                ),
                auto_memory,
                memory_injection_config,
                plugin_manager,
                input_mode: InputMode::SingleLine,
                renderer_mode,
            },
            overlays: crate::app::state_model::OverlayState {
                command_palette,
                showing_command_palette: false,
                model_selector: ModelSelector::with_models(all_available_models()),
                showing_provider_selector: false,
                file_selector: FileSelector::new(Vec::new()),
                showing_error: false,
                showing_plugin_manager: false,
                showing_marketplace_browser: false,
                last_esc_press: None,
                showing_skill_palette: false,
            },
            panels: crate::app::state_model::ToolExecutionPanel {
                tool_panel: ToolPanelState::new(),
                ast_phase_state: crate::ui::ast_progress::AstPhaseState::new(),
                clarification_panel: crate::ui::clarification::ClarificationPanel::hidden(),
                awaiting_clarification: false,
                tool_approval: crate::app::tool_approval_state::ToolApprovalState::new(
                    ToolApprovalManager::new(),
                ),
            },
            theme: crate::app::state_model::ThemeNotificationState {
                theme_colors,
                theme_preview,
                theme_switcher,
                toast_manager,
                error_manager,
            },
            model: crate::app::state_model::ProviderModelState {
                current_model: rustycode_llm::load_model_from_config().unwrap_or_else(|e| {
                    tracing::warn!("Failed to load model config: {}", e);
                    String::new()
                }),
                current_effort: "medium".to_string(),
                token_budget: crate::app::token_budget::TokenBudget::new(),
                plan_mode: {
                    use rustycode_orchestration::plan_mode::{PlanMode, PlanModeConfig};
                    use rustycode_protocol::AgentRole;

                    let mut plan_mode = PlanMode::new(PlanModeConfig::default());
                    plan_mode.set_role(AgentRole::Worker);
                    plan_mode
                },
                api_key_warning: Self::compute_api_key_warning(),
                show_task_dashboard: false,
            },
            search: crate::app::state_model::MessageSearchState {
                search_state: SearchState::new(),
                file_finder: crate::ui::file_finder::FileFinder::new(cwd.clone()),
                tag_filter: TagFilter::new(),
                message_areas: std::cell::RefCell::new(Vec::new()),
                message_line_offsets: std::cell::RefCell::new(Vec::new()),
            },
            team: crate::app::state_model::TeamModeState {
                team_panel: crate::ui::team_panel::TeamPanel::new(),
                team_handler: TeamModeHandler::new(),
                worker_panel: crate::ui::worker_panel::WorkerPanel::new(),
                agent_manager: AgentManager::new(),
            },
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
            ui: crate::app::state_model::UIComponents {
                message_renderer: MessageRenderer::new(),
                input_handler,
                animator: Animator::new(4, false),
                marketplace_browser,
                skill_palette,
                plugin_manager_ui,
                help_state: HelpState::new(),
                sidebar_area: std::cell::Cell::new(Rect::default()),
                view: crate::app::view_state::ViewState::new(),
                keyboard_handler: KeyboardShortcutHandler::new(false),
                tui_config: TUIConfig::default(),
                stashed_prompt: None,
                status_bar_collapsed: false,
                footer_collapsed: false,
            },
            integration: crate::app::state_model::ServiceIntegrationState {
                services,
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
                    let (pt, mdl, _) = rustycode_llm::load_provider_config_from_env()
                        .unwrap_or_else(|_| {
                            (
                                "anthropic".into(),
                                "claude-haiku-4-5-20251001".into(),
                                Default::default(),
                            )
                        });
                    let pipeline_provider = rustycode_llm::create_provider(&pt, &mdl)
                        .unwrap_or_else(|_| {
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
                rate_limit: RateLimitHandler::new(),
                rate_limit_tracker: crate::services::rate_limit_tracker::RateLimitTracker::default(
                ),
                lsp: LspStatus::new_forced_refresh(),
                mcp: McpStatus::new_forced_refresh(),
                mcp_manager: std::sync::Arc::new(tokio::sync::RwLock::new(
                    rustycode_mcp::McpServerManager::new(rustycode_mcp::ManagerConfig::default()),
                )),
                start_time: Instant::now(),
                event_receiver: tokio::sync::broadcast::channel(crate::app::EVENT_CHANNEL_CAPACITY)
                    .1,
                todo_state: rustycode_tools::todo::new_todo_state(),
                todo_event_bus: Some(std::sync::Arc::new(rustycode_bus::EventBus::new())),
                todo_dirty: Arc::new(AtomicBool::new(false)),
                storage: None,
                tool_manager: crate::services::tool_manager::ToolManager::new(),
                session_manager: crate::services::session_manager::SessionManager::new(None),
                hook_manager: rustycode_tools::hooks::HookManager::new(
                    PathBuf::from(".rustycode/hooks"),
                    rustycode_tools::hooks::HookProfile::Standard,
                    String::new(),
                ),
                skill_manager: Arc::new(RwLock::new(SkillStateManager::new())),
            },
            workspace: crate::app::state_model::TaskWorkspaceState {
                workspace_loaded: false,
                workspace_context: None,
                workspace_tasks: load_tasks(),
                last_extraction: None,
                workspace_scan_progress: None,
                git_branch: None,
            },
            session: crate::app::state_model::InteractionSessionState {
                messages: Vec::new(),
                streaming: crate::app::streaming_state::StreamingState::new(),
                plan_mode_banner: None,
                execution_trace: None,
                active_tools: std::collections::HashMap::new(),
                auto_continue: AutoContinueState::from_env(),
                turn_snapshot: None,
                doom_loop: crate::app::doom_loop::DoomLoopDetector::new(),
                pending_doom_note: None,
                reasoning_budget: std::sync::Mutex::new(
                    rustycode_tools::providers::reasoning_types::BudgetState::default(),
                ),
                session_recovery:
                    crate::app::session_recovery_integration::SessionRecoveryManager::new(
                        crate::app::session_recovery_integration::SessionRecoveryConfig::default(),
                    )
                    .ok(),
                session_sidebar: SessionSidebar::new(),
                wizard: WizardHandler::new(&PathBuf::from("."), false),
                undo: crate::app::undo_state::UndoState::new(),
            },
            sys: crate::app::state_model::SystemState {
                running: true,
                dirty: true,
                needs_full_redraw: false,
                compaction: crate::app::compaction_state::CompactionState::new(
                    context_monitor,
                    compaction_config,
                ),
                auto_memory,
                memory_injection_config,
                plugin_manager,
                input_mode: InputMode::SingleLine,
                renderer_mode,
            },
            overlays: crate::app::state_model::OverlayState {
                command_palette,
                showing_command_palette: false,
                model_selector: ModelSelector::with_models(all_available_models()),
                showing_provider_selector: false,
                file_selector: FileSelector::new(Vec::new()),
                showing_error: false,
                showing_plugin_manager: false,
                showing_marketplace_browser: false,
                last_esc_press: None,
                showing_skill_palette: false,
            },
            panels: crate::app::state_model::ToolExecutionPanel {
                tool_panel: ToolPanelState::new(),
                ast_phase_state: crate::ui::ast_progress::AstPhaseState::new(),
                clarification_panel: crate::ui::clarification::ClarificationPanel::hidden(),
                awaiting_clarification: false,
                tool_approval: crate::app::tool_approval_state::ToolApprovalState::new(
                    ToolApprovalManager::new(),
                ),
            },
            theme: crate::app::state_model::ThemeNotificationState {
                theme_colors,
                theme_preview,
                theme_switcher,
                toast_manager,
                error_manager,
            },
            model: crate::app::state_model::ProviderModelState {
                current_model: rustycode_llm::load_model_from_config().unwrap_or_else(|e| {
                    tracing::warn!("Failed to load model config: {}", e);
                    String::new()
                }),
                current_effort: "medium".to_string(),
                token_budget: crate::app::token_budget::TokenBudget::new(),
                plan_mode: {
                    use rustycode_orchestration::plan_mode::{PlanMode, PlanModeConfig};
                    use rustycode_protocol::AgentRole;

                    let mut plan_mode = PlanMode::new(PlanModeConfig::default());
                    plan_mode.set_role(AgentRole::Worker);
                    plan_mode
                },
                api_key_warning: String::new(),
                show_task_dashboard: false,
            },
            search: crate::app::state_model::MessageSearchState {
                search_state: SearchState::new(),
                file_finder: crate::ui::file_finder::FileFinder::new(PathBuf::from(".")),
                tag_filter: TagFilter::new(),
                message_areas: std::cell::RefCell::new(Vec::new()),
                message_line_offsets: std::cell::RefCell::new(Vec::new()),
            },
            team: crate::app::state_model::TeamModeState {
                team_panel: crate::ui::team_panel::TeamPanel::new(),
                team_handler: TeamModeHandler::new(),
                worker_panel: crate::ui::worker_panel::WorkerPanel::new(),
                agent_manager: AgentManager::new(),
            },
        }
    }

    /// Initialize all background services
    pub fn init_services(&mut self) -> Result<()> {
        crate::info_log!("init_services starting");

        if self.integration.storage.is_none() {
            let db_path =
                rustycode_tools::workspace::paths::AppPaths::data_dir().join("rustycode.db");
            match rustycode_storage::Storage::open(&db_path) {
                Ok(s) => {
                    tracing::debug!("Opened storage at {}", db_path.display());
                    let storage_arc = std::sync::Arc::new(s);
                    self.integration.storage = Some(storage_arc.clone());

                    let cwd = self.integration.services.cwd();
                    let reloaded = load_tasks_from_storage(storage_arc.as_ref(), cwd);
                    let had_json_data = !self.workspace.workspace_tasks.tasks.is_empty()
                        || !self.workspace.workspace_tasks.todos.is_empty();
                    if !reloaded.tasks.is_empty() || !reloaded.todos.is_empty() || !had_json_data {
                        self.workspace.workspace_tasks = reloaded;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to open storage at {}: {} (todos will be ephemeral)",
                        db_path.display(),
                        e
                    );
                }
            }
        }

        let config = ConversationConfig::default();
        let mut tool_registry = ToolRegistry::new();

        // Register built-in tools - these are essential for AI coding assistant functionality
        self.register_builtin_tools(&mut tool_registry);

        if let Some(ref bus) = self.integration.todo_event_bus {
            let dirty_flag = self.integration.todo_dirty.clone();
            use rustycode_shared_runtime::SHARED_RUNTIME;
            let _sub_handle =
                SHARED_RUNTIME.block_on(bus.subscribe_callback("todo.updated", move |_event| {
                    dirty_flag.store(true, Ordering::SeqCst);
                    Ok(())
                }));
        }

        // Register structured thinking tool for AgentSession path
        tool_registry.register(
            rustycode_orchestration::structured_thinking_tool_impl::StructuredThinkingTool,
        );

        // Load MCP tools if MCP servers are configured
        self.load_mcp_tools(&mut tool_registry);

        // Count tools before moving registry
        let tool_count = tool_registry.list().len();

        self.integration
            .services
            .start_conversation(config, tool_registry)
            .context("failed to start conversation service")?;
        crate::info_log!(
            "start_conversation OK, pipeline={}",
            self.integration.services.has_pipeline()
        );
        self.integration
            .services
            .start_workspace_loading()
            .context("failed to start workspace loading")?;

        self.refresh_mcp_status(true);

        // Wire shared todo state into service manager so LLM can use todo tools
        self.integration
            .services
            .set_todo_state(self.integration.todo_state.clone());

        tracing::info!("Services initialized with {} tools", tool_count);

        Ok(())
    }

    /// Refresh sidebar LSP status from discovery.
    fn refresh_lsp_status(&mut self, force: bool) -> bool {
        if !force && self.integration.lsp.last_lsp_refresh.elapsed() < REFRESH_COOLDOWN {
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
            || display_connected != self.integration.lsp.last_lsp_connected
            || lsp_names != self.integration.lsp.last_lsp_servers;

        if changed {
            self.session.session_sidebar.update_lsp_status(
                display_connected,
                lsp_names.clone(),
                std::collections::HashMap::new(),
            );
            self.integration.lsp.last_lsp_connected = display_connected;
            self.integration.lsp.last_lsp_servers = lsp_names;
            self.sys.dirty = true;
        }

        self.integration.lsp.last_lsp_refresh = Instant::now();
        changed
    }

    /// Refresh sidebar MCP status from the live proxy cache and config discovery.
    fn refresh_mcp_status(&mut self, force: bool) -> bool {
        if !force && self.integration.mcp.last_mcp_refresh.elapsed() < REFRESH_COOLDOWN {
            return false;
        }

        let connected_servers: HashSet<String> =
            if let Some(mcp_proxies) = self.integration.tool_manager.mcp_proxies() {
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

        if mcp_servers.is_empty() && self.integration.tool_manager.mcp_proxies().is_some() {
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
            || mcp_connected != self.integration.mcp.last_mcp_connected
            || mcp_servers != self.integration.mcp.last_mcp_servers;

        if changed {
            self.session
                .session_sidebar
                .update_mcp_status(mcp_connected, mcp_servers.clone());
            self.integration.mcp.last_mcp_connected = mcp_connected;
            self.integration.mcp.last_mcp_servers = mcp_servers;
            self.sys.dirty = true;
        }

        self.integration.mcp.last_mcp_refresh = Instant::now();
        changed
    }

    /// Refresh sidebar tool call summary from the current tool history.
    fn refresh_tool_call_summary(&mut self) {
        let recent = self
            .panels
            .tool_panel
            .tool_panel_history
            .last()
            .map(|tool| format!("{} {}", tool.status.icon(), tool.result_summary));
        self.session
            .session_sidebar
            .update_tool_call_summary(self.session.active_tools.len(), recent);
    }

    /// Resume the most recent session from disk.
    ///
    /// Called when `--resume` flag is passed on the CLI. Finds the most
    /// recently saved session and loads its messages/scroll state.
    pub fn resume_most_recent_session(&mut self) {
        match self.integration.session_manager.find_most_recent_session() {
            Ok(Some(session)) => {
                self.reset_conversation_state();
                self.ui.view.scroll_offset_line = session.scroll_position;
                self.session.messages = session.messages;
                self.sys
                    .compaction
                    .context_monitor
                    .update(&self.session.messages);
                if !self.session.messages.is_empty() {
                    self.ui.view.selected_message = self.session.messages.len().saturating_sub(1);
                }

                if let Some(ref storage) = self.integration.storage {
                    match storage.todos(&session.session_id) {
                        Ok(db_todos) if !db_todos.is_empty() => {
                            let items: Vec<rustycode_tools::todo::TodoItem> = db_todos
                                .into_iter()
                                .filter(|t| {
                                    !matches!(
                                        t.status,
                                        rustycode_storage::task_store::TodoStatus::Cancelled
                                    )
                                })
                                .map(|t| rustycode_tools::todo::TodoItem {
                                    id: t.id,
                                    title: t.content,
                                    status: match t.status {
                                        rustycode_storage::task_store::TodoStatus::Pending => {
                                            rustycode_tools::todo::TodoStatus::Pending
                                        }
                                        rustycode_storage::task_store::TodoStatus::InProgress => {
                                            rustycode_tools::todo::TodoStatus::InProgress
                                        }
                                        rustycode_storage::task_store::TodoStatus::Completed => {
                                            rustycode_tools::todo::TodoStatus::Completed
                                        }
                                        _ => rustycode_tools::todo::TodoStatus::Pending,
                                    },
                                    active_form: None,
                                })
                                .collect();
                            if let Ok(mut state) = self.integration.todo_state.lock() {
                                *state = items;
                            }
                            crate::app::tasks::sync_from_todo_state(
                                &mut self.workspace.workspace_tasks,
                                &self.integration.todo_state,
                            );
                            tracing::info!(
                                "Restored {} todos for session {}",
                                self.integration
                                    .todo_state
                                    .lock()
                                    .map(|s| s.len())
                                    .unwrap_or(0),
                                session.session_id
                            );
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(
                                "Failed to load todos for session {}: {}",
                                session.session_id,
                                e
                            );
                        }
                    }
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
                self.sys.dirty = true;
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
        self.integration.tool_manager.register_builtin_tools(
            tool_registry,
            &self.integration.pipeline_ctx.provider,
            &self.integration.pipeline_ctx.current_model,
            self.integration.services.cwd(),
            &self.integration.skill_manager,
            &self.integration.todo_state,
            self.integration.storage.clone(),
            self.integration.todo_event_bus.clone(),
        );
    }

    /// Load tools from configured MCP servers
    fn load_mcp_tools(&mut self, tool_registry: &mut ToolRegistry) {
        self.integration.tool_manager.load_mcp_tools(tool_registry);
    }

    /// Run the TUI main loop
    pub fn run(&mut self, resume: bool) -> Result<()> {
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
        if let Some(dir_name) = self
            .integration
            .services
            .cwd()
            .file_name()
            .and_then(|n| n.to_str())
        {
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

        // Render loading screen immediately
        let loading_widget = ratatui::widgets::Paragraph::new(" Initializing services... ")
            .style(ratatui::style::Style::default().fg(ratatui::style::Color::Yellow));
        terminal.draw(|f| {
            f.render_widget(loading_widget, f.area());
        })?;

        let t_init = Instant::now();
        if let Err(e) = self.init_services() {
            tracing::warn!(
                "Service initialization failed (TUI will run in degraded mode): {}",
                e
            );
        }
        crate::info_log!(
            "[PERF] init_services took {}ms",
            t_init.elapsed().as_millis()
        );

        if resume {
            self.resume_most_recent_session();
        }

        crate::info_log!(
            "[PERF] total startup took {}ms",
            t_init.elapsed().as_millis()
        );

        // Cleanup happens automatically when _cleanup_guard goes out of scope

        let mut startup_notes: Vec<String> = Vec::new();

        if let Some(ref recovery) = self.session.session_recovery {
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

        while self.sys.running {
            loop_iterations += 1;

            // Check for shutdown signal (Ctrl+C)
            if shutdown_rx.try_recv().is_ok() {
                // User requested shutdown
                self.sys.running = false;
                break;
            }

            let frame_start = Instant::now();
            let debug_enabled = crate::logging::is_debug_enabled();

            // Calculate delta time for animations (in milliseconds)
            let delta_ms = last_frame_time.elapsed().as_millis() as u64;
            last_frame_time = frame_start;

            // Phase 1: Update animations (only marks dirty when frame actually advances)
            let animation_start = Instant::now();
            if self.ui.animator.update() {
                // Only mark dirty if an animation is visible (streaming or active tools)
                if self.session.streaming.is_streaming || !self.session.active_tools.is_empty() {
                    self.sys.dirty = true;
                }
            }
            let animation_elapsed = animation_start.elapsed();

            // Update session sidebar info
            let sidebar_start = Instant::now();
            self.session
                .session_sidebar
                .update_session_info(self.session.messages.len(), self.session.active_tools.len());
            self.session
                .session_sidebar
                .set_rate_limited(self.integration.rate_limit.until.is_some());
            self.refresh_tool_call_summary();
            self.refresh_lsp_status(false);
            self.refresh_mcp_status(false);
            let sidebar_elapsed = sidebar_start.elapsed();

            // Update toast animations
            let toast_start = Instant::now();
            let has_active_toasts = self.theme.toast_manager.tick(delta_ms);
            if has_active_toasts {
                self.sys.dirty = true; // Mark dirty for animation updates
            }
            let toast_elapsed = toast_start.elapsed();

            // Error auto-dismiss: If error_manager is showing, mark dirty so
            // the next render can check is_showing() and clear the error overlay
            // after the auto-dismiss timeout (10s). Without this, the error
            // indicator persists indefinitely when no other state changes occur.
            if self.theme.error_manager.is_showing() {
                self.sys.dirty = true;
            }

            // Phase 2: Poll async sources (ONE item each)
            let service_poll_start = Instant::now();
            self.poll_services()
                .context("failed to poll background services")?;
            self.poll_mcp_events()?;

            if self.integration.todo_dirty.swap(false, Ordering::SeqCst) {
                if crate::app::tasks::sync_from_todo_state(
                    &mut self.workspace.workspace_tasks,
                    &self.integration.todo_state,
                ) {
                    crate::app::tasks::save_tasks_with_storage(
                        &self.workspace.workspace_tasks,
                        self.integration.storage.as_deref(),
                        self.integration.services.cwd(),
                        None,
                    );
                }
                self.sys.dirty = true;
            }

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
                self.sys.dirty = true; // Mark dirty if countdown updated
            }
            let countdown_elapsed = countdown_start.elapsed();

            // Update running agents
            let agents_start = Instant::now();
            self.team.agent_manager.update_running_agents();

            // Periodic cleanup: remove completed/failed agents older than 1 hour
            // and cap total terminal agents at 50
            self.team.agent_manager.cleanup_old_agents(3600);
            self.team.agent_manager.cleanup_excess_agents(50);
            let agents_elapsed = agents_start.elapsed();

            // Session auto-save (every 30s when dirty)
            let autosave_start = Instant::now();
            if let Some(ref mut recovery) = self.session.session_recovery {
                if recovery.should_auto_save() {
                    let state = recovery.create_state(
                        &self.session.messages,
                        self.ui.view.scroll_offset_line,
                        self.session.execution_trace.clone(),
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
                let should_render = self.sys.dirty || frame_count < 3;

                if should_render {
                    let render_start = Instant::now();
                    if self.sys.needs_full_redraw {
                        terminal
                            .clear()
                            .context("failed to clear terminal for full redraw")?;
                        self.sys.needs_full_redraw = false;
                    }
                    terminal
                        .draw(|f| self.render(f))
                        .context("failed to draw TUI frame")?;
                    frame_count += 1;
                    self.sys.dirty = false;
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
                    self.sys.dirty,
                    rendered,
                    input_polled,
                    input_handled,
                    self.session.messages.len(),
                    self.session.active_tools.len(),
                    self.session.streaming.is_streaming,
                    self.ui.view.user_scrolled,
                    self.ui.view.viewport_height
                );
            }
        }

        // Cleanup: stop any active stream
        if self.session.streaming.is_streaming {
            self.integration.services.submit_op(Op::StopStream).ok();
            self.session.streaming.stream_cancelled = true;
            // Don't set is_streaming=false here — let the async stream task's
            // Done handler clean up to avoid racing with channel receivers.
        }

        // Shutdown MCP servers to prevent orphaned child processes
        if let Some(mcp_proxies) = self.integration.tool_manager.mcp_proxies() {
            let proxies = mcp_proxies.clone();
            // Spawn a small tokio runtime for async cleanup since we're in sync context.
            let _ = std::thread::spawn(move || {
                rustycode_shared_runtime::block_on_shared(async move {
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
            })
            .join();
        }

        // Reset terminal title on exit so it doesn't show stale rustycode state
        print!("\x1b]0;\x07");
        let _ = std::io::stdout().flush();

        // Save history on exit
        self.save_history();

        // Session recovery shutdown: save state and release lock
        if let Some(ref mut recovery) = self.session.session_recovery {
            let state = recovery.create_state(
                &self.session.messages,
                self.ui.view.scroll_offset_line,
                self.session.execution_trace.clone(),
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
            self.ui.input_handler.state.mode = crate::ui::input_state::InputMode::MultiLine;
        }
        self.ui.input_handler.state.insert_text_at_cursor(content);

        // Switch to multi-line mode when pasting content with newlines,
        // matching the behavior of the Ctrl+V paste handler.
        if content.contains('\n') {
            self.ui.input_handler.state.mode = crate::ui::input_state::InputMode::MultiLine;
            self.sys.input_mode = crate::ui::input_state::InputMode::MultiLine;
        }

        self.sys.dirty = true;
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
            self.sys.dirty = true;
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
                let current = self.model.plan_mode.current_phase();
                match current {
                    "planning" => {
                        if let Err(e) = self.model.plan_mode.approve() {
                            tracing::warn!("Plan approval failed: {}", e);
                            self.add_system_message(format!("Plan approval failed: {}", e));
                            return Ok(());
                        }
                        self.model
                            .plan_mode
                            .set_role(rustycode_protocol::AgentRole::Worker);
                        self.integration
                            .services
                            .set_ai_mode(crate::services::agent_mode::AiMode::Ask);
                        self.clear_plan_mode_banner();
                        self.add_system_message(
                            "Plan mode: switched to implementation phase".to_string(),
                        );
                    }
                    _ => {
                        self.model.plan_mode.reset();
                        self.model
                            .plan_mode
                            .set_role(rustycode_protocol::AgentRole::Planner);
                        self.integration
                            .services
                            .set_ai_mode(crate::services::agent_mode::AiMode::Plan);
                        self.show_planning_banner("Manual");
                        self.add_system_message(
                            "Plan mode: switched to planning phase".to_string(),
                        );
                    }
                }
                self.sys.dirty = true;
                return Ok(());
            }

            let task = task_text.unwrap_or_default();
            let current = self.model.plan_mode.current_phase();
            if current != "planning" {
                self.model.plan_mode.reset();
                self.model
                    .plan_mode
                    .set_role(rustycode_protocol::AgentRole::Planner);
                self.integration
                    .services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Plan);
                self.show_planning_banner("Manual");
                self.add_system_message("Plan mode: switched to planning phase".to_string());
            }
            self.sys.dirty = true;
            self.auto_scroll();
            self.integration
                .services
                .submit_op(Op::SendMessage { content: task })?;
            return Ok(());
        }

        // Handle /r locally as regenerate alias
        if matches!(parts[0], "/r" | "/regen" | "/regenerate") {
            self.regenerate_last_response()?;
            return Ok(());
        }

        // Handle AI mode switching commands
        if matches!(parts[0], "/yolo" | "/auto") {
            let current_mode = self.integration.services.ai_mode();
            if matches!(current_mode, crate::services::agent_mode::AiMode::Yolo) {
                self.exit_plan_mode();
                self.integration
                    .services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Ask);
                self.add_system_message(
                    "🔒 YOLO mode deactivated — tools require approval again.".to_string(),
                );
            } else {
                self.exit_plan_mode();
                self.integration
                    .services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Yolo);
                self.add_system_message(
                    "🚀 YOLO mode activated — tools auto-approved, fully autonomous.\n\
                     Use /yolo again or /ask to return to approval mode."
                        .to_string(),
                );
            }
            self.sys.dirty = true;
            self.auto_scroll();
            if parts.len() > 1 {
                let task = parts[1..].join(" ");
                self.integration
                    .services
                    .submit_op(Op::SendMessage { content: task })?;
            }
            return Ok(());
        }

        if matches!(parts[0], "/act") {
            let current_mode = self.integration.services.ai_mode();
            if matches!(current_mode, crate::services::agent_mode::AiMode::Act) {
                self.exit_plan_mode();
                self.integration
                    .services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Ask);
                self.add_system_message(
                    "💬 ACT mode deactivated — tools require approval again.".to_string(),
                );
            } else {
                self.exit_plan_mode();
                self.integration
                    .services
                    .set_ai_mode(crate::services::agent_mode::AiMode::Act);
                self.add_system_message(
                    "⚡ ACT mode — execute with brief summaries, minimal approval.\n\
                     Use /act again or /ask to return to full approval mode."
                        .to_string(),
                );
            }
            self.sys.dirty = true;
            self.auto_scroll();
            if parts.len() > 1 {
                let task = parts[1..].join(" ");
                self.integration
                    .services
                    .submit_op(Op::SendMessage { content: task })?;
            }
            return Ok(());
        }

        if matches!(parts[0], "/ask") {
            self.exit_plan_mode();
            self.integration
                .services
                .set_ai_mode(crate::services::agent_mode::AiMode::Ask);
            self.add_system_message(
                "💬 ASK mode — tools require approval, full summaries.".to_string(),
            );
            self.sys.dirty = true;
            self.auto_scroll();
            if parts.len() > 1 {
                let task = parts[1..].join(" ");
                self.integration
                    .services
                    .submit_op(Op::SendMessage { content: task })?;
            }
            return Ok(());
        }

        if let Some(command_tx) = self.integration.services.command_sender() {
            let cwd = self.integration.services.cwd().clone();
            let effect = dispatch_registered_slash_command(
                input,
                CommandContext {
                    cwd: &cwd,
                    command_tx,
                    workspace_tasks: &mut self.workspace.workspace_tasks,
                    messages: &mut self.session.messages,
                    current_stream_content: &mut self.session.streaming.current_stream_content,
                    is_streaming: &mut self.session.streaming.is_streaming,
                    last_extraction: &mut self.workspace.last_extraction,
                    services: &mut self.integration.services,
                    agent_manager: &mut self.team.agent_manager,
                    memory_injection_config: &mut self.sys.memory_injection_config,
                    theme_colors: &self.theme.theme_colors,
                    skill_manager: &self.integration.skill_manager,
                    plugin_manager: &self.sys.plugin_manager,
                    running: &mut self.sys.running,
                    context_monitor: &mut self.sys.compaction.context_monitor,
                    compaction_config: &mut self.sys.compaction.compaction_config,
                    showing_compaction_preview: &mut self.sys.compaction.showing_preview,
                    pending_compaction: &mut self.sys.compaction.pending,
                    file_undo_stack: &mut self.session.undo,
                    session_input_tokens: self.model.token_budget.session_input_tokens,
                    session_output_tokens: self.model.token_budget.session_output_tokens,
                    session_cost_usd: self.model.token_budget.session_cost_usd,
                    current_model: self.model.current_model.clone(),
                    session_start: self.integration.start_time,
                    mcp_manager: self.integration.mcp_manager.clone(),
                },
            )?;

            if let Some(effect) = effect {
                self.apply_slash_command_effect(effect)?;
                self.ui.view.user_scrolled = false;
                self.sys.dirty = true;
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

        self.sys.dirty = true;
        self.auto_scroll();
        Ok(())
    }

    /// Render the active frame backend.
    pub(crate) fn render(&mut self, frame: &mut ratatui::Frame) {
        let debug_enabled = crate::logging::is_debug_enabled();
        let render_start = Instant::now();
        let renderer = self.sys.renderer_mode;
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
                    self.session.messages.len(),
                    self.session.streaming.is_streaming,
                    self.sys.dirty
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
        let input_text = self.ui.input_handler.state.all_text();
        let input_line_count = input_text.lines().count().max(1);
        let input_rows: u16 = if input_line_count > 1 {
            2u16.saturating_add(input_line_count.min(6) as u16)
        } else {
            2
        };

        let size = frame.area();
        let header_rows: u16 = if self.ui.status_bar_collapsed { 0 } else { 1 };
        let footer_rows: u16 = if self.ui.footer_collapsed { 0 } else { 1 };
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
                    self.session.session_sidebar.is_visible(),
                    self.ui.view.scroll_offset_line,
                    self.ui.view.user_scrolled,
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
                self.session.messages.len(),
                total_lines,
                self.session.streaming.is_streaming
            );
        }

        if let Some(sidebar_area) = layout.sidebar_area {
            self.session.session_sidebar.render(frame, sidebar_area);
        }

        // Overlay panels on top of brutalist UI

        // Worker status panel overlay (Ctrl+W)
        if self.team.worker_panel.visible {
            let panel_width = 50u16.min(size.width.saturating_sub(10));
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let x = size.width.saturating_sub(panel_width);
            let y = 2u16;
            let panel_area = Rect::new(x, y, panel_width, panel_height);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            self.team
                .worker_panel
                .render(panel_area, frame.buffer_mut());
        }

        // Team agent timeline overlay (Ctrl+G)
        if self.team.team_panel.visible {
            let panel_width = 60u16.min(size.width.saturating_sub(10));
            let panel_height = 20u16.min(size.height.saturating_sub(4));
            let x = size.width.saturating_sub(panel_width);
            let y = 2u16;
            let panel_area = Rect::new(x, y, panel_width, panel_height);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            frame.render_widget(self.team.team_panel.clone(), panel_area);
        }

        // Overlay: clarification panel (when AI asks a question)
        if self.panels.awaiting_clarification && self.panels.clarification_panel.visible {
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let panel_width = (size.width * 3 / 4).min(60);
            let panel_area =
                crate::app::render::shared::centered_rect(panel_width, panel_height, size);
            frame.render_widget(ratatui::widgets::Clear, panel_area);
            frame.render_widget(self.panels.clarification_panel.clone(), panel_area);
        }

        // Overlay: search box (position at bottom of message area, not over footer)
        if self.search.search_state.visible {
            let search_area = Rect {
                x: size.x,
                y: header_rows,
                width: size.width,
                height: main_height,
            };
            crate::app::renderer::render_search_box(self, frame, search_area);
        }

        // Tool panel overlay (Ctrl+P) - over message area
        if self.panels.tool_panel.showing_tool_panel {
            let tool_area = Rect {
                x: size.x,
                y: header_rows,
                width: size.width,
                height: main_height,
            };
            crate::app::renderer::render_tool_panel(self, frame, tool_area);
        }

        // Overlay: provider selector
        if self.overlays.showing_provider_selector {
            crate::app::renderer::render_provider_selector(frame);
        }

        // Overlay: file finder
        if self.search.file_finder.is_visible() {
            self.search.file_finder.render(frame, size);
        }

        // Overlay: model selector (Alt+P)
        if self.overlays.model_selector.is_visible() {
            self.overlays.model_selector.render(frame, size);
        }

        // Overlay: file selector (@)
        if self.overlays.file_selector.is_visible() {
            self.overlays.file_selector.render(frame, size);
        }

        // Overlay: skill palette
        if self.ui.skill_palette.is_visible() {
            self.ui.skill_palette.render(frame, size);
        }

        if self.overlays.showing_plugin_manager {
            let mut manager = self
                .sys
                .plugin_manager
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let _ = manager.reload_from_disk();
            self.ui.plugin_manager_ui.render(frame, size, &manager);
        }

        if self.overlays.showing_marketplace_browser {
            self.ui.marketplace_browser.render(frame, size);
        }

        // Overlay: theme preview
        if self.theme.theme_preview.is_visible() {
            self.theme.theme_preview.render(frame, size);
        }

        // Overlay: command palette (Ctrl+K / Ctrl+Shift+P)
        if self.overlays.command_palette.is_visible() {
            let input = self.ui.input_handler.state.all_text();
            self.overlays.command_palette.sync_query_from_input(&input);
            self.overlays.command_palette.render(frame, size);
        }

        // Overlay: help panel (?)
        if self.ui.help_state.visible {
            crate::help::render_help(frame, size, &self.ui.help_state);
        }

        // Overlay: approval dialog (before error display so errors can appear on top)
        if self.panels.tool_approval.awaiting {
            if let Some(req) = self.panels.tool_approval.pending_requests.front() {
                let (panel_height, panel_width) =
                    crate::tool_approval::approval_panel_size(req, size);
                let panel_area =
                    crate::app::render::shared::centered_rect(panel_width, panel_height, size);
                crate::tool_approval::render_approval_prompt(frame, panel_area, req, size);
            }
        }

        // Overlay: error display
        if self.theme.error_manager.is_showing() {
            self.theme.error_manager.render(frame, size);
        }

        // Overlay: compaction preview (while pending)
        if self.sys.compaction.showing_preview {
            self.render_compaction_preview(frame, size);
        }

        // Overlay: first-run wizard (covers entire screen)
        if self.session.wizard.showing_wizard {
            if let Some(ref mut wizard) = self.session.wizard.wizard {
                frame.render_widget(ratatui::widgets::Clear, size);
                wizard.render(frame, size);
            }
        }

        // Overlay: toast notifications (topmost — always visible)
        self.theme.toast_manager.render(
            frame,
            size,
            Some(
                &self
                    .theme
                    .theme_colors
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            ),
        );

        if debug_enabled {
            let total_elapsed = render_start.elapsed();
            if total_elapsed > DEBUG_SLOW_THRESHOLD {
                crate::debug_log!(
                    "Brutalist render ran long: width={} height={} messages={} total_lines={} heights={} layout_ms={} draw_ms={} total_ms={} streaming={} user_scrolled={} selected_message={}",
                    size.width,
                    size.height,
                    self.session.messages.len(),
                    total_lines,
                    layout.heights.len(),
                    layout_elapsed.as_millis(),
                    draw_elapsed.as_millis(),
                    total_elapsed.as_millis(),
                    self.session.streaming.is_streaming,
                    self.ui.view.user_scrolled,
                    self.ui.view.selected_message
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
        self.model.current_effort = crate::services::provider_manager::cycle_effort_level(
            &self.model.current_effort,
            &self.model.current_model,
        );
        self.integration
            .services
            .set_effort(self.model.current_effort.clone());
        self.model.current_effort.clone()
    }

    /// Save command history on exit
    pub(crate) fn save_history(&mut self) {
        let history = self.ui.input_handler.history();
        crate::services::session_manager::SessionManager::save_history(history);
    }
}

#[cfg(test)]
impl Default for TUI {
    fn default() -> Self {
        Self::new_for_test()
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
