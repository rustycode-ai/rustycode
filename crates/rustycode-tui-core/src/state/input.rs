//! Input State Management
//!
//! Manages input handling state including modes, buffers, and user interaction.

use std::time::Instant;

/// Input state for handling user input and interaction modes
#[derive(Debug)]
pub struct InputState {
    /// Current input mode
    pub mode: InputMode,

    /// Input handler state (placeholder for future input handler integration)
    pub input_state: crate::placeholders::InputState,

    /// Double-Esc detection
    pub last_esc_press: Option<Instant>,

    /// Stashed prompt (Ctrl+S)
    pub stashed_prompt: Option<String>,

    /// Command palette state
    pub showing_command_palette: bool,
    pub showing_skill_palette: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            mode: InputMode::SingleLine,
            input_state: crate::placeholders::InputState,
            last_esc_press: None,
            stashed_prompt: None,
            showing_command_palette: false,
            showing_skill_palette: false,
        }
    }
}

/// Input modes for different interaction styles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    SingleLine,
    MultiLine,
}

impl InputMode {
    /// Toggle between single and multi-line modes
    pub const fn toggle(&mut self) {
        *self = match self {
            Self::SingleLine => Self::MultiLine,
            Self::MultiLine => Self::SingleLine,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_state_default() {
        let state = InputState::default();
        assert_eq!(state.mode, InputMode::SingleLine);
        assert!(state.last_esc_press.is_none());
        assert!(state.stashed_prompt.is_none());
        assert!(!state.showing_command_palette);
        assert!(!state.showing_skill_palette);
    }

    #[test]
    fn test_input_mode_toggle_single_to_multi() {
        let mut mode = InputMode::SingleLine;
        mode.toggle();
        assert_eq!(mode, InputMode::MultiLine);
    }

    #[test]
    fn test_input_mode_toggle_multi_to_single() {
        let mut mode = InputMode::MultiLine;
        mode.toggle();
        assert_eq!(mode, InputMode::SingleLine);
    }

    #[test]
    fn test_input_mode_toggle_roundtrip() {
        let mut mode = InputMode::SingleLine;
        mode.toggle();
        assert_eq!(mode, InputMode::MultiLine);
        mode.toggle();
        assert_eq!(mode, InputMode::SingleLine);
    }

    #[test]
    fn test_input_mode_equality() {
        assert_eq!(InputMode::SingleLine, InputMode::SingleLine);
        assert_eq!(InputMode::MultiLine, InputMode::MultiLine);
        assert_ne!(InputMode::SingleLine, InputMode::MultiLine);
    }

    #[test]
    fn test_input_state_stashed_prompt() {
        let state = InputState {
            stashed_prompt: Some("my draft".to_string()),
            ..InputState::default()
        };
        assert_eq!(state.stashed_prompt.as_deref(), Some("my draft"));
    }

    #[test]
    fn test_input_state_mode_multi() {
        let state = InputState {
            mode: InputMode::MultiLine,
            showing_command_palette: true,
            ..InputState::default()
        };
        assert_eq!(state.mode, InputMode::MultiLine);
        assert!(state.showing_command_palette);
    }
}
