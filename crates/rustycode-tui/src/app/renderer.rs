//! Renderer dispatch layer for the TUI.

use crate::app::event_loop::TUI;
use crate::app::render::shared::centered_rect;
use crate::theme::parse_color;
use crate::ui::footer::Footer;
use crate::ui::header::{Header, HeaderStatus};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;
use serde::{Deserialize, Serialize};
use std::rc::Rc;

// RENDERER STATE — unified snapshot (replaces the old asymmetric RenderContext)

/// Unified snapshot of TUI state for a single render frame.
///
/// Extracted once per frame from `TUI` and passed to renderer backends,
/// avoiding the previous pattern where `PolishedRenderer` used `RenderContext`
/// while `BrutalistRenderer` was built via a 25-field builder on `TUI`.
///
/// **Rule:** Fields that are shared between ≥ 2 backends belong here.
/// Backend-specific fields (theme colours, input cursor position, …) stay
/// inside the concrete renderer struct.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RendererState {
    // ── Layout ──────────────────────────────────────────────────────────────
    /// Full terminal area for the frame.
    pub area: Rect,

    // ── Context strings ──────────────────────────────────────────────────────
    /// Directory basename used as the project label in chrome.
    pub project_name: String,
    /// Current git branch (cached — not re-queried every frame).
    pub git_branch: Option<String>,
    /// Short model name for display (e.g. `"sonnet-4-5"` not the full path).
    pub current_model: String,

    // ── Status ───────────────────────────────────────────────────────────────
    /// High-level header status driven by streaming / error / idle state.
    pub header_status: HeaderStatus,
    /// Number of user turns (= user messages) in the current session.
    pub turn_count: usize,
    /// Number of active tool executions.
    pub pending_tools: usize,
    /// Active plan-mode banner, if any.
    pub plan_mode_banner: Option<crate::app::plan_mode_ops::PlanModeBanner>,
    /// Current AI autonomy level.
    pub ai_mode: crate::ui::header::AiModeLabel,

    // ── Tasks ─────────────────────────────────────────────────────────────────
    /// Total task count in workspace.
    pub task_count: usize,
    /// Compact summary string like `"✓3 ☐2"`.
    pub task_summary: String,

    // ── Session ───────────────────────────────────────────────────────────────
    /// Session wall-clock duration in seconds.
    pub session_secs: u64,
    /// Cumulative cost of this session in USD.
    pub session_cost: f64,

    // ── Chrome visibility ────────────────────────────────────────────────────
    pub status_bar_collapsed: bool,
    pub footer_collapsed: bool,
}

impl RendererState {
    /// Extract a `RendererState` from live `TUI` state.
    ///
    /// This is the single construction site — both `PolishedRenderer` and
    /// `BrutalistRenderer` call this before they build their own structs.
    pub fn from_tui(tui: &mut TUI, area: Rect) -> Self {
        let project_name = tui
            .services
            .cwd()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let header_status = if let Some(banner) = &tui.plan_mode_banner {
            banner.header_status()
        } else if tui.error_manager.is_showing() {
            HeaderStatus::Error
        } else if tui.streaming.is_streaming {
            if tui.active_tools.is_empty() {
                HeaderStatus::Thinking
            } else {
                HeaderStatus::RunningTools
            }
        } else {
            HeaderStatus::Ready
        };

        let turn_count = tui
            .messages
            .iter()
            .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
            .count();

        let done_count = tui
            .workspace_tasks
            .tasks
            .iter()
            .filter(|t| matches!(t.status, crate::app::tasks::TaskStatus::Completed))
            .count();
        let pending_count = tui
            .workspace_tasks
            .tasks
            .iter()
            .filter(|t| matches!(t.status, crate::app::tasks::TaskStatus::Pending))
            .count();
        let task_summary = if done_count > 0 || pending_count > 0 {
            format!("{}/{} tasks", done_count, done_count + pending_count)
        } else {
            String::new()
        };

        let current_model = tui
            .current_model
            .rsplit('/')
            .next()
            .map(|s| s.strip_prefix("claude-").unwrap_or(s))
            .unwrap_or(&tui.current_model)
            .to_string();

        Self {
            area,
            project_name,
            git_branch: tui.git_branch.clone(),
            current_model,
            header_status,
            turn_count,
            pending_tools: tui.active_tools.len(),
            plan_mode_banner: tui.plan_mode_banner.clone(),
            ai_mode: {
                use crate::ui::header::AiModeLabel;
                match tui.services.ai_mode() {
                    crate::services::agent_mode::AiMode::Ask => AiModeLabel::Ask,
                    crate::services::agent_mode::AiMode::Plan => AiModeLabel::Plan,
                    crate::services::agent_mode::AiMode::Act => AiModeLabel::Act,
                    crate::services::agent_mode::AiMode::Yolo => AiModeLabel::Yolo,
                }
            },
            task_count: tui.workspace_tasks.tasks.len(),
            task_summary,
            session_secs: tui.start_time.elapsed().as_secs(),
            session_cost: tui.token_budget.session_cost_usd,
            status_bar_collapsed: tui.status_bar_collapsed,
            footer_collapsed: tui.footer_collapsed,
        }
    }
}

// RENDERER CONFIG / LAYOUT

/// Tunable renderer settings for the polished backend.
///
/// The goal is to move hardcoded layout thresholds and overlay copy into one
/// place so future UI work can adjust behavior without scattering constants
/// through the paint path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct RendererConfig {
    /// Minimum terminal width before showing the small-terminal fallback.
    pub min_width: u16,
    /// Minimum terminal height before showing the small-terminal fallback.
    pub min_height: u16,
    /// Height below which the chrome auto-collapses.
    pub collapse_chrome_below_height: u16,
    /// Background color for the header and footer chrome.
    pub chrome_background: String,
    /// Width percentage for the confirmation modal.
    pub confirmation_width_percent: u16,
    /// Height percentage for the confirmation modal.
    pub confirmation_height_percent: u16,
    /// Confirmation modal title.
    pub confirmation_title: String,
    /// Confirmation modal body copy.
    pub confirmation_text: String,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            min_width: 40,
            min_height: 8,
            collapse_chrome_below_height: 12,
            chrome_background: "#171717".to_string(),
            confirmation_width_percent: 60,
            confirmation_height_percent: 20,
            confirmation_title: "Confirm Action".to_string(),
            confirmation_text: "⚠️  Confirmation required: This action may discard local changes.\n\nPress Y to proceed, N to cancel, A to approve for this session, or Esc to cancel.".to_string(),
        }
    }
}

impl RendererConfig {
    fn chrome_background_color(&self) -> Color {
        parse_color(&self.chrome_background)
    }
}

/// Layout derived from the frame size and chrome visibility.
#[derive(Debug, Clone)]
struct RendererLayout {
    size: Rect,
    chunks: Rc<[Rect]>,
    is_too_small: bool,
}

impl RendererLayout {
    fn build(
        size: Rect,
        status_bar_collapsed: bool,
        footer_collapsed: bool,
        config: &RendererConfig,
    ) -> Self {
        let is_too_small = size.width < config.min_width || size.height < config.min_height;
        let status_bar_height = if status_bar_collapsed { 0 } else { 1 };
        let footer_height = if footer_collapsed { 0 } else { 1 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(status_bar_height),
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Length(footer_height),
            ])
            .split(size);

        Self {
            size,
            chunks,
            is_too_small,
        }
    }

    fn confirmation_area(&self, config: &RendererConfig) -> Rect {
        centered_rect(
            config.confirmation_width_percent,
            config.confirmation_height_percent,
            self.size,
        )
    }
}

// POLISHED RENDERER

/// Polished renderer backend — clean chrome + markdown-rendered messages.
#[non_exhaustive]
pub struct PolishedRenderer {
    state: RendererState,
    config: RendererConfig,
}

impl PolishedRenderer {
    /// Construct a `PolishedRenderer` from live `TUI` state.
    pub fn from_tui(tui: &mut TUI, area: Rect) -> Self {
        Self::with_config(tui, area, RendererConfig::default())
    }

    /// Construct a `PolishedRenderer` with explicit configuration.
    pub fn with_config(tui: &mut TUI, area: Rect, config: RendererConfig) -> Self {
        Self {
            state: RendererState::from_tui(tui, area),
            config,
        }
    }

    pub fn render(&self, tui: &mut TUI, frame: &mut Frame) {
        let render_start = std::time::Instant::now();
        let layout = RendererLayout::build(
            self.state.area,
            tui.status_bar_collapsed,
            tui.footer_collapsed,
            &self.config,
        );
        let size = layout.size;

        // Minimum size guard
        if layout.is_too_small {
            frame.render_widget(Clear, size);
            let msg = Paragraph::new(format!(
                "Terminal too small (min {}×{})",
                self.config.min_width, self.config.min_height
            ))
            .style(Style::default().fg(Color::Yellow));
            frame.render_widget(msg, size);
            return;
        }

        frame.render_widget(Clear, size);

        // Auto-collapse chrome on very small terminals
        if size.height < self.config.collapse_chrome_below_height {
            tui.status_bar_collapsed = true;
            tui.footer_collapsed = true;
        }

        // If there are any pending confirmation requests from background tasks,
        // show a centered modal prompting the user to Approve/Reject the action.
        let pending = crate::app::confirmation::pending_list();
        if !pending.is_empty() {
            // Show first pending request
            let area = layout.confirmation_area(&self.config);
            frame.render_widget(Clear, area);
            let para = Paragraph::new(self.config.confirmation_text.clone())
                .block(
                    Block::default()
                        .title(self.config.confirmation_title.clone())
                        .borders(ratatui::widgets::Borders::ALL),
                )
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(para, area);
            return;
        }

        let chunks = &layout.chunks;

        let mut message_area = chunks[2];
        let mut sidebar_area = None;
        if tui.session_sidebar.is_visible() && message_area.width > 100 {
            let sidebar_width = (message_area.width / 3).clamp(24, 34);
            if message_area.width > sidebar_width {
                let split = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Min(0), Constraint::Length(sidebar_width)])
                    .split(message_area);
                message_area = split[0];
                sidebar_area = Some(split[1]);
            }
        }
        tui.sidebar_area.set(sidebar_area.unwrap_or_default());

        tui.view.viewport_height = message_area.height.max(1) as usize;
        tui.view.messages_area.set(message_area);

        let header_bg =
            Block::default().style(Style::default().bg(self.config.chrome_background_color()));
        frame.render_widget(header_bg, chunks[0]);

        let header = Header::new()
            .with_app_name("rustycode")
            .with_project_name(self.state.project_name.clone())
            .with_git_branch(self.state.git_branch.clone())
            .with_counts(self.state.task_count, self.state.pending_tools)
            .with_turn_count(self.state.turn_count)
            .with_status(self.state.header_status)
            .with_ai_mode(Some(self.state.ai_mode))
            .with_spinner_frame(tui.animator.current_frame().progress_frame / 5);
        header.render(frame, chunks[0]);

        if !tui.status_bar_collapsed {
            self.render_status(tui, frame, chunks[1]);
        }

        let messages_start = std::time::Instant::now();
        self.render_messages(tui, frame, message_area);
        let messages_elapsed = messages_start.elapsed();
        self.render_input(tui, frame, chunks[3]);

        if !tui.footer_collapsed {
            let footer_bg =
                Block::default().style(Style::default().bg(self.config.chrome_background_color()));
            frame.render_widget(footer_bg, chunks[4]);

            let footer = Footer::new()
                .with_session_duration(Footer::format_duration(self.state.session_secs))
                .with_task_summary(self.state.task_summary.clone())
                .with_model(self.state.current_model.clone())
                .with_session_cost(self.state.session_cost);
            footer.render(frame, chunks[4]);
        }

        // ── Overlays (rendered last so they appear on top) ──────────────────
        self.render_overlays(tui, frame, size, chunks, message_area);

        if let Some(sidebar_area) = sidebar_area {
            tui.session_sidebar.render(frame, sidebar_area);
        }

        if crate::logging::is_debug_enabled() {
            let total_elapsed = render_start.elapsed();
            if total_elapsed > crate::app::DEBUG_SLOW_THRESHOLD {
                crate::debug_log!(
                    "Polished render ran long: width={} height={} messages={} message_ms={} total_ms={} streaming={} user_scrolled={}",
                    size.width,
                    size.height,
                    tui.messages.len(),
                    messages_elapsed.as_millis(),
                    total_elapsed.as_millis(),
                    tui.streaming.is_streaming,
                    tui.view.user_scrolled
                );
            }
        }
    }

    /// Render all overlay widgets (search, panels, dialogs, …).
    fn render_overlays(
        &self,
        tui: &mut TUI,
        frame: &mut Frame,
        size: Rect,
        _chunks: &[Rect],
        message_area: Rect,
    ) {
        // Overlay: search box (over message area - chunks[2])
        if tui.search_state.visible {
            render_search_box(tui, frame, message_area);
        }

        if tui.tool_panel.showing_tool_panel {
            render_tool_panel(tui, frame, message_area);
        }

        // Worker status panel overlay (Ctrl+W) - right side overlay
        if tui.worker_panel.visible {
            render_worker_panel(tui, frame, message_area);
        }

        if tui.team_panel.visible {
            frame.render_widget(Clear, message_area);
            frame.render_widget(tui.team_panel.clone(), message_area);
        }

        if tui.awaiting_clarification && tui.clarification_panel.visible {
            let panel_height = 15u16.min(size.height.saturating_sub(4));
            let panel_width = (size.width * 3 / 4).min(60);
            let panel_area = centered_rect(panel_width, panel_height, size);
            frame.render_widget(Clear, panel_area);
            frame.render_widget(tui.clarification_panel.clone(), panel_area);
        }

        // Overlay: provider selector
        if tui.showing_provider_selector {
            render_provider_selector(frame);
        }

        if tui.file_finder.is_visible() {
            tui.file_finder.render(frame, size);
        }

        if tui.model_selector.is_visible() {
            tui.model_selector.render(frame, size);
        }

        if tui.file_selector.is_visible() {
            tui.file_selector.render(frame, size);
        }

        if tui.skill_palette.is_visible() {
            tui.skill_palette.render(frame, size);
        }

        if tui.showing_plugin_manager {
            let mut manager = tui
                .plugin_manager
                .write()
                .unwrap_or_else(|e| e.into_inner());
            let _ = manager.reload_from_disk();
            tui.plugin_manager_ui.render(frame, size, &manager);
        }

        if tui.showing_marketplace_browser {
            tui.marketplace_browser.render(frame, size);
        }

        if tui.theme_preview.is_visible() {
            tui.theme_preview.render(frame, size);
        }

        if tui.command_palette.is_visible() {
            tui.command_palette.render(frame, size);
        }

        if tui.help_state.visible {
            crate::help::render_help(frame, size, &tui.help_state);
        }

        if tui.tool_approval.awaiting {
            if let Some(req) = tui.tool_approval.pending_requests.front() {
                let (panel_height, panel_width) =
                    crate::tool_approval::approval_panel_size(req, size);
                let panel_area = centered_rect(panel_width, panel_height, size);
                crate::tool_approval::render_approval_prompt(frame, panel_area, req, size);
            }
        }

        if tui.error_manager.is_showing() {
            frame.render_widget(Clear, size);
            tui.error_manager.render(frame, size);
        }

        // Overlay: compaction preview (while pending)
        if tui.compaction.showing_preview {
            tui.render_compaction_preview(frame, size);
        }

        if tui.wizard.showing_wizard {
            if let Some(ref mut wizard) = tui.wizard.wizard {
                frame.render_widget(Clear, size);
                wizard.render(frame, size);
            }
        }

        tui.toast_manager.render(
            frame,
            size,
            Some(&tui.theme_colors.lock().unwrap_or_else(|e| e.into_inner())),
        );
    }
}

// RENDERER MODE — selector enum

/// Available frame-renderer backends for the TUI.
///
/// The enum is `Copy` so it can be captured inside closures and passed through
/// channels without fighting borrow-checker friction.
///
/// # Adding a new backend
///
/// 1. Add a variant here (e.g. `Minimal`).
/// 2. Add a `match` arm in `FrameRenderer for RendererMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RendererMode {
    Polished,
    Brutalist,
}

impl RendererMode {
    /// Select a mode based on a boolean flag (`true` → Brutalist).
    pub fn from_brutalist(enabled: bool) -> Self {
        if enabled {
            Self::Brutalist
        } else {
            Self::Polished
        }
    }

    /// Returns `true` if the active backend is `Brutalist`.
    pub fn is_brutalist(self) -> bool {
        matches!(self, Self::Brutalist)
    }

    /// Short human-readable label used in the command palette and status bar.
    pub fn label(self) -> &'static str {
        match self {
            Self::Polished => "polished",
            Self::Brutalist => "brutalist",
        }
    }

    /// Toggle between the two built-in backends.
    pub fn toggled(self) -> Self {
        match self {
            Self::Polished => Self::Brutalist,
            Self::Brutalist => Self::Polished,
        }
    }
}

// FRAME RENDERER TRAIT — dispatch interface

/// Common frame-rendering dispatch interface.
///
/// The enum implementation keeps the active backend inside `TUI` without
/// borrow-checker friction (no `Box<dyn …>` required for the built-in
/// variants).
pub trait FrameRenderer {
    fn render(self, tui: &mut TUI, frame: &mut Frame);
}

impl FrameRenderer for RendererMode {
    fn render(self, tui: &mut TUI, frame: &mut Frame) {
        match self {
            RendererMode::Polished => tui.render_polished(frame),
            RendererMode::Brutalist => tui.render_brutalist(frame),
        }
    }
}

// TESTS

// Bring in the modular render implementations for PolishedRenderer
include!("tui_render_impl.rs");

#[cfg(test)]
mod tests {
    use super::{RendererConfig, RendererLayout, RendererMode};
    use ratatui::layout::Rect;

    #[test]
    fn toggles_between_backends() {
        assert_eq!(RendererMode::Polished.toggled(), RendererMode::Brutalist);
        assert_eq!(RendererMode::Brutalist.toggled(), RendererMode::Polished);
    }

    #[test]
    fn preserves_mode_labels() {
        assert_eq!(RendererMode::Polished.label(), "polished");
        assert_eq!(RendererMode::Brutalist.label(), "brutalist");
    }

    #[test]
    fn from_brutalist_flag() {
        assert_eq!(RendererMode::from_brutalist(true), RendererMode::Brutalist);
        assert_eq!(RendererMode::from_brutalist(false), RendererMode::Polished);
    }

    #[test]
    fn renderer_config_defaults_match_current_layout() {
        let config = RendererConfig::default();
        assert_eq!(config.min_width, 40);
        assert_eq!(config.min_height, 8);
        assert_eq!(config.collapse_chrome_below_height, 12);
        assert_eq!(config.confirmation_width_percent, 60);
        assert_eq!(config.confirmation_height_percent, 20);
    }

    #[test]
    fn renderer_layout_builds_consistent_message_area() {
        let config = RendererConfig::default();
        let layout = RendererLayout::build(Rect::new(0, 0, 80, 24), false, false, &config);

        assert!(!layout.is_too_small);
        assert_eq!(layout.chunks[2].height, 18);
        assert_eq!(layout.chunks[2].width, 80);
    }

    #[test]
    fn renderer_layout_flags_small_terminals() {
        let config = RendererConfig::default();
        let layout = RendererLayout::build(
            Rect::new(0, 0, config.min_width - 1, config.min_height),
            false,
            false,
            &config,
        );

        assert!(layout.is_too_small);
    }
}
