//! Thinking block rendering for AI reasoning display
//!
//! This module provides rendering for thinking/reasoning blocks that show
//! the AI's internal thought process, with expandable sections.
//!
//! Note: The main thinking rendering is now integrated directly into
//! `app/render/messages.rs` as `render_thinking_block()`. This module
//! is kept for the `MessageRenderer`-based rendering path.

use anyhow::Result as anyhowResult;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    Frame,
};

/// Render thinking block header
///
/// # Arguments
///
/// * `f` - The ratatui frame for rendering
/// * `area` - The area to render in
/// * `thinking` - The thinking content
/// * `pipe` - The pipe character for visual consistency
/// * `color` - The color for styling
/// * `theme` - The message theme
pub fn render_thinking_header(
    f: &mut Frame,
    area: Rect,
    thinking: &str,
    pipe: char,
    color: Color,
    theme: &super::MessageTheme,
) -> anyhowResult<()> {
    let size = thinking.len();

    // Format size
    let size_str = if size < 1024 {
        format!("{}b", size)
    } else {
        format!("{:.1}kb", size as f64 / 1024.0)
    };

    // Build header line
    let header_text = format!("💭 [thinking] {} [▾ show]", size_str);

    let header_line = Line::from(vec![
        Span::styled(format!("{} ", pipe), Style::default().fg(color)),
        Span::styled(header_text, Style::default().fg(theme.thinking_color)),
    ]);

    let paragraph = ratatui::widgets::Paragraph::new(header_line);
    f.render_widget(paragraph, area);

    Ok(())
}

/// Render expanded thinking content with border
///
/// # Arguments
///
/// * `f` - The ratatui frame for rendering
/// * `area` - The area to render in
/// * `thinking` - The thinking content
/// * `pipe` - The pipe character for visual consistency
/// * `color` - The color for styling
/// * `theme` - The message theme
pub fn render_thinking_content(
    f: &mut Frame,
    area: Rect,
    thinking: &str,
    pipe: char,
    color: Color,
    theme: &super::MessageTheme,
) -> anyhowResult<()> {
    // Build lines
    let mut lines = vec![];

    // Add border line
    lines.push(Line::from(vec![
        Span::styled(format!("{} ┌", pipe), Style::default().fg(color)),
        Span::styled(
            "─".repeat(area.width.saturating_sub(4) as usize),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("┐", Style::default().fg(Color::DarkGray)),
    ]));

    // Add thinking content (truncated and wrapped)
    let max_lines = 8;
    for line in thinking.lines().take(max_lines) {
        lines.push(Line::from(vec![
            Span::styled(format!("{} │ ", pipe), Style::default().fg(color)),
            Span::styled(
                line.to_string(),
                Style::default().fg(theme.thinking_text_color),
            ),
        ]));
    }

    // Add border line
    lines.push(Line::from(vec![
        Span::styled(format!("{} └", pipe), Style::default().fg(color)),
        Span::styled(
            "─".repeat(area.width.saturating_sub(4) as usize),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("┘", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = ratatui::widgets::Paragraph::new(lines);
    f.render_widget(paragraph, area);

    Ok(())
}

/// Calculate the height needed for thinking block rendering
///
/// # Arguments
///
/// * `has_thinking` - Whether there is thinking content
/// * `is_expanded` - Whether the thinking block is expanded
///
/// # Returns
///
/// The number of lines needed for rendering
pub fn calculate_thinking_height(has_thinking: bool, is_expanded: bool) -> usize {
    if !has_thinking {
        return 0;
    }

    if is_expanded {
        // Header + top border + content (max 8 lines) + bottom border
        1 + 1 + 8 + 1
    } else {
        // Just header
        1
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_terminal() -> (ratatui::Terminal<ratatui::backend::TestBackend>, Rect) {
        // Create a minimal test backend
        let backend = ratatui::backend::TestBackend::new(80, 20);
        let terminal = ratatui::Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, 80, 10);
        (terminal, area)
    }

    #[test]
    fn test_render_thinking_header() {
        let (mut terminal, area) = create_test_terminal();
        let theme = super::super::MessageTheme::default();
        let thinking = "Some thinking content";

        let result = terminal.draw(|f| {
            render_thinking_header(f, area, thinking, '│', Color::Blue, &theme).ok();
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_thinking_content() {
        let (mut terminal, area) = create_test_terminal();
        let theme = super::super::MessageTheme::default();
        let thinking = "Line 1\nLine 2\nLine 3";

        let result = terminal.draw(|f| {
            render_thinking_content(f, area, thinking, '│', Color::Blue, &theme).ok();
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_thinking_content_empty() {
        let (mut terminal, area) = create_test_terminal();
        let theme = super::super::MessageTheme::default();

        let result = terminal.draw(|f| {
            render_thinking_content(f, area, "", '│', Color::Blue, &theme).ok();
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_thinking_content_long() {
        let (mut terminal, area) = create_test_terminal();
        let theme = super::super::MessageTheme::default();
        let long_thinking = (0..20)
            .map(|i| format!("Line {}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let result = terminal.draw(|f| {
            render_thinking_content(f, area, &long_thinking, '│', Color::Blue, &theme).ok();
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_calculate_thinking_height_none() {
        let height = calculate_thinking_height(false, false);
        assert_eq!(height, 0);
    }

    #[test]
    fn test_calculate_thinking_height_collapsed() {
        let height = calculate_thinking_height(true, false);
        assert_eq!(height, 1);
    }

    #[test]
    fn test_calculate_thinking_height_expanded() {
        let height = calculate_thinking_height(true, true);
        assert_eq!(height, 11); // 1 + 1 + 8 + 1
    }

    #[test]
    fn test_calculate_thinking_height_no_content_expanded() {
        let height = calculate_thinking_height(false, true);
        assert_eq!(height, 0);
    }
}
