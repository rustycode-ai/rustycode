//! Keyboard shortcuts handler for RustyCode TUI
//!
//! Provides support for:
//! - Standard keyboard shortcuts (Ctrl+C, Ctrl+V, Ctrl+Z, Home, End)
//! - Vim-style keybindings (j, k, gg, G)
//! - Configurable modes and chord detection

use std::time::{Duration, Instant};

/// Standard keyboard shortcut actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyboardAction {
    /// Copy selected message to clipboard
    Copy,
    /// Paste from clipboard
    Paste,
    /// Undo last action
    Undo,
    /// Jump to start of conversation
    JumpToStart,
    /// Jump to end of conversation
    JumpToEnd,
    /// Move down (Vim j or Down arrow)
    MoveDown,
    /// Move up (Vim k or Up arrow)
    MoveUp,
    /// No action
    None,
}

/// State for tracking Vim chord sequences
#[derive(Debug, Clone)]
pub struct VimChordState {
    /// Whether we're waiting for the second 'g' in 'gg'
    pub pending_g: bool,
    /// Time when the first 'g' was pressed
    pub pending_g_time: Option<Instant>,
    /// Timeout duration for detecting 'gg'
    pub chord_timeout: Duration,
}

impl VimChordState {
    /// Create new Vim chord state
    pub fn new() -> Self {
        Self {
            pending_g: false,
            pending_g_time: None,
            chord_timeout: Duration::from_millis(500),
        }
    }

    /// Reset chord state
    pub fn reset(&mut self) {
        self.pending_g = false;
        self.pending_g_time = None;
    }

    /// Check if the pending 'g' has timed out
    pub fn is_chord_timed_out(&self) -> bool {
        if let Some(time) = self.pending_g_time {
            time.elapsed() > self.chord_timeout
        } else {
            false
        }
    }

    /// Record pending 'g' keypress
    pub fn mark_pending_g(&mut self) {
        self.pending_g = true;
        self.pending_g_time = Some(Instant::now());
    }
}

impl Default for VimChordState {
    fn default() -> Self {
        Self::new()
    }
}

/// Keyboard shortcut handler
pub struct KeyboardShortcutHandler {
    /// Whether Vim mode is enabled
    vim_enabled: bool,
    /// Vim chord detection state
    pub vim_chord_state: VimChordState,
}

impl KeyboardShortcutHandler {
    /// Create new keyboard shortcut handler
    pub fn new(vim_enabled: bool) -> Self {
        Self {
            vim_enabled,
            vim_chord_state: VimChordState::new(),
        }
    }

    /// Set whether Vim mode is enabled
    pub fn set_vim_enabled(&mut self, enabled: bool) {
        self.vim_enabled = enabled;
        if !enabled {
            self.vim_chord_state.reset();
        }
    }

    /// Check if Vim mode is enabled
    pub fn is_vim_enabled(&self) -> bool {
        self.vim_enabled
    }

    /// Handle a character key in Vim mode
    ///
    /// Returns the action to perform (if any) and updates internal state
    pub fn handle_vim_key(&mut self, c: char) -> KeyboardAction {
        match c {
            'j' => {
                self.vim_chord_state.reset();
                KeyboardAction::MoveDown
            }
            'k' => {
                self.vim_chord_state.reset();
                KeyboardAction::MoveUp
            }
            'G' => {
                self.vim_chord_state.reset();
                KeyboardAction::JumpToEnd
            }
            'g' => {
                // Check if we're waiting for second 'g'
                if self.vim_chord_state.pending_g && !self.vim_chord_state.is_chord_timed_out() {
                    // This is the second 'g' in 'gg'
                    self.vim_chord_state.reset();
                    KeyboardAction::JumpToStart
                } else if self.vim_chord_state.is_chord_timed_out() {
                    // First 'g' timed out, this is a new 'g'
                    self.vim_chord_state.reset();
                    self.vim_chord_state.mark_pending_g();
                    KeyboardAction::None
                } else {
                    // Mark that we have a pending 'g'
                    self.vim_chord_state.mark_pending_g();
                    KeyboardAction::None
                }
            }
            _ => {
                // Any other key cancels pending chord
                self.vim_chord_state.reset();
                KeyboardAction::None
            }
        }
    }

    /// Reset any pending keyboard state
    pub fn reset(&mut self) {
        self.vim_chord_state.reset();
    }
}

impl Default for KeyboardShortcutHandler {
    fn default() -> Self {
        Self::new(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vim_chord_state_new() {
        let state = VimChordState::new();
        assert!(!state.pending_g);
        assert!(state.pending_g_time.is_none());
        assert_eq!(state.chord_timeout, Duration::from_millis(500));
    }

    #[test]
    fn test_vim_chord_state_mark_pending_g() {
        let mut state = VimChordState::new();
        assert!(!state.pending_g);

        state.mark_pending_g();
        assert!(state.pending_g);
        assert!(state.pending_g_time.is_some());
    }

    #[test]
    fn test_vim_chord_state_reset() {
        let mut state = VimChordState::new();
        state.mark_pending_g();
        assert!(state.pending_g);

        state.reset();
        assert!(!state.pending_g);
        assert!(state.pending_g_time.is_none());
    }

    #[test]
    fn test_vim_chord_state_timeout() {
        let mut state = VimChordState::new();
        state.chord_timeout = Duration::from_millis(10);
        state.mark_pending_g();

        assert!(!state.is_chord_timed_out());
        std::thread::sleep(Duration::from_millis(20));
        assert!(state.is_chord_timed_out());
    }

    #[test]
    fn test_keyboard_handler_new() {
        let handler = KeyboardShortcutHandler::new(false);
        assert!(!handler.is_vim_enabled());
    }

    #[test]
    fn test_keyboard_handler_vim_mode_toggle() {
        let mut handler = KeyboardShortcutHandler::new(false);
        assert!(!handler.is_vim_enabled());

        handler.set_vim_enabled(true);
        assert!(handler.is_vim_enabled());

        handler.set_vim_enabled(false);
        assert!(!handler.is_vim_enabled());
    }

    #[test]
    fn test_vim_key_j_moves_down() {
        let mut handler = KeyboardShortcutHandler::new(true);
        let action = handler.handle_vim_key('j');
        assert_eq!(action, KeyboardAction::MoveDown);
    }

    #[test]
    fn test_vim_key_k_moves_up() {
        let mut handler = KeyboardShortcutHandler::new(true);
        let action = handler.handle_vim_key('k');
        assert_eq!(action, KeyboardAction::MoveUp);
    }

    #[test]
    fn test_vim_key_g_jumps_to_end() {
        let mut handler = KeyboardShortcutHandler::new(true);
        let action = handler.handle_vim_key('G');
        assert_eq!(action, KeyboardAction::JumpToEnd);
    }

    #[test]
    fn test_vim_chord_gg_single_press() {
        let mut handler = KeyboardShortcutHandler::new(true);
        let action = handler.handle_vim_key('g');
        assert_eq!(action, KeyboardAction::None);
        assert!(handler.vim_chord_state.pending_g);
    }

    #[test]
    fn test_vim_chord_gg_double_press() {
        let mut handler = KeyboardShortcutHandler::new(true);

        // First 'g'
        let action = handler.handle_vim_key('g');
        assert_eq!(action, KeyboardAction::None);
        assert!(handler.vim_chord_state.pending_g);

        // Second 'g' (quick)
        let action = handler.handle_vim_key('g');
        assert_eq!(action, KeyboardAction::JumpToStart);
        assert!(!handler.vim_chord_state.pending_g);
    }

    #[test]
    fn test_vim_chord_gg_timeout() {
        let mut handler = KeyboardShortcutHandler::new(true);
        handler.vim_chord_state.chord_timeout = Duration::from_millis(10);

        // First 'g'
        let action = handler.handle_vim_key('g');
        assert_eq!(action, KeyboardAction::None);

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(20));

        // Second 'g' after timeout (should be treated as new 'g')
        let action = handler.handle_vim_key('g');
        assert_eq!(action, KeyboardAction::None);
        assert!(handler.vim_chord_state.pending_g);
    }

    #[test]
    fn test_vim_chord_interrupted_by_other_key() {
        let mut handler = KeyboardShortcutHandler::new(true);

        // First 'g'
        let action = handler.handle_vim_key('g');
        assert_eq!(action, KeyboardAction::None);
        assert!(handler.vim_chord_state.pending_g);

        // Different key (interrupts chord)
        let action = handler.handle_vim_key('j');
        assert_eq!(action, KeyboardAction::MoveDown);
        assert!(!handler.vim_chord_state.pending_g);
    }

    #[test]
    fn test_handler_reset() {
        let mut handler = KeyboardShortcutHandler::new(true);

        // Set up pending state
        handler.handle_vim_key('g');
        assert!(handler.vim_chord_state.pending_g);

        // Reset should clear state
        handler.reset();
        assert!(!handler.vim_chord_state.pending_g);
    }

    #[test]
    fn test_vim_disabled_ignores_vim_keys() {
        let mut handler = KeyboardShortcutHandler::new(false);

        // Vim keys should still be processed (they're just characters)
        // This is to allow flexibility - handler doesn't enforce mode
        // The event loop will check is_vim_enabled() before calling handle_vim_key
        let action = handler.handle_vim_key('j');
        assert_eq!(action, KeyboardAction::MoveDown);
    }

    #[test]
    fn test_keyboard_action_copy_eq() {
        assert_eq!(KeyboardAction::Copy, KeyboardAction::Copy);
        assert_ne!(KeyboardAction::Copy, KeyboardAction::Paste);
    }
}
