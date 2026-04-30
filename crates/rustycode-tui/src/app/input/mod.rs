//! Input handling module
//!
//! Extracts input handling logic from event_loop_input.rs into focused submodules:
//! - keyboard: Key event handling, Vim keybindings, global shortcuts
//! - mouse: Mouse scroll events
//! - text_input: Text composition, search box, command palette
//! - special_handlers: Wizard, approval, clarification, and modal states

pub mod keyboard;
pub mod mouse;
pub mod special_handlers;
pub mod text_input;

// Re-export common types
