//! Toast notification system for transient messages

use crate::theme::ThemeColors;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::time::{Duration, Instant};

/// Animation phase for toast notifications
///
/// Each toast goes through three phases:
/// 1. **Entering**: Sliding in from the right with fade-in (120ms)
/// 2. **Visible**: Fully displayed (3000ms default)
/// 3. **Exiting**: Sliding out to the right with fade-out (120ms)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastPhase {
    /// Toast is sliding in from the right
    Entering,
    /// Toast is fully visible
    Visible,
    /// Toast is sliding out to the right
    Exiting,
}

impl ToastPhase {
    pub fn duration(&self) -> Duration {
        match self {
            Self::Entering | Self::Exiting => Duration::from_millis(120),
            Self::Visible => Duration::from_secs(3),
        }
    }
}

/// Toast notification level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastLevel {
    /// Informational message
    Info,
    /// Success message
    Success,
    /// Warning message
    Warning,
    /// Error message
    Error,
}

impl ToastLevel {
    /// Get color for this level
    pub fn color(&self) -> Color {
        match self {
            Self::Info => Color::Cyan,
            Self::Success => Color::Green,
            Self::Warning => Color::Yellow,
            Self::Error => Color::Red,
        }
    }

    /// Get icon for this level
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Info => "ℹ",
            Self::Success => "✓",
            Self::Warning => "⚠",
            Self::Error => "✗",
        }
    }
}

/// A toast notification
#[derive(Clone, Debug)]
pub struct Toast {
    /// Notification level
    pub level: ToastLevel,
    /// Message content
    pub message: String,
    /// Optional title
    pub title: Option<String>,
    /// When the toast was created
    pub created_at: Instant,
    /// How long to display the toast (visible phase duration)
    pub duration: Duration,
    /// Unique ID for this toast
    pub id: usize,
    /// Current animation phase
    pub phase: ToastPhase,
    /// When the current phase started
    pub phase_created_at: Instant,
    /// How long has elapsed in the current phase
    pub phase_elapsed_ms: u64,
}

impl Toast {
    const SLIDE_OFFSET_COLS: u16 = 4;

    pub fn new(level: ToastLevel, message: impl Into<String>) -> Self {
        let now = Instant::now();
        Self {
            level,
            message: message.into(),
            title: None,
            created_at: now,
            duration: Duration::from_secs(3),
            id: 0, // Will be set by manager
            phase: ToastPhase::Entering,
            phase_created_at: now,
            phase_elapsed_ms: 0,
        }
    }

    /// Add a title to the toast
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set custom duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.duration
    }

    /// Get remaining time
    pub fn remaining(&self) -> Duration {
        self.duration.saturating_sub(self.created_at.elapsed())
    }

    /// Calculate animation progress (0.0 to 1.0) for current phase
    ///
    /// - **Entering**: 0.0 (hidden) → 1.0 (fully visible)
    /// - **Visible**: Always 1.0
    /// - **Exiting**: 1.0 (visible) → 0.0 (hidden)
    pub fn animation_progress(&self) -> f32 {
        match self.phase {
            ToastPhase::Entering | ToastPhase::Exiting => {
                let duration_ms = self.phase.duration().as_millis() as f32;
                if duration_ms > 0.0 {
                    (self.phase_elapsed_ms as f32 / duration_ms).min(1.0)
                } else {
                    1.0
                }
            }
            ToastPhase::Visible => 1.0,
        }
    }

    /// Calculate horizontal slide offset for animation
    ///
    /// During Entering: Starts at +4 columns (off-screen right), slides to 0
    /// During Exiting: Starts at 0, slides to +4 columns (off-screen right)
    pub fn slide_offset(&self) -> u16 {
        let progress = self.animation_progress();
        let hidden = (1.0 - progress) * Self::SLIDE_OFFSET_COLS as f32;
        hidden.round() as u16
    }

    /// Calculate opacity based on phase and progress
    ///
    /// Returns an alpha value from 0.0 (transparent) to 1.0 (fully visible)
    pub fn opacity(&self) -> f32 {
        self.animation_progress()
    }

    /// Update the toast's animation phase
    ///
    /// Returns true if the toast should be removed (exiting phase complete)
    pub fn tick(&mut self, delta_ms: u64) -> bool {
        self.phase_elapsed_ms += delta_ms;

        // Check if we need to transition to the next phase
        let phase_duration = self.phase.duration();
        if self.phase_elapsed_ms >= phase_duration.as_millis() as u64 {
            match self.phase {
                ToastPhase::Entering => {
                    // Transition from Entering to Visible
                    self.phase = ToastPhase::Visible;
                    self.phase_created_at = Instant::now();
                    self.phase_elapsed_ms = 0;
                }
                ToastPhase::Visible => {
                    // Transition from Visible to Exiting
                    self.phase = ToastPhase::Exiting;
                    self.phase_created_at = Instant::now();
                    self.phase_elapsed_ms = 0;
                }
                ToastPhase::Exiting => {
                    // Exiting complete, remove the toast
                    return true;
                }
            }
        }

        false
    }

    /// Force immediate exit (skip visible phase)
    pub fn dismiss(&mut self) {
        if self.phase == ToastPhase::Entering || self.phase == ToastPhase::Visible {
            self.phase = ToastPhase::Exiting;
            self.phase_created_at = Instant::now();
            self.phase_elapsed_ms = 0;
        }
    }
}

/// Convenience constructors
impl Toast {
    /// Create an info toast
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(ToastLevel::Info, message)
    }

    /// Create a success toast
    pub fn success(message: impl Into<String>) -> Self {
        Self::new(ToastLevel::Success, message)
    }

    /// Create a warning toast
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(ToastLevel::Warning, message)
    }

    /// Create an error toast
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(ToastLevel::Error, message)
    }
}

/// Toast notification manager
///
/// Manages multiple active toasts and handles rendering them.
pub struct ToastManager {
    /// Active toasts
    toasts: Vec<Toast>,
    /// Next toast ID
    next_id: usize,
    /// Maximum number of toasts to show at once
    max_toasts: usize,
}

impl ToastManager {
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 0,
            max_toasts: 3,
        }
    }

    /// Set maximum number of toasts
    pub fn with_max_toasts(mut self, max: usize) -> Self {
        self.max_toasts = max.min(5); // Cap at 5
        self
    }

    /// Add a toast notification
    ///
    /// If the number of active toasts exceeds `max_toasts`, the oldest
    /// visible toast is immediately dismissed (forced into exit animation).
    pub fn add(&mut self, toast: Toast) -> usize {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        // Enforce capacity: force-exit oldest toasts if we'd exceed the cap.
        // We count non-exiting toasts as "visible" slots.
        while self
            .toasts
            .iter()
            .filter(|t| t.phase != ToastPhase::Exiting)
            .count()
            >= self.max_toasts
        {
            if let Some(oldest_visible) = self
                .toasts
                .iter_mut()
                .find(|t| t.phase != ToastPhase::Exiting)
            {
                oldest_visible.dismiss();
            } else {
                break;
            }
        }

        let mut toast = toast;
        toast.id = id;
        self.toasts.push(toast);
        id
    }

    /// Add an info toast
    pub fn info(&mut self, message: impl Into<String>) -> usize {
        self.add(Toast::info(message))
    }

    /// Add a success toast
    pub fn success(&mut self, message: impl Into<String>) -> usize {
        self.add(Toast::success(message))
    }

    /// Add a warning toast
    pub fn warning(&mut self, message: impl Into<String>) -> usize {
        self.add(Toast::warning(message))
    }

    /// Add an error toast
    pub fn error(&mut self, message: impl Into<String>) -> usize {
        self.add(Toast::error(message))
    }

    /// Remove a toast by ID
    pub fn remove(&mut self, id: usize) -> bool {
        if let Some(pos) = self.toasts.iter().position(|t| t.id == id) {
            self.toasts.remove(pos);
            true
        } else {
            false
        }
    }

    /// Clear all toasts
    pub fn clear(&mut self) {
        self.toasts.clear();
    }

    /// Remove expired toasts
    pub fn cleanup(&mut self) {
        self.toasts.retain(|t| !t.is_expired());
    }

    /// Update all toast animations
    ///
    /// Call this every frame with delta time in milliseconds
    /// Returns true if any toasts are still active
    pub fn tick(&mut self, delta_ms: u64) -> bool {
        // Update each toast and remove those that are done
        self.toasts.retain(|t| {
            // Keep toasts that haven't completed their exit animation
            t.phase != ToastPhase::Exiting
                || t.phase_elapsed_ms < t.phase.duration().as_millis() as u64
        });

        // Tick each toast and track if any are still animating
        for toast in &mut self.toasts {
            toast.tick(delta_ms);
        }

        self.has_active()
    }

    /// Get active toasts
    pub fn active(&self) -> &[Toast] {
        &self.toasts
    }

    pub fn has_active(&self) -> bool {
        !self.toasts.is_empty()
    }

    /// Render all active toasts with animations
    pub fn render(&self, frame: &mut Frame, area: Rect, theme_colors: Option<&ThemeColors>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        // Get active toasts (exclude those that have completed exit animation)
        let toasts: Vec<_> = self
            .toasts
            .iter()
            .filter(|t| {
                t.phase != ToastPhase::Exiting
                    || t.phase_elapsed_ms < t.phase.duration().as_millis() as u64
            })
            .rev() // Show newest first
            .take(self.max_toasts)
            .collect();

        if toasts.is_empty() {
            return;
        }

        // Render each toast stacked vertically with slide animation
        let toast_height = 4;
        let toast_width = 60.min(area.width.saturating_sub(4));
        if toast_width < 6 {
            return; // Terminal too narrow for meaningful toast
        }

        for (i, toast) in toasts.iter().enumerate() {
            let input_area_reserve = 5u16;
            let base_y = area
                .height
                .saturating_sub(toast_height * (i + 1) as u16)
                .saturating_sub(2)
                .saturating_sub(input_area_reserve);
            let base_x = area.width.saturating_sub(toast_width).saturating_sub(2);

            // Apply slide offset (animation)
            let slide_offset = toast.slide_offset();
            let x = base_x.saturating_add(slide_offset);
            let y = base_y;

            if y < toast_height {
                break; // Not enough space
            }

            // Skip rendering if toast has completely slid off-screen
            if x > area.width {
                continue;
            }

            // Clamp toast area to stay within the frame buffer
            let clamped_width = toast_width.min(area.width.saturating_sub(x));
            let clamped_height = toast_height.min(area.height.saturating_sub(y));
            if clamped_width == 0 || clamped_height == 0 {
                continue;
            }

            let toast_area = Rect::new(x, y, clamped_width, clamped_height);
            self.render_toast(frame, toast_area, toast, theme_colors);
        }
    }

    /// Render a single toast with opacity for fade effects
    fn render_toast(
        &self,
        frame: &mut Frame,
        area: Rect,
        toast: &Toast,
        theme_colors: Option<&ThemeColors>,
    ) {
        frame.render_widget(Clear, area);

        let color = theme_colors
            .map(|tc| tc.toast_level_color(&toast.level))
            .unwrap_or_else(|| toast.level.color());
        let opacity = toast.opacity();

        // Skip rendering if toast is invisible
        if opacity <= 0.01 {
            return;
        }

        let icon = toast.level.icon();

        let title = toast.title.as_deref().unwrap_or(match toast.level {
            ToastLevel::Info => "Information",
            ToastLevel::Success => "Success",
            ToastLevel::Warning => "Warning",
            ToastLevel::Error => "Error",
        });

        // Apply opacity to colors
        let apply_opacity = |c: Color| -> Color {
            // Note: ratatui doesn't support true alpha blending,
            // so we simulate opacity by dimming the color
            match c {
                Color::Rgb(r, g, b) => {
                    let factor = opacity;
                    Color::Rgb(
                        (r as f32 * factor).round() as u8,
                        (g as f32 * factor).round() as u8,
                        (b as f32 * factor).round() as u8,
                    )
                }
                _ => c, // Can't easily dim named colors
            }
        };

        let dimmed_color = apply_opacity(color);
        let dimmed_gray = apply_opacity(theme_colors.map(|tc| tc.muted).unwrap_or(Color::Gray));

        let lines = vec![
            Line::from(vec![
                Span::styled(icon, Style::default().fg(dimmed_color)),
                Span::raw(" "),
                Span::styled(
                    title,
                    Style::default()
                        .fg(dimmed_color)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                &toast.message,
                Style::default().fg(dimmed_gray),
            )]),
        ];

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(dimmed_color)),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_creation() {
        let toast = Toast::info("Test message");
        assert_eq!(toast.level, ToastLevel::Info);
        assert_eq!(toast.message, "Test message");
        assert!(toast.title.is_none());
    }

    #[test]
    fn test_toast_with_title() {
        let toast = Toast::success("Success!").with_title("Operation Complete");
        assert_eq!(toast.title, Some("Operation Complete".to_string()));
    }

    #[test]
    fn test_toast_expiration() {
        let toast = Toast::warning("Warning").with_duration(Duration::from_millis(500));

        assert!(!toast.is_expired());
        std::thread::sleep(Duration::from_millis(600));
        assert!(toast.is_expired());
    }

    #[test]
    fn test_toast_levels() {
        let info = Toast::info("Info");
        assert_eq!(info.level, ToastLevel::Info);
        assert_eq!(info.level.color(), Color::Cyan);

        let success = Toast::success("Success");
        assert_eq!(success.level, ToastLevel::Success);
        assert_eq!(success.level.color(), Color::Green);

        let warning = Toast::warning("Warning");
        assert_eq!(warning.level, ToastLevel::Warning);
        assert_eq!(warning.level.color(), Color::Yellow);

        let error = Toast::error("Error");
        assert_eq!(error.level, ToastLevel::Error);
        assert_eq!(error.level.color(), Color::Red);
    }

    #[test]
    fn test_toast_manager_new() {
        let manager = ToastManager::new();
        assert!(!manager.has_active());
        assert_eq!(manager.active().len(), 0);
    }

    #[test]
    fn test_toast_manager_add() {
        let mut manager = ToastManager::new();
        manager.info("Test");

        assert!(manager.has_active());
        assert_eq!(manager.active().len(), 1);
    }

    #[test]
    fn test_toast_manager_multiple() {
        let mut manager = ToastManager::new();
        manager.info("First");
        manager.success("Second");
        manager.warning("Third");

        assert_eq!(manager.active().len(), 3);
    }

    #[test]
    fn test_toast_manager_remove() {
        let mut manager = ToastManager::new();
        let id = manager.info("Test");
        assert!(manager.has_active());

        assert!(manager.remove(id));
        assert!(!manager.has_active());
    }

    #[test]
    fn test_toast_manager_clear() {
        let mut manager = ToastManager::new();
        manager.info("First");
        manager.success("Second");

        manager.clear();
        assert!(!manager.has_active());
    }

    #[test]
    fn test_toast_manager_cleanup() {
        let mut manager = ToastManager::new();
        manager.info("First");
        manager.add(Toast::warning("Second").with_duration(Duration::from_millis(10)));

        std::thread::sleep(Duration::from_millis(20));
        manager.cleanup();

        assert_eq!(manager.active().len(), 1);
    }

    #[test]
    fn test_toast_manager_max_toasts() {
        let mut manager = ToastManager::new().with_max_toasts(2);

        manager.info("First");
        manager.success("Second");
        manager.warning("Third");

        // Should still have all 3 in internal storage
        assert_eq!(manager.active().len(), 3);
    }

    #[test]
    fn test_toast_manager_convenience() {
        let mut manager = ToastManager::new();

        manager.info("Info");
        manager.success("Success");
        manager.warning("Warning");
        manager.error("Error");

        assert_eq!(manager.active().len(), 4);
    }

    /// Regression: toast rendering panics or renders garbage on very narrow terminals
    /// (width < 6). Added guard `if toast_width < 6 { return; }`.
    #[test]
    fn test_toast_no_panic_on_narrow_terminal() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut manager = ToastManager::new();
        manager.info("Test message");

        let backend = TestBackend::new(3, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                manager.render(f, f.area(), None);
            })
            .unwrap();
    }

    /// Regression: ensure toast renders normally on a standard-width terminal.
    #[test]
    fn test_toast_render_normal_width() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut manager = ToastManager::new();
        manager.success("All good");

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                manager.render(f, f.area(), None);
            })
            .unwrap();
    }
}
