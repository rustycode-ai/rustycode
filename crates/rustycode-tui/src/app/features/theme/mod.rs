//! Theme feature module.
//!
//! Self-contained feature for theme management (preview, switch, live preview).
//! Implements the [`TuiFeature`] trait and owns all theme-related state.
//!
//! ## State
//! - [`ThemeState`]: Wraps [`ThemePreviewRenderer`](crate::ui::theme_preview::ThemePreviewRenderer)
//!   and [`ThemeSwitcher`](crate::ui::theme_preview::ThemeSwitcher) plus shared theme colors.
//!
//! ## Events Handled
//! - `TuiEvent::Key`: Delegated to theme preview when visible.
//! - `TuiEvent::Tick`: No-op (animation handled by render).
//!
//! ## Surfaces
//! - `"theme_preview"`: Theme preview overlay.
//!
//! ## Routes
//! - `"theme"`: Navigation route to open the theme picker.
//!
//! ## Commands
//! - `"/theme"`: Open the theme picker.
//! - `"/t"`: Alias for `/theme`.
//!
//! ## Rendering
//! Delegates to [`ThemePreviewRenderer::render`](crate::ui::theme_preview::ThemePreviewRenderer::render)
//! when visible.

use crate::app::features::{
    FeatureRegistry, ModalId, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature,
    UpdateCtx,
};
use crate::theme::ThemeColors;
use crate::ui::theme_preview::{ThemePreviewRenderer, ThemeSwitcher};
use crossterm::event::KeyEvent;
use ratatui::Frame;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Theme feature state.
///
/// Holds all theme-related data needed by the feature. Fields are wrapped in
/// interior mutability types because the feature must be `Send + Sync`.
pub struct ThemeState {
    /// Shared theme colors (Arc<Mutex> because updated by preview/switcher).
    pub theme_colors: Arc<Mutex<ThemeColors>>,
    /// Theme preview renderer with its own visibility state.
    pub preview: Mutex<ThemePreviewRenderer>,
    /// Quick theme cycler (no overlay, just switches immediately).
    pub switcher: Mutex<ThemeSwitcher>,
    /// Whether the theme preview overlay is currently visible.
    pub visible: bool,
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

/// Theme feature implementing [`TuiFeature`].
///
/// Handles key events for theme navigation/search and delegates rendering
/// to [`ThemePreviewRenderer`].
///
/// Backward-compatible alias: [`ThemeFeature`] re-exports this type
/// so existing callers can continue to use the old name.
pub struct ThemeFeature {
    state: ThemeState,
}

/// Backward-compatible alias — the old name for the feature type.
pub use ThemeFeature as ThemeStateCompat;

impl ThemeFeature {
    /// Create a new `ThemeFeature`.
    ///
    /// Takes an `Arc<Mutex<ThemeColors>>` for the shared theme colors.
    /// The preview and switcher start hidden.
    pub fn new(theme_colors: Arc<Mutex<ThemeColors>>) -> Self {
        let colors_clone = Arc::clone(&theme_colors);
        let colors_clone2 = Arc::clone(&theme_colors);
        Self {
            state: ThemeState {
                theme_colors,
                preview: Mutex::new(ThemePreviewRenderer::new(colors_clone)),
                switcher: Mutex::new(ThemeSwitcher::new(colors_clone2)),
                visible: false,
            },
        }
    }

    /// Show the theme preview overlay.
    pub fn show(&mut self) {
        self.state.visible = true;
        if let Ok(mut preview) = self.state.preview.lock() {
            preview.show();
        }
    }

    /// Hide the theme preview overlay.
    pub fn hide(&mut self) {
        self.state.visible = false;
        if let Ok(mut preview) = self.state.preview.lock() {
            preview.hide();
        }
    }

    /// Check if the theme preview overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.state.visible
    }

    /// Surface ID used by this feature.
    const SURFACE: SurfaceId = SurfaceId::new("theme_preview");

    /// Route ID for navigating to the theme picker.
    const ROUTE: RouteId = RouteId::new("theme");

    /// Modal ID for the theme preview overlay.
    const MODAL: ModalId = ModalId::new("theme_preview");

    /// Slash command to open the theme picker.
    const CMD_OPEN: &str = "/theme";

    /// Short alias for the slash command.
    const CMD_OPEN_ALIAS: &str = "/t";

    /// Keyboard shortcut to cycle to next theme (Ctrl+T).
    const KEYMAP_NEXT: &str = "Ctrl+T";

    /// Keyboard shortcut to open the theme picker (Ctrl+Shift+T).
    const KEYMAP_TOGGLE: &str = "Ctrl+Shift+T";
}

impl TuiFeature for ThemeFeature {
    fn id(&self) -> &'static str {
        "theme"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
        reg.register_command(Self::CMD_OPEN, self.id());
        reg.register_command(Self::CMD_OPEN_ALIAS, self.id());
        reg.register_keymap(Self::KEYMAP_NEXT.to_string(), self.id(), "cycle_next_theme");
        reg.register_keymap(
            Self::KEYMAP_TOGGLE.to_string(),
            self.id(),
            "toggle_theme_preview",
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

        let preview = match self.state.preview.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("theme preview lock poisoned: {e}");
                e.into_inner()
            }
        };

        preview.render(frame, ctx.frame_area);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Public API: command handling
// ---------------------------------------------------------------------------

impl ThemeFeature {
    /// Handle a slash command dispatched to this feature.
    ///
    /// Returns actions for the shell to process. Recognized commands:
    /// - `"/theme"` — shows the theme preview overlay
    /// - `"/t"` — alias for `/theme`
    pub fn handle_command(&mut self, command: &str) -> Vec<TuiAction> {
        match command {
            cmd if cmd == Self::CMD_OPEN || cmd == Self::CMD_OPEN_ALIAS => {
                self.show();
                vec![TuiAction::OpenModal(Self::MODAL)]
            }
            _ => Vec::new(),
        }
    }

    /// Cycle to the next theme using the quick switcher.
    ///
    /// Called when the keymap shortcut is pressed. Returns a
    /// `StatusMessage` with the new theme name.
    pub fn cycle_next_theme(&mut self) -> Vec<TuiAction> {
        if let Ok(mut switcher) = self.state.switcher.lock() {
            if let Some(theme) = switcher.next_theme() {
                let name = theme.name.clone();
                return vec![TuiAction::StatusMessage(format!("Theme: {name}"))];
            }
        }
        Vec::new()
    }

    /// Toggle theme preview visibility.
    ///
    /// Called when the keymap shortcut is pressed. Returns `OpenModal` if
    /// shown, `CloseModal` if hidden.
    pub fn toggle_visibility(&mut self) -> Vec<TuiAction> {
        if self.state.visible {
            self.hide();
            vec![TuiAction::CloseModal]
        } else {
            self.show();
            vec![TuiAction::OpenModal(Self::MODAL)]
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

impl ThemeFeature {
    /// Handle a keyboard event when the theme preview is visible.
    fn handle_key_event(&mut self, key: KeyEvent) -> Vec<TuiAction> {
        if !self.state.visible {
            return Vec::new();
        }

        let mut preview = match self.state.preview.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("theme preview lock poisoned: {e}");
                e.into_inner()
            }
        };

        let handled = preview.handle_key(key);

        if !handled {
            return Vec::new();
        }

        // Check if the preview was closed by the key handler (Escape / Enter).
        if !preview.state().visible {
            drop(preview);
            self.state.visible = false;
            vec![TuiAction::CloseModal]
        } else {
            vec![TuiAction::MarkDirty]
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

    fn make_feature() -> ThemeFeature {
        ThemeFeature::new(Arc::new(Mutex::new(test_theme_colors())))
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

    // -- new() tests -------------------------------------------------------

    #[test]
    fn new_creates_hidden_state() {
        let feature = make_feature();
        assert_eq!(feature.id(), "theme");
        assert!(!feature.state.visible);
    }

    // -- register() tests --------------------------------------------------

    #[test]
    fn register_registers_surface_and_route() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(
            reg.surface_feature(SurfaceId::new("theme_preview")),
            Some("theme")
        );
        assert_eq!(reg.route_feature(RouteId::new("theme")), Some("theme"));
    }

    // -- visibility tests --------------------------------------------------

    #[test]
    fn show_sets_visible() {
        let mut feature = make_feature();
        assert!(!feature.state.visible);
        feature.show();
        assert!(feature.state.visible);
    }

    #[test]
    fn hide_clears_visible() {
        let mut feature = make_feature();
        feature.show();
        assert!(feature.state.visible);
        feature.hide();
        assert!(!feature.state.visible);
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
                feature.render(SurfaceId::new("theme_preview"), frame, &ctx);
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

    // -- Command registration tests ----------------------------------------

    #[test]
    fn register_registers_slash_commands() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(reg.command_feature("/theme"), Some("theme"));
        assert_eq!(reg.command_feature("/t"), Some("theme"));
        assert_eq!(reg.command_feature("/themes"), None);
    }

    #[test]
    fn register_registers_keymaps() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        let (fid, action) = reg
            .keymap_feature("Ctrl+T")
            .expect("keymap should be registered");
        assert_eq!(fid, "theme");
        assert_eq!(action, "cycle_next_theme");

        let (fid, action) = reg
            .keymap_feature("Ctrl+Shift+T")
            .expect("keymap should be registered");
        assert_eq!(fid, "theme");
        assert_eq!(action, "toggle_theme_preview");
    }

    // -- handle_command() tests --------------------------------------------

    #[test]
    fn handle_command_open_shows_preview() {
        let mut feature = make_feature();
        assert!(!feature.is_visible());

        let actions = feature.handle_command("/theme");
        assert!(feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
        if let TuiAction::OpenModal(id) = &actions[0] {
            assert_eq!(id.as_str(), "theme_preview");
        }
    }

    #[test]
    fn handle_command_alias_opens_preview() {
        let mut feature = make_feature();
        let actions = feature.handle_command("/t");
        assert!(feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
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

    // -- cycle_next_theme() tests ------------------------------------------

    #[test]
    fn cycle_next_theme_returns_status_message() {
        let mut feature = make_feature();
        let actions = feature.cycle_next_theme();
        assert_eq!(actions.len(), 1);
        if let TuiAction::StatusMessage(msg) = &actions[0] {
            assert!(msg.starts_with("Theme: "));
        } else {
            panic!("Expected StatusMessage action");
        }
    }

    // -- as_any_mut() downcast test ----------------------------------------

    #[test]
    fn as_any_mut_allows_downcast() {
        let mut feature = make_feature();
        let any_ref = feature.as_any_mut();
        let downcast = any_ref.downcast_mut::<ThemeFeature>();
        assert!(downcast.is_some());

        let downcast = downcast.expect("downcast");
        assert!(!downcast.is_visible());
        downcast.show();
        assert!(downcast.is_visible());
    }
}
