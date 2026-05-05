//! Live theme preview panel with fuzzy search and instant switching.

use crate::theme::{builtin_themes, is_dark_theme, parse_color, Theme, ThemeColors};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::sync::{Arc, Mutex};

// THEME PREVIEW STATE

/// Theme preview state
#[derive(Debug, Clone)]
pub struct ThemePreviewState {
    /// Available themes
    pub themes: Vec<Theme>,

    /// Currently selected theme index
    pub selected_index: usize,

    /// Whether the preview is visible
    pub visible: bool,

    /// Search query
    pub query: String,

    /// Filtered theme indices
    pub filtered_indices: Vec<usize>,

    /// Whether we're in live preview mode (temporarily showing preview)
    pub live_preview: bool,

    /// The theme being previewed (if in live preview mode)
    pub preview_theme: Option<Theme>,
}

impl ThemePreviewState {
    /// Create new theme preview state
    pub fn new() -> Self {
        let themes = builtin_themes();
        let filtered_indices = (0..themes.len()).collect();

        Self {
            themes,
            selected_index: 0,
            visible: false,
            query: String::new(),
            filtered_indices,
            live_preview: false,
            preview_theme: None,
        }
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.query.clear();
        self.selected_index = 0;
        self.update_filtered();
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.query.clear();
        self.live_preview = false;
        self.preview_theme = None;
    }

    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    fn update_filtered(&mut self) {
        self.filtered_indices = if self.query.is_empty() {
            (0..self.themes.len()).collect()
        } else {
            let query_lower = self.query.to_lowercase();
            self.themes
                .iter()
                .enumerate()
                .filter(|(_, t)| t.name.to_lowercase().contains(&query_lower))
                .map(|(idx, _)| idx)
                .collect()
        };

        if self.filtered_indices.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self
                .selected_index
                .min(self.filtered_indices.len().saturating_sub(1));
        }
    }

    pub fn selected_theme(&self) -> Option<&Theme> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|&idx| self.themes.get(idx))
    }

    pub fn insert_char(&mut self, c: char) {
        self.query.push(c);
        self.update_filtered();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.update_filtered();
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.update_filtered();
    }

    pub fn move_up(&mut self) {
        if !self.filtered_indices.is_empty() && self.selected_index > 0 {
            self.selected_index -= 1;
            self.update_live_preview();
        }
    }

    pub fn move_down(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.filtered_indices.len() - 1);
            self.update_live_preview();
        }
    }

    /// Update live preview theme when selection changes
    fn update_live_preview(&mut self) {
        if self.live_preview {
            self.preview_theme = self.selected_theme().cloned();
        }
    }

    /// Enable live preview mode
    pub fn enable_live_preview(&mut self) {
        self.live_preview = true;
        self.update_live_preview();
    }

    /// Disable live preview mode
    pub fn disable_live_preview(&mut self) {
        self.live_preview = false;
        self.preview_theme = None;
    }

    /// Get number of filtered themes
    pub fn filtered_count(&self) -> usize {
        self.filtered_indices.len()
    }
}

impl Default for ThemePreviewState {
    fn default() -> Self {
        Self::new()
    }
}

// THEME PREVIEW RENDERER

/// Theme preview renderer
pub struct ThemePreviewRenderer {
    /// Visual state
    state: ThemePreviewState,
    /// Current theme colors (for actual application)
    theme_colors: Arc<Mutex<ThemeColors>>,
    /// Original theme (before preview)
    original_theme: Option<Theme>,
}

impl ThemePreviewRenderer {
    pub fn new(theme_colors: Arc<Mutex<ThemeColors>>) -> Self {
        Self {
            state: ThemePreviewState::new(),
            theme_colors,
            original_theme: None,
        }
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut ThemePreviewState {
        &mut self.state
    }

    pub fn state(&self) -> &ThemePreviewState {
        &self.state
    }

    pub fn show(&mut self) {
        self.state.show();
    }

    /// Restores the original theme before preview, then hides.
    pub fn hide(&mut self) {
        // Restore original theme if we were in preview mode
        if self.original_theme.is_some() {
            if let Some(original) = &self.original_theme {
                let colors = ThemeColors::from(original);
                *self.theme_colors.lock().unwrap_or_else(|e| e.into_inner()) = colors;
            }
            self.original_theme = None;
        }
        self.state.hide();
    }

    /// Apply the selected theme
    pub fn apply_selected_theme(&mut self) -> Result<(), String> {
        if let Some(theme) = self.state.selected_theme() {
            let colors = ThemeColors::from(theme);
            *self.theme_colors.lock().unwrap_or_else(|e| e.into_inner()) = colors;
            self.original_theme = Some(theme.clone());
            Ok(())
        } else {
            Err("No theme selected".to_string())
        }
    }

    /// Handle a key event
    ///
    /// Returns true if the event was handled
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            // Close preview on Escape
            (KeyCode::Esc, KeyModifiers::NONE) => {
                self.hide();
                true
            }

            // Navigate up
            (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
                self.state.move_up();
                // Apply theme for live preview
                if self.state.live_preview {
                    let _ = self.apply_selected_theme();
                }
                true
            }

            // Navigate down
            (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
                self.state.move_down();
                // Apply theme for live preview
                if self.state.live_preview {
                    let _ = self.apply_selected_theme();
                }
                true
            }

            // Select and apply theme on Enter
            (KeyCode::Enter, KeyModifiers::NONE) => {
                if let Some(theme) = self.state.selected_theme() {
                    let colors = ThemeColors::from(theme);
                    *self.theme_colors.lock().unwrap_or_else(|e| e.into_inner()) = colors;
                    self.original_theme = Some(theme.clone());
                }
                self.hide();
                true
            }

            // Toggle live preview with 'p'
            (KeyCode::Char('p'), KeyModifiers::NONE) => {
                if self.state.live_preview {
                    self.state.disable_live_preview();
                    // Restore original
                    if let Some(original) = &self.original_theme {
                        let colors = ThemeColors::from(original);
                        *self.theme_colors.lock().unwrap_or_else(|e| e.into_inner()) = colors;
                    }
                } else {
                    // Save current as original
                    if self.original_theme.is_none() {
                        self.original_theme = self.state.selected_theme().cloned();
                    }
                    self.state.enable_live_preview();
                    let _ = self.apply_selected_theme();
                }
                true
            }

            // Typing characters
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.state.insert_char(c);
                true
            }

            // Backspace
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                self.state.backspace();
                true
            }

            // Clear query on Ctrl+U
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.state.clear_query();
                true
            }

            _ => false,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        if !self.state.visible {
            return;
        }

        // Calculate modal size (70% width, 60% height)
        let width = (area.width * 70) / 100;
        let height = (area.height * 60) / 100;

        // Center the modal
        let x = area.x + (area.width - width) / 2;
        let y = area.y + (area.height - height) / 2;
        let modal_area = Rect::new(x, y, width, height);

        // Clear the area behind the modal
        f.render_widget(Clear, modal_area);

        // Split into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(3), // Header
                    Constraint::Length(8), // Preview area
                    Constraint::Min(0),    // Theme list
                    Constraint::Length(2), // Footer
                ]
                .as_ref(),
            )
            .split(modal_area);

        // Render header
        self.render_header(f, chunks[0]);

        // Render preview
        self.render_preview(f, chunks[1]);

        // Render theme list
        self.render_theme_list(f, chunks[2]);

        // Render footer
        self.render_footer(f, chunks[3]);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let title = if self.state.live_preview {
            "Theme Preview (Live)"
        } else {
            "Theme Preview"
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled("Theme: ", Style::default().fg(Color::Gray)),
            Span::styled(
                &self.state.query,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title)
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .wrap(Wrap { trim: false });

        f.render_widget(header, area);
    }

    fn render_preview(&self, f: &mut Frame, area: Rect) {
        let theme = self.state.selected_theme();

        if let Some(theme) = theme {
            let bg = parse_color(&theme.colors.background);
            let fg = parse_color(&theme.colors.foreground);
            let primary = parse_color(&theme.colors.primary);
            let secondary = parse_color(&theme.colors.secondary);
            let accent = parse_color(&theme.colors.accent);
            let success = parse_color(&theme.colors.success);
            let warning = parse_color(&theme.colors.warning);
            let error = parse_color(&theme.colors.error);

            // Create preview using the theme's colors
            let preview_content = vec![
                Line::from(vec![
                    Span::styled("Primary: ", Style::default().fg(fg)),
                    Span::styled(
                        "Header & Title Text",
                        Style::default().fg(primary).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Accent: ", Style::default().fg(fg)),
                    Span::styled("Buttons & Highlights", Style::default().fg(accent)),
                ]),
                Line::from(vec![
                    Span::styled("Success: ", Style::default().fg(fg)),
                    Span::styled("Good news completed", Style::default().fg(success)),
                ]),
                Line::from(vec![
                    Span::styled("Warning: ", Style::default().fg(fg)),
                    Span::styled("Warning message", Style::default().fg(warning)),
                ]),
                Line::from(vec![
                    Span::styled("Error: ", Style::default().fg(fg)),
                    Span::styled("Error message", Style::default().fg(error)),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("Code sample: ", Style::default().fg(fg)),
                    Span::styled("fn main() ", Style::default().fg(primary)),
                    Span::styled("{ ", Style::default().fg(fg)),
                    Span::styled("println!", Style::default().fg(accent)),
                    Span::styled("(\"", Style::default().fg(fg)),
                    Span::styled("Hello", Style::default().fg(success)),
                    Span::styled("\"); }", Style::default().fg(fg)),
                ]),
            ];

            let preview = Paragraph::new(preview_content)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(secondary))
                        .title("Preview")
                        .title_style(Style::default().fg(secondary)),
                )
                .style(Style::default().fg(fg).bg(bg))
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Left);

            f.render_widget(preview, area);
        } else {
            let no_preview = Paragraph::new("No theme selected")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray))
                        .title("Preview"),
                )
                .alignment(Alignment::Center);

            f.render_widget(no_preview, area);
        }
    }

    /// Render the theme list
    fn render_theme_list(&self, f: &mut Frame, area: Rect) {
        if self.state.filtered_indices.is_empty() {
            let no_results = Paragraph::new(Line::from(vec![Span::styled(
                "No themes found",
                Style::default().fg(Color::DarkGray),
            )]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .alignment(Alignment::Center);

            f.render_widget(no_results, area);
            return;
        }

        let items: Vec<ListItem> = self
            .state
            .filtered_indices
            .iter()
            .map(|&idx| {
                let theme = &self.state.themes[idx];
                let is_dark = is_dark_theme(&theme.colors.background);
                let icon = if is_dark { "🌙" } else { "☀️" };

                let name_line = Line::from(vec![
                    Span::raw(icon.to_string() + " "),
                    Span::styled(&theme.name, Style::default().fg(Color::White)),
                ]);

                let colors_line = Line::from(vec![
                    Span::styled(
                        "● ",
                        Style::default().fg(parse_color(&theme.colors.primary)),
                    ),
                    Span::styled(
                        "● ",
                        Style::default().fg(parse_color(&theme.colors.success)),
                    ),
                    Span::styled("● ", Style::default().fg(parse_color(&theme.colors.error))),
                    Span::styled("● ", Style::default().fg(parse_color(&theme.colors.accent))),
                ]);

                ListItem::new(vec![name_line, colors_line])
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        let mut list_state = ListState::default();
        list_state.select(Some(self.state.selected_index));

        f.render_stateful_widget(list, area, &mut list_state);
    }

    /// Render the footer with keybinding hints
    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let live_indicator = if self.state.live_preview {
            " [LIVE]"
        } else {
            ""
        };

        let footer = Paragraph::new(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(Color::Cyan)),
            Span::styled(" Navigate ", Style::default().fg(Color::Gray)),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::styled(" Apply ", Style::default().fg(Color::Gray)),
            Span::styled("P", Style::default().fg(Color::Cyan)),
            Span::raw(format!(" Live Preview{}", live_indicator)),
            Span::raw("  "),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::styled(" Close ", Style::default().fg(Color::Gray)),
        ]))
        .alignment(Alignment::Center);

        f.render_widget(footer, area);
    }
}

// HIGH-LEVEL API

/// High-level theme preview API
pub struct ThemePreview {
    /// Renderer with embedded state
    renderer: ThemePreviewRenderer,
}

impl ThemePreview {
    pub fn new(theme_colors: Arc<Mutex<ThemeColors>>) -> Self {
        Self {
            renderer: ThemePreviewRenderer::new(theme_colors),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.renderer.state().visible
    }

    pub fn is_live_preview(&self) -> bool {
        self.renderer.state().live_preview
    }

    pub fn show(&mut self) {
        self.renderer.show();
    }

    pub fn hide(&mut self) {
        self.renderer.hide();
    }

    pub fn toggle(&mut self) {
        if self.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        self.renderer.handle_key(key)
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        self.renderer.render(f, area);
    }

    pub fn state_mut(&mut self) -> &mut ThemePreviewState {
        self.renderer.state_mut()
    }

    pub fn state(&self) -> &ThemePreviewState {
        self.renderer.state()
    }

    pub fn selected_theme(&self) -> Option<&Theme> {
        self.renderer.state().selected_theme()
    }
}

// THEME SWITCHER (SIMPLIFIED API)

/// Quick theme cycler without a full preview UI.
pub struct ThemeSwitcher {
    themes: Vec<Theme>,
    current_index: usize,
    theme_colors: Arc<Mutex<ThemeColors>>,
}

impl ThemeSwitcher {
    pub fn new(theme_colors: Arc<Mutex<ThemeColors>>) -> Self {
        let themes = builtin_themes();
        Self {
            themes,
            current_index: 0,
            theme_colors,
        }
    }

    pub fn next_theme(&mut self) -> Option<&Theme> {
        if self.themes.is_empty() {
            return None;
        }
        self.current_index = (self.current_index + 1) % self.themes.len();
        self.apply_current();
        Some(&self.themes[self.current_index])
    }

    pub fn prev(&mut self) -> Option<&Theme> {
        if self.themes.is_empty() {
            return None;
        }
        self.current_index = if self.current_index == 0 {
            self.themes.len() - 1
        } else {
            self.current_index - 1
        };
        self.apply_current();
        Some(&self.themes[self.current_index])
    }

    pub fn switch_to(&mut self, name: &str) -> Option<&Theme> {
        if let Some(idx) = self.themes.iter().position(|t| t.name == name) {
            self.current_index = idx;
            self.apply_current();
            Some(&self.themes[self.current_index])
        } else {
            None
        }
    }

    pub fn current(&self) -> Option<&Theme> {
        self.themes.get(self.current_index)
    }

    pub fn all_themes(&self) -> &[Theme] {
        &self.themes
    }

    fn apply_current(&self) {
        if let Some(theme) = self.themes.get(self.current_index) {
            let colors = ThemeColors::from(theme);
            *self.theme_colors.lock().unwrap_or_else(|e| e.into_inner()) = colors;
        }
    }
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_preview_state() {
        let state = ThemePreviewState::new();
        assert!(!state.visible);
        assert_eq!(state.selected_index, 0);
        assert!(state.filtered_count() > 0);
    }

    #[test]
    fn test_theme_preview_navigation() {
        let mut state = ThemePreviewState::new();
        let _count = state.filtered_count();

        state.move_down();
        assert_eq!(state.selected_index, 1);

        state.move_up();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_theme_preview_filter() {
        let mut state = ThemePreviewState::new();

        state.insert_char('d');
        assert_eq!(state.query, "d");
        assert!(state.filtered_count() > 0);

        state.backspace();
        assert_eq!(state.query, "");
    }

    #[test]
    fn test_theme_switcher() {
        let theme_colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let mut switcher = ThemeSwitcher::new(theme_colors);

        let first_name = switcher.current().unwrap().name.clone();
        let next = switcher.next_theme().unwrap();
        assert_ne!(first_name, next.name);

        let prev = switcher.prev().unwrap();
        assert_eq!(first_name, prev.name);
    }

    #[test]
    fn test_theme_switcher_by_name() {
        let theme_colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let mut switcher = ThemeSwitcher::new(theme_colors);

        let result = switcher.switch_to("dracula");
        assert!(result.is_some());
        assert_eq!(result.unwrap().name, "dracula");

        let result = switcher.switch_to("nonexistent");
        assert!(result.is_none());
    }
}
