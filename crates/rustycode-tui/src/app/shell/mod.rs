//! AppShell — thin host for the decomposed TUI feature architecture.
//!
//! Owns focus routing, frame budget, and feature registry.
//! Runs alongside existing `TUI` god struct (dual-path).
//!
//! ## Design Decisions (from Metis review)
//!
//! - **NO terminal ownership** — terminal lifecycle stays in `lib.rs` entry point.
//! - **NO event_rx ownership** — events are passed in via `handle_event()` method.
//! - **features HashMap** stores actual feature instances (registry stores routing metadata).
//! - **handle_event() pattern** — receives events, routes to features, returns actions.
//!
//! ## Guardrails
//!
//! - Features NEVER receive `&mut AppShell` (GUARDRAIL-AB-1).
//! - Event routing: input events → focused feature only; service/stream/tick → all features.

pub mod focus;

use crate::app::features::{
    FeatureRegistry, ModalId, RenderCtx, RouteId, SurfaceId, TuiAction, TuiEvent, TuiFeature,
    UpdateCtx,
};
use crate::app::shell::focus::FocusRing;
use crate::theme::{Theme, ThemeColors};
use ratatui::Frame;
use std::collections::HashMap;
use std::sync::Arc;

/// Thin host shell for feature-based TUI architecture.
///
/// Features NEVER receive `&mut AppShell` (GUARDRAIL-AB-1).
pub struct AppShell {
    /// Feature registry: routes, commands, surfaces, keymaps.
    registry: FeatureRegistry,
    /// Ordered focus ring for surface-based input routing.
    focus: FocusRing,
    /// Shared theme reference for rendering.
    theme: Arc<Theme>,
    /// Cached resolved theme colors (derived from `theme`).
    theme_colors: ThemeColors,
    /// Feature instances keyed by their ID.
    features: HashMap<&'static str, Box<dyn TuiFeature>>,
    /// Active modal, if any.
    active_modal: Option<&'static str>,
    /// Current navigation route.
    current_route: RouteId,
    /// Whether the shell should exit on the next loop iteration.
    should_exit: bool,
}

impl AppShell {
    /// Create a new empty shell with the given theme.
    pub fn new(theme: Arc<Theme>) -> Self {
        let theme_colors = ThemeColors::from(theme.as_ref());
        Self {
            registry: FeatureRegistry::new(),
            focus: FocusRing::new(),
            theme,
            theme_colors,
            features: HashMap::new(),
            active_modal: None,
            current_route: RouteId::new("home"),
            should_exit: false,
        }
    }

    /// Register a feature with the shell.
    ///
    /// Calls `feature.register()` to populate the registry with routes,
    /// surfaces, commands, and keymaps. Adds the feature's surfaces to the
    /// focus ring.
    pub fn register_feature(&mut self, feature: Box<dyn TuiFeature>) {
        let id = feature.id();
        feature.register(&mut self.registry);

        // Add surfaces declared by this feature to the focus ring.
        for surface in self.registry.surfaces() {
            if self.registry.surface_feature(surface) == Some(id) {
                self.focus.add(surface);
            }
        }

        self.features.insert(id, feature);
    }

    /// Handle an incoming event by routing it to the appropriate feature(s).
    ///
    /// Input events (Key, FocusGained, FocusLost, Resize) are sent only to the
    /// focused feature. Service/Stream/Tick events are broadcast to all features.
    ///
    /// Returns a list of actions for the host shell to process.
    pub fn handle_event(&mut self, event: TuiEvent) -> Vec<TuiAction> {
        let focused_surface = self.focus.focused();
        let theme_colors = &self.theme_colors;

        match &event {
            TuiEvent::Key(_)
            | TuiEvent::FocusGained
            | TuiEvent::FocusLost
            | TuiEvent::Resize { .. } => {
                // Route input events only to the focused feature.
                let feature_id = focused_surface.and_then(|s| self.registry.surface_feature(s));
                if let Some(fid) = feature_id {
                    if let Some(feature) = self.features.get_mut(fid) {
                        let mut nav_fn = |_: RouteId| {};
                        let mut cmd_fn = |_: &str| {};
                        let mut approve_fn = |_: String, _: bool| {};
                        let mut ctx = UpdateCtx {
                            has_focus: focused_surface.is_some(),
                            focused_surface,
                            is_streaming: false,
                            pending_tools: 0,
                            plan_mode_active: false,
                            auto_continue_enabled: false,
                            theme_colors,
                            navigate: &mut nav_fn,
                            dispatch_command: &mut cmd_fn,
                            approve_tool: &mut approve_fn,
                        };
                        return feature.update(&event, &mut ctx);
                    }
                }
                Vec::new()
            }
            TuiEvent::Stream(_) | TuiEvent::Service(_) | TuiEvent::Tick => {
                // Broadcast to all features.
                let mut all_actions = Vec::new();
                for feature in self.features.values_mut() {
                    let mut nav_fn = |_: RouteId| {};
                    let mut cmd_fn = |_: &str| {};
                    let mut approve_fn = |_: String, _: bool| {};
                    let mut ctx = UpdateCtx {
                        has_focus: focused_surface.is_some(),
                        focused_surface,
                        is_streaming: false,
                        pending_tools: 0,
                        plan_mode_active: false,
                        auto_continue_enabled: false,
                        theme_colors,
                        navigate: &mut nav_fn,
                        dispatch_command: &mut cmd_fn,
                        approve_tool: &mut approve_fn,
                    };
                    let actions = feature.update(&event, &mut ctx);
                    all_actions.extend(actions);
                }
                all_actions
            }
        }
    }

    /// Render all features onto the given frame.
    pub fn render_frame(&self, frame: &mut Frame) {
        let focused_surface = self.focus.focused();
        let frame_area = frame.area();

        for (feature_id, feature) in &self.features {
            // Render each surface owned by this feature.
            for surface in self.registry.surfaces() {
                if self.registry.surface_feature(surface) == Some(feature_id) {
                    let ctx = RenderCtx {
                        frame_area,
                        focused_surface,
                        theme_colors: &self.theme_colors,
                    };
                    feature.render(surface, frame, &ctx);
                }
            }
        }
    }

    /// Process actions returned by features.
    ///
    /// Handles navigation, focus changes, modal open/close, status messages,
    /// and dirty marking.
    pub fn process_actions(&mut self, actions: Vec<TuiAction>) {
        for action in actions {
            match action {
                TuiAction::Navigate(route) => {
                    self.current_route = route;
                }
                TuiAction::RequestFocus(surface) => {
                    self.focus.focus_set(surface);
                }
                TuiAction::OpenModal(modal_id) => {
                    self.active_modal = Some(modal_id.as_str());
                }
                TuiAction::CloseModal => {
                    self.active_modal = None;
                }
                TuiAction::MarkDirty | TuiAction::StatusMessage(_) => {
                    // Host shell handles these externally (redraw trigger, toast display).
                    // No internal state change needed in AppShell itself.
                }
            }
        }
    }

    /// Get a reference to the feature registry.
    pub fn registry(&self) -> &FeatureRegistry {
        &self.registry
    }

    /// Get a reference to the focus ring.
    pub fn focus(&self) -> &FocusRing {
        &self.focus
    }

    /// Get a mutable reference to the focus ring (for tests).
    pub fn focus_mut(&mut self) -> &mut FocusRing {
        &mut self.focus
    }

    /// Get the current navigation route.
    pub fn current_route(&self) -> RouteId {
        self.current_route
    }

    /// Check whether the shell should exit.
    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    /// Request the shell to exit on the next loop iteration.
    pub fn request_exit(&mut self) {
        self.should_exit = true;
    }

    /// Get the active modal ID, if any.
    pub fn active_modal(&self) -> Option<&'static str> {
        self.active_modal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::features::SurfaceId;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct DummyFeature {
        id: &'static str,
        surface: SurfaceId,
        route: RouteId,
        update_count: AtomicUsize,
        actions_on_update: Vec<TuiAction>,
    }

    impl DummyFeature {
        fn new(id: &'static str, surface: SurfaceId, route: RouteId) -> Self {
            Self {
                id,
                surface,
                route,
                update_count: AtomicUsize::new(0),
                actions_on_update: Vec::new(),
            }
        }

        fn with_actions(
            id: &'static str,
            surface: SurfaceId,
            route: RouteId,
            actions: Vec<TuiAction>,
        ) -> Self {
            Self {
                id,
                surface,
                route,
                update_count: AtomicUsize::new(0),
                actions_on_update: actions,
            }
        }
    }

    impl TuiFeature for DummyFeature {
        fn id(&self) -> &'static str {
            self.id
        }

        fn register(&self, reg: &mut FeatureRegistry) {
            reg.register_surface(self.surface, self.id);
            reg.register_route(self.route, self.id);
        }

        fn update(&mut self, _event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
            self.update_count.fetch_add(1, Ordering::SeqCst);
            self.actions_on_update.clone()
        }

        fn render(&self, _surface: SurfaceId, _frame: &mut Frame, _ctx: &RenderCtx) {}
    }

    /// Helper to create a default theme Arc.
    fn default_theme() -> Arc<Theme> {
        Arc::new(Theme::default())
    }

    // ── Core lifecycle tests ──────────────────────────────────────────

    #[test]
    fn app_shell_new_is_empty() {
        let shell = AppShell::new(default_theme());
        assert_eq!(shell.registry().routes().count(), 0);
        assert_eq!(shell.registry().surfaces().count(), 0);
        assert!(shell.focus().is_empty());
        assert_eq!(shell.current_route(), RouteId::new("home"));
        assert!(!shell.should_exit());
        assert!(shell.active_modal().is_none());
    }

    #[test]
    fn register_feature_populates_registry() {
        let mut shell = AppShell::new(default_theme());
        let feature = DummyFeature::new("chat", SurfaceId::new("chat_view"), RouteId::new("chat"));
        shell.register_feature(Box::new(feature));

        assert_eq!(shell.registry().surfaces().count(), 1);
        assert_eq!(shell.registry().routes().count(), 1);
        assert_eq!(
            shell
                .registry()
                .surface_feature(SurfaceId::new("chat_view")),
            Some("chat")
        );
        assert_eq!(
            shell.registry().route_feature(RouteId::new("chat")),
            Some("chat")
        );
        assert_eq!(shell.focus().len(), 1);
        assert_eq!(shell.focus().focused(), Some(SurfaceId::new("chat_view")));
    }

    #[test]
    fn handle_event_routes_to_focused_feature() {
        let mut shell = AppShell::new(default_theme());

        // Register two features with distinct marker actions.
        let f1 = Box::new(DummyFeature::with_actions(
            "a",
            SurfaceId::new("sa"),
            RouteId::new("ra"),
            vec![TuiAction::StatusMessage("from_a".into())],
        ));
        let f2 = Box::new(DummyFeature::with_actions(
            "b",
            SurfaceId::new("sb"),
            RouteId::new("rb"),
            vec![TuiAction::StatusMessage("from_b".into())],
        ));
        shell.register_feature(f1);
        shell.register_feature(f2);

        // Focus feature "a".
        shell.focus_mut().focus_set(SurfaceId::new("sa"));

        // Send a key event — only the focused feature should receive it.
        let key_event = TuiEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('x'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let actions = shell.handle_event(key_event);

        // Only feature "a" should have responded.
        let messages: Vec<&str> = actions
            .iter()
            .filter_map(|a| match a {
                TuiAction::StatusMessage(msg) => Some(msg.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(messages, vec!["from_a"]);
    }

    #[test]
    fn navigate_action_changes_route() {
        let mut shell = AppShell::new(default_theme());
        assert_eq!(shell.current_route(), RouteId::new("home"));

        shell.process_actions(vec![TuiAction::Navigate(RouteId::new("settings"))]);
        assert_eq!(shell.current_route(), RouteId::new("settings"));
    }

    #[test]
    fn request_focus_action_changes_focus() {
        let mut shell = AppShell::new(default_theme());

        // Register two features with surfaces.
        let f1 = DummyFeature::new("a", SurfaceId::new("sa"), RouteId::new("ra"));
        let f2 = DummyFeature::new("b", SurfaceId::new("sb"), RouteId::new("rb"));
        shell.register_feature(Box::new(f1));
        shell.register_feature(Box::new(f2));

        // First surface should be auto-focused.
        assert_eq!(shell.focus().focused(), Some(SurfaceId::new("sa")));

        // Request focus to second surface.
        shell.process_actions(vec![TuiAction::RequestFocus(SurfaceId::new("sb"))]);
        assert_eq!(shell.focus().focused(), Some(SurfaceId::new("sb")));
    }

    #[test]
    fn request_exit_sets_flag() {
        let mut shell = AppShell::new(default_theme());
        assert!(!shell.should_exit());
        shell.request_exit();
        assert!(shell.should_exit());
    }

    #[test]
    fn open_close_modal_updates_state() {
        let mut shell = AppShell::new(default_theme());
        assert!(shell.active_modal().is_none());

        shell.process_actions(vec![TuiAction::OpenModal(ModalId::new("help"))]);
        assert_eq!(shell.active_modal(), Some("help"));

        shell.process_actions(vec![TuiAction::CloseModal]);
        assert!(shell.active_modal().is_none());
    }

    #[test]
    fn broadcast_events_reach_all_features() {
        let mut shell = AppShell::new(default_theme());

        // Register two features with distinct marker actions.
        let f1 = Box::new(DummyFeature::with_actions(
            "a",
            SurfaceId::new("sa"),
            RouteId::new("ra"),
            vec![TuiAction::StatusMessage("from_a".into())],
        ));
        let f2 = Box::new(DummyFeature::with_actions(
            "b",
            SurfaceId::new("sb"),
            RouteId::new("rb"),
            vec![TuiAction::StatusMessage("from_b".into())],
        ));
        shell.register_feature(f1);
        shell.register_feature(f2);

        // Tick is a broadcast event — both features should be called.
        let actions = shell.handle_event(TuiEvent::Tick);

        // Both features should have returned their marker actions.
        let messages: Vec<&str> = actions
            .iter()
            .filter_map(|a| match a {
                TuiAction::StatusMessage(msg) => Some(msg.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            messages.contains(&"from_a"),
            "feature 'a' should have been called"
        );
        assert!(
            messages.contains(&"from_b"),
            "feature 'b' should have been called"
        );
    }

    // ── Plugin Manager integration tests ──────────────────────────────

    #[test]
    fn plugin_manager_feature_can_be_registered() {
        use crate::app::features::plugin_manager::PluginManagerState;
        use std::sync::{Arc, RwLock};

        let mut shell = AppShell::new(default_theme());

        let manager = Arc::new(RwLock::new(crate::plugin::PluginManager::default()));
        let feature = PluginManagerState::new(manager);

        shell.register_feature(Box::new(feature));

        // Plugin Manager should be registered with its surface.
        assert_eq!(
            shell
                .registry()
                .surface_feature(SurfaceId::new("plugin-manager")),
            Some("plugin-manager")
        );
    }

    #[test]
    fn plugin_manager_escape_key_hides_manager() {
        use crate::app::features::plugin_manager::PluginManagerState;
        use crate::theme::ThemeColors;
        use ratatui::style::Color;
        use std::sync::{Arc, RwLock};

        let manager = Arc::new(RwLock::new(crate::plugin::PluginManager::default()));
        let mut feature = PluginManagerState::new(manager);

        // Show the plugin manager.
        feature.show();
        assert!(feature.is_visible());

        // Create a dummy theme for context.
        let theme_colors = ThemeColors {
            background: Color::Black,
            foreground: Color::White,
            primary: Color::Blue,
            secondary: Color::Cyan,
            accent: Color::Yellow,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
        };

        // Escape key should be handled (in TuiFeature::update).
        let mut ctx = UpdateCtx {
            has_focus: true,
            focused_surface: Some(SurfaceId::new("plugin-manager")),
            is_streaming: false,
            pending_tools: 0,
            plan_mode_active: false,
            auto_continue_enabled: false,
            theme_colors: &theme_colors,
            navigate: &mut |_| {},
            dispatch_command: &mut |_| {},
            approve_tool: &mut |_, _| {},
        };

        let escape_event = TuiEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));

        let actions = feature.update(&escape_event, &mut ctx);

        // Should return MarkDirty action (and hide internally).
        assert!(!feature.is_visible());
        assert!(actions.iter().any(|a| matches!(a, TuiAction::MarkDirty)));
    }

    #[test]
    fn plugin_manager_multiple_features_coexist() {
        use crate::app::features::plugin_manager::PluginManagerState;
        use std::sync::{Arc, RwLock};

        let mut shell = AppShell::new(default_theme());

        // Register a dummy feature.
        let dummy = DummyFeature::new("chat", SurfaceId::new("chat"), RouteId::new("chat"));
        shell.register_feature(Box::new(dummy));

        // Register Plugin Manager feature.
        let manager = Arc::new(RwLock::new(crate::plugin::PluginManager::default()));
        let plugin_feature = PluginManagerState::new(manager);
        shell.register_feature(Box::new(plugin_feature));

        // Both should be registered.
        assert_eq!(shell.registry().surfaces().count(), 2);
        assert_eq!(shell.registry().routes().count(), 1);
        assert_eq!(shell.focus().len(), 2);
    }

    #[test]
    fn plugin_manager_surfaces_separated_from_main_features() {
        use crate::app::features::plugin_manager::PluginManagerState;
        use std::sync::{Arc, RwLock};

        let mut shell = AppShell::new(default_theme());

        let manager = Arc::new(RwLock::new(crate::plugin::PluginManager::default()));
        let feature = PluginManagerState::new(manager);

        shell.register_feature(Box::new(feature));

        // Plugin Manager surface should be distinct from other surfaces.
        let all_surfaces: Vec<SurfaceId> = shell.registry().surfaces().collect();
        assert!(all_surfaces.contains(&SurfaceId::new("plugin-manager")));
        assert_eq!(all_surfaces.len(), 1);
    }
}
