//! Session State Management
//!
//! Manages session-related state including recovery, history, and undo operations.

use std::collections::VecDeque;

/// Session state for persistence and recovery
#[derive(Debug)]
pub struct SessionState {
    /// Session recovery manager
    pub recovery_manager: Option<crate::SessionRecoveryManager>,

    /// Cost tracking
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cost_usd: f64,
    pub cost_tracker: crate::state::placeholders::CostTracker,

    /// Auto-continue mode
    pub auto_continue_enabled: bool,
    pub auto_continue_pending: bool,
    pub auto_continue_iterations: usize,

    /// Last extraction results
    pub last_extraction: Option<(Vec<crate::Task>, Vec<crate::Todo>)>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            recovery_manager: None,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            cost_tracker: crate::state::placeholders::CostTracker::default(),
            auto_continue_enabled: false,
            auto_continue_pending: false,
            auto_continue_iterations: 0,
            last_extraction: None,
        }
    }
}

/// Undo/redo state for operations
#[derive(Debug, Default)]
pub struct UndoState {
    /// Scroll position undo stack
    pub scroll_undo_stack: VecDeque<(usize, usize)>,

    /// File undo stack for /undo command
    pub file_undo_stack: Vec<Vec<(String, String)>>,
}

/// Search and filter state
#[derive(Debug, Default)]
pub struct SearchState {
    /// Message search state
    pub message_search: crate::SearchState,
    pub tag_filter: crate::TagFilter,
    pub file_finder: crate::FileFinder,
}
