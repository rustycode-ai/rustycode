//! Main application module
//!
//! Organized by domain: state sub-structs, input handling, rendering,
//! tool execution, memory, commands, pipeline, and event loop.

// ── State sub-structs (extracted from TUI god object) ──────────
pub mod auto_continue_state;
pub mod compaction_state;
pub mod streaming_state;
pub mod token_budget;
pub mod tool_panel_state;
pub mod undo_state;
pub mod view_state;

// ── Input handling ────────────────────────────────────────────
pub mod input;
pub mod keyboard_shortcuts;

// ── Rendering ─────────────────────────────────────────────────
pub mod renderer;
pub mod render {
    pub mod brutalist_helpers;
    pub mod brutalist_renderer;
    pub mod event_loop;
    pub mod layout;
    pub mod shared;
}
pub mod streaming_render_buffer;

// ── Tool execution ────────────────────────────────────────────
pub mod tool_approval_state;
pub mod tool_confirmation_router;
pub mod tool_errors;
pub mod tool_helpers;
pub mod tool_output_format;
pub mod tool_search;

// ── Commands ──────────────────────────────────────────────────
pub mod commands;
pub mod task_commands;

// ── Memory & messages ─────────────────────────────────────────
pub mod memory_manager;
pub mod memory_ops;
pub mod message_management;
pub mod message_ops;

// ── Clipboard ─────────────────────────────────────────────────
pub mod clipboard_export;
pub mod clipboard_ops;

// ── Pipeline & services ───────────────────────────────────────
pub mod pipeline;
pub mod service_integration;
pub mod service_polling;

// ── Agent orchestration ───────────────────────────────────────
pub mod orchestration_client;
pub mod orchestration_integration;
pub mod team_mode_handler;

// ── Task management ───────────────────────────────────────────
pub mod task_extraction;
pub mod tasks;

// ── UI components ─────────────────────────────────────────────
pub mod confirmation;
pub mod task_dashboard;
pub mod thinking_messages;
pub mod wizard_handler;

// ── Status & monitoring ───────────────────────────────────────
pub mod context_usage;
pub mod doom_loop;
pub mod extraction_analytics;
pub mod lsp_status;
pub mod mcp_status;
pub mod rate_limit;
pub mod rate_limit_handler;
pub mod stall_detector;

// ── Session & recovery ────────────────────────────────────────
pub mod session_recovery_integration;
pub mod storage_bridge;
pub mod turn_snapshot;
pub mod workspace_manager;

// ── Plan mode ─────────────────────────────────────────────────
pub mod plan_mode_ops;

// ── Auto-tool parsing ─────────────────────────────────────────
pub mod auto_tool_parser;

// ── State management ──────────────────────────────────────────
pub mod state;

// ── Async & event loop (core runtime) ─────────────────────────
pub mod async_;
pub mod event_loop;
pub mod handlers;
pub mod streaming;

#[cfg(test)]
mod event_loop_tests;

// ── Re-exports ────────────────────────────────────────────────
pub use event_loop::TUI;
pub use keyboard_shortcuts::{KeyboardAction, KeyboardShortcutHandler};
pub use memory_manager::MemoryManager;
pub use orchestration_client::OrchestrationClient;
pub use session_recovery_integration::{SessionRecoveryConfig, SessionRecoveryManager};
pub use state::state_manager::StateManager;

use std::time::Duration;

pub const FRAME_BUDGET_60FPS: Duration = Duration::from_millis(16);
pub const MAX_INPUT_LATENCY: Duration = Duration::from_millis(50);
/// Threshold for logging slow operations in debug builds
pub const DEBUG_SLOW_THRESHOLD: Duration = Duration::from_millis(2);
/// Cooldown between LSP/MCP status refreshes
pub const REFRESH_COOLDOWN: Duration = Duration::from_secs(30);
/// Event poll timeout before returning to the main loop
pub const EVENT_POLL_TIMEOUT: Duration = Duration::from_millis(1);
pub const MAX_UNDO_ENTRIES: usize = 5;
