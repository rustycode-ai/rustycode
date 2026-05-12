//! TUI State Model
//!
//! Groups related TUI fields into logical sub-structs to reduce the god object problem.

use crate::agents::AgentManager;
use crate::app::auto_continue_state::AutoContinueState;
use crate::app::compaction_state::CompactionState;
use crate::app::doom_loop::DoomLoopDetector;
use crate::app::pipeline::registry::{PipelineContext, PipelineRegistry};
use crate::app::pipeline::scheduler::ScheduledPhaseEvent;
use crate::app::plan_mode_ops::PlanModeBanner;
use crate::app::rate_limit_handler::RateLimitHandler;
use crate::app::service_integration::ServiceManager;
use crate::app::streaming_state::StreamingState;
use crate::app::tasks::{Task, Todo, WorkspaceTasks};
use crate::app::team_mode_handler::TeamModeHandler;
use crate::app::token_budget::TokenBudget;
use crate::app::tool_approval_state::ToolApprovalState;
use crate::app::tool_panel_state::ToolPanelState;
use crate::app::turn_snapshot::TurnSnapshot;
use crate::help::HelpState;
use crate::memory::memory_auto::ThreadSafeAutoMemory;
use crate::memory::memory_injection::InjectionConfig;
use crate::plugin::PluginManager;
use crate::plugin::PluginManagerUI;
use crate::services::rate_limit_tracker::RateLimitTracker;
use crate::theme::ThemeColors;
use crate::ui::animator::Animator;
use crate::ui::ast_progress::AstPhaseState;
use crate::ui::clarification::ClarificationPanel;
use crate::ui::command_palette::CommandPalette;
use crate::ui::errors::ErrorManager;
use crate::ui::file_finder::FileFinder;
use crate::ui::file_selector::FileSelector;
use crate::ui::input::{InputHandler, InputMode};
use crate::ui::marketplace_browser::MarketplaceBrowser;
use crate::ui::message::MessageRenderer;
use crate::ui::message::{Message, ToolExecution};
use crate::ui::message_search::SearchState;
use crate::ui::message_tags::TagFilter;
use crate::ui::model_selector::ModelSelector;
use crate::ui::skill_palette::SkillPalette;
use crate::ui::theme_preview::{ThemePreview, ThemeSwitcher};
use crate::ui::toast::ToastManager;
use ratatui::layout::Rect;
use rustycode_tools::providers::reasoning_types::BudgetState;

/// UI Components sub-struct
///
/// Groups all UI rendering, interaction, and configuration components.
pub(crate) struct UIComponents {
    pub(crate) message_renderer: MessageRenderer,
    pub(crate) input_handler: InputHandler,
    pub(crate) animator: Animator,
    pub(crate) marketplace_browser: MarketplaceBrowser,
    pub(crate) skill_palette: SkillPalette,
    pub(crate) plugin_manager_ui: PluginManagerUI,
    pub(crate) help_state: HelpState,
    pub(crate) sidebar_area: std::cell::Cell<Rect>,
    pub(crate) view: crate::app::view_state::ViewState,
    pub(crate) tui_config: crate::services::config::TUIConfig,
    pub(crate) keyboard_handler: crate::app::keyboard_shortcuts::KeyboardShortcutHandler,
    pub(crate) stashed_prompt: Option<String>,
    pub(crate) status_bar_collapsed: bool,
    pub(crate) footer_collapsed: bool,
}

/// Service Integration sub-struct
///
/// Groups background service management, pipeline execution, and external integrations.
pub(crate) struct ServiceIntegrationState {
    pub(crate) services: ServiceManager,
    pub(crate) pipeline: PipelineRegistry,
    pub(crate) pipeline_ctx: PipelineContext,
    pub(crate) scheduler_rx: Option<std::sync::mpsc::Receiver<ScheduledPhaseEvent>>,
    pub(crate) active_scheduled_phases: std::collections::HashSet<String>,
    pub(crate) max_concurrent_phases: usize,
    pub(crate) rate_limit: RateLimitHandler,
    pub(crate) rate_limit_tracker: RateLimitTracker,
    pub(crate) lsp: crate::app::lsp_status::LspStatus,
    pub(crate) mcp: crate::app::mcp_status::McpStatus,
    pub(crate) mcp_manager: std::sync::Arc<tokio::sync::RwLock<rustycode_mcp::McpServerManager>>,
    pub(crate) start_time: std::time::Instant,
    pub(crate) event_receiver: tokio::sync::broadcast::Receiver<rustycode_mcp::protocol::McpEvent>,
    pub(crate) todo_state: rustycode_tools::todo::TodoState,
    pub(crate) todo_event_bus: Option<std::sync::Arc<rustycode_bus::EventBus>>,
    pub(crate) todo_dirty: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(crate) storage: Option<std::sync::Arc<rustycode_storage::Storage>>,
    pub(crate) tool_manager: crate::services::tool_manager::ToolManager,
    pub(crate) session_manager: crate::services::session_manager::SessionManager,
    pub(crate) hook_manager: rustycode_tools::hooks::HookManager,
    pub(crate) skill_manager: std::sync::Arc<std::sync::RwLock<crate::skills::SkillStateManager>>,
}

/// Task Workspace sub-struct
///
/// Groups workspace loading, task extraction, and git branch state.
pub(crate) struct TaskWorkspaceState {
    pub(crate) workspace_loaded: bool,
    pub(crate) workspace_context: Option<String>,
    pub(crate) workspace_tasks: WorkspaceTasks,
    pub(crate) last_extraction: Option<(Vec<Task>, Vec<Todo>)>,
    pub(crate) workspace_scan_progress: Option<(usize, usize)>,
    pub(crate) git_branch: Option<String>,
}

/// Interaction Session sub-struct
///
/// Groups conversational state, tool execution, reasoning tracking, and session UI.
pub(crate) struct InteractionSessionState {
    pub(crate) messages: Vec<Message>,
    pub(crate) streaming: StreamingState,
    pub(crate) plan_mode_banner: Option<PlanModeBanner>,
    pub(crate) execution_trace: Option<serde_json::Value>,
    pub(crate) active_tools: std::collections::HashMap<String, ToolExecution>,
    pub(crate) auto_continue: AutoContinueState,
    pub(crate) turn_snapshot: Option<TurnSnapshot>,
    pub(crate) doom_loop: DoomLoopDetector,
    pub(crate) pending_doom_note: Option<String>,
    pub(crate) reasoning_budget: std::sync::Mutex<BudgetState>,
    pub(crate) session_recovery:
        Option<crate::app::session_recovery_integration::SessionRecoveryManager>,
    pub(crate) session_sidebar: crate::ui::session_sidebar::SessionSidebar,
    pub(crate) wizard: crate::app::wizard_handler::WizardHandler,
    pub(crate) undo: crate::app::undo_state::UndoState,
}

/// System State sub-struct
///
/// Groups runtime flags, memory systems, plugin management, and mode state.
pub(crate) struct SystemState {
    pub(crate) running: bool,
    pub(crate) dirty: bool,
    pub(crate) needs_full_redraw: bool,
    pub(crate) compaction: CompactionState,
    pub(crate) auto_memory: Option<std::sync::Arc<ThreadSafeAutoMemory>>,
    pub(crate) memory_injection_config: InjectionConfig,
    pub(crate) plugin_manager: std::sync::Arc<std::sync::RwLock<PluginManager>>,
    pub(crate) input_mode: InputMode,
    pub(crate) renderer_mode: crate::app::renderer::RendererMode,
}

/// Overlay State sub-struct
///
/// Groups overlay UI components (palettes, selectors, modals) and their visibility flags.
pub(crate) struct OverlayState {
    pub(crate) command_palette: CommandPalette,
    pub(crate) showing_command_palette: bool,
    pub(crate) model_selector: ModelSelector,
    pub(crate) showing_provider_selector: bool,
    pub(crate) file_selector: FileSelector,
    pub(crate) showing_error: bool,
    pub(crate) showing_plugin_manager: bool,
    pub(crate) showing_marketplace_browser: bool,
    pub(crate) last_esc_press: Option<std::time::Instant>,
    pub(crate) showing_skill_palette: bool,
}

/// Tool Execution Panel sub-struct
///
/// Groups tool panel display, AST progress, clarification, and approval state.
pub(crate) struct ToolExecutionPanel {
    pub(crate) tool_panel: ToolPanelState,
    pub(crate) ast_phase_state: AstPhaseState,
    pub(crate) clarification_panel: ClarificationPanel,
    pub(crate) awaiting_clarification: bool,
    pub(crate) tool_approval: ToolApprovalState,
}

/// Theme and Notification State sub-struct
///
/// Groups theme management, toast notifications, and error display.
pub(crate) struct ThemeNotificationState {
    pub(crate) theme_colors: std::sync::Arc<std::sync::Mutex<ThemeColors>>,
    pub(crate) theme_preview: ThemePreview,
    pub(crate) theme_switcher: ThemeSwitcher,
    pub(crate) toast_manager: ToastManager,
    pub(crate) error_manager: ErrorManager,
}

/// Team Mode State sub-struct
///
/// Groups team panel, handler, worker panel, and agent management.
pub(crate) struct TeamModeState {
    pub(crate) team_panel: crate::ui::team_panel::TeamPanel,
    pub(crate) team_handler: TeamModeHandler,
    pub(crate) worker_panel: crate::ui::worker_panel::WorkerPanel,
    pub(crate) agent_manager: AgentManager,
}

/// Message Search State sub-struct
///
/// Groups search, file finding, tag filtering, and message area tracking.
pub(crate) struct MessageSearchState {
    pub(crate) search_state: SearchState,
    pub(crate) file_finder: FileFinder,
    pub(crate) tag_filter: TagFilter,
    pub(crate) message_areas: std::cell::RefCell<Vec<(usize, ratatui::layout::Rect)>>,
    pub(crate) message_line_offsets: std::cell::RefCell<Vec<usize>>,
}

/// Provider and Model State sub-struct
///
/// Groups current model selection, effort level, token budget, plan mode, and API key status.
pub(crate) struct ProviderModelState {
    pub(crate) current_model: String,
    pub(crate) current_effort: String,
    pub(crate) token_budget: TokenBudget,
    pub(crate) plan_mode: rustycode_orchestration::plan_mode::PlanMode,
    pub(crate) api_key_warning: String,
    pub(crate) show_task_dashboard: bool,
}
