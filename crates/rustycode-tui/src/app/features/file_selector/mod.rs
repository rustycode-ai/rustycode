//! File Selector / File Finder feature module.
//!
//! Self-contained feature for fuzzy file search and selection.
//! Implements the [`TuiFeature`] trait and owns all file-selector state.
//!
//! ## State
//! - [`FileSelectorFeatureState`]: Wraps a [`FileFinder`] and a [`FileSelector`],
//!   plus visibility tracking.
//!
//! ## Events Handled
//! - `TuiEvent::Key`: Delegated to [`FileFinder::handle_key`] or
//!   [`FileSelector::handle_key`] when visible.
//! - Other events (Tick, Stream, Service) are ignored.
//!
//! ## Surfaces
//! - `"file_selector"`: File selector overlay surface.
//!
//! ## Routes
//! - `"file_selector"`: Navigation route to open the file selector.
//!
//! ## Rendering
//! Delegates to [`FileFinder::render`] when visible.

use crate::app::features::{
    FeatureRegistry, ModalId, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature,
    UpdateCtx,
};
use crate::ui::file_finder::FileFinder;
use crate::ui::file_selector::FileSelector;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// File Selector feature state.
///
/// Holds the file finder (fuzzy search over project files), the legacy file
/// selector, and a shared project root. The `file_finder` is wrapped in
/// [`Mutex`] because [`FileFinder`] contains interior-mutable state
/// (`ListState`, `FileFuzzyMatcher`) that is `Send` but not `Sync`.
pub struct FileSelectorFeatureState {
    /// VS Code-style fuzzy file finder (thread-safe wrapped for `Sync`).
    pub file_finder: Mutex<FileFinder>,
    /// Legacy file selector used for @-mention file attachment.
    pub file_selector: Mutex<FileSelector>,
    /// Whether the file selector overlay is currently visible.
    pub visible: bool,
    /// Project root directory (used for re-indexing).
    pub project_root: PathBuf,
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

/// File Selector feature implementing [`TuiFeature`].
///
/// Handles key events for fuzzy file navigation/search and delegates rendering
/// to [`FileFinder`].
///
/// Backward-compatible alias: [`FileSelectorFeatureState`] re-exports this type
/// so existing callers can continue to use the old name.
pub struct FileSelectorFeature {
    state: FileSelectorFeatureState,
}

/// Backward-compatible alias — the old name for the feature type.
pub use FileSelectorFeature as FileSelectorFeatureStateCompat;

impl FileSelectorFeature {
    /// Create a new `FileSelectorFeature`.
    ///
    /// Takes a `PathBuf` for the project root to index files, and a `Vec<String>`
    /// of file paths for the legacy file selector. The UI starts hidden.
    pub fn new(project_root: PathBuf, files: Vec<String>) -> Self {
        let file_finder = FileFinder::new(project_root.clone());
        let file_selector = FileSelector::new(files);
        Self {
            state: FileSelectorFeatureState {
                file_finder: Mutex::new(file_finder),
                file_selector: Mutex::new(file_selector),
                visible: false,
                project_root,
            },
        }
    }

    /// Show the file finder overlay.
    pub fn show(&mut self) {
        self.state.visible = true;
        if let Ok(mut finder) = self.state.file_finder.lock() {
            finder.show();
        }
    }

    /// Hide the file finder overlay.
    pub fn hide(&mut self) {
        self.state.visible = false;
        if let Ok(mut finder) = self.state.file_finder.lock() {
            finder.hide();
        }
    }

    /// Check if the file finder overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.state.visible
    }

    /// Surface ID used by this feature.
    const SURFACE: SurfaceId = SurfaceId::new("file_selector");

    /// Route ID for navigating to the file selector.
    const ROUTE: RouteId = RouteId::new("file_selector");

    /// Modal ID for the file selector overlay.
    const MODAL: ModalId = ModalId::new("file_selector");

    /// Slash command to open the file selector.
    const CMD_OPEN: &str = "/file open";

    /// Slash command to close the file selector.
    const CMD_CLOSE: &str = "/file close";

    /// Keyboard shortcut to toggle the file finder (Ctrl+P).
    const KEYMAP_TOGGLE: &str = "Ctrl+P";
}

impl TuiFeature for FileSelectorFeature {
    fn id(&self) -> &'static str {
        "file_selector"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
        reg.register_command(Self::CMD_OPEN, self.id());
        reg.register_command(Self::CMD_CLOSE, self.id());
        reg.register_keymap(
            Self::KEYMAP_TOGGLE.to_string(),
            self.id(),
            "toggle_file_selector",
        );
    }

    fn update(&mut self, event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
        match event {
            TuiEvent::Key(key) => self.handle_key_event(*key),
            _ => Vec::new(),
        }
    }

    fn render(&self, surface: SurfaceId, frame: &mut Frame, ctx: &RenderCtx) {
        if surface != Self::SURFACE || !self.state.visible {
            return;
        }

        let finder = match self.state.file_finder.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("file finder lock poisoned: {e}");
                e.into_inner()
            }
        };

        finder.render(frame, ctx.frame_area);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Public API: command handling
// ---------------------------------------------------------------------------

impl FileSelectorFeature {
    /// Handle a slash command dispatched to this feature.
    ///
    /// Returns actions for the shell to process. Recognized commands:
    /// - `"/file open"` — shows the file selector overlay
    /// - `"/file close"` — hides the file selector overlay
    pub fn handle_command(&mut self, command: &str) -> Vec<TuiAction> {
        match command {
            cmd if cmd == Self::CMD_OPEN => {
                self.show();
                vec![TuiAction::OpenModal(Self::MODAL)]
            }
            cmd if cmd == Self::CMD_CLOSE => {
                self.hide();
                vec![TuiAction::CloseModal]
            }
            _ => Vec::new(),
        }
    }

    /// Toggle file selector visibility.
    ///
    /// Called when the keymap shortcut (Ctrl+P) is pressed. Returns `OpenModal`
    /// if shown, `CloseModal` if hidden.
    pub fn toggle_visibility(&mut self) -> Vec<TuiAction> {
        if self.state.visible {
            self.hide();
            vec![TuiAction::CloseModal]
        } else {
            self.show();
            vec![TuiAction::OpenModal(Self::MODAL)]
        }
    }

    /// Take the selected file path after Enter confirmation.
    ///
    /// Returns `Some(path)` once after the user confirms with Enter, then
    /// `None` until the next selection.
    pub fn take_selected(&mut self) -> Option<String> {
        let mut finder = match self.state.file_finder.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("file finder lock poisoned: {e}");
                e.into_inner()
            }
        };
        finder
            .take_selected()
            .map(|fi| fi.path.display().to_string())
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

impl FileSelectorFeature {
    /// Handle a keyboard event when the file finder is visible.
    fn handle_key_event(&mut self, key: KeyEvent) -> Vec<TuiAction> {
        if !self.state.visible {
            return Vec::new();
        }

        let mut finder = match self.state.file_finder.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("file finder lock poisoned: {e}");
                e.into_inner()
            }
        };

        let handled = finder.handle_key(key);

        if !finder.is_visible() {
            // UI was hidden by handle_key (e.g., Esc or Enter).
            drop(finder);
            self.state.visible = false;
            vec![TuiAction::CloseModal]
        } else if handled {
            vec![TuiAction::MarkDirty]
        } else {
            Vec::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeColors;
    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::style::Color;

    // -- Helpers -----------------------------------------------------------

    fn make_feature() -> FileSelectorFeature {
        FileSelectorFeature::new(
            std::env::temp_dir(),
            vec![
                "src/main.rs".to_string(),
                "src/lib.rs".to_string(),
                "Cargo.toml".to_string(),
            ],
        )
    }

    fn test_theme_colors() -> ThemeColors {
        ThemeColors {
            background: Color::Black,
            foreground: Color::White,
            primary: Color::Cyan,
            secondary: Color::Magenta,
            accent: Color::Yellow,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
        }
    }

    /// Build a no-op `UpdateCtx` for testing `update()`.
    fn make_update_ctx<'a>(
        theme_colors: &'a ThemeColors,
        navigate: &'a mut dyn FnMut(RouteId),
        dispatch_command: &'a mut dyn FnMut(&str),
        approve_tool: &'a mut dyn FnMut(String, bool),
    ) -> UpdateCtx<'a> {
        UpdateCtx {
            has_focus: false,
            focused_surface: None,
            is_streaming: false,
            pending_tools: 0,
            plan_mode_active: false,
            auto_continue_enabled: false,
            theme_colors,
            navigate,
            dispatch_command,
            approve_tool,
        }
    }

    fn make_render_ctx(theme_colors: &ThemeColors) -> RenderCtx<'_> {
        RenderCtx {
            frame_area: ratatui::layout::Rect::new(0, 0, 80, 24),
            focused_surface: None,
            theme_colors,
        }
    }

    fn esc_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    }

    fn char_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn ctrl_p() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)
    }

    // -- new() tests -------------------------------------------------------

    #[test]
    fn new_creates_hidden_state() {
        let feature = make_feature();
        assert_eq!(feature.id(), "file_selector");
        assert!(!feature.state.visible);
        assert!(!feature.state.file_finder.lock().expect("lock").is_visible());
    }

    // -- register() tests --------------------------------------------------

    #[test]
    fn register_registers_surface_and_route() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(
            reg.surface_feature(SurfaceId::new("file_selector")),
            Some("file_selector")
        );
        assert_eq!(
            reg.route_feature(RouteId::new("file_selector")),
            Some("file_selector")
        );
    }

    // -- visibility tests --------------------------------------------------

    #[test]
    fn show_sets_visible() {
        let mut feature = make_feature();
        assert!(!feature.state.visible);
        feature.show();
        assert!(feature.state.visible);
        assert!(feature.state.file_finder.lock().expect("lock").is_visible());
    }

    #[test]
    fn hide_clears_visible() {
        let mut feature = make_feature();
        feature.show();
        assert!(feature.state.visible);
        feature.hide();
        assert!(!feature.state.visible);
        assert!(!feature.state.file_finder.lock().expect("lock").is_visible());
    }

    // -- update() tests ----------------------------------------------------

    #[test]
    fn update_key_ignored_when_not_visible() {
        let mut feature = make_feature();
        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(char_key('a')), &mut ctx);
        assert!(actions.is_empty());
    }

    #[test]
    fn update_esc_returns_close_modal_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(esc_key()), &mut ctx);

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::CloseModal));
        assert!(!feature.state.visible);
    }

    #[test]
    fn update_char_input_returns_mark_dirty_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(char_key('s')), &mut ctx);

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::MarkDirty));
        assert!(feature.state.visible);
    }

    #[test]
    fn update_ignores_non_key_events() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        assert!(feature.update(&TuiEvent::Tick, &mut ctx).is_empty());
        assert!(feature
            .update(
                &TuiEvent::Resize {
                    width: 100,
                    height: 30
                },
                &mut ctx
            )
            .is_empty());
    }

    // -- render() tests ----------------------------------------------------

    #[test]
    fn render_produces_no_output_when_not_visible() {
        let feature = make_feature();
        let theme = test_theme_colors();
        let ctx = make_render_ctx(&theme);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                feature.render(SurfaceId::new("file_selector"), frame, &ctx);
            })
            .expect("draw");
    }

    #[test]
    fn render_produces_no_output_for_wrong_surface() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let ctx = make_render_ctx(&theme);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                feature.render(SurfaceId::new("other_surface"), frame, &ctx);
            })
            .expect("draw");
    }

    // -- Command / keymap registration tests --------------------------------

    #[test]
    fn register_registers_slash_commands() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(reg.command_feature("/file open"), Some("file_selector"));
        assert_eq!(reg.command_feature("/file close"), Some("file_selector"));
        assert_eq!(reg.command_feature("/file"), None);
    }

    #[test]
    fn register_registers_keymap() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        let (feature_id, action) = reg
            .keymap_feature("Ctrl+P")
            .expect("keymap should be registered");
        assert_eq!(feature_id, "file_selector");
        assert_eq!(action, "toggle_file_selector");
    }

    // -- handle_command() tests --------------------------------------------

    #[test]
    fn handle_command_open_shows_selector() {
        let mut feature = make_feature();
        assert!(!feature.is_visible());

        let actions = feature.handle_command("/file open");
        assert!(feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
        if let TuiAction::OpenModal(id) = &actions[0] {
            assert_eq!(id.as_str(), "file_selector");
        }
    }

    #[test]
    fn handle_command_close_hides_selector() {
        let mut feature = make_feature();
        feature.show();

        let actions = feature.handle_command("/file close");
        assert!(!feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::CloseModal));
    }

    #[test]
    fn handle_command_unknown_returns_empty() {
        let mut feature = make_feature();
        let actions = feature.handle_command("/unknown");
        assert!(actions.is_empty());
        assert!(!feature.is_visible());
    }

    // -- toggle_visibility() tests -----------------------------------------

    #[test]
    fn toggle_visibility_opens_when_closed() {
        let mut feature = make_feature();
        assert!(!feature.is_visible());

        let actions = feature.toggle_visibility();
        assert!(feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
    }

    #[test]
    fn toggle_visibility_closes_when_open() {
        let mut feature = make_feature();
        feature.show();

        let actions = feature.toggle_visibility();
        assert!(!feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::CloseModal));
    }

    #[test]
    fn toggle_visibility_roundtrip() {
        let mut feature = make_feature();
        assert!(!feature.is_visible());

        feature.toggle_visibility();
        assert!(feature.is_visible());

        feature.toggle_visibility();
        assert!(!feature.is_visible());
    }

    // -- as_any_mut() downcast test ----------------------------------------

    #[test]
    fn as_any_mut_allows_downcast() {
        let mut feature = make_feature();
        let any_ref = feature.as_any_mut();
        let downcast = any_ref.downcast_mut::<FileSelectorFeature>();
        assert!(downcast.is_some());

        let downcast = downcast.expect("downcast");
        assert!(!downcast.is_visible());
        downcast.show();
        assert!(downcast.is_visible());
    }

    // -- format_key_event integration tests --------------------------------

    #[test]
    fn ctrl_p_formats_correctly() {
        use crate::app::features::format_key_event;
        assert_eq!(format_key_event(&ctrl_p()), "Ctrl+P");
    }
}
