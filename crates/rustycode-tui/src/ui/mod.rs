//! UI Components - Plugin-based Architecture
//!
//! This module provides independent, reusable UI components following a **plugin-based architecture**.
//! Each component is self-contained with clear responsibilities and can be used independently or
//! combined as needed.
#![allow(unexpected_cfgs)]
//!
//! # Architecture Principles
//!
//! - **Components, not monoliths**: Each module is a focused, single-purpose component
//! - **Independent plugins**: Components can be used standalone or combined
//! - **Clear APIs**: Each component exposes a clean, well-documented public API
//! - **Testable in isolation**: Components have their own comprehensive tests
//! - **No tight coupling**: Components communicate via well-defined interfaces, not direct dependencies

// MODULE DECLARATIONS

// Message system
pub mod message;
pub mod message_export;
pub mod message_image;
pub mod message_renderer;
pub mod message_search;
pub mod message_tags;
pub mod message_thinking;
pub mod message_types;

// Input handling
pub mod file_selector;
pub mod input;
pub mod input_handler;
pub mod input_history;
pub mod input_image;
pub mod input_paste;
pub mod input_state;

// Status system
pub mod status;

// Animation
pub mod animator;

// Progress tracking
pub mod progress;

// Spinner component
pub mod spinner;

// Help system (active help is in crate::help, not ui::help)
// NOTE: ui/help.rs and ui/shortcuts.rs were dead code — removed

// Command palette
pub mod command_palette;

// Toast notifications
pub mod toast;

// Session sidebar
pub mod session_sidebar;

// Model selector
pub mod model_selector;

// File finder
pub mod file_finder;

// Team panel (agent timeline)
pub mod team_panel;

// Bookmark system for messages
pub mod bookmarks;

// Error display system
pub mod error_messages;
pub mod errors;

// Clarification questions
pub mod clarification;

// Skill palette
pub mod marketplace_browser;
pub mod skill_palette;

// Theme preview
pub mod theme_preview;

// Polished header component
pub mod header;

// Polished footer component
pub mod footer;

// Fuzzy matcher (shared utility)
pub mod fuzzy_matcher;

// First-run configuration wizard
pub mod wizard;

// Tests
pub mod tests;

// Message tags integration tests
#[cfg(test)]
mod message_tags_integration_test;

// Debug rendering tests
#[cfg(test)]
mod debug_rendering;

// Worker status panel (sub-agent orchestration display)
pub mod worker_panel;

// AST pipeline phase progress
pub mod ast_progress;

// Code rendering utilities
pub mod diff_renderer;

// Accessibility features
pub mod accessibility;

// PUBLIC API RE-EXPORTS

// Message system
pub use message::{Message, MessageRole};
pub use message_export::{ConversationExporter, ExportFormat, ExportOptions};
pub use message_search::{MatchPosition, RoleFilter, SearchEngine, SearchState};
pub use message_tags::{Tag, TagFilter, TagRegistry, TagType};
pub use rustycode_ui_core::MessageTheme;

// Markdown rendering
pub use rustycode_ui_core::MarkdownRenderer;
pub use rustycode_ui_core::SyntaxHighlighter;

// Code rendering utilities
pub use diff_renderer::DiffRenderer;

// Clarification questions
pub use clarification::{detect_questions, ClarificationPanel, Question};
