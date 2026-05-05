//! Plug-and-play renderer trait for TUI backends.
//!
//! Any renderer implementation must satisfy [`TuiRenderer`]. New backends
//! (compact, web, accessibility, …) can be added without touching the event
//! loop or the existing renderer dispatch code — only a new `RendererMode`
//! variant and a `match` arm in `FrameRenderer::render` are needed.
//!
//! # Architecture
//!
//! ```text
//! rustycode-ui-core
//!   └─ renderer.rs   TuiRenderer + RendererFrame (this file)
//!
//! rustycode-tui
//!   └─ app/renderer.rs   RendererMode enum, PolishedRenderer, FrameRenderer
//!   └─ app/brutalist_renderer.rs   BrutalistRenderer
//! ```

use ratatui::{layout::Rect, Frame};

// RENDERER FRAME — lightweight snapshot passed to every backend each frame

/// Lightweight, renderer-agnostic context passed to every backend on each frame.
///
/// Fields are deliberately minimal — only data that *every* renderer needs.
/// Backend-specific data (theme colours, token counts, input state, …) lives
/// in the concrete renderer struct, not here.
#[derive(Debug, Clone)]
pub struct RendererFrame {
    /// Full terminal area available to the renderer.
    pub area: Rect,

    /// Whether the AI is currently streaming a response.
    pub is_streaming: bool,

    /// Human-visible project name (usually the working-directory basename).
    pub project_name: String,

    /// Optional current git branch, if available.
    pub git_branch: Option<String>,

    /// Number of active tool executions (used for status indicators).
    pub active_tool_count: usize,

    /// Whether the status bar / header area is collapsed.
    pub status_bar_collapsed: bool,

    /// Whether the footer area is collapsed.
    pub footer_collapsed: bool,
}

impl RendererFrame {
    /// Construct a new [`RendererFrame`].
    ///
    /// Prefer the builder-style construction methods for ergonomic
    /// construction when optional fields are needed.
    pub const fn new(area: Rect) -> Self {
        Self {
            area,
            is_streaming: false,
            project_name: String::new(),
            git_branch: None,
            active_tool_count: 0,
            status_bar_collapsed: false,
            footer_collapsed: false,
        }
    }

    // ── Convenience setters (builder-style) ─────────────────────────────────

    /// Set `is_streaming`.
    pub const fn with_streaming(mut self, streaming: bool) -> Self {
        self.is_streaming = streaming;
        self
    }

    /// Set `project_name`.
    pub fn with_project_name(mut self, name: impl Into<String>) -> Self {
        self.project_name = name.into();
        self
    }

    /// Set `git_branch`.
    pub fn with_git_branch(mut self, branch: Option<String>) -> Self {
        self.git_branch = branch;
        self
    }

    /// Set `active_tool_count`.
    pub const fn with_active_tool_count(mut self, count: usize) -> Self {
        self.active_tool_count = count;
        self
    }

    /// Set collapsed state for status bar and footer.
    pub const fn with_collapsed(mut self, status_bar: bool, footer: bool) -> Self {
        self.status_bar_collapsed = status_bar;
        self.footer_collapsed = footer;
        self
    }
}

// TUI RENDERER TRAIT

/// Core renderer trait — implement this to add a new TUI rendering backend.
///
/// # Contract
///
/// Implementors are responsible for:
/// - Rendering the **full** terminal area (`ctx.area`) on each call
/// - Clearing stale content before drawing (e.g. `frame.render_widget(Clear, …)`)
/// - Respecting `ctx.status_bar_collapsed` / `ctx.footer_collapsed` to avoid
///   rendering chrome elements that the user has toggled off
///
/// # Lifetime
///
/// A single renderer instance is kept alive for the duration of the session;
/// `&mut self` allows accumulating frame-to-frame state such as render caches,
/// scroll positions, or animation frames.
pub trait TuiRenderer: Send {
    /// Render one complete frame into `frame` using the shared context and any
    /// internal backend state.
    fn render(&mut self, frame: &mut Frame, ctx: &RendererFrame);

    /// Called by the event loop whenever the terminal is resized.
    ///
    /// Implementations should invalidate layout caches and re-compute widths
    /// that depend on the terminal dimensions. The default implementation is
    /// a no-op.
    #[allow(unused_variables)]
    fn on_resize(&mut self, new_area: Rect) {}

    /// Human-readable name of this backend, shown in the command palette and
    /// status bar when toggling renderers.
    fn name(&self) -> &'static str;
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn renderer_frame_new_defaults() {
        let area = Rect::new(0, 0, 80, 24);
        let frame = RendererFrame::new(area);
        assert_eq!(frame.area, area);
        assert!(!frame.is_streaming);
        assert!(frame.project_name.is_empty());
        assert!(frame.git_branch.is_none());
        assert_eq!(frame.active_tool_count, 0);
        assert!(!frame.status_bar_collapsed);
        assert!(!frame.footer_collapsed);
    }

    #[test]
    fn renderer_frame_builder_chain() {
        let area = Rect::new(0, 0, 120, 40);
        let frame = RendererFrame::new(area)
            .with_streaming(true)
            .with_project_name("rustycode")
            .with_git_branch(Some("main".into()))
            .with_active_tool_count(3)
            .with_collapsed(true, false);

        assert!(frame.is_streaming);
        assert_eq!(frame.project_name, "rustycode");
        assert_eq!(frame.git_branch.as_deref(), Some("main"));
        assert_eq!(frame.active_tool_count, 3);
        assert!(frame.status_bar_collapsed);
        assert!(!frame.footer_collapsed);
    }
}
