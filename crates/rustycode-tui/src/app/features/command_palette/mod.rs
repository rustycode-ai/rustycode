//! Command Palette feature module.
//!
//! Self-contained feature for the command palette overlay (fuzzy search, commands).
//! Implements the [`TuiFeature`] trait and owns all command palette state.
//!
//! ## State
//! - [`CommandPaletteFeatureState`]: Wraps [`CommandPalette`] (visibility, query, filtered list).
//!
//! ## Events Handled
//! - `TuiEvent::Key`: Opens on `Ctrl+K`, closes on `Escape`, handles input and selection.
//!
//! ## Surfaces
//! - `"command_palette"`: Command palette overlay surface.
//!
//! ## Routes
//! - `"command_palette"`: Navigation route to open the command palette.
//!
//! ## Rendering
//! Delegates to [`crate::ui::command_palette::CommandPalette::render`] when visible.

use crate::app::features::{
    FeatureRegistry, ModalId, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature,
    UpdateCtx,
};
use crate::ui::command_palette::CommandPalette;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Command Palette feature state.
///
/// Holds the [`CommandPalette`] instance which manages query, filtered commands,
/// selected index, and visibility.
pub struct CommandPaletteFeatureState {
    /// Command palette UI (query, filtered list, selected index).
    pub palette: std::sync::Mutex<CommandPalette>,
    /// Whether the command palette overlay is currently visible.
    pub visible: bool,
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

/// Command Palette feature implementing [`TuiFeature`].
///
/// Handles key events for palette interaction and delegates rendering
/// to the existing `CommandPalette`.
pub struct CommandPaletteFeature {
    state: CommandPaletteFeatureState,
}

/// Backward-compatible alias.
pub use CommandPaletteFeature as CommandPaletteFeatureCompat;

impl CommandPaletteFeature {
    /// Create a new `CommandPaletteFeature`. The palette starts hidden.
    pub fn new(palette: CommandPalette) -> Self {
        Self {
            state: CommandPaletteFeatureState {
                visible: palette.is_visible(),
                palette: std::sync::Mutex::new(palette),
            },
        }
    }

    /// Show the command palette overlay.
    pub fn show(&mut self) {
        self.state.visible = true;
        if let Ok(mut p) = self.state.palette.lock() {
            p.show();
        }
    }

    /// Hide the command palette overlay.
    pub fn hide(&mut self) {
        self.state.visible = false;
        if let Ok(mut p) = self.state.palette.lock() {
            p.hide();
        }
    }

    /// Check if the command palette overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.state.visible
    }

    /// Surface ID used by this feature.
    const SURFACE: SurfaceId = SurfaceId::new("command_palette");

    /// Route ID for navigating to the command palette.
    const ROUTE: RouteId = RouteId::new("command_palette");

    /// Modal ID for the command palette overlay.
    const MODAL: ModalId = ModalId::new("command_palette");

    /// Slash command to open the command palette.
    const CMD_OPEN: &str = "/palette";

    /// Slash command to close the command palette.
    const CMD_CLOSE: &str = "/palette close";

    /// Keyboard shortcut: `Ctrl+K`.
    const KEYMAP_TOGGLE: &str = "Ctrl+K";
}

impl TuiFeature for CommandPaletteFeature {
    fn id(&self) -> &'static str {
        "command_palette"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
        reg.register_command(Self::CMD_OPEN, self.id());
        reg.register_command(Self::CMD_CLOSE, self.id());
        reg.register_keymap(
            Self::KEYMAP_TOGGLE.to_string(),
            self.id(),
            "toggle_command_palette",
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

        let palette = match self.state.palette.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("command palette lock poisoned: {e}");
                e.into_inner()
            }
        };

        palette.render(frame, ctx.frame_area);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Public API: command handling
// ---------------------------------------------------------------------------

impl CommandPaletteFeature {
    /// Handle a slash command dispatched to this feature.
    ///
    /// Returns actions for the shell to process. Recognized commands:
    /// - `"/palette"` — shows the command palette overlay
    /// - `"/palette close"` — hides the command palette overlay
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

    /// Toggle command palette visibility.
    ///
    /// Called when the keymap shortcut (Ctrl+K) is pressed.
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

impl CommandPaletteFeature {
    /// Handle a keyboard event when the command palette is visible.
    fn handle_key_event(&mut self, key: KeyEvent) -> Vec<TuiAction> {
        if key.code == KeyCode::Char('k')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::SHIFT)
            && !key.modifiers.contains(KeyModifiers::ALT)
        {
            return self.toggle_visibility();
        }

        if !self.state.visible {
            return Vec::new();
        }

        let mut palette = match self.state.palette.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("command palette lock poisoned: {e}");
                e.into_inner()
            }
        };

        let handled = palette.handle_key(key);

        if !palette.is_visible() {
            drop(palette);
            self.state.visible = false;
            vec![TuiAction::CloseModal]
        } else if handled {
            // Check if a command was selected (Enter key)
            if let Some(_selected) = palette.take_selected() {
                drop(palette);
                self.state.visible = false;
                vec![TuiAction::CloseModal]
            } else {
                vec![TuiAction::MarkDirty]
            }
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
    use crossterm::event::KeyModifiers;
    use ratatui::style::Color;

    // -- Helpers -----------------------------------------------------------

    fn make_feature() -> CommandPaletteFeature {
        CommandPaletteFeature::new(CommandPalette::new())
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

    fn ctrl_k() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)
    }

    fn char_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn down_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
    }

    fn up_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)
    }

    fn enter_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    }

    // -- new() tests -------------------------------------------------------

    #[test]
    fn new_creates_hidden_state() {
        let feature = make_feature();
        assert_eq!(feature.id(), "command_palette");
        assert!(!feature.state.visible);
        assert!(!feature.is_visible());
    }

    // -- register() tests --------------------------------------------------

    #[test]
    fn register_registers_surface_and_route() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(
            reg.surface_feature(SurfaceId::new("command_palette")),
            Some("command_palette")
        );
        assert_eq!(
            reg.route_feature(RouteId::new("command_palette")),
            Some("command_palette")
        );
    }

    #[test]
    fn register_registers_commands() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(reg.command_feature("/palette"), Some("command_palette"));
        assert_eq!(
            reg.command_feature("/palette close"),
            Some("command_palette")
        );
    }

    #[test]
    fn register_registers_keymap() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        let (fid, action) = reg.keymap_feature("Ctrl+K").expect("keymap Ctrl+K");
        assert_eq!(fid, "command_palette");
        assert_eq!(action, "toggle_command_palette");
    }

    #[test]
    fn register_registers_everything() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert!(reg
            .surface_feature(SurfaceId::new("command_palette"))
            .is_some());
        assert!(reg.route_feature(RouteId::new("command_palette")).is_some());
        assert!(reg.command_feature("/palette").is_some());
        assert!(reg.command_feature("/palette close").is_some());
        assert!(reg.keymap_feature("Ctrl+K").is_some());
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
    fn update_ctrl_k_opens_palette() {
        let mut feature = make_feature();
        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(ctrl_k()), &mut ctx);

        assert!(feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
    }

    #[test]
    fn update_esc_closes_palette_when_visible() {
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
    fn update_char_input_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(char_key('h')), &mut ctx);
        assert!(!actions.is_empty());
    }

    #[test]
    fn update_arrow_navigation_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(down_key()), &mut ctx);
        assert!(!actions.is_empty());
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
    fn update_ctrl_k_toggles_off_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_r: RouteId| {};
        let mut dispatch = |_c: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        // When visible, Ctrl+K is handled by CommandPalette (which toggles off)
        let _actions = feature.update(&TuiEvent::Key(ctrl_k()), &mut ctx);
        // CommandPalette.handle_key on Ctrl+K should toggle off
        assert!(!feature.is_visible());
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
                feature.render(SurfaceId::new("command_palette"), frame, &ctx);
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
    fn handle_command_open_shows_palette() {
        let mut feature = make_feature();
        let actions = feature.handle_command("/palette");
        assert!(feature.is_visible());
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
    }

    #[test]
    fn handle_command_close_hides_palette() {
        let mut feature = make_feature();
        feature.show();
        let actions = feature.handle_command("/palette close");
        assert!(!feature.is_visible());
        assert!(matches!(actions[0], TuiAction::CloseModal));
    }

    #[test]
    fn handle_command_unknown_returns_empty() {
        let mut feature = make_feature();
        let actions = feature.handle_command("/unknown");
        assert!(actions.is_empty());
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

    // -- as_any_mut() downcast test ----------------------------------------

    #[test]
    fn as_any_mut_allows_downcast() {
        let mut feature = make_feature();
        let any_ref = feature.as_any_mut();
        let downcast = any_ref.downcast_mut::<CommandPaletteFeature>();
        assert!(downcast.is_some());

        let downcast = downcast.expect("downcast");
        assert!(!downcast.is_visible());
        downcast.show();
        assert!(downcast.is_visible());
    }

    // -- format_key_event integration test ---------------------------------

    #[test]
    fn ctrl_k_formats_correctly() {
        use crate::app::features::format_key_event;
        assert_eq!(format_key_event(&ctrl_k()), "Ctrl+K");
    }
}
