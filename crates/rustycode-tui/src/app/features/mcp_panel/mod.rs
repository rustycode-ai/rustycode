//! MCP Panel feature module.
//!
//! Self-contained feature for the MCP (Model Context Protocol) panel that shows
//! server list, tool discovery, and tool execution. Implements [`TuiFeature`].
//!
//! ## State
//! - [`McpPanelState`]: Wraps [`McpMode`] and the shared [`McpServerManager`].
//!
//! ## Events Handled
//! - `TuiEvent::Key`: Navigation keys when visible (server/tool selection, Esc to close).
//! - Other events (Tick, Stream, Service) are ignored.
//!
//! ## Surfaces
//! - `"mcp_panel"`: Main MCP panel overlay.
//!
//! ## Routes
//! - `"mcp"`: Navigation route to open the MCP panel.
//!
//! ## Rendering
//! Delegates to [`McpMode::render`] when visible.

use crate::app::features::{
    FeatureRegistry, ModalId, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature,
    UpdateCtx,
};
use crate::services::mcp_mode::McpMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// MCP Panel feature state.
///
/// Holds all MCP-related data needed by the feature. The `mode` field is
/// wrapped in [`RwLock`] because [`McpMode`] contains interior-mutable state
/// (e.g., selected indices, search query) that must be `Send + Sync`.
pub struct McpPanelState {
    /// MCP mode UI state (thread-safe wrapped for `Sync`).
    pub mode: RwLock<McpMode>,
    /// MCP server manager backend, shared with the rest of the TUI.
    pub manager: Arc<tokio::sync::RwLock<rustycode_mcp::McpServerManager>>,
    /// Whether the MCP panel overlay is currently visible.
    pub visible: bool,
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

/// MCP Panel feature implementing [`TuiFeature`].
///
/// Handles key events for server/tool navigation and delegates rendering
/// to [`McpMode`].
///
/// Backward-compatible alias: [`McpPanelState`] re-exports this type
/// so existing callers can continue to use the old name.
pub struct McpPanelFeature {
    state: McpPanelState,
}

/// Backward-compatible alias -- the old name for the feature type.
pub use McpPanelFeature as McpPanelStateCompat;

impl McpPanelFeature {
    /// Create a new `McpPanelFeature`.
    ///
    /// Takes an `Arc<tokio::sync::RwLock<McpServerManager>>` for the shared MCP
    /// manager backend. The panel starts hidden.
    pub fn new(
        mode: McpMode,
        manager: Arc<tokio::sync::RwLock<rustycode_mcp::McpServerManager>>,
    ) -> Self {
        Self {
            state: McpPanelState {
                mode: RwLock::new(mode),
                manager,
                visible: false,
            },
        }
    }

    /// Show the MCP panel overlay.
    pub fn show(&mut self) {
        self.state.visible = true;
    }

    /// Hide the MCP panel overlay.
    pub fn hide(&mut self) {
        self.state.visible = false;
    }

    /// Check if the MCP panel overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.state.visible
    }

    /// Surface ID used by this feature.
    const SURFACE: SurfaceId = SurfaceId::new("mcp_panel");

    /// Route ID for navigating to the MCP panel.
    const ROUTE: RouteId = RouteId::new("mcp");

    /// Modal ID for the MCP panel overlay.
    const MODAL: ModalId = ModalId::new("mcp_panel");

    /// Slash command to open the MCP panel.
    const CMD_OPEN: &str = "/mcp";

    /// Slash command to close the MCP panel.
    const CMD_CLOSE: &str = "/mcp close";

    /// Keyboard shortcut to toggle the MCP panel (Ctrl+M).
    const KEYMAP_TOGGLE: &str = "Ctrl+M";
}

impl TuiFeature for McpPanelFeature {
    fn id(&self) -> &'static str {
        "mcp_panel"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
        reg.register_command(Self::CMD_OPEN, self.id());
        reg.register_command(Self::CMD_CLOSE, self.id());
        reg.register_keymap(
            Self::KEYMAP_TOGGLE.to_string(),
            self.id(),
            "toggle_mcp_panel",
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

        let mut mode = match self.state.mode.write() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("MCP mode lock poisoned: {e}");
                e.into_inner()
            }
        };

        mode.render(frame, ctx.frame_area);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Public API: command handling
// ---------------------------------------------------------------------------

impl McpPanelFeature {
    /// Handle a slash command dispatched to this feature.
    ///
    /// Returns actions for the shell to process. Recognized commands:
    /// - `"/mcp"` -- shows the MCP panel overlay
    /// - `"/mcp close"` -- hides the MCP panel overlay
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

    /// Toggle MCP panel visibility.
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

impl McpPanelFeature {
    /// Handle a keyboard event when the MCP panel is visible.
    fn handle_key_event(&mut self, key: KeyEvent) -> Vec<TuiAction> {
        if !self.state.visible {
            return Vec::new();
        }

        // Handle escape to close the panel
        if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
            self.state.visible = false;
            return vec![TuiAction::CloseModal];
        }

        // Delegate navigation keys to McpMode
        if let Ok(mut mode) = self.state.mode.write() {
            match key.code {
                KeyCode::Down => mode.next_tool(),
                KeyCode::Up => mode.prev_tool(),
                KeyCode::Right | KeyCode::Tab => mode.next_server(),
                KeyCode::Left => mode.prev_server(),
                KeyCode::Char('r') => mode.toggle_resources(),
                KeyCode::Char('e') => mode.switch_execution_mode(),
                _ => {}
            }
        }

        vec![TuiAction::MarkDirty]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeColors;
    use ratatui::style::Color;

    // -- Helpers -----------------------------------------------------------

    fn make_mode() -> McpMode {
        McpMode {
            selected_server: 0,
            selected_tool: 0,
            show_resources: false,
            server_health: Vec::new(),
            tools: Vec::new(),
            execution_results: Vec::new(),
            search_query: String::new(),
            execution_mode: crate::services::mcp_mode::ExecutionMode::Single,
            server_proxies: std::collections::HashMap::new(),
            server_resources: std::collections::HashMap::new(),
            loading_state: None,
        }
    }

    fn make_feature() -> McpPanelFeature {
        let mode = make_mode();
        let manager = Arc::new(tokio::sync::RwLock::new(
            rustycode_mcp::McpServerManager::default_config(),
        ));
        McpPanelFeature::new(mode, manager)
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

    fn down_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)
    }

    fn up_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)
    }

    fn right_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)
    }

    fn char_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    // -- new() tests -------------------------------------------------------

    #[test]
    fn new_creates_hidden_state() {
        let feature = make_feature();
        assert_eq!(feature.id(), "mcp_panel");
        assert!(!feature.state.visible);
    }

    // -- register() tests --------------------------------------------------

    #[test]
    fn register_registers_surface_and_route() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(
            reg.surface_feature(SurfaceId::new("mcp_panel")),
            Some("mcp_panel")
        );
        assert_eq!(reg.route_feature(RouteId::new("mcp")), Some("mcp_panel"));
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
    fn update_navigation_key_returns_mark_dirty_when_visible() {
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
                feature.render(SurfaceId::new("mcp_panel"), frame, &ctx);
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

        assert_eq!(reg.command_feature("/mcp"), Some("mcp_panel"));
        assert_eq!(reg.command_feature("/mcp close"), Some("mcp_panel"));
        assert_eq!(reg.command_feature("/mcp open"), None);
    }

    #[test]
    fn register_registers_keymap() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        let (feature_id, action) = reg
            .keymap_feature("Ctrl+M")
            .expect("keymap should be registered");
        assert_eq!(feature_id, "mcp_panel");
        assert_eq!(action, "toggle_mcp_panel");
    }

    // -- handle_command() tests --------------------------------------------

    #[test]
    fn handle_command_open_shows_panel() {
        let mut feature = make_feature();
        assert!(!feature.is_visible());

        let actions = feature.handle_command("/mcp");
        assert!(feature.is_visible());
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
        if let TuiAction::OpenModal(id) = &actions[0] {
            assert_eq!(id.as_str(), "mcp_panel");
        }
    }

    #[test]
    fn handle_command_close_hides_panel() {
        let mut feature = make_feature();
        feature.show();

        let actions = feature.handle_command("/mcp close");
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
        let downcast = any_ref.downcast_mut::<McpPanelFeature>();
        assert!(downcast.is_some());

        let downcast = downcast.expect("downcast");
        assert!(!downcast.is_visible());
        downcast.show();
        assert!(downcast.is_visible());
    }

    // -- format_key_event integration test ---------------------------------

    #[test]
    fn ctrl_m_formats_correctly() {
        use crate::app::features::format_key_event;
        let key = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL);
        assert_eq!(format_key_event(&key), "Ctrl+M");
    }

    // -- update() server navigation tests ----------------------------------

    #[test]
    fn update_right_arrow_navigates_servers() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(right_key()), &mut ctx);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::MarkDirty));
    }

    #[test]
    fn update_char_r_toggles_resources() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(char_key('r')), &mut ctx);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::MarkDirty));
    }
}
