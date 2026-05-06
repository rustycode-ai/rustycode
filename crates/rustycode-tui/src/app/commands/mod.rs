//! Slash command dispatch and handler modules.

mod file_commands;
mod info_commands;
mod lifecycle_commands;
mod memory_commands;
mod orchestra_commands;
mod provider_commands;
mod slash_commands;
mod task_commands;
mod workers_commands;

use file_commands::{handle_diff_command, handle_export_command, handle_undo_command};
use info_commands::{
    handle_checkpoint_command, handle_cost_command, handle_help_command, handle_hook_command,
    handle_lsp_command, handle_marketplace_command, handle_mcp_command, handle_plugin_command,
    handle_skill_command, handle_skillify_command, handle_stats_command, handle_theme_command,
    handle_track_command,
};
use lifecycle_commands::{
    handle_extract_command, handle_load_command, handle_rename_command, handle_resume_command,
    handle_retry_command, handle_save_command, handle_sessions_command, handle_tokens_command,
};
use memory_commands::handle_memory_command;
use orchestra_commands::handle_orchestra_command;
use provider_commands::{handle_model_command, handle_provider_command};
use slash_commands::{
    handle_act_command, handle_agent_command, handle_ask_command, handle_clear_command,
    handle_copilot_login, handle_harness_command, handle_plan_command, handle_quit_command,
    handle_team_command, handle_workspace_command, handle_yolo_command,
};
use task_commands::{
    handle_compact_command, handle_learnings_command, handle_review_command,
    handle_task_todo_command,
};
use workers_commands::{handle_cron_command, handle_workers_command};

use crate::agents::AgentManager;
use crate::app::service_integration::ServiceManager;
use crate::app::tasks::WorkspaceTasks;
use crate::memory::compaction::{CompactionConfig, ContextMonitor};
use crate::memory::memory_injection::InjectionConfig;
use crate::plugin::PluginManager;
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
    pub workspace_tasks: &'a mut WorkspaceTasks,
    /// Messages shown in the TUI
    pub messages: &'a mut Vec<Message>,
    /// Current streaming buffer
    pub current_stream_content: &'a mut String,
    /// Whether a response is currently streaming
    pub is_streaming: &'a mut bool,
    /// Last extracted tasks/todos snapshot
    pub last_extraction:
        &'a mut Option<(Vec<crate::app::tasks::Task>, Vec<crate::app::tasks::Todo>)>,
    /// Service manager for workspace reloads and other async services
    pub services: &'a mut ServiceManager,
    pub agent_manager: &'a mut AgentManager,
    pub memory_injection_config: &'a mut InjectionConfig,
    /// Shared theme colors
    pub theme_colors: &'a Arc<Mutex<crate::theme::ThemeColors>>,
    /// Shared skill manager
    pub skill_manager: &'a Arc<RwLock<crate::skills::SkillStateManager>>,
    /// Shared plugin manager
    pub plugin_manager: &'a Arc<RwLock<PluginManager>>,
    pub running: &'a mut bool,
    /// Token compaction context monitor
    pub context_monitor: &'a mut ContextMonitor,
    /// Token compaction configuration
    pub compaction_config: &'a mut CompactionConfig,
    pub showing_compaction_preview: &'a mut bool,
    pub pending_compaction: &'a mut bool,
    /// Undo stacks for message positions and file edits
    pub file_undo_stack: &'a mut crate::app::undo_state::UndoState,
    /// Total session input tokens
    pub session_input_tokens: usize,
    /// Total session output tokens
    pub session_output_tokens: usize,
    pub session_cost_usd: f64,
    /// Current model name
    pub current_model: String,
    /// Session start time
    pub session_start: std::time::Instant,
}

/// Result of executing a command
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
        names: &["/yolo", "/auto"],
        handler: handle_yolo_command,
    },
    SlashCommandPlugin {
        names: &["/act"],
        handler: handle_act_command,
    },
    SlashCommandPlugin {
        names: &["/ask"],
        handler: handle_ask_command,
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
    SlashCommandPlugin {
        names: &["/sessions"],
        handler: handle_sessions_command,
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

pub fn is_registered_command(name: &str) -> bool {
    REGISTERED_SLASH_COMMANDS
        .iter()
        .any(|plugin| plugin.names.contains(&name))
}
