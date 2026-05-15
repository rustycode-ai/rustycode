//! Plugin Manager feature module
//!
//! Self-contained feature for managing plugins (list, search, install, uninstall).
//! Implements the TuiFeature trait and owns all plugin manager state.
//!
//! ## State
//! - `PluginManagerState`: Wraps PluginManagerUI and visibility flag
//!
//! ## Events Handled
//! - Key events (when focused): Navigation, search, enter/escape
//! - Tick: Periodic updates (if needed)
//!
//! ## Surfaces
//! - "plugin-manager": Main plugin manager overlay
//!
//! ## Rendering
//! Renders to the plugin manager surface when visible

use crate::app::features::{
    FeatureRegistry, RenderCtx, SurfaceId, TuiAction, TuiEvent, TuiFeature, UpdateCtx,
};
use crate::plugin::{PluginManager, PluginManagerUI};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use std::sync::{Arc, Mutex, RwLock};

/// Plugin Manager feature state
pub struct PluginManagerState {
    /// Plugin manager UI state (thread-safe wrapped)
    ui: Arc<Mutex<PluginManagerUI>>,
    /// Plugin manager backend (shared with main TUI)
    manager: Arc<RwLock<PluginManager>>,
    /// Surface ID for rendering
    surface: SurfaceId,
}

impl PluginManagerState {
    /// Create a new Plugin Manager feature
    pub fn new(manager: Arc<RwLock<PluginManager>>) -> Self {
        Self {
            ui: Arc::new(Mutex::new(PluginManagerUI::new())),
            manager,
            surface: SurfaceId::new("plugin-manager"),
        }
    }

    /// Check if the plugin manager is visible
    pub fn is_visible(&self) -> bool {
        self.ui.lock().map(|ui| ui.is_visible()).unwrap_or(false)
    }

    /// Show the plugin manager
    pub fn show(&mut self) {
        if let Ok(mut ui) = self.ui.lock() {
            ui.show();
        }
    }

    /// Hide the plugin manager
    pub fn hide(&mut self) {
        if let Ok(mut ui) = self.ui.lock() {
            ui.hide();
        }
    }

    /// Toggle plugin manager visibility
    pub fn toggle(&mut self) {
        if let Ok(mut ui) = self.ui.lock() {
            ui.toggle();
        }
    }

    /// Handle a key event
    fn handle_key(&mut self, key: KeyEvent) -> Vec<TuiAction> {
        if !self.is_visible() {
            return Vec::new();
        }

        let mut actions = Vec::new();

        match (key.code, key.modifiers) {
            // Escape closes the plugin manager
            (KeyCode::Esc, _) => {
                self.hide();
                actions.push(TuiAction::MarkDirty);
            }
            // Navigation keys
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                // TODO: Navigate up in plugin list
                actions.push(TuiAction::MarkDirty);
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                // TODO: Navigate down in plugin list
                actions.push(TuiAction::MarkDirty);
            }
            // Search mode
            (KeyCode::Char(c), _) if c.is_alphanumeric() => {
                // TODO: Enter search mode
                actions.push(TuiAction::MarkDirty);
            }
            // Enter - select plugin
            (KeyCode::Enter, _) => {
                // TODO: Select/install plugin
                actions.push(TuiAction::MarkDirty);
            }
            _ => {}
        }

        actions
    }
}

impl TuiFeature for PluginManagerState {
    fn id(&self) -> &'static str {
        "plugin-manager"
    }

    fn register(&self, reg: &mut FeatureRegistry) {
        // Register the surface this feature owns
        reg.register_surface(SurfaceId::new("plugin-manager"), self.id());
    }

    fn update(&mut self, event: &TuiEvent, _ctx: &mut UpdateCtx) -> Vec<TuiAction> {
        match event {
            TuiEvent::Key(key_event) => self.handle_key(*key_event),
            TuiEvent::Tick => {
                // Reload plugin data from disk on tick if visible
                if self.is_visible() {
                    if let Ok(mut manager) = self.manager.write() {
                        let _ = manager.reload_from_disk();
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn render(&self, surface: SurfaceId, frame: &mut Frame, _ctx: &RenderCtx) {
        // Only render if this is our surface and we're visible
        if surface != self.surface || !self.is_visible() {
            return;
        }

        let size = frame.area();

        // Get locks on both UI and manager for rendering
        if let (Ok(ui), Ok(manager)) = (self.ui.lock(), self.manager.read()) {
            ui.render(frame, size, &manager);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_manager_feature_id() {
        let manager = Arc::new(RwLock::new(PluginManager::default()));
        let feature = PluginManagerState::new(manager);
        assert_eq!(feature.id(), "plugin-manager");
    }

    #[test]
    fn plugin_manager_visibility() {
        let manager = Arc::new(RwLock::new(PluginManager::default()));
        let mut feature = PluginManagerState::new(manager);

        assert!(!feature.is_visible());
        feature.show();
        assert!(feature.is_visible());
        feature.hide();
        assert!(!feature.is_visible());
    }

    #[test]
    fn plugin_manager_toggle() {
        let manager = Arc::new(RwLock::new(PluginManager::default()));
        let mut feature = PluginManagerState::new(manager);

        assert!(!feature.is_visible());
        feature.toggle();
        assert!(feature.is_visible());
        feature.toggle();
        assert!(!feature.is_visible());
    }

    #[test]
    fn plugin_manager_registers_surface() {
        let manager = Arc::new(RwLock::new(PluginManager::default()));
        let feature = PluginManagerState::new(manager);

        let mut reg = FeatureRegistry::new();
        feature.register(&mut reg);

        // Check that the surface was registered
        assert_eq!(
            reg.surface_feature(SurfaceId::new("plugin-manager")),
            Some("plugin-manager")
        );
    }
}
