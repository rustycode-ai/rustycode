//! Advanced multi-line input handling with clipboard support.
//!
//! This module provides sophisticated input management including:
//! - Single-line and multi-line input modes
//! - Clipboard paste support (text and images)
//! - ASCII image preview generation
//! - Keyboard navigation and editing
//! - Proper Unicode and grapheme cluster support for Thai, Arabic, emoji, etc.
//!
//! # Module Organization
//!
//! This module is a re-export facade for the actual implementation:
//! - [`input_state`](super::input_state) - Core state types (InputMode, InputState, ImageAttachment)
//! - [`input_handler`](super::input_handler) - Input handling logic (InputHandler, PasteHandler, InputAction)
//! - [`input_image`](super::input_image) - Image preview generation
//!
//! For backward compatibility, all types are re-exported from this module.

// Re-export all public items from sibling modules
pub use super::input_handler::{InputAction, InputHandler};
pub use super::input_state::{ImageAttachment, InputMode, InputState};
