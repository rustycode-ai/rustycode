//! Plugin Manager feature module.
//!
//! Self-contained feature for managing plugins (list, search, install, uninstall).
//! Implements the [`TuiFeature`] trait and owns all plugin manager state.
//!
//! ## State
//! - [`PluginManagerState`]: Wraps [`PluginManagerUI`] and the shared [`PluginManager`].
//!
//! ## Events Handled
//! - `TuiEvent::Key`: Delegated to [`PluginManagerUI::handle_key`] when visible.
//! - Other events (Tick, Stream, Service) are ignored.
//!
//! ## Surfaces
//! - `"plugin_manager"`: Main plugin manager overlay.
//!
//! ## Routes
//! - `"plugins"`: Navigation route to open the plugin manager.
//!
//! ## Rendering
//! Delegates to [`PluginManagerUI::render`] when visible.

use crate::app::features::{
    FeatureRegistry, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature, UpdateCtx,
};
use crate::plugin::{PluginManager, PluginManagerUI};
use crossterm::event::KeyEvent;
use ratatui::Frame;
use std::sync::{Arc, Mutex, RwLock};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Plugin Manager feature state.
///
/// Holds all plugin-related data needed by the feature. The `ui` field is
/// wrapped in [`Mutex`] because [`PluginManagerUI`] contains `Cell<usize>`
/// which is `Send` but not `Sync`, and [`TuiFeature`] requires `Send + Sync`.
pub struct PluginManagerState {
    /// Plugin manager UI state (thread-safe wrapped for `Sync`).
    pub ui: Mutex<PluginManagerUI>,
    /// Plugin manager backend, shared with the rest of the TUI.
    pub manager: Arc<RwLock<PluginManager>>,
    /// Whether the plugin manager overlay is currently visible.
    pub visible: bool,
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

/// Plugin Manager feature implementing [`TuiFeature`].
///
/// Handles key events for plugin navigation/search and delegates rendering
/// to [`PluginManagerUI`].
///
/// Backward-compatible alias: [`PluginManagerState`] re-exports this type
/// so existing callers can continue to use the old name.
pub struct PluginManagerFeature {
    state: PluginManagerState,
}

/// Backward-compatible alias — the old name for the feature type.
pub use PluginManagerFeature as PluginManagerStateCompat;

impl PluginManagerFeature {
    /// Create a new `PluginManagerFeature`.
    ///
    /// Takes an `Arc<RwLock<PluginManager>>` for the shared plugin manager
    /// backend. The UI starts hidden.
    pub fn new(manager: Arc<RwLock<PluginManager>>) -> Self {
        Self {
            state: PluginManagerState {
                ui: Mutex::new(PluginManagerUI::new()),
                manager,
                visible: false,
            },
        }
    }

    /// Show the plugin manager overlay.
    pub fn show(&mut self) {
        self.state.visible = true;
        if let Ok(mut ui) = self.state.ui.lock() {
            ui.show();
        }
    }

    /// Hide the plugin manager overlay.
    pub fn hide(&mut self) {
        self.state.visible = false;
        if let Ok(mut ui) = self.state.ui.lock() {
            ui.hide();
        }
    }

    /// Check if the plugin manager overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.state.visible
    }

    /// Surface ID used by this feature.
    const SURFACE: SurfaceId = SurfaceId::new("plugin_manager");

    /// Route ID for navigating to the plugin manager.
    const ROUTE: RouteId = RouteId::new("plugins");
}

impl TuiFeature for PluginManagerFeature {
    fn id(&self) -> &'static str {
        "plugin_manager"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
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

        let ui = match self.state.ui.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("plugin manager UI lock poisoned: {e}");
                e.into_inner()
            }
        };

        let manager = match self.state.manager.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("plugin manager lock poisoned: {e}");
                e.into_inner()
            }
        };

        ui.render(frame, ctx.frame_area, &manager);
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

impl PluginManagerFeature {
    /// Handle a keyboard event when the plugin manager is visible.
    fn handle_key_event(&mut self, key: KeyEvent) -> Vec<TuiAction> {
        if !self.state.visible {
            return Vec::new();
        }

        let mut ui = match self.state.ui.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("plugin manager UI lock poisoned: {e}");
                e.into_inner()
            }
        };

        let _handled = ui.handle_key(key, &self.state.manager);

        if !ui.is_visible() {
            // UI was hidden by handle_key (e.g., Esc in list mode).
            drop(ui);
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

    fn make_feature() -> PluginManagerFeature {
        let manager = Arc::new(RwLock::new(PluginManager::default()));
        PluginManagerFeature::new(manager)
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

    // -- new() tests -------------------------------------------------------

    #[test]
    fn new_creates_empty_state() {
        let feature = make_feature();
        assert_eq!(feature.id(), "plugin_manager");
        assert!(!feature.state.visible);

        // UI should start not visible
        let ui = feature.state.ui.lock().expect("lock");
        assert!(!ui.is_visible());
        assert!(ui.query.is_empty());
        assert_eq!(ui.selected_index, 0);
    }

    // -- register() tests --------------------------------------------------

    #[test]
    fn register_registers_surface_and_route() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(
            reg.surface_feature(SurfaceId::new("plugin_manager")),
            Some("plugin_manager")
        );
        assert_eq!(
            reg.route_feature(RouteId::new("plugins")),
            Some("plugin_manager")
        );
    }

    // -- visibility tests --------------------------------------------------

    #[test]
    fn show_sets_visible() {
        let mut feature = make_feature();
        assert!(!feature.state.visible);
        feature.show();
        assert!(feature.state.visible);
        assert!(feature.state.ui.lock().expect("lock").is_visible());
    }

    #[test]
    fn hide_clears_visible() {
        let mut feature = make_feature();
        feature.show();
        assert!(feature.state.visible);
        feature.hide();
        assert!(!feature.state.visible);
        assert!(!feature.state.ui.lock().expect("lock").is_visible());
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
    fn update_handled_key_returns_mark_dirty_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        // Down arrow is handled by the UI for navigation
        let down_key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let actions = feature.update(&TuiEvent::Key(down_key), &mut ctx);

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
                feature.render(SurfaceId::new("plugin_manager"), frame, &ctx);
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
}
