//! Help System for TUI
//!
//! Comprehensive help system with:
//! - Categorized help topics
//! - Keyboard shortcuts reference
//! - Slash commands documentation
//! - Contextual help based on current mode

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub mod topics;

/// Help topic category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HelpCategory {
    /// Keyboard navigation shortcuts
    Navigation,
    /// Text editing and input
    Editing,
    /// Tool execution and management
    Tools,
    /// Slash commands
    Commands,
    /// Configuration and settings
    Settings,
}

/// Help topic with title and content
#[derive(Debug, Clone)]
pub struct HelpTopic {
    pub title: String,
    pub category: HelpCategory,
    pub content: String,
    pub key_bindings: Vec<String>,
}

/// Help UI state
pub struct HelpState {
    pub visible: bool,
    pub selected_category: Option<HelpCategory>,
    pub search_query: String,
    pub scroll_offset: usize,
}

impl HelpState {
    pub fn new() -> Self {
        Self {
            visible: false,
            selected_category: None,
            search_query: String::new(),
            scroll_offset: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }
}

impl Default for HelpState {
    fn default() -> Self {
        Self::new()
    }
}

/// Get all help topics
pub fn help_topics() -> Vec<HelpTopic> {
    topics::all_topics()
}

/// Filter topics by category and search query
pub fn filter_topics(
    topics: &[HelpTopic],
    category: Option<HelpCategory>,
    query: &str,
) -> Vec<HelpTopic> {
    topics
        .iter()
        .filter(|topic| {
            let category_match = category.is_none_or(|c| topic.category == c);
            let query_match = query.is_empty()
                || topic.title.to_lowercase().contains(&query.to_lowercase())
                || topic.content.to_lowercase().contains(&query.to_lowercase());
            category_match && query_match
        })
        .cloned()
        .collect()
}

/// Render help UI as a centered bordered dialog
pub fn render_help(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, state: &HelpState) {
    // Only render if help is visible
    if !state.visible {
        return;
    }

    let topics = help_topics();
    let filtered = filter_topics(&topics, state.selected_category, &state.search_query);

    // Calculate dialog dimensions (80% of screen, centered)
    // Clamp to screen bounds to prevent overflow on small terminals
    let dialog_width = (area.width * 4 / 5).clamp(30, area.width.saturating_sub(2).min(100));
    let dialog_height = (area.height * 4 / 5).clamp(8, area.height.saturating_sub(2).min(40));
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = ratatui::layout::Rect {
        x: dialog_x,
        y: dialog_y,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the dialog area first
    frame.render_widget(ratatui::widgets::Clear, dialog_area);

    if filtered.is_empty() {
        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Line::from(vec![
                Span::styled(
                    " Help ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
                Span::styled(
                    "— press Esc to close ",
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        let paragraph = Paragraph::new("No help topics found.")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(paragraph, dialog_area);
        return;
    }

    // Group topics by category for organized display
    let mut lines: Vec<Line> = Vec::new();
    let mut current_category: Option<HelpCategory> = None;

    for topic in &filtered {
        // Add category header when category changes
        if current_category != Some(topic.category) {
            current_category = Some(topic.category);
            if !lines.is_empty() {
                lines.push(Line::raw("")); // Spacer between categories
            }
            let (cat_name, cat_color) = match topic.category {
                HelpCategory::Navigation => ("Navigation", Color::Cyan),
                HelpCategory::Editing => ("Editing", Color::Green),
                HelpCategory::Tools => ("Tools", Color::Yellow),
                HelpCategory::Commands => ("Commands", Color::Magenta),
                HelpCategory::Settings => ("Settings", Color::Gray),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", cat_name.to_uppercase()),
                    Style::default()
                        .fg(cat_color)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
                Span::styled(
                    "─".repeat(dialog_width as usize / 2),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        // Format: "  title  key1, key2"
        let keys_str = topic.key_bindings.join(", ");
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(&topic.title, Style::default().fg(Color::White)),
            Span::raw("  "),
            Span::styled(keys_str, Style::default().fg(Color::DarkGray)),
        ]));
    }

    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Line::from(vec![
            Span::styled(
                " Help ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(
                if lines.len() > dialog_height as usize - 2 {
                    " ↑↓ scroll · Esc close "
                } else {
                    " Esc to close "
                },
                Style::default().fg(Color::DarkGray),
            ),
        ]));

    // Scroll: skip lines above scroll offset, show only what fits
    let inner_height = dialog_height.saturating_sub(2) as usize; // subtract borders
    let max_scroll = lines.len().saturating_sub(inner_height);
    let scroll_offset = state.scroll_offset.min(max_scroll);
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(scroll_offset)
        .take(inner_height)
        .collect();

    let paragraph = Paragraph::new(visible_lines)
        .block(block)
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(paragraph, dialog_area);
}
