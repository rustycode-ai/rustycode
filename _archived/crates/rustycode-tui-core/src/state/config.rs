//! Performance State Management
//!
//! Manages performance-related state including dirty flags, rendering optimization,
//! and animation state.

/// Performance state for rendering optimization
#[derive(Debug)]
pub struct PerformanceState {
    /// Dirty flag for selective rendering
    pub dirty: bool,

    /// Full redraw flag (after external editor, etc.)
    pub needs_full_redraw: bool,

    /// Animation state
    pub animator: crate::Animator,

    /// Session start time
    pub start_time: std::time::Instant,
}

impl Default for PerformanceState {
    fn default() -> Self {
        Self {
            dirty: true, // Start dirty to ensure initial render
            needs_full_redraw: false,
            animator: crate::Animator,
            start_time: std::time::Instant::now(),
        }
    }
}

/// Configuration state for user preferences
#[derive(Debug)]
pub struct ConfigState {
    /// TUI configuration
    pub tui_config: crate::TUIConfig,

    /// Theme colors
    pub theme_colors: std::sync::Arc<std::sync::Mutex<crate::ThemeColors>>,

    /// Current model
    pub current_model: String,

    /// Cached API key warning
    pub api_key_warning: String,
}

impl Default for ConfigState {
    fn default() -> Self {
        Self {
            tui_config: crate::TUIConfig,
            theme_colors: std::sync::Arc::new(std::sync::Mutex::new(crate::ThemeColors)),
            current_model: String::new(),
            api_key_warning: String::new(),
        }
    }
}
