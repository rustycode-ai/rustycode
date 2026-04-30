//! Slash command handling for the event loop
//!
//! Command handlers are organized into modules by category:
//! - `slash_commands` - Core slash commands (agent, team, plan, etc.)
//! - `orchestra_commands` - Orchestra (Get Stuff Done) framework commands
//! - `memory_commands` - Memory management commands
//! - `provider_commands` - Provider/model configuration
//! - `lifecycle_commands` - Session lifecycle (clear, quit, save, load)
//! - `info_commands` - Help, marketplace, skill, mcp, hook, theme, track
//! - `file_commands` - File operations (undo, diff, extract, rename)
//! - `task_commands` - Task/todo, review, compact, learnings
//! - `workers_commands` - Worker and cron management

mod file_commands;
mod info_commands;
mod lifecycle_commands;
mod memory_commands;
mod orchestra_commands;
mod provider_commands;
mod slash_commands;
mod task_commands;
mod workers_commands;

pub use file_commands::*;
pub use info_commands::*;
pub use lifecycle_commands::*;
pub use memory_commands::*;
pub use orchestra_commands::*;
pub use provider_commands::*;
pub use slash_commands::*;
pub use task_commands::*;
pub use workers_commands::*;

use crate::agents::AgentManager;
use crate::app::service_integration::ServiceManager;
use crate::compaction::{CompactionConfig, ContextMonitor};
use crate::memory_injection::InjectionConfig;
use crate::plugin::PluginManager;
use crate::tasks::WorkspaceTasks;
use crate::ui::message::Message;
use anyhow::Result;
use std::sync::RwLock;
use std::sync::{Arc, Mutex};

/// Context for executing slash commands
pub struct CommandContext<'a> {
    /// Working directory
    pub cwd: &'a std::path::Path,
    /// Command sender for async operations
    pub command_tx: std::sync::mpsc::SyncSender<crate::app::async_::SlashCommandResult>,
    /// Workspace tasks
    pub workspace_tasks: &'a mut WorkspaceTasks,
    /// Messages shown in the TUI
    pub messages: &'a mut Vec<Message>,
    /// Current streaming buffer
    pub current_stream_content: &'a mut String,
    /// Whether a response is currently streaming
    pub is_streaming: &'a mut bool,
    /// Last extracted tasks/todos snapshot
    pub last_extraction: &'a mut Option<(Vec<crate::tasks::Task>, Vec<crate::tasks::Todo>)>,
    /// Service manager for workspace reloads and other async services
    pub services: &'a mut ServiceManager,
    /// Agent manager
    pub agent_manager: &'a mut AgentManager,
    /// Memory injection config
    pub memory_injection_config: &'a mut InjectionConfig,
    /// Shared theme colors
    pub theme_colors: &'a Arc<Mutex<crate::theme::ThemeColors>>,
    /// Shared skill manager
    pub skill_manager: &'a Arc<RwLock<crate::skills::SkillStateManager>>,
    /// Shared plugin manager
    pub plugin_manager: &'a Arc<RwLock<PluginManager>>,
    /// Whether the TUI main loop is still running
    pub running: &'a mut bool,
    /// Token compaction context monitor
    pub context_monitor: &'a mut ContextMonitor,
    /// Token compaction configuration
    pub compaction_config: &'a mut CompactionConfig,
    /// Whether compaction preview is showing
    pub showing_compaction_preview: &'a mut bool,
    /// Whether compaction is pending
    pub pending_compaction: &'a mut bool,
    /// File undo stack — each entry is a batch of (path, old_content) pairs
    pub file_undo_stack: &'a mut Vec<Vec<(String, String)>>,
    /// Total session input tokens
    pub session_input_tokens: usize,
    /// Total session output tokens
    pub session_output_tokens: usize,
    /// Session cost in USD
    pub session_cost_usd: f64,
    /// Current model name
    pub current_model: String,
    /// Session start time
    pub session_start: std::time::Instant,
}

/// Result of executing a command
#[non_exhaustive]
pub enum CommandEffect {
    /// No immediate effect (async operation started)
    AsyncStarted(String),
    /// System message to display
    SystemMessage(String),
    /// Multiple system messages
    MultipleMessages(Vec<String>),
    /// Toggle the help overlay
    ShowHelp,
    /// Show the plugin manager overlay
    ShowPluginManager,
    /// Start team orchestration with the given task
    StartTeam { task: String },
    /// Cancel a running team task
    CancelTeam,
    /// Clear conversation and reset session state
    ClearConversation,
    /// Load a saved session — replace messages
    LoadSession {
        name: String,
        messages: Vec<Message>,
        summary: String,
    },
    /// Switch the active model (update env var + TUI header)
    ModelSwitch { model_id: String },
    /// Set execution middleware plan mode
    SetPlanMode { planning: bool },
    /// Set cost budget limit (in USD)
    SetBudget { limit: Option<f64> },
    /// Retry the last user message
    RetryLastMessage,
    /// No output needed
    None,
}

type SlashHandler = fn(&[&str], CommandContext<'_>) -> Result<CommandEffect>;

struct SlashCommandPlugin {
    names: &'static [&'static str],
    handler: SlashHandler,
}

const REGISTERED_SLASH_COMMANDS: &[SlashCommandPlugin] = &[
    SlashCommandPlugin {
        names: &["/agent"],
        handler: handle_agent_command,
    },
    SlashCommandPlugin {
        names: &["/team"],
        handler: handle_team_command,
    },
    SlashCommandPlugin {
        names: &["/plan"],
        handler: handle_plan_command,
    },
    SlashCommandPlugin {
        names: &["/harness"],
        handler: handle_harness_command,
    },
    SlashCommandPlugin {
        names: &["/clear"],
        handler: handle_clear_command,
    },
    SlashCommandPlugin {
        names: &["/workspace"],
        handler: handle_workspace_command,
    },
    SlashCommandPlugin {
        names: &["/extract"],
        handler: handle_extract_command,
    },
    SlashCommandPlugin {
        names: &["/rename"],
        handler: handle_rename_command,
    },
    SlashCommandPlugin {
        names: &["/quit", "/exit", "/q"],
        handler: handle_quit_command,
    },
    SlashCommandPlugin {
        names: &["/compact"],
        handler: handle_compact_command,
    },
    SlashCommandPlugin {
        names: &["/review"],
        handler: handle_review_command,
    },
    SlashCommandPlugin {
        names: &["/save"],
        handler: handle_save_command,
    },
    SlashCommandPlugin {
        names: &["/load"],
        handler: handle_load_command,
    },
    SlashCommandPlugin {
        names: &["/memory"],
        handler: handle_memory_command,
    },
    SlashCommandPlugin {
        names: &["/marketplace"],
        handler: handle_marketplace_command,
    },
    SlashCommandPlugin {
        names: &["/plugin", "/plugins"],
        handler: handle_plugin_command,
    },
    SlashCommandPlugin {
        names: &["/task", "/todo"],
        handler: handle_task_todo_command,
    },
    SlashCommandPlugin {
        names: &["/orchestra"],
        handler: handle_orchestra_command,
    },
    SlashCommandPlugin {
        names: &["/help"],
        handler: handle_help_command,
    },
    SlashCommandPlugin {
        names: &["/copilot-login"],
        handler: handle_copilot_login,
    },
    SlashCommandPlugin {
        names: &["/theme", "/t"],
        handler: handle_theme_command,
    },
    SlashCommandPlugin {
        names: &["/model"],
        handler: handle_model_command,
    },
    SlashCommandPlugin {
        names: &["/provider"],
        handler: handle_provider_command,
    },
    SlashCommandPlugin {
        names: &["/skill", "/skills"],
        handler: handle_skill_command,
    },
    SlashCommandPlugin {
        names: &["/skillify"],
        handler: handle_skillify_command,
    },
    SlashCommandPlugin {
        names: &["/mcp"],
        handler: handle_mcp_command,
    },
    SlashCommandPlugin {
        names: &["/lsp"],
        handler: handle_lsp_command,
    },
    SlashCommandPlugin {
        names: &["/hook"],
        handler: handle_hook_command,
    },
    SlashCommandPlugin {
        names: &["/undo"],
        handler: handle_undo_command,
    },
    SlashCommandPlugin {
        names: &["/diff"],
        handler: handle_diff_command,
    },
    SlashCommandPlugin {
        names: &["/export"],
        handler: handle_export_command,
    },
    SlashCommandPlugin {
        names: &["/learnings"],
        handler: handle_learnings_command,
    },
    SlashCommandPlugin {
        names: &["/workers"],
        handler: handle_workers_command,
    },
    SlashCommandPlugin {
        names: &["/cron"],
        handler: handle_cron_command,
    },
    SlashCommandPlugin {
        names: &["/stats"],
        handler: handle_stats_command,
    },
    SlashCommandPlugin {
        names: &["/track", "/progress"],
        handler: handle_track_command,
    },
    SlashCommandPlugin {
        names: &["/cost", "/usage"],
        handler: handle_cost_command,
    },
    SlashCommandPlugin {
        names: &["/checkpoint", "/checkpoints"],
        handler: handle_checkpoint_command,
    },
    SlashCommandPlugin {
        names: &["/resume"],
        handler: handle_resume_command,
    },
    SlashCommandPlugin {
        names: &["/tokens"],
        handler: handle_tokens_command,
    },
    SlashCommandPlugin {
        names: &["/retry"],
        handler: handle_retry_command,
    },
];

/// Dispatch a registered slash command plugin if one matches the input.
pub fn dispatch_registered_slash_command(
    input: &str,
    ctx: CommandContext<'_>,
) -> Result<Option<CommandEffect>> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let Some(cmd) = parts.first().copied() else {
        return Ok(None);
    };

    for plugin in REGISTERED_SLASH_COMMANDS {
        if plugin.names.contains(&cmd) {
            return Ok(Some((plugin.handler)(&parts, ctx)?));
        }
    }

    Ok(None)
}
