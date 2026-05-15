//! Worker Panel feature module.
//!
//! Self-contained feature for displaying worker (sub-agent) status.
//! Implements the [`TuiFeature`] trait and owns all worker panel state.
//!
//! ## State
//! - [`WorkerPanelFeatureState`]: Wraps [`WorkerPanel`] and cached worker list.
//!
//! ## Events Handled
//! - `TuiEvent::Key`: Navigation when visible.
//! - Other events (Tick, Stream, Service) are ignored.
//!
//! ## Surfaces
//! - `"worker"`: Main worker panel overlay.
//!
//! ## Routes
//! - `"worker"`: Navigation route to open the worker panel.
//!
//! ## Rendering
//! Delegates to [`WorkerPanel`] rendering when visible.

use crate::app::features::{
    FeatureRegistry, ModalId, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature,
    UpdateCtx,
};
use crate::ui::worker_panel::WorkerPanel;
use crossterm::event::KeyEvent;
use ratatui::prelude::Widget;
use ratatui::Frame;
use rustycode_orchestration::worker_registry::{Worker, WorkerEvent};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Worker Panel feature state.
///
/// Holds all worker-related data needed by the feature. The `panel` field
/// is wrapped in [`Mutex`] because [`WorkerPanel`] contains interior-mutable
/// types which are `Send` but not `Sync`. [`TuiFeature`] requires `Send + Sync`.
pub struct WorkerPanelFeatureState {
    /// Worker panel UI state (thread-safe wrapped for `Sync`).
    pub panel: Mutex<WorkerPanel>,
    /// Whether the worker panel overlay is currently visible.
    pub visible: bool,
    /// Index of the currently selected worker in the list.
    pub selected_worker: usize,
    /// Cached worker list for navigation.
    pub worker_list: Mutex<Vec<Worker>>,
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

/// Worker Panel feature implementing [`TuiFeature`].
///
/// Handles key events for worker navigation and delegates rendering
/// to [`WorkerPanel`].
pub struct WorkerPanelFeature {
    state: WorkerPanelFeatureState,
}

impl WorkerPanelFeature {
    /// Create a new `WorkerPanelFeature`.
    ///
    /// The UI starts hidden.
    pub fn new() -> Self {
        Self {
            state: WorkerPanelFeatureState {
                panel: Mutex::new(WorkerPanel::new()),
                visible: false,
                selected_worker: 0,
                worker_list: Mutex::new(Vec::new()),
            },
        }
    }

    /// Show the worker panel overlay.
    pub fn show(&mut self) {
        self.state.visible = true;
        if let Ok(mut panel) = self.state.panel.lock() {
            panel.visible = true;
        }
    }

    /// Hide the worker panel overlay.
    pub fn hide(&mut self) {
        self.state.visible = false;
        if let Ok(mut panel) = self.state.panel.lock() {
            panel.visible = false;
        }
    }

    /// Check if the worker panel overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.state.visible
    }

    /// Surface ID used by this feature.
    const SURFACE: SurfaceId = SurfaceId::new("worker");

    /// Route ID for navigating to the worker panel.
    const ROUTE: RouteId = RouteId::new("worker");

    /// Modal ID for the worker panel overlay.
    const MODAL: ModalId = ModalId::new("worker");

    /// Slash command to open the worker panel.
    const CMD_OPEN: &str = "/worker";

    /// Slash command to close the worker panel.
    const CMD_CLOSE: &str = "/worker close";

    /// Keyboard shortcut to toggle the worker panel (Ctrl+W).
    const KEYMAP_TOGGLE: &str = "Ctrl+W";
}

impl Default for WorkerPanelFeature {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiFeature for WorkerPanelFeature {
    fn id(&self) -> &'static str {
        "worker_panel"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        reg.register_surface(Self::SURFACE, self.id());
        reg.register_route(Self::ROUTE, self.id());
        reg.register_command(Self::CMD_OPEN, self.id());
        reg.register_command(Self::CMD_CLOSE, self.id());
        reg.register_keymap(
            Self::KEYMAP_TOGGLE.to_string(),
            self.id(),
            "toggle_worker_panel",
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

        let panel = match self.state.panel.lock() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::warn!("worker panel lock poisoned: {e}");
                e.into_inner()
            }
        };

        let panel_clone = panel.clone();
        drop(panel);
        frame.render_widget(panel_clone, ctx.frame_area);
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Public API: command handling
// ---------------------------------------------------------------------------

impl WorkerPanelFeature {
    /// Handle a slash command dispatched to this feature.
    ///
    /// Recognized commands:
    /// - `"/worker"` — shows the worker panel overlay
    /// - `"/worker close"` — hides the worker panel overlay
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

    /// Toggle worker panel visibility.
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

impl WorkerPanelFeature {
    fn handle_key_event(&mut self, key: KeyEvent) -> Vec<TuiAction> {
        use crossterm::event::{KeyCode, KeyModifiers};

        if !self.state.visible {
            return Vec::new();
        }

        match key {
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Ok(mut panel) = self.state.panel.lock() {
                    panel.visible = false;
                }
                self.state.visible = false;
                vec![TuiAction::CloseModal]
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                let worker_count = self.state.worker_list.lock().map(|w| w.len()).unwrap_or(0);
                self.state.selected_worker =
                    (self.state.selected_worker + 1).min(worker_count.saturating_sub(1));
                vec![TuiAction::MarkDirty]
            }
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.state.selected_worker = self.state.selected_worker.saturating_sub(1);
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

    fn make_feature() -> WorkerPanelFeature {
        WorkerPanelFeature::new()
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

    fn sample_worker(name: &str) -> Worker {
        use rustycode_orchestration::worker_registry::WorkerStatus;
        Worker {
            worker_id: name.to_string(),
            status: WorkerStatus::Running,
            cwd: "/tmp".to_string(),
            task_id: Some(format!("task-{name}")),
            task_description: Some(format!("task for {name}")),
            trust_gate_cleared: true,
            last_error: None,
            result_summary: None,
            created_at: 0,
            updated_at: 0,
            events: vec![],
        }
    }

    #[test]
    fn new_creates_hidden_state() {
        let feature = make_feature();
        assert_eq!(feature.id(), "worker_panel");
        assert!(!feature.state.visible);
        assert_eq!(feature.state.selected_worker, 0);
    }

    #[test]
    fn default_creates_hidden_state() {
        let feature = WorkerPanelFeature::default();
        assert!(!feature.is_visible());
    }

    #[test]
    fn register_registers_surface_and_route() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(
            reg.surface_feature(SurfaceId::new("worker")),
            Some("worker_panel")
        );
        assert_eq!(
            reg.route_feature(RouteId::new("worker")),
            Some("worker_panel")
        );
    }

    #[test]
    fn register_registers_slash_commands() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert_eq!(reg.command_feature("/worker"), Some("worker_panel"));
        assert_eq!(reg.command_feature("/worker close"), Some("worker_panel"));
    }

    #[test]
    fn register_registers_keymap() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        let (feature_id, action) = reg
            .keymap_feature("Ctrl+W")
            .expect("keymap should be registered");
        assert_eq!(feature_id, "worker_panel");
        assert_eq!(action, "toggle_worker_panel");
    }

    #[test]
    fn register_registers_everything() {
        let feature = make_feature();
        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        assert!(reg.surface_feature(SurfaceId::new("worker")).is_some());
        assert!(reg.route_feature(RouteId::new("worker")).is_some());
        assert!(reg.command_feature("/worker").is_some());
        assert!(reg.command_feature("/worker close").is_some());
        assert!(reg.keymap_feature("Ctrl+W").is_some());
    }

    #[test]
    fn show_sets_visible() {
        let mut feature = make_feature();
        assert!(!feature.state.visible);
        feature.show();
        assert!(feature.state.visible);
        assert!(feature.state.panel.lock().expect("lock").visible);
    }

    #[test]
    fn hide_clears_visible() {
        let mut feature = make_feature();
        feature.show();
        feature.hide();
        assert!(!feature.state.visible);
        assert!(!feature.state.panel.lock().expect("lock").visible);
    }

    #[test]
    fn is_visible_matches_state() {
        let mut feature = make_feature();
        assert!(!feature.is_visible());
        feature.show();
        assert!(feature.is_visible());
    }

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
    }

    #[test]
    fn update_up_returns_mark_dirty_when_visible() {
        let mut feature = make_feature();
        feature.show();
        feature.state.selected_worker = 2;

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        let actions = feature.update(&TuiEvent::Key(up_key()), &mut ctx);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], TuiAction::MarkDirty));
        assert_eq!(feature.state.selected_worker, 1);
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
    }

    #[test]
    fn render_no_output_when_not_visible() {
        let feature = make_feature();
        let theme = test_theme_colors();
        let ctx = make_render_ctx(&theme);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                feature.render(SurfaceId::new("worker"), frame, &ctx);
            })
            .expect("draw");
    }

    #[test]
    fn render_no_output_for_wrong_surface() {
        let mut feature = make_feature();
        feature.show();

        let theme = test_theme_colors();
        let ctx = make_render_ctx(&theme);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                feature.render(SurfaceId::new("other"), frame, &ctx);
            })
            .expect("draw");
    }

    #[test]
    fn handle_command_open_shows_panel() {
        let mut feature = make_feature();
        let actions = feature.handle_command("/worker");
        assert!(feature.is_visible());
        assert!(matches!(actions[0], TuiAction::OpenModal(_)));
    }

    #[test]
    fn handle_command_close_hides_panel() {
        let mut feature = make_feature();
        feature.show();
        let actions = feature.handle_command("/worker close");
        assert!(!feature.is_visible());
        assert!(matches!(actions[0], TuiAction::CloseModal));
    }

    #[test]
    fn handle_command_unknown_returns_empty() {
        let mut feature = make_feature();
        let actions = feature.handle_command("/unknown");
        assert!(actions.is_empty());
    }

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
        feature.toggle_visibility();
        assert!(feature.is_visible());
        feature.toggle_visibility();
        assert!(!feature.is_visible());
    }

    #[test]
    fn as_any_mut_allows_downcast() {
        let mut feature = make_feature();
        let any_ref = feature.as_any_mut();
        let downcast = any_ref.downcast_mut::<WorkerPanelFeature>();
        assert!(downcast.is_some());
    }

    #[test]
    fn down_key_clamps_to_worker_count() {
        let mut feature = make_feature();
        feature.show();
        feature
            .state
            .worker_list
            .lock()
            .unwrap()
            .push(sample_worker("w1"));
        feature
            .state
            .worker_list
            .lock()
            .unwrap()
            .push(sample_worker("w2"));

        let theme = test_theme_colors();
        let mut nav = |_route: RouteId| {};
        let mut dispatch = |_cmd: &str| {};
        let mut approve = |_id: String, _ok: bool| {};
        let mut ctx = make_update_ctx(&theme, &mut nav, &mut dispatch, &mut approve);

        for _ in 0..20 {
            feature.update(&TuiEvent::Key(down_key()), &mut ctx);
        }
        assert!(feature.state.selected_worker <= 1);
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

        feature.state.selected_worker = 0;
        feature.update(&TuiEvent::Key(up_key()), &mut ctx);
        assert_eq!(feature.state.selected_worker, 0);
    }

    #[test]
    fn worker_list_initially_empty() {
        let feature = make_feature();
        let list = feature.state.worker_list.lock().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn worker_list_can_be_updated() {
        let feature = make_feature();
        {
            let mut list = feature.state.worker_list.lock().unwrap();
            list.push(sample_worker("w1"));
            list.push(sample_worker("w2"));
        }
        let list = feature.state.worker_list.lock().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].worker_id, "w1");
    }
}
