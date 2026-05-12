//! TUI State Model
//!
//! Groups related TUI fields into logical sub-structs to reduce the god object problem.

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
use crate::app::turn_snapshot::TurnSnapshot;
use crate::help::HelpState;
use crate::memory::memory_auto::ThreadSafeAutoMemory;
use crate::memory::memory_injection::InjectionConfig;
use crate::plugin::PluginManager;
use crate::plugin::PluginManagerUI;
use crate::services::rate_limit_tracker::RateLimitTracker;
use crate::ui::animator::Animator;
use crate::ui::input::InputHandler;
use crate::ui::marketplace_browser::MarketplaceBrowser;
use crate::ui::message::{Message, ToolExecution};
use crate::ui::message::MessageRenderer;
use crate::ui::skill_palette::SkillPalette;
use ratatui::layout::Rect;
use rustycode_tools::providers::reasoning_types::BudgetState;

/// UI Components sub-struct
///
/// Groups all UI rendering and interaction components.
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
}

/// Service Integration sub-struct
///
/// Groups background service management and pipeline execution state.
pub(crate) struct ServiceIntegrationState {
    pub(crate) services: ServiceManager,
    pub(crate) pipeline: PipelineRegistry,
    pub(crate) pipeline_ctx: PipelineContext,
    pub(crate) scheduler_rx: Option<std::sync::mpsc::Receiver<ScheduledPhaseEvent>>,
    pub(crate) active_scheduled_phases: std::collections::HashSet<String>,
    pub(crate) max_concurrent_phases: usize,
    pub(crate) rate_limit: RateLimitHandler,
    pub(crate) rate_limit_tracker: RateLimitTracker,
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
/// Groups conversational state, tool execution, and reasoning tracking.
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
}

/// System State sub-struct
///
/// Groups runtime flags, memory systems, and plugin management.
pub(crate) struct SystemState {
    pub(crate) running: bool,
    pub(crate) dirty: bool,
    pub(crate) needs_full_redraw: bool,
    pub(crate) compaction: CompactionState,
    pub(crate) auto_memory: Option<std::sync::Arc<ThreadSafeAutoMemory>>,
    pub(crate) memory_injection_config: InjectionConfig,
    pub(crate) plugin_manager: std::sync::Arc<std::sync::RwLock<PluginManager>>,
}
