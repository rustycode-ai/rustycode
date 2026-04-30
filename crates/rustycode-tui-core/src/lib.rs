//! RustyCode TUI Core - Core UI framework.
//!
//! This crate provides the foundational UI components and event handling:
//!
//! - **Terminal Management**: Backend abstraction and terminal control
//! - **Event Loop**: Responsive event processing with frame budgeting
//! - **UI Components**: Basic widgets and layout primitives
//! - **Input Handling**: Keyboard and mouse event processing
//! - **Rendering**: Efficient screen updates and frame management

#![allow(clippy::doc_markdown, clippy::uninlined_format_args)]

pub mod backend;
pub mod event_loop;
pub mod input;
pub mod placeholders;
pub mod render;
pub mod state;
pub mod terminal;
pub mod widgets;

pub use backend::TuiBackend;
pub use event_loop::{EventLoop, EventLoopConfig, FRAME_BUDGET_60FPS, MAX_INPUT_LATENCY};
pub use terminal::{TerminalCleanupGuard, TerminalManager};

// Re-export placeholder types at crate root for `crate::TypeName` references
pub use placeholders::{
    Animator, CostTracker, FileFinder, InputState, PipelineContext, PipelineGuardian,
    PipelineRegistry, ScheduledPhaseEvent, SearchState, SessionRecoveryManager,
    StreamingRenderBuffer, TUIConfig, TagFilter, Task, ThemeColors, Todo, WorkspaceTasks,
};
