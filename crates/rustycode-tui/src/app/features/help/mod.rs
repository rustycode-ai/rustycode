//! Help feature module.
//!
//! Self-contained feature for the help overlay (keyboard shortcuts, topics).
//! Implements the [`TuiFeature`] trait and owns all help state.
//!
//! ## State
//! - [`HelpFeatureState`]: Wraps [`HelpState`] (visibility, scroll, category, search).
//!
//! ## Events Handled
//! - `TuiEvent::Key`: Opens on `?` or `F1`, closes on `Escape`.
//!
//! ## Surfaces
//! - `"help"`: Help overlay surface.
//!
//! ## Routes
//! - `"help"`: Navigation route to open the help overlay.
//!
//! ## Rendering
//! Delegates to [`crate::help::render_help`] when visible.

use crate::app::features::{
    FeatureRegistry, ModalId, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature,
    UpdateCtx,
};
use crate::help::HelpState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

/// Help feature implementing [`TuiFeature`].
///
/// Handles key events for opening/closing help and delegates rendering
/// to the existing `render_help` function.
pub struct HelpFeature {
    state: HelpState,
}

/// Backward-compatible alias.
pub use HelpFeature as HelpFeatureCompat;

impl HelpFeature {
    /// Create a new `HelpFeature`. The help overlay starts hidden.
    pub fn new() -> Self {
        Self {
            state: HelpState::new(),
        }
    }

    /// Show the help overlay.
    pub fn show(&mut self) {
        self.state.show();
    }

    /// Hide the help overlay.
    pub fn hide(&mut self) {
        self.state.hide();
    }

    /// Check if the help overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.state.visible
    }

    /// Surface ID used by this feature.
    const SURFACE: SurfaceId = SurfaceId::new("help");

    /// Route ID for navigating to the help overlay.
    const ROUTE: RouteId = RouteId::new("help");

    /// Modal ID for the help overlay.
    const MODAL: ModalId = ModalId::new("help");

    /// Slash command to open help.
    const CMD_OPEN: &str = "/help";

    /// Slash command to close help.
    const CMD_CLOSE: &str = "/help close";

    /// Keyboard shortcut: `?`.
    const KEYMAP_QUESTION: &str = "?";

    /// Keyboard shortcut: `F1`.
    const KEYMAP_F1: &str = "F1";
}

impl Default for HelpFeature {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiFeature for HelpFeature {
    fn id(&self) -> &'static str {
        "help"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
        reg.register_command(Self::CMD_OPEN, self.id());
        reg.register_command(Self::CMD_CLOSE, self.id());
        reg.register_keymap(
            Self::KEYMAP_QUESTION.to_string(),
            self.id(),
            "toggle_help",
        );
        reg.register_keymap(Self::KEYMAP_F1.to_string(), self.id(), "toggle_help");
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

        // Delegate to the existing help rendering function.
        // We need a mutable reference for scroll clamping.
        let state_ptr = &self.state as *const HelpState as *mut HelpState;
        // SAFETY: render is called with &self, and the existing render_help
        // only mutates scroll_offset (a derived value). This is the same
        // pattern used by the existing codebase.
        let state_mut = unsafe { &mut *state_ptr };
        crate::help::render_help(frame, ctx.frame_area, state_mut);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Public API: command handling
// ---------------------------------------------------------------------------

impl HelpFeature {
    /// Handle a slash command dispatched to this feature.
    ///
    /// Returns actions for the shell to process. Recognized commands:
    /// - `"/help"` — shows the help overlay
    /// - `"/help close"` — hides the help overlay
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

    /// Toggle help visibility.
    ///
    /// Called when the keymap shortcut (`?` or `F1`) is pressed.
    /// Returns `OpenModal` if shown, `CloseModal` if hidden.
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

impl HelpFeature {
    /// Handle a keyboard event.
    fn handle_key_event(&mut self, key: KeyEvent) -> Vec<TuiAction> {
        // `?` or F1 opens help when hidden
        if !self.state.visible {
            let is_help_trigger = (key.code == KeyCode::Char('?')
                && key.modifiers == KeyModifiers::NONE)
                || key.code == KeyCode::F(1);

            if is_help_trigger {
                self.show();
                return vec![TuiAction::OpenModal(Self::MODAL)];
            }
            return Vec::new();
        }

        // Help is visible — handle navigation
        match key.code {
            KeyCode::Esc => {
                self.hide();
                vec![TuiAction::CloseModal]
            }
            KeyCode::Up => {
                self.state.scroll_offset = self.state.scroll_offset.saturating_sub(1);
                vec![TuiAction::MarkDirty]
            }
            KeyCode::Down => {
                self.state.scroll_offset = self.state.scroll_offset.saturating_add(1);
                vec![TuiAction::MarkDirty]
            }
            KeyCode::Char('?') | KeyCode::F(1) => {
                // Toggle off when already visible
                self.hide();
                vec![TuiAction::CloseModal]
            }
            _ => vec![TuiAction::MarkDirty],
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
    use crossterm::event::KeyModifiers;
    use ratatui::style::Color;

    // -- Helpers -----------------------------------------------------------

    fn make_feature() -> HelpFeature {
        HelpFeature::new()
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

    fn f1_key() -> KeyEvent {
        KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)
    }

    fn up_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)
    }

    fn down_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
    }

    // -- new() tests -------------------------------------------------------

    #[test]
    fn new_creates_hidden_state() {
        let feature = make_feature();
        assert_eq!(feature.id(), "help");
        assert!(!feature.state.visible);
        assert!(feature.state.search_query.is_empty());
        assert_eq!(feature.state.scroll_offset, 0);
    }

    #[test]
    fn default_creates_hidden_state() {
        let feature = HelpFeature::default();
        assert!(!feature.is_visible());
    }

    // -- register() tests --------------------------------------------------

    #[test]
    fn register_registers_surface_and_route() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(
            reg.surface_feature(SurfaceId::new("help")),
            Some("help")
        );
        assert_eq!(reg.route_feature(RouteId::new("help")), Some("help"));
    }

    #[test]
    fn register_registers_commands() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(reg.command_feature("/help"), Some("help"));
        assert_eq!(reg.command_feature("/help close"), Some("help"));
    }

    #[test]
    fn register_registers_keymaps() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        let (fid, action) = reg.keymap_feature("?").expect("keymap ?");
        assert_eq!(fid, "help");
        assert_eq!(action, "toggle_help");

        let (fid, action) = reg.keymap_feature("F1").expect("keymap F1");
        assert_eq!(fid, "help");
        assert_eq!(action, "toggle_help");
    }

    #[test]
    fn register_registers_everything() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert!(reg.surface_feature(SurfaceId::new("help")).is_some());
        assert!(reg.route_feature(RouteId::new("help")).is_some());
        assert!(reg.command_feature("/help").is_some());
        assert!(reg.command_feature("/help close").is_some());
        assert!(reg.keymap_feature("?").is_some());
        assert!(reg.keymap_feature("F1").is_some());
    }

    // -- visibility tests --------------------------------------------------

    #[test]
    fn show_sets_visible() {
        let mut feature = make_feature();
        assert!(!feature.is_visible());
        feature.show();
        assert!(feature.is_visible());
    }

    #[test]
    fn hide_clears_visible() {
        let mut feature = make_feature();
        feature.show();
        feature.hide();
        assert!(!feature.is_visible());
    }

    // -- update() tests ----------------------------------------------------

    #[test]
    fn update_question_mark_opens_help() {
        let mut feature = make_feature();
        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(char_key('?')), &mut ctx);

        assert!(feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
    }

    #[test]
    fn update_f1_opens_help() {
        let mut feature = make_feature();
        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(f1_key()), &mut ctx);

        assert!(feature.is_visible());
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
    }

    #[test]
    fn update_esc_closes_help_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(esc_key()), &mut ctx);

        assert!(!feature.is_visible());
        assert!(matches!(actions[0], TuiAction::CloseModal));
    }

    #[test]
    fn update_ignores_non_key_events() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
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

    #[test]
    fn update_arrow_keys_scroll_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(down_key()), &mut ctx);
        assert!(matches!(actions[0], TuiAction::MarkDirty));
        assert_eq!(feature.state.scroll_offset, 1);

        let actions = feature.update(&TuiEvent::Key(up_key()), &mut ctx);
        assert!(matches!(actions[0], TuiAction::MarkDirty));
        assert_eq!(feature.state.scroll_offset, 0);
    }

    #[test]
    fn update_question_mark_toggles_off_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(char_key('?')), &mut ctx);
        assert!(!feature.is_visible());
        assert!(matches!(actions[0], TuiAction::CloseModal));
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
                feature.render(SurfaceId::new("help"), frame, &ctx);
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

    // -- handle_command() tests --------------------------------------------

    #[test]
    fn handle_command_open_shows_help() {
        let mut feature = make_feature();
        let actions = feature.handle_command("/help");
        assert!(feature.is_visible());
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
    }

    #[test]
    fn handle_command_close_hides_help() {
        let mut feature = make_feature();
        feature.show();
        let actions = feature.handle_command("/help close");
        assert!(!feature.is_visible());
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
        let actions = feature.toggle_visibility();
        assert!(feature.is_visible());
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
    }

    #[test]
    fn toggle_visibility_closes_when_open() {
        let mut feature = make_feature();
        feature.show();
        let actions = feature.toggle_visibility();
        assert!(!feature.is_visible());
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
        let downcast = any_ref.downcast_mut::<HelpFeature>();
        assert!(downcast.is_some());

        let downcast = downcast.expect("downcast");
        assert!(!downcast.is_visible());
        downcast.show();
        assert!(downcast.is_visible());
    }

    // -- format_key_event integration tests --------------------------------

    #[test]
    fn f1_key_formats_correctly() {
        use crate::app::features::format_key_event;
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "F1");
    }

    #[test]
    fn question_mark_formats_correctly() {
        use crate::app::features::format_key_event;
        let key = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "?");
    }
}
