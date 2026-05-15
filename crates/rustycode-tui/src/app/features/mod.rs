//! TUI Feature trait, contexts, event/action types, and feature registry.
//!
//! This module defines the contract for feature modules in the decomposed TUI
//! architecture. Each feature implements [`TuiFeature`] and operates through
//! narrow context types ([`UpdateCtx`], [`RenderCtx`]) that borrow from the
//! host shell rather than owning data.
//!
//! ## Design Principles
//!
//! - **GUARDRAIL-ASYNC-1**: `TuiEvent` has SEPARATE `StreamChunk` and `ServiceEvent`
//!   variants — never merged into one.
//! - No new event channel — `TuiEvent` wraps existing channel types.
//! - `UpdateCtx`/`RenderCtx` borrow from the host, never own data.
//! - No `&mut AppShell` or `&mut TUI` in any feature module.

// ── Feature implementations ──────────────────────────────────────────────
pub mod plugin_manager;
pub mod session_streaming;

use crate::app::async_::StreamChunk;
use ratatui::Frame;
use rustycode_protocol::EventMsg;
use std::collections::HashMap;

// ── ID Types (newtype wrappers) ──────────────────────────────────────────────

/// Unique identifier for a rendering surface (e.g., "chat", "sidebar", "tool-panel").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(&'static str);

impl SurfaceId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Unique identifier for a navigation route (e.g., "chat", "settings", "tasks").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RouteId(&'static str);

impl RouteId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

/// Unique identifier for a modal dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModalId(&'static str);

impl ModalId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

// ── Event Types ──────────────────────────────────────────────────────────────

/// Events dispatched to features via [`TuiFeature::update`].
///
/// GUARDRAIL-ASYNC-1: `StreamChunk` and `ServiceEvent` are SEPARATE variants.
/// They must never be merged into a single variant.
#[derive(Debug)]
pub enum TuiEvent {
    /// LLM stream chunk (text, thinking, tool, done, error, etc.)
    Stream(StreamChunk),
    /// Unified event message from protocol layer
    Service(EventMsg),
    /// Terminal key event
    Key(crossterm::event::KeyEvent),
    /// Terminal resize event
    Resize { width: u16, height: u16 },
    /// Focus gained for this feature
    FocusGained,
    /// Focus lost from this feature
    FocusLost,
    /// Tick for periodic updates (animation, status polling)
    Tick,
}

// ── Action Types ─────────────────────────────────────────────────────────────

/// Actions returned by [`TuiFeature::update`] for the host shell to process.
#[derive(Debug, Clone)]
pub enum TuiAction {
    /// Navigate to a different route
    Navigate(RouteId),
    /// Request focus for this feature's surface
    RequestFocus(SurfaceId),
    /// Open a modal dialog
    OpenModal(ModalId),
    /// Close the current modal
    CloseModal,
    /// Status message to display (e.g., toast)
    StatusMessage(String),
    /// Mark the display as needing a redraw
    MarkDirty,
}

// ── Context Types ────────────────────────────────────────────────────────────

/// Narrow mutable context provided to [`TuiFeature::update`].
///
/// Provides access to shared services, command dispatch, focus query,
/// tool approval, and route navigation — without exposing the full TUI/AppShell.
/// Maximum 10 fields to keep the surface narrow and auditable.
pub struct UpdateCtx<'a> {
    /// Whether the feature currently has focus
    pub has_focus: bool,
    /// Currently focused surface, if any
    pub focused_surface: Option<SurfaceId>,
    /// Whether a stream is active
    pub is_streaming: bool,
    /// Number of pending tool executions
    pub pending_tools: usize,
    /// Whether the session is in plan mode
    pub plan_mode_active: bool,
    /// Whether auto-continue is enabled
    pub auto_continue_enabled: bool,
    /// Theme colors for consistent styling
    pub theme_colors: &'a crate::theme::ThemeColors,
    /// Route navigation callback — call with RouteId to navigate
    pub navigate: &'a mut dyn FnMut(RouteId),
    /// Command dispatch callback — call with command string to execute
    pub dispatch_command: &'a mut dyn FnMut(&str),
    /// Tool approval callback — call (tool_id, approved) to respond
    pub approve_tool: &'a mut dyn FnMut(String, bool),
}

/// Narrow immutable context provided to [`TuiFeature::render`].
///
/// Provides frame area allocation, theme/styles access, and read-only focus state.
pub struct RenderCtx<'a> {
    /// Total frame area available for rendering
    pub frame_area: ratatui::layout::Rect,
    /// Currently focused surface (read-only)
    pub focused_surface: Option<SurfaceId>,
    /// Theme colors for consistent styling
    pub theme_colors: &'a crate::theme::ThemeColors,
}

// ── Feature Trait ────────────────────────────────────────────────────────────

/// Core trait for TUI feature modules.
///
/// Each feature (chat, sidebar, tool panel, etc.) implements this trait
/// and is registered with the [`FeatureRegistry`]. The host shell calls
/// `update()` for events and `render()` for drawing.
///
/// ## Lifecycle
///
/// 1. Feature is constructed with its own state
/// 2. `register()` is called once to populate the registry
/// 3. `update()` is called for each relevant event
/// 4. `render()` is called each frame for each assigned surface
pub trait TuiFeature: Send + Sync + 'static {
    /// Unique identifier for this feature (e.g., "chat", "sidebar").
    fn id(&self) -> &'static str;

    /// Register routes, commands, keymaps, and surfaces with the registry.
    /// Called once during initialization.
    fn register(&self, reg: &mut FeatureRegistry);

    /// Handle an event and return actions for the host shell.
    /// Returns a vec of actions (may be empty).
    fn update(&mut self, event: &TuiEvent, ctx: &mut UpdateCtx) -> Vec<TuiAction>;

    /// Render the feature onto the given surface within the provided frame.
    fn render(&self, surface: SurfaceId, frame: &mut Frame, ctx: &RenderCtx);
}

// ── Feature Registry ─────────────────────────────────────────────────────────

/// Registry for routes, commands, keymaps, and surfaces declared by features.
///
/// Populated by each feature's [`TuiFeature::register`] call during initialization.
#[derive(Debug)]
pub struct FeatureRegistry {
    /// Maps route IDs to the feature that owns them
    routes: HashMap<RouteId, &'static str>,
    /// Maps slash command names to the feature that handles them
    commands: HashMap<&'static str, &'static str>,
    /// Maps keyboard shortcuts to actions (feature_id, action_name)
    keymaps: HashMap<String, (&'static str, &'static str)>,
    /// Maps surface IDs to the feature that renders them
    surfaces: HashMap<SurfaceId, &'static str>,
}

impl FeatureRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            commands: HashMap::new(),
            keymaps: HashMap::new(),
            surfaces: HashMap::new(),
        }
    }

    /// Register a route owned by a feature.
    pub fn register_route(&mut self, route: RouteId, feature_id: &'static str) {
        self.routes.insert(route, feature_id);
    }

    /// Register a slash command owned by a feature.
    pub fn register_command(&mut self, name: &'static str, feature_id: &'static str) {
        self.commands.insert(name, feature_id);
    }

    /// Register a keyboard shortcut mapping.
    pub fn register_keymap(&mut self, key: String, feature_id: &'static str, action: &'static str) {
        self.keymaps.insert(key, (feature_id, action));
    }

    /// Register a surface owned by a feature.
    pub fn register_surface(&mut self, surface: SurfaceId, feature_id: &'static str) {
        self.surfaces.insert(surface, feature_id);
    }

    /// Look up which feature owns a route.
    pub fn route_feature(&self, route: RouteId) -> Option<&'static str> {
        self.routes.get(&route).copied()
    }

    /// Look up which feature owns a command.
    pub fn command_feature(&self, name: &str) -> Option<&'static str> {
        self.commands.get(name).copied()
    }

    /// Look up which feature owns a surface.
    pub fn surface_feature(&self, surface: SurfaceId) -> Option<&'static str> {
        self.surfaces.get(&surface).copied()
    }

    /// Get all registered route IDs.
    pub fn routes(&self) -> impl Iterator<Item = RouteId> + '_ {
        self.routes.keys().copied()
    }

    /// Get all registered surface IDs.
    pub fn surfaces(&self) -> impl Iterator<Item = SurfaceId> + '_ {
        self.surfaces.keys().copied()
    }

    /// Get all registered command names.
    pub fn commands(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.commands.keys().copied()
    }
}

impl Default for FeatureRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Unit Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ID type tests ─────────────────────────────────────────────────────

    #[test]
    fn surface_id_newtype_wraps_str() {
        let id = SurfaceId::new("chat");
        assert_eq!(id.as_str(), "chat");
    }

    #[test]
    fn route_id_newtype_wraps_str() {
        let id = RouteId::new("settings");
        assert_eq!(id.as_str(), "settings");
    }

    #[test]
    fn modal_id_newtype_wraps_str() {
        let id = ModalId::new("help");
        assert_eq!(id.as_str(), "help");
    }

    #[test]
    fn surface_id_equality() {
        assert_eq!(SurfaceId::new("chat"), SurfaceId::new("chat"));
        assert_ne!(SurfaceId::new("chat"), SurfaceId::new("sidebar"));
    }

    #[test]
    fn route_id_hashable() {
        let mut map = HashMap::new();
        map.insert(RouteId::new("home"), "chat_feature");
        assert_eq!(map.get(&RouteId::new("home")), Some(&"chat_feature"));
    }

    // ── TuiEvent tests ────────────────────────────────────────────────────

    #[test]
    fn tui_event_stream_variant_exists() {
        let _event = TuiEvent::Stream(StreamChunk::Text("hello".to_string()));
    }

    #[test]
    fn tui_event_service_variant_exists() {
        let _event = TuiEvent::Service(EventMsg::Done);
    }

    #[test]
    fn tui_event_key_variant_exists() {
        let _event = TuiEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ));
    }

    #[test]
    fn tui_event_resize_variant_exists() {
        let _event = TuiEvent::Resize {
            width: 80,
            height: 24,
        };
    }

    #[test]
    fn tui_event_focus_variants_exist() {
        // These don't carry data but must exist as variants
        let _gained = TuiEvent::FocusGained;
        let _lost = TuiEvent::FocusLost;
    }

    #[test]
    fn tui_event_tick_variant_exists() {
        let _event = TuiEvent::Tick;
    }

    // ── TuiAction tests ───────────────────────────────────────────────────

    #[test]
    fn tui_action_navigate() {
        let action = TuiAction::Navigate(RouteId::new("chat"));
        assert!(matches!(action, TuiAction::Navigate(_)));
    }

    #[test]
    fn tui_action_request_focus() {
        let action = TuiAction::RequestFocus(SurfaceId::new("input"));
        assert!(matches!(action, TuiAction::RequestFocus(_)));
    }

    #[test]
    fn tui_action_open_close_modal() {
        let open = TuiAction::OpenModal(ModalId::new("help"));
        let close = TuiAction::CloseModal;
        assert!(matches!(open, TuiAction::OpenModal(_)));
        assert!(matches!(close, TuiAction::CloseModal));
    }

    #[test]
    fn tui_action_status_message() {
        let action = TuiAction::StatusMessage("Saved".to_string());
        if let TuiAction::StatusMessage(msg) = action {
            assert_eq!(msg, "Saved");
        } else {
            panic!("Expected StatusMessage");
        }
    }

    #[test]
    fn tui_action_mark_dirty() {
        assert!(matches!(TuiAction::MarkDirty, TuiAction::MarkDirty));
    }

    // ── FeatureRegistry tests ─────────────────────────────────────────────

    #[test]
    fn registry_new_is_empty() {
        let reg = FeatureRegistry::new();
        assert_eq!(reg.routes().count(), 0);
        assert_eq!(reg.surfaces().count(), 0);
        assert_eq!(reg.commands().count(), 0);
    }

    #[test]
    fn registry_default_is_empty() {
        let reg = FeatureRegistry::default();
        assert_eq!(reg.routes().count(), 0);
    }

    #[test]
    fn registry_register_and_lookup_route() {
        let mut reg = FeatureRegistry::new();
        reg.register_route(RouteId::new("chat"), "chat_feature");
        assert_eq!(
            reg.route_feature(RouteId::new("chat")),
            Some("chat_feature")
        );
        assert_eq!(reg.route_feature(RouteId::new("missing")), None);
    }

    #[test]
    fn registry_register_and_lookup_command() {
        let mut reg = FeatureRegistry::new();
        reg.register_command("/help", "help_feature");
        assert_eq!(reg.command_feature("/help"), Some("help_feature"));
        assert_eq!(reg.command_feature("/unknown"), None);
    }

    #[test]
    fn registry_register_and_lookup_surface() {
        let mut reg = FeatureRegistry::new();
        reg.register_surface(SurfaceId::new("messages"), "chat_feature");
        assert_eq!(
            reg.surface_feature(SurfaceId::new("messages")),
            Some("chat_feature")
        );
    }

    #[test]
    fn registry_register_keymap() {
        let mut reg = FeatureRegistry::new();
        reg.register_keymap("Ctrl+S".to_string(), "save_feature", "save");
        assert!(reg.keymaps.contains_key("Ctrl+S"));
    }

    #[test]
    fn registry_overwrites_duplicate_route() {
        let mut reg = FeatureRegistry::new();
        reg.register_route(RouteId::new("chat"), "old_feature");
        reg.register_route(RouteId::new("chat"), "new_feature");
        assert_eq!(reg.route_feature(RouteId::new("chat")), Some("new_feature"));
    }

    #[test]
    fn registry_iterates_routes() {
        let mut reg = FeatureRegistry::new();
        reg.register_route(RouteId::new("chat"), "f1");
        reg.register_route(RouteId::new("tasks"), "f2");
        let routes: Vec<_> = reg.routes().collect();
        assert_eq!(routes.len(), 2);
    }

    #[test]
    fn registry_iterates_surfaces() {
        let mut reg = FeatureRegistry::new();
        reg.register_surface(SurfaceId::new("s1"), "f1");
        reg.register_surface(SurfaceId::new("s2"), "f2");
        let surfaces: Vec<_> = reg.surfaces().collect();
        assert_eq!(surfaces.len(), 2);
    }

    #[test]
    fn registry_iterates_commands() {
        let mut reg = FeatureRegistry::new();
        reg.register_command("/a", "f1");
        reg.register_command("/b", "f2");
        let cmds: Vec<_> = reg.commands().collect();
        assert_eq!(cmds.len(), 2);
    }
}
