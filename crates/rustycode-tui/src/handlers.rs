//! Type aliases for TUI key event handlers in the component-based architecture.
#![allow(dead_code)]

use crossterm::event::{KeyCode, KeyModifiers};

/// Result of a key handler - whether the key was handled
pub type KeyHandled = bool;

/// Legacy type alias kept for API compatibility.
/// Active handlers are methods on InputHandler dispatched by the event loop.
pub type KeyHandler = fn(/* &mut App, */ KeyCode, KeyModifiers) -> KeyHandled;

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
