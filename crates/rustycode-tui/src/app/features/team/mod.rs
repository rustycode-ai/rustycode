//! Team Mode feature module.
//!
//! Self-contained feature for multi-agent team orchestration display.
//! Implements the [`TuiFeature`] trait and owns all team mode state.
//!
//! ## State
//! - [`TeamFeatureState`]: Wraps [`TeamPanel`], [`TeamModeHandler`], and [`AgentManager`].
//!
//! ## Events Handled
//! - `TuiEvent::Key`: Navigation and toggle when visible.
//! - Other events (Tick, Stream, Service) are ignored.
//!
//! ## Surfaces
//! - `"team"`: Main team panel overlay.
//!
//! ## Routes
//! - `"team"`: Navigation route to open the team panel.
//!
//! ## Rendering
//! Delegates to [`TeamPanel`] rendering when visible.

use crate::agents::AgentManager;
use crate::app::features::{
    FeatureRegistry, ModalId, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature,
    UpdateCtx,
};
use crate::app::team_mode_handler::TeamModeHandler;
use crate::ui::team_panel::TeamPanel;
use crossterm::event::KeyEvent;
use ratatui::Frame;
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Team Mode feature state.
///
/// Holds all team-related data needed by the feature. The `team_panel` and
/// `team_handler` fields are wrapped in [`Mutex`] because they contain
/// interior-mutable types (`Cell`) which are `Send` but not `Sync`.
/// [`TuiFeature`] requires `Send + Sync`.
pub struct TeamFeatureState {
    /// Team panel UI state (thread-safe wrapped for `Sync`).
    pub team_panel: Mutex<TeamPanel>,
    /// Team mode event handler (thread-safe wrapped for `Sync`).
    pub team_handler: Mutex<TeamModeHandler>,
    /// Agent manager, shared with the rest of the TUI.
    pub agent_manager: Mutex<AgentManager>,
    /// Whether the team panel overlay is currently visible.
    pub visible: bool,
    /// Index of the currently selected agent in the list.
    pub selected_agent: usize,
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

/// Team Mode feature implementing [`TuiFeature`].
///
/// Handles key events for agent navigation and delegates rendering
/// to [`TeamPanel`].
///
/// Backward-compatible alias: [`TeamFeatureState`] re-exports this type
/// so existing callers can continue to use the old name.
pub struct TeamFeature {
    state: TeamFeatureState,
}

/// Backward-compatible alias — the old name for the feature type.
pub use TeamFeature as TeamFeatureStateCompat;

impl TeamFeature {
    /// Create a new `TeamFeature`.
    ///
    /// The UI starts hidden.
    pub fn new() -> Self {
        Self {
            state: TeamFeatureState {
                team_panel: Mutex::new(TeamPanel::new()),
                team_handler: Mutex::new(TeamModeHandler::new()),
                agent_manager: Mutex::new(AgentManager::new()),
                visible: false,
                selected_agent: 0,
            },
        }
    }

    /// Show the team panel overlay.
    pub fn show(&mut self) {
        self.state.visible = true;
        if let Ok(mut panel) = self.state.team_panel.lock() {
            panel.visible = true;
        }
    }

    /// Hide the team panel overlay.
    pub fn hide(&mut self) {
        self.state.visible = false;
        if let Ok(mut panel) = self.state.team_panel.lock() {
            panel.visible = false;
        }
    }

    /// Check if the team panel overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.state.visible
    }

    /// Surface ID used by this feature.
    const SURFACE: SurfaceId = SurfaceId::new("team");

    /// Route ID for navigating to the team panel.
    const ROUTE: RouteId = RouteId::new("team");

    /// Modal ID for the team panel overlay.
    const MODAL: ModalId = ModalId::new("team");

    /// Slash command to open the team panel.
    const CMD_OPEN: &str = "/team";

    /// Slash command to close the team panel.
    const CMD_CLOSE: &str = "/team close";

    /// Keyboard shortcut to toggle the team panel (Ctrl+T).
    const KEYMAP_TOGGLE: &str = "Ctrl+T";
}

impl Default for TeamFeature {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiFeature for TeamFeature {
    fn id(&self) -> &'static str {
        "team"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
        reg.register_command(Self::CMD_OPEN, self.id());
        reg.register_command(Self::CMD_CLOSE, self.id());
        reg.register_keymap(
            Self::KEYMAP_TOGGLE.to_string(),
            self.id(),
            "toggle_team",
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

        let panel = match self.state.team_panel.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("team panel lock poisoned: {e}");
                e.into_inner()
            }
        };

        // Delegate rendering to the TeamPanel widget
        panel.render(frame, ctx.frame_area);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Public API: command handling
// ---------------------------------------------------------------------------

impl TeamFeature {
    /// Handle a slash command dispatched to this feature.
    ///
    /// Returns actions for the shell to process. Recognized commands:
    /// - `"/team"` — shows the team panel overlay
    /// - `"/team close"` — hides the team panel overlay
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

    /// Toggle team panel visibility.
    ///
    /// Called when the keymap shortcut is pressed. Returns `OpenModal` if shown,
    /// `CloseModal` if hidden.
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

impl TeamFeature {
    /// Handle a keyboard event when the team panel is visible.
    fn handle_key_event(&mut self, key: KeyEvent) -> Vec<TuiAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        if !self.state.visible {
            return Vec::new();
        }

        let mut panel = match self.state.team_panel.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("team panel lock poisoned: {e}");
                e.into_inner()
            }
        };

        match key {
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                panel.visible = false;
                drop(panel);
                self.state.visible = false;
                vec![TuiAction::CloseModal]
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let agent_count = panel.active_agent_name().map_or(5, |_| 5);
                self.state.selected_agent =
                    (self.state.selected_agent + 1).min(agent_count.saturating_sub(1));
                vec![TuiAction::MarkDirty]
            }
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.selected_agent = self.state.selected_agent.saturating_sub(1);
                vec![TuiAction::MarkDirty]
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
    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::style::Color;

    // -- Helpers -----------------------------------------------------------

    fn make_feature() -> TeamFeature {
        TeamFeature::new()
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

    fn down_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
    }

    fn up_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)
    }

    // -- new() tests -------------------------------------------------------

    #[test]
    fn new_creates_hidden_state() {
        let feature = make_feature();
        assert_eq!(feature.id(), "team");
        assert!(!feature.state.visible);
        assert_eq!(feature.state.selected_agent, 0);
    }

    #[test]
    fn default_creates_hidden_state() {
        let feature = TeamFeature::default();
        assert!(!feature.is_visible());
    }

    // -- register() tests --------------------------------------------------

    #[test]
    fn register_registers_surface_and_route() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(
            reg.surface_feature(SurfaceId::new("team")),
            Some("team")
        );
        assert_eq!(
            reg.route_feature(RouteId::new("team")),
            Some("team")
        );
    }

    #[test]
    fn register_registers_slash_commands() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(reg.command_feature("/team"), Some("team"));
        assert_eq!(reg.command_feature("/team close"), Some("team"));
        assert_eq!(reg.command_feature("/team unknown"), None);
    }

    #[test]
    fn register_registers_keymap() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        let (feature_id, action) = reg
            .keymap_feature("Ctrl+T")
            .expect("keymap should be registered");
        assert_eq!(feature_id, "team");
        assert_eq!(action, "toggle_team");
    }

    #[test]
    fn register_registers_everything() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert!(reg.surface_feature(SurfaceId::new("team")).is_some());
        assert!(reg.route_feature(RouteId::new("team")).is_some());
        assert!(reg.command_feature("/team").is_some());
        assert!(reg.command_feature("/team close").is_some());
        assert!(reg.keymap_feature("Ctrl+T").is_some());
    }

    // -- visibility tests --------------------------------------------------

    #[test]
    fn show_sets_visible() {
        let mut feature = make_feature();
        assert!(!feature.state.visible);
        feature.show();
        assert!(feature.state.visible);
        assert!(feature.state.team_panel.lock().expect("lock").visible);
    }

    #[test]
    fn hide_clears_visible() {
        let mut feature = make_feature();
        feature.show();
        assert!(feature.state.visible);
        feature.hide();
        assert!(!feature.state.visible);
        assert!(!feature.state.team_panel.lock().expect("lock").visible);
    }

    #[test]
    fn is_visible_matches_state() {
        let mut feature = make_feature();
        assert!(!feature.is_visible());
        feature.show();
        assert!(feature.is_visible());
        feature.hide();
        assert!(!feature.is_visible());
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

        let actions = feature.update(&TuiEvent::Key(down_key()), &mut ctx);
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
    fn update_down_returns_mark_dirty_when_visible() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(down_key()), &mut ctx);

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::MarkDirty));
        assert!(feature.state.visible);
    }

    #[test]
    fn update_up_returns_mark_dirty_when_visible() {
        let mut feature = make_feature();
        feature.show();
        feature.state.selected_agent = 2;

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(up_key()), &mut ctx);

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::MarkDirty));
        assert_eq!(feature.state.selected_agent, 1);
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
                feature.render(SurfaceId::new("team"), frame, &ctx);
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
    fn handle_command_open_shows_team() {
        let mut feature = make_feature();
        assert!(!feature.is_visible());

        let actions = feature.handle_command("/team");
        assert!(feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
    }

    #[test]
    fn handle_command_close_hides_team() {
        let mut feature = make_feature();
        feature.show();

        let actions = feature.handle_command("/team close");
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
        let downcast = any_ref.downcast_mut::<TeamFeature>();
        assert!(downcast.is_some());

        let downcast = downcast.expect("downcast");
        assert!(!downcast.is_visible());
        downcast.show();
        assert!(downcast.is_visible());
    }

    // -- selected_agent bounds tests ---------------------------------------

    #[test]
    fn down_key_clamps_to_max() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        // Press down many times — should clamp
        for _ in 0..20 {
            feature.update(&TuiEvent::Key(down_key()), &mut ctx);
        }
        // selected_agent should be clamped (max = 4 since agent_count = 5)
        assert!(feature.state.selected_agent <= 4);
    }

    #[test]
    fn up_key_clamps_at_zero() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        feature.state.selected_agent = 0;
        feature.update(&TuiEvent::Key(up_key()), &mut ctx);
        assert_eq!(feature.state.selected_agent, 0);
    }
}
