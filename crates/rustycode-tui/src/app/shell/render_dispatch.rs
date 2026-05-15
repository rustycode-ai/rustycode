//! Feature-aware render dispatch for AppShell.
//!
//! Extracts layout computation and surface allocation from the monolithic renderer
//! and dispatches feature rendering to registered surfaces.
//!
//! Preserves the `RendererState` snapshot pattern from the original `PolishedRenderer`.

use crate::app::features::{RenderCtx, SurfaceId, TuiFeature};
use crate::theme::ThemeColors;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::collections::HashMap;

/// Surface layout descriptor — defines how a surface is sized and positioned.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceLayout {
    /// The allocated rectangle for this surface.
    pub area: Rect,
    /// Whether this surface is visible in the current layout.
    pub visible: bool,
}

/// Orchestrates feature rendering with layout management.
///
/// RenderDispatch:
/// 1. Computes surface allocations based on terminal size and layout rules
/// 2. Routes rendering to each feature's surfaces
/// 3. Allows gradual decomposition of the monolithic renderer
///
/// For now, all surfaces get the full frame area. Future versions can implement
/// more sophisticated layout (sidebar, modal layer, etc).
pub struct RenderDispatch {
    /// Computed surface allocations for this frame.
    surfaces: HashMap<SurfaceId, SurfaceLayout>,
}

impl RenderDispatch {
    /// Create a new render dispatcher.
    ///
    /// Does NOT perform rendering — use `dispatch_all()` to actually render.
    pub fn new() -> Self {
        Self {
            surfaces: HashMap::new(),
        }
    }

    /// Look up the allocated layout for a surface.
    pub fn surface_layout(&self, surface: SurfaceId) -> Option<SurfaceLayout> {
        self.surfaces.get(&surface).copied()
    }

    /// Compute and cache surface allocations for a given set of surfaces.
    ///
    /// This is called before rendering begins to ensure all surfaces know
    /// their allocated areas.
    ///
    /// For now, all surfaces are visible and take the full frame area.
    /// In future, this can be more sophisticated (sidebar, modal layout, etc).
    pub fn allocate_surfaces(
        &mut self,
        surfaces: impl Iterator<Item = SurfaceId>,
        frame_area: Rect,
    ) {
        for surface in surfaces {
            let layout = SurfaceLayout {
                area: frame_area,
                visible: true,
            };
            self.surfaces.insert(surface, layout);
        }
    }

    /// Dispatch rendering to a single feature for all its surfaces.
    ///
    /// The feature's `render()` method is called with each of its registered
    /// surfaces, along with the render context (frame, allocated area, theme).
    pub fn render_feature(
        &self,
        feature: &dyn TuiFeature,
        surfaces: impl Iterator<Item = SurfaceId>,
        focused_surface: Option<SurfaceId>,
        theme_colors: &ThemeColors,
        frame: &mut Frame,
    ) {
        for surface in surfaces {
            if let Some(layout) = self.surface_layout(surface) {
                if !layout.visible {
                    continue;
                }

                let ctx = RenderCtx {
                    frame_area: layout.area,
                    focused_surface,
                    theme_colors,
                };

                feature.render(surface, frame, &ctx);
            }
        }
    }

    /// Full render dispatch workflow.
    ///
    /// Allocates surfaces, renders all features, handles overlays.
    /// This is the main entry point for AppShell rendering.
    pub fn dispatch_all(
        &mut self,
        features: &HashMap<&'static str, Box<dyn TuiFeature>>,
        registry: &crate::app::features::FeatureRegistry,
        focused_surface: Option<SurfaceId>,
        theme_colors: &ThemeColors,
        frame_area: Rect,
        frame: &mut Frame,
    ) {
        // Allocate surfaces from the registry
        self.allocate_surfaces(registry.surfaces(), frame_area);

        // Render each feature's surfaces
        for (feature_id, feature) in features {
            let surfaces = registry
                .surfaces()
                .filter(|s| registry.surface_feature(*s) == Some(feature_id));

            self.render_feature(
                feature.as_ref(),
                surfaces,
                focused_surface,
                theme_colors,
                frame,
            );
        }
    }
}

impl Default for RenderDispatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_layout_is_copyable() {
        let layout = SurfaceLayout {
            area: Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
            visible: true,
        };
        let _copy = layout;
        let _another = layout;
    }

    #[test]
    fn render_dispatch_new_creates_empty_surfaces() {
        let dispatch = RenderDispatch::new();
        assert!(dispatch.surfaces.is_empty());
    }

    #[test]
    fn allocate_surfaces_adds_to_map() {
        let mut dispatch = RenderDispatch::new();
        let frame_area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let surfaces = vec![SurfaceId::new("main"), SurfaceId::new("sidebar")];
        dispatch.allocate_surfaces(surfaces.into_iter(), frame_area);

        assert_eq!(dispatch.surfaces.len(), 2);
        assert!(dispatch.surface_layout(SurfaceId::new("main")).is_some());
        assert!(dispatch.surface_layout(SurfaceId::new("sidebar")).is_some());
    }

    #[test]
    fn surface_layout_lookup_returns_none_for_unallocated() {
        let dispatch = RenderDispatch::new();
        assert!(dispatch
            .surface_layout(SurfaceId::new("nonexistent"))
            .is_none());
    }
}
