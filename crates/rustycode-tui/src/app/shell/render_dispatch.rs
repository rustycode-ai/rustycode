//! Feature-aware render dispatch for AppShell.
//!
//! Extracts layout computation and surface allocation from the monolithic renderer
//! and dispatches feature rendering to registered surfaces.
//!
//! Preserves the `RendererState` snapshot pattern from the original `PolishedRenderer`.
//!
//! ## Layout Model
//!
//! The terminal is split into 5 vertical chunks (top to bottom):
//!
//! ```text
//! ┌──────────────────────┐
//! │ header          (1h) │  ← always 1 line
//! ├──────────────────────┤
//! │ status_bar   (0-1h)  │  ← 0 when collapsed
//! ├──────────┬───────────┤
//! │ message  │ sidebar   │  ← fills remaining space; sidebar splits off
//! │ area     │ (opt)     │     when visible and terminal is wide enough
//! ├──────────┴───────────┤
//! │ input           (3h) │
//! ├──────────────────────┤
//! │ footer       (0-1h)  │  ← 0 when collapsed
//! └──────────────────────┘
//! ```

use crate::app::features::{RenderCtx, SurfaceId, TuiFeature};
use crate::theme::ThemeColors;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Frame;
use std::collections::HashMap;

// ── Layout Types ──────────────────────────────────────────────────────────

/// Computed layout for a single render frame.
///
/// Each field is a `Rect` describing the pixel area allocated to that section.
/// `sidebar` is `None` when the sidebar is hidden or the terminal is too narrow.
/// `is_too_small` is `true` when the terminal falls below [`RendererConfig::min_width`]
/// or [`RendererConfig::min_height`].
#[derive(Debug, Clone, Copy)]
pub struct FrameLayout {
    /// Header bar (always 1 line tall).
    pub header: Rect,
    /// Status bar (1 line when visible, 0 when collapsed).
    pub status_bar: Rect,
    /// Main message/chat area. If a sidebar is present, this is the *left* portion.
    pub message_area: Rect,
    /// Input area (always 3 lines tall).
    pub input: Rect,
    /// Footer bar (1 line when visible, 0 when collapsed).
    pub footer: Rect,
    /// Sidebar area, split from the right edge of the message area.
    /// `None` when sidebar is not visible or terminal is too narrow.
    pub sidebar: Option<Rect>,
    /// `true` when the terminal is smaller than the configured minimums.
    pub is_too_small: bool,
}

/// Tunable renderer settings ported from `PolishedRenderer`.
///
/// These defaults match the values in `crate::app::renderer::RendererConfig`.
/// Kept as a plain struct (no serde dependency) — serialisation lives in the
/// monolithic renderer config.
#[derive(Debug, Clone, Copy)]
pub struct RendererConfig {
    /// Minimum terminal width before showing the small-terminal fallback.
    pub min_width: u16,
    /// Minimum terminal height before showing the small-terminal fallback.
    pub min_height: u16,
    /// Height below which chrome auto-collapses.
    pub collapse_chrome_below_height: u16,
    /// Width below which the sidebar will NOT be split (even if visible).
    pub sidebar_min_message_width: u16,
    /// Minimum sidebar width.
    pub sidebar_min_width: u16,
    /// Maximum sidebar width.
    pub sidebar_max_width: u16,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            min_width: 40,
            min_height: 8,
            collapse_chrome_below_height: 12,
            sidebar_min_message_width: 100,
            sidebar_min_width: 24,
            sidebar_max_width: 34,
        }
    }
}

// ── Surface Layout ────────────────────────────────────────────────────────

/// Surface layout descriptor — defines how a surface is sized and positioned.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceLayout {
    /// The allocated rectangle for this surface.
    pub area: Rect,
    /// Whether this surface is visible in the current layout.
    pub visible: bool,
}

// ── RenderDispatch ────────────────────────────────────────────────────────

/// Orchestrates feature rendering with layout management.
///
/// RenderDispatch:
/// 1. Computes surface allocations based on terminal size and layout rules
/// 2. Routes rendering to each feature's surfaces
/// 3. Allows gradual decomposition of the monolithic renderer
pub struct RenderDispatch {
    /// Computed surface allocations for this frame.
    surfaces: HashMap<SurfaceId, SurfaceLayout>,
    /// Renderer configuration (thresholds, sizes).
    config: RendererConfig,
}

impl RenderDispatch {
    /// Create a new render dispatcher with default config.
    ///
    /// Does NOT perform rendering — use `dispatch_all()` to actually render.
    pub fn new() -> Self {
        Self {
            surfaces: HashMap::new(),
            config: RendererConfig::default(),
        }
    }

    /// Create a new render dispatcher with explicit config.
    pub fn with_config(config: RendererConfig) -> Self {
        Self {
            surfaces: HashMap::new(),
            config,
        }
    }

    /// Look up the allocated layout for a surface.
    pub fn surface_layout(&self, surface: SurfaceId) -> Option<SurfaceLayout> {
        self.surfaces.get(&surface).copied()
    }

    /// Compute the frame layout from the terminal size and chrome flags.
    ///
    /// Ports the exact constraint values from `RendererLayout::build()` in
    /// `crate::app::renderer`:
    ///
    /// ```text
    /// [Length(1), Length(status_bar_height), Min(0), Length(3), Length(footer_height)]
    /// ```
    ///
    /// Then optionally splits the message area horizontally for the sidebar
    /// when `sidebar_visible && message_area.width > sidebar_min_message_width`.
    pub fn compute_layout(
        &self,
        frame_area: Rect,
        status_bar_collapsed: bool,
        footer_collapsed: bool,
        sidebar_visible: bool,
    ) -> FrameLayout {
        let is_too_small =
            frame_area.width < self.config.min_width || frame_area.height < self.config.min_height;

        let status_bar_height: u16 = if status_bar_collapsed { 0 } else { 1 };
        let footer_height: u16 = if footer_collapsed { 0 } else { 1 };

        // 5 vertical chunks — exact constraint values from PolishedRenderer.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),                 // header
                Constraint::Length(status_bar_height), // status bar
                Constraint::Min(0),                    // message area
                Constraint::Length(3),                 // input
                Constraint::Length(footer_height),     // footer
            ])
            .split(frame_area);

        let mut message_area = chunks[2];
        let mut sidebar_area = None;

        // Sidebar horizontal split — ported from PolishedRenderer::render()
        // (lines 326-336 of renderer.rs).
        if sidebar_visible && message_area.width > self.config.sidebar_min_message_width {
            let sidebar_width = (message_area.width / 3)
                .clamp(self.config.sidebar_min_width, self.config.sidebar_max_width);
            if message_area.width > sidebar_width {
                let split = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Min(0), Constraint::Length(sidebar_width)])
                    .split(message_area);
                message_area = split[0];
                sidebar_area = Some(split[1]);
            }
        }

        FrameLayout {
            header: chunks[0],
            status_bar: chunks[1],
            message_area,
            input: chunks[3],
            footer: chunks[4],
            sidebar: sidebar_area,
            is_too_small,
        }
    }

    /// Compute and cache surface allocations for a given set of surfaces.
    ///
    /// Uses the provided [`FrameLayout`] to assign areas to surfaces.
    /// Overlays are allocated the full frame area so they render on top.
    pub fn allocate_surfaces(
        &mut self,
        surfaces: impl Iterator<Item = (SurfaceId, SurfaceZone)>,
        layout: &FrameLayout,
    ) {
        for (surface, zone) in surfaces {
            let (area, visible) = match zone {
                SurfaceZone::Header => (layout.header, true),
                SurfaceZone::StatusBar => (layout.status_bar, layout.status_bar.height > 0),
                SurfaceZone::MessageArea => (layout.message_area, true),
                SurfaceZone::Input => (layout.input, true),
                SurfaceZone::Footer => (layout.footer, layout.footer.height > 0),
                SurfaceZone::Sidebar => {
                    let sb = layout.sidebar.unwrap_or_default();
                    (sb, layout.sidebar.is_some())
                }
                SurfaceZone::Overlay => (
                    Rect {
                        x: 0,
                        y: 0,
                        width: layout.header.width,
                        height: layout
                            .header
                            .height
                            .saturating_add(layout.status_bar.height)
                            .saturating_add(layout.message_area.height)
                            .saturating_add(layout.input.height)
                            .saturating_add(layout.footer.height),
                    },
                    true,
                ),
            };

            self.surfaces.insert(
                surface,
                SurfaceLayout {
                    area,
                    visible: visible && !layout.is_too_small,
                },
            );
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
    /// Computes layout, allocates surfaces, renders all features, handles overlays.
    /// This is the main entry point for AppShell rendering.
    pub fn dispatch_all(
        &mut self,
        features: &HashMap<&'static str, Box<dyn TuiFeature>>,
        registry: &crate::app::features::FeatureRegistry,
        focused_surface: Option<SurfaceId>,
        theme_colors: &ThemeColors,
        frame_area: Rect,
        frame: &mut Frame,
        status_bar_collapsed: bool,
        footer_collapsed: bool,
        sidebar_visible: bool,
    ) {
        // Compute layout from frame area and chrome flags.
        let layout = self.compute_layout(
            frame_area,
            status_bar_collapsed,
            footer_collapsed,
            sidebar_visible,
        );

        // Allocate surfaces with zone-based areas from the computed layout.
        // We assign zones based on surface naming conventions:
        //   - surfaces containing "sidebar" → Sidebar zone
        //   - surfaces containing "overlay" or known overlay names → Overlay zone
        //   - everything else → MessageArea zone
        let surface_zones = registry.surfaces().map(|s| {
            let zone = classify_surface(s);
            (s, zone)
        });
        self.allocate_surfaces(surface_zones, &layout);

        // Render each feature's surfaces.
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

/// Logical zone within the frame — determines which part of [`FrameLayout`]
/// a surface is allocated to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceZone {
    /// Header bar (1 line).
    Header,
    /// Status bar (0-1 lines).
    StatusBar,
    /// Main message/chat area.
    MessageArea,
    /// Input area (3 lines).
    Input,
    /// Footer bar (0-1 lines).
    Footer,
    /// Sidebar panel (right split of message area).
    Sidebar,
    /// Full-frame overlay (renders on top of everything).
    Overlay,
}

/// Classify a surface into a zone based on its ID.
///
/// Uses naming conventions from the feature registry:
/// - `"sidebar"` → [`SurfaceZone::Sidebar`]
/// - `"header"` → [`SurfaceZone::Header`]
/// - `"status_bar"` → [`SurfaceZone::StatusBar`]
/// - `"input"` → [`SurfaceZone::Input`]
/// - `"footer"` → [`SurfaceZone::Footer`]
/// - overlay-related names → [`SurfaceZone::Overlay`]
/// - everything else → [`SurfaceZone::MessageArea`]
fn classify_surface(surface: SurfaceId) -> SurfaceZone {
    let name = surface.as_str();

    // Check for sidebar.
    if name.contains("sidebar") {
        return SurfaceZone::Sidebar;
    }

    // Check for header.
    if name == "header" {
        return SurfaceZone::Header;
    }

    // Check for status_bar.
    if name == "status_bar" {
        return SurfaceZone::StatusBar;
    }

    // Check for input.
    if name == "input" {
        return SurfaceZone::Input;
    }

    // Check for footer.
    if name == "footer" {
        return SurfaceZone::Footer;
    }

    // Check for overlays — known overlay surface names from PolishedRenderer.
    // These are rendered on top of the full frame area.
    const OVERLAY_NAMES: &[&str] = &[
        "search",
        "tool_panel",
        "worker_panel",
        "team_panel",
        "task_dashboard",
        "clarification_panel",
        "provider_selector",
        "file_finder",
        "model_selector",
        "file_selector",
        "skill_palette",
        "marketplace_browser",
        "theme_preview",
        "command_palette",
        "help",
        "tool_approval",
        "error_manager",
        "compaction_preview",
        "wizard",
        "toast",
        "plugin_manager",
        "overlay",
    ];

    if OVERLAY_NAMES.contains(&name) || name.contains("overlay") {
        return SurfaceZone::Overlay;
    }

    // Default: message area.
    SurfaceZone::MessageArea
}

impl Default for RenderDispatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper ────────────────────────────────────────────────────────────

    fn rect(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    fn default_dispatch() -> RenderDispatch {
        RenderDispatch::new()
    }

    // ── compute_layout tests ──────────────────────────────────────────────

    #[test]
    fn layout_80x24_returns_correct_chunks() {
        let dispatch = default_dispatch();
        let layout = dispatch.compute_layout(rect(80, 24), false, false, false);

        assert!(!layout.is_too_small);
        assert_eq!(layout.header, Rect::new(0, 0, 80, 1));
        assert_eq!(layout.status_bar, Rect::new(0, 1, 80, 1));
        assert_eq!(layout.input, Rect::new(0, 20, 80, 3));
        assert_eq!(layout.footer, Rect::new(0, 23, 80, 1));
        // header(1) + status(1) + input(3) + footer(1) = 6 fixed
        // message_area = 24 - 6 = 18, y starts at 2
        assert_eq!(layout.message_area, Rect::new(0, 2, 80, 18));
        assert_eq!(layout.sidebar, None);
    }

    #[test]
    fn layout_collapsed_chrome_gives_zero_height() {
        let dispatch = default_dispatch();
        let layout = dispatch.compute_layout(rect(80, 24), true, true, false);

        assert_eq!(layout.status_bar.height, 0);
        assert_eq!(layout.footer.height, 0);
        // With chrome collapsed, message area gets 2 extra lines:
        // header(1) + status(0) + input(3) + footer(0) = 4 fixed
        // message_area = 24 - 4 = 20, y starts at 1
        assert_eq!(layout.message_area, Rect::new(0, 1, 80, 20));
    }

    #[test]
    fn layout_detects_too_small_terminal() {
        let dispatch = default_dispatch();
        // min_width=40, min_height=8
        let small_w = dispatch.compute_layout(rect(39, 24), false, false, false);
        assert!(small_w.is_too_small);

        let small_h = dispatch.compute_layout(rect(80, 7), false, false, false);
        assert!(small_h.is_too_small);

        let just_right = dispatch.compute_layout(rect(40, 8), false, false, false);
        assert!(!just_right.is_too_small);
    }

    #[test]
    fn layout_splits_sidebar_when_visible_and_wide_enough() {
        let dispatch = default_dispatch();
        // 120 wide — well above sidebar_min_message_width (100).
        let layout = dispatch.compute_layout(rect(120, 24), false, false, true);

        assert!(layout.sidebar.is_some());
        let sidebar = layout.sidebar.unwrap();
        assert!(sidebar.width >= 24); // sidebar_min_width
        assert!(sidebar.width <= 34); // sidebar_max_width

        // Message area should be narrower than full width.
        assert!(layout.message_area.width < 120);
        assert_eq!(layout.message_area.width + sidebar.width, 120);
    }

    #[test]
    fn layout_does_not_split_sidebar_when_narrow() {
        let dispatch = default_dispatch();
        // 80 wide — message_area will be ~74 wide after chrome, but
        // message_area.width will be 80 (no vertical side-chrome in this path).
        // Actually, message_area.width = 80 (full width since no horizontal split yet).
        // The check is message_area.width > 100. 80 < 100, so no sidebar.
        let layout = dispatch.compute_layout(rect(80, 24), false, false, true);

        assert_eq!(layout.sidebar, None);
        // Message area should span full width.
        assert_eq!(layout.message_area.width, 80);
    }

    #[test]
    fn layout_sidebar_clamps_width() {
        let dispatch = default_dispatch();
        // 300 wide — sidebar width = 300/3 = 100, clamped to max 34.
        let layout = dispatch.compute_layout(rect(300, 24), false, false, true);

        let sidebar = layout.sidebar.expect("sidebar should be present");
        assert_eq!(sidebar.width, 34);
    }

    #[test]
    fn layout_sidebar_hidden_when_not_visible() {
        let dispatch = default_dispatch();
        let layout = dispatch.compute_layout(rect(120, 24), false, false, false);
        assert_eq!(layout.sidebar, None);
    }

    #[test]
    fn renderer_config_defaults_match_renderer() {
        let config = RendererConfig::default();
        assert_eq!(config.min_width, 40);
        assert_eq!(config.min_height, 8);
        assert_eq!(config.collapse_chrome_below_height, 12);
        assert_eq!(config.sidebar_min_message_width, 100);
        assert_eq!(config.sidebar_min_width, 24);
        assert_eq!(config.sidebar_max_width, 34);
    }

    // ── classify_surface tests ────────────────────────────────────────────

    #[test]
    fn classify_surface_sidebar() {
        assert_eq!(
            classify_surface(SurfaceId::new("sidebar")),
            SurfaceZone::Sidebar
        );
        assert_eq!(
            classify_surface(SurfaceId::new("session_sidebar")),
            SurfaceZone::Sidebar
        );
    }

    #[test]
    fn classify_surface_overlays() {
        for name in &[
            "search",
            "tool_panel",
            "help",
            "command_palette",
            "toast",
            "plugin_manager",
        ] {
            assert_eq!(
                classify_surface(SurfaceId::new(name)),
                SurfaceZone::Overlay,
                "expected {name} to be classified as Overlay"
            );
        }
    }

    #[test]
    fn classify_surface_default_is_message_area() {
        assert_eq!(
            classify_surface(SurfaceId::new("chat")),
            SurfaceZone::MessageArea
        );
        assert_eq!(
            classify_surface(SurfaceId::new("messages")),
            SurfaceZone::MessageArea
        );
        assert_eq!(
            classify_surface(SurfaceId::new("unknown_surface")),
            SurfaceZone::MessageArea
        );
    }

    // ── allocate_surfaces tests ───────────────────────────────────────────

    #[test]
    fn allocate_surfaces_maps_zones_to_layout_areas() {
        let mut dispatch = default_dispatch();
        let layout = dispatch.compute_layout(rect(80, 24), false, false, false);

        let surfaces = vec![
            (SurfaceId::new("header"), SurfaceZone::Header),
            (SurfaceId::new("chat"), SurfaceZone::MessageArea),
            (SurfaceId::new("input"), SurfaceZone::Input),
            (SurfaceId::new("footer"), SurfaceZone::Footer),
        ];
        dispatch.allocate_surfaces(surfaces.into_iter(), &layout);

        let header_layout = dispatch.surface_layout(SurfaceId::new("header")).unwrap();
        assert_eq!(header_layout.area, layout.header);
        assert!(header_layout.visible);

        let chat_layout = dispatch.surface_layout(SurfaceId::new("chat")).unwrap();
        assert_eq!(chat_layout.area, layout.message_area);
        assert!(chat_layout.visible);
    }

    #[test]
    fn allocate_surfaces_sidebar_visible() {
        let mut dispatch = default_dispatch();
        let layout = dispatch.compute_layout(rect(120, 24), false, false, true);

        let surfaces = vec![(SurfaceId::new("sidebar"), SurfaceZone::Sidebar)];
        dispatch.allocate_surfaces(surfaces.into_iter(), &layout);

        let sb_layout = dispatch.surface_layout(SurfaceId::new("sidebar")).unwrap();
        assert!(sb_layout.visible);
        assert_eq!(sb_layout.area, layout.sidebar.unwrap());
    }

    #[test]
    fn allocate_surfaces_sidebar_invisible_when_not_split() {
        let mut dispatch = default_dispatch();
        let layout = dispatch.compute_layout(rect(80, 24), false, false, true);

        let surfaces = vec![(SurfaceId::new("sidebar"), SurfaceZone::Sidebar)];
        dispatch.allocate_surfaces(surfaces.into_iter(), &layout);

        let sb_layout = dispatch.surface_layout(SurfaceId::new("sidebar")).unwrap();
        assert!(!sb_layout.visible);
    }

    #[test]
    fn allocate_surfaces_collapsed_chrome_is_not_visible() {
        let mut dispatch = default_dispatch();
        let layout = dispatch.compute_layout(rect(80, 24), true, true, false);

        let surfaces = vec![
            (SurfaceId::new("status_bar"), SurfaceZone::StatusBar),
            (SurfaceId::new("footer"), SurfaceZone::Footer),
        ];
        dispatch.allocate_surfaces(surfaces.into_iter(), &layout);

        let sb = dispatch
            .surface_layout(SurfaceId::new("status_bar"))
            .unwrap();
        assert!(!sb.visible);
        let ft = dispatch.surface_layout(SurfaceId::new("footer")).unwrap();
        assert!(!ft.visible);
    }

    #[test]
    fn allocate_surfaces_too_small_marks_all_invisible() {
        let mut dispatch = default_dispatch();
        let layout = dispatch.compute_layout(rect(30, 5), false, false, false);
        assert!(layout.is_too_small);

        let surfaces = vec![
            (SurfaceId::new("header"), SurfaceZone::Header),
            (SurfaceId::new("chat"), SurfaceZone::MessageArea),
        ];
        dispatch.allocate_surfaces(surfaces.into_iter(), &layout);

        assert!(
            !dispatch
                .surface_layout(SurfaceId::new("header"))
                .unwrap()
                .visible
        );
        assert!(
            !dispatch
                .surface_layout(SurfaceId::new("chat"))
                .unwrap()
                .visible
        );
    }

    // ── Legacy tests (preserved) ──────────────────────────────────────────

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
    fn surface_layout_lookup_returns_none_for_unallocated() {
        let dispatch = RenderDispatch::new();
        assert!(dispatch
            .surface_layout(SurfaceId::new("nonexistent"))
            .is_none());
    }
}
