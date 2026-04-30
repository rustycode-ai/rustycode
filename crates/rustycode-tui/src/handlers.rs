//! Event handlers for TUI key events.
//!
//! This module provides type aliases and documentation for key handling patterns
//! in the component-based architecture.
#![allow(dead_code)]
//!
//! # Component Architecture
//!
//! Key handling is implemented using:
//! - **InputHandler** component (ui::input) for keyboard event processing
//! - **Event loop** (app::event_loop) for handler dispatch
//! - **Mode-aware shortcuts** via InputMode state
//!
//! # Handler Types
//!
//! 1. **Input Mode Handlers** - Single-line vs Multi-line input
//! 2. **Navigation Handlers** - Message scrolling, focus management
//! 3. **Command Handlers** - Slash commands, palette
//! 4. **Global Shortcuts** - Cross-mode shortcuts (Ctrl+C, Ctrl+P, etc.)
//!
//! Each handler returns `InputAction` enum (Consumed/Ignored/SendMessage/etc.)
//! instead of a simple bool, providing more context about what happened.

use crossterm::event::{KeyCode, KeyModifiers};

/// Result of a key handler - whether the key was handled
pub type KeyHandled = bool;

/// Function signature for mode-specific key handlers
///
/// In the new component architecture, handlers are implemented as methods
/// on InputHandler and dispatched by the event loop based on current mode.
/// This type alias is kept for API compatibility but is not actively used.
pub type KeyHandler = fn(/* &mut App, */ KeyCode, KeyModifiers) -> KeyHandled;

// ── Handler Documentation ───────────────────────────────────────────────────

/// Key handling patterns in the component-based architecture:
///
/// ## Event Flow
///
/// 1. **Event Capture**: crossterm captures keyboard events
/// 2. **Mode Detection**: Current InputMode determines which handlers to invoke
/// 3. **Handler Dispatch**: InputHandler processes the event based on mode
/// 4. **Action Return**: Handler returns InputAction describing what happened
/// 5. **State Update**: Event loop updates application state based on action
///
/// ## Handler Patterns
///
/// - **Consumed**: Event was handled, don't propagate further
/// - **Ignored**: Event not relevant to current mode, try other handlers
/// - **SendMessage**: Input complete, send message to LLM
/// - **ChangeMode**: Switch to different input mode
/// - **TriggerCommand**: Execute a command or open a palette
///
/// This pattern makes the code more testable and easier to understand,
/// as each mode's logic is isolated in its own function.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_aliases_exist() {
        // Verify type aliases compile correctly
        let _handler: KeyHandler = |_, _| false;
        let _result: KeyHandled = false;
    }
}
