//! Main application module

pub mod auto_tool_parser;
pub mod brutalist_helpers;
pub mod brutalist_renderer;
pub mod clipboard_export;
pub mod clipboard_ops;
pub mod commands;
pub mod context_usage;
pub mod doom_loop;
pub mod event_loop_agents;
pub mod event_loop_commands;
pub mod event_loop_input;
pub mod event_loop_render;
pub mod extraction_analytics;
pub mod input;
pub mod keyboard_shortcuts;
pub mod memory_manager;
pub mod memory_ops;
pub mod message_management;
pub mod message_ops;
pub mod orchestration_integration;
pub mod plan_mode_ops;
pub mod rate_limit;
pub mod rate_limit_handler;
pub mod renderer;
pub mod scrolling_ops;
pub mod service_polling;
pub mod session_recovery_integration;
pub mod stall_detector;
pub mod state_manager;
pub mod task_commands;
pub mod task_extraction;
pub mod tasks;
pub mod team_mode_handler;
pub mod tool_errors;
pub mod tool_helpers;
pub mod tool_output_format;
pub mod tool_search;
pub mod turn_snapshot;
pub mod wizard_handler;
pub mod workspace_manager;

pub mod confirmation;
pub mod pipeline;
pub mod storage_bridge;
pub mod streaming_render_buffer;
pub mod task_dashboard;
pub mod thinking_messages;
pub mod tool_confirmation_router;

pub mod async_;
pub mod event_loop;
pub mod handlers;
pub mod service_integration;
pub mod streaming;

pub mod render {
    pub mod shared;
}

#[cfg(test)]
mod event_loop_tests;

pub use async_::*;
pub use event_loop::TUI;
pub use event_loop_agents::*;
pub use event_loop_commands::*;
pub use event_loop_input::*;
pub use event_loop_render::*;
pub use keyboard_shortcuts::{KeyboardAction, KeyboardShortcutHandler};
pub use memory_manager::MemoryManager;
pub use service_integration::*;
pub use session_recovery_integration::{SessionRecoveryConfig, SessionRecoveryManager};
pub use state_manager::StateManager;

use std::time::Duration;

pub const FRAME_BUDGET_60FPS: Duration = Duration::from_millis(16);
pub const MAX_INPUT_LATENCY: Duration = Duration::from_millis(50);
pub const MAX_UNDO_ENTRIES: usize = 5;
