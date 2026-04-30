//! Modular TUI State Management
//!
//! Composes all focused state modules into a cohesive TUI state structure.

pub mod config;
pub mod input;
pub mod placeholders;
pub mod session;
pub mod streaming;
pub mod ui;
pub mod workspace;

pub use config::{ConfigState, PerformanceState};
pub use input::{InputMode, InputState};
pub use session::{SearchState, SessionState, UndoState};
pub use streaming::{StreamingState, ToolExecutionState};
pub use ui::UiState;
pub use workspace::{PipelineState, WorkspaceState};

/// Modular TUI state that composes all focused state modules
#[derive(Debug)]
pub struct TuiState {
    // Core UI state
    pub ui: UiState,

    // Input handling state
    pub input: InputState,

    // Streaming and tool execution state
    pub streaming: StreamingState,
    pub tool_execution: ToolExecutionState,

    // Workspace and pipeline state
    pub workspace: WorkspaceState,
    pub pipeline: PipelineState,

    // Performance and configuration
    pub performance: PerformanceState,
    pub config: ConfigState,

    // Session management
    pub session: SessionState,
    pub undo: UndoState,
    pub search: SearchState,

    // Global flags
    pub running: bool,
    pub dirty: bool,
}

impl Default for TuiState {
    fn default() -> Self {
        Self {
            ui: UiState::default(),
            input: InputState::default(),
            streaming: StreamingState::default(),
            tool_execution: ToolExecutionState::default(),
            workspace: WorkspaceState::default(),
            pipeline: PipelineState::default(),
            performance: PerformanceState::default(),
            config: ConfigState::default(),
            session: SessionState::default(),
            undo: UndoState::default(),
            search: SearchState::default(),
            running: true,
            dirty: true,
        }
    }
}

impl TuiState {
    /// Create new TUI state
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark state as dirty (needs re-rendering)
    pub const fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Clear dirty flag
    pub const fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Check if state needs rendering
    pub const fn needs_render(&self) -> bool {
        self.dirty
    }

    /// Stop the TUI
    pub const fn stop(&mut self) {
        self.running = false;
    }

    /// Check if TUI is running
    pub const fn is_running(&self) -> bool {
        self.running
    }
}
