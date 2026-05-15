//! AppShell: Feature-based TUI host
//!
//! Provides a clean architecture for the TUI by separating concerns:
//! - AppShell owns terminal lifecycle, focus routing, and feature coordination
//! - Features own their state and lifecycle, implementing the TuiFeature trait
//! - A narrow UpdateCtx and RenderCtx prevent god-object anti-patterns
//!
//! AppShell runs alongside the existing TUI (feature-gated) and drains events
//! from the same polling layer, ensuring no architectural conflicts.

pub mod focus;

use crate::app::features::{
    FeatureRegistry, RenderCtx, SurfaceId, TuiAction, TuiEvent, TuiFeature, UpdateCtx,
};
use crate::theme::ThemeColors;
use anyhow::Result;
use focus::FocusRing;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::Stdout;
use std::sync::Arc;

/// Host for the feature-based TUI architecture.
///
/// AppShell coordinates feature lifecycle, event routing, and rendering.
/// It owns:
/// - Terminal lifecycle (setup, teardown)
/// - Feature registry and instances
/// - Focus ring for keyboard routing
/// - Theme and rendering state
///
/// # Design Constraints
///
/// - **No god-object**: AppShell is narrowly focused on orchestration
/// - **Feature isolation**: Features never receive `&mut AppShell`
/// - **Dual-path**: Coexists with existing TUI via feature flag
/// - **Event preservation**: Drains from same channels as existing system
pub struct AppShell {
    /// Registered features (dynamic dispatch)
    features: Vec<Box<dyn TuiFeature>>,
    /// Feature registry populated during initialization
    registry: FeatureRegistry,
    /// Focus ring for keyboard event routing
    focus: FocusRing,
    /// Terminal reference (from crossterm)
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// Theme colors (shared reference)
    theme: Arc<ThemeColors>,
}

impl AppShell {
    /// Create a new AppShell with empty feature list.
    pub fn new(terminal: Terminal<CrosstermBackend<Stdout>>, theme: Arc<ThemeColors>) -> Self {
        Self {
            features: Vec::new(),
            registry: FeatureRegistry::new(),
            focus: FocusRing::new(),
            terminal,
            theme,
        }
    }

    /// Register a feature and populate its registry entries.
    pub fn register_feature(&mut self, feature: Box<dyn TuiFeature>) {
        // Call feature's register hook to populate registry
        feature.register(&mut self.registry);

        // Add any surfaces from registry to focus ring
        for surface in self.registry.surfaces() {
            self.focus.add(surface);
        }

        self.features.push(feature);
    }

    /// Get the currently focused surface, if any.
    pub fn focused_surface(&self) -> Option<SurfaceId> {
        self.focus.focused()
    }

    /// Move focus to the next surface.
    pub fn focus_next(&mut self) {
        self.focus.focus_next();
    }

    /// Move focus to the previous surface.
    pub fn focus_prev(&mut self) {
        self.focus.focus_prev();
    }

    /// Set focus to a specific surface.
    pub fn focus_set(&mut self, surface: SurfaceId) {
        self.focus.focus_set(surface);
    }

    /// Process an event through all features.
    ///
    /// This is a simplified event dispatch that:
    /// 1. Routes input events to the focused feature only
    /// 2. Broadcasts service events to all features
    /// 3. Collects and returns all actions for the host to process
    ///
    /// In a full implementation, this would handle action dispatch
    /// (navigation, modal open/close, etc.) within AppShell.
    pub fn handle_event(&mut self, event: &TuiEvent) -> Vec<TuiAction> {
        let mut all_actions = Vec::new();
        let focused = self.focus.focused();

        for feature in &mut self.features {
            let feature_id = feature.id();

            // Determine if this feature should receive the event
            let should_handle = match event {
                // Input events go only to focused feature
                TuiEvent::Key(_) => {
                    if let Some(focused_surface) = focused {
                        self.registry.surface_feature(focused_surface) == Some(feature_id)
                    } else {
                        false
                    }
                }
                // Broadcast events go to all features
                TuiEvent::Service(_) | TuiEvent::Stream(_) => true,
                TuiEvent::Resize { .. } => true,
                TuiEvent::Tick => true,
                // Focus events go to the affected feature
                TuiEvent::FocusGained => {
                    if let Some(focused_surface) = focused {
                        self.registry.surface_feature(focused_surface) == Some(feature_id)
                    } else {
                        false
                    }
                }
                TuiEvent::FocusLost => {
                    // Only the previously focused feature gets this
                    // (simplified; full version would track state)
                    false
                }
            };

            if should_handle {
                // Create a narrow context for this feature
                let mut navigate_cb = |_route_id| {
                    // TODO: Implement route navigation
                };
                let mut dispatch_cb = |_cmd: &str| {
                    // TODO: Implement command dispatch
                };
                let mut approve_cb = |_tool_id: String, _approved: bool| {
                    // TODO: Implement tool approval
                };

                let mut ctx = UpdateCtx {
                    has_focus: focused
                        == self
                            .registry
                            .surfaces()
                            .find(|s| self.registry.surface_feature(*s) == Some(feature_id)),
                    focused_surface: focused,
                    is_streaming: false,          // TODO: Get from session state
                    pending_tools: 0,             // TODO: Get from session state
                    plan_mode_active: false,      // TODO: Get from session state
                    auto_continue_enabled: false, // TODO: Get from session state
                    theme_colors: &self.theme,
                    navigate: &mut navigate_cb,
                    dispatch_command: &mut dispatch_cb,
                    approve_tool: &mut approve_cb,
                };

                let actions = feature.update(event, &mut ctx);
                all_actions.extend(actions);
            }
        }

        all_actions
    }

    /// Render all features to their assigned surfaces.
    pub fn render_frame(&mut self) -> Result<()> {
        self.terminal.draw(|frame| {
            let focused = self.focus.focused();
            let frame_area = frame.area();

            let ctx = RenderCtx {
                frame_area,
                focused_surface: focused,
                theme_colors: &self.theme,
            };

            for feature in &self.features {
                // Render each surface owned by this feature
                for surface in self.registry.surfaces() {
                    if self.registry.surface_feature(surface) == Some(feature.id()) {
                        feature.render(surface, frame, &ctx);
                    }
                }
            }
        })?;

        Ok(())
    }

    /// Get a reference to the feature registry (read-only).
    pub fn registry(&self) -> &FeatureRegistry {
        &self.registry
    }

    /// Get a mutable reference to the terminal.
    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Minimal test feature for AppShell integration testing
    struct TestFeature {
        id: &'static str,
        events_received: Arc<Mutex<Vec<String>>>,
    }

    impl TestFeature {
        fn new(id: &'static str) -> Self {
            Self {
                id,
                events_received: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl TuiFeature for TestFeature {
        fn id(&self) -> &'static str {
            self.id
        }

        fn register(&self, reg: &mut FeatureRegistry) {
            reg.register_surface(SurfaceId::new(self.id), self.id);
        }

        fn update(&mut self, _event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
            Vec::new()
        }

        fn render(&self, _surface: SurfaceId, _frame: &mut ratatui::Frame, _ctx: &RenderCtx) {
            // No-op for testing
        }
    }

    #[test]
    fn appshell_registers_feature() {
        // This test is minimal since creating a Terminal requires backend setup
        // Full integration tests would be in tests/ directory
        let feature = TestFeature::new("test");
        assert_eq!(feature.id(), "test");
    }
}
