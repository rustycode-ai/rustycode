//! `RustyCode` TUI Widgets - Specialized UI components.
//!
//! This crate provides RustyCode-specific UI widgets:
//!
//! - **Message Display**: Conversation message rendering
//! - **Tool Panels**: Tool execution status and results
//! - **Input Components**: Command input with history and completion
//! - **Status Bars**: Session info, model selection, progress indicators
//! - **Sidebars**: Session management, file browsers, help panels

pub mod input;
pub mod message;
pub mod sidebar;
pub mod status;
pub mod tool_panel;

// pub use input::CommandInput;
// pub use message::MessageWidget;
// pub use sidebar::SessionSidebar;
// pub use status::StatusBar;
// pub use tool_panel::ToolPanel;
