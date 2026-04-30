//! Image preview rendering for message attachments
//!
//! This module provides rendering for image attachments with ASCII previews
//! and support for multiple images in a grid layout.

// Complete implementation - pending integration with message rendering flow
#![allow(dead_code)]

use super::message_types::ImageAttachment;
use anyhow::Result as anyhowResult;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    Frame,
};

/// Render image previews header
///
/// # Arguments
///
/// * `f` - The ratatui frame for rendering
/// * `area` - The area to render in
/// * `image_count` - Number of images to display
/// * `pipe` - The pipe character for visual consistency
/// * `color` - The color for styling
pub fn render_image_header(
    f: &mut Frame,
    area: Rect,
    image_count: usize,
    pipe: char,
    color: Color,
) -> anyhowResult<()> {
    // Build header line
    let header_line = Line::from(vec![
        Span::styled(format!("{} ", pipe), Style::default().fg(color)),
        Span::styled(
            format!("[IMG] {} image(s) attached", image_count),
            Style::default().fg(Color::Cyan),
        ),
    ]);

    // Render header
    let header_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    f.render_widget(ratatui::widgets::Paragraph::new(header_line), header_area);

    Ok(())
}

/// Render a single image preview
///
/// # Arguments
///
/// * `f` - The ratatui frame for rendering
/// * `area` - The area to render in
/// * `img` - The image attachment to render
/// * `pipe` - The pipe character for visual consistency
/// * `color` - The color for styling
pub fn render_single_image_preview(
    f: &mut Frame,
    area: Rect,
    img: &ImageAttachment,
    pipe: char,
    color: Color,
) -> anyhowResult<()> {
    let mut lines = vec![];

    // Title with filename and remove option
    let filename = img
        .path
        .as_ref()
        .and_then(|p| std::path::Path::new(p).file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("image");

    let title = format!("📷 {} [x] remove", filename);
    let title_line = Line::from(vec![
        Span::styled(format!("{} ", pipe), Style::default().fg(color)),
        Span::styled(title, Style::default().fg(Color::Cyan)),
    ]);
    lines.push(title_line);

    // ASCII preview (if available)
    if let Some(ref preview) = img.preview {
        for preview_line in preview.lines().take(6) {
            lines.push(Line::from(vec![
                Span::styled(format!("{} │ ", pipe), Style::default().fg(color)),
                Span::styled(preview_line, Style::default().fg(Color::White)),
            ]));
        }
    } else {
        // Placeholder
        for _ in 0..6 {
            lines.push(Line::from(vec![
                Span::styled(format!("{} │ ", pipe), Style::default().fg(color)),
                Span::styled("No preview available", Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    let paragraph = ratatui::widgets::Paragraph::new(lines)
        .block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });

    f.render_widget(paragraph, area);

    Ok(())
}

/// Calculate image preview area height
///
/// # Arguments
///
/// * `has_images` - Whether there are images
/// * `image_count` - Number of images
///
/// # Returns
///
/// The number of lines needed for rendering
pub fn calculate_image_height(has_images: bool, image_count: usize) -> usize {
    if !has_images {
        return 0;
    }

    let images_per_row = 3; // Max 3 images per row
    let rows = image_count.saturating_add(images_per_row - 1) / images_per_row;

    // 1 header line + rows * 8 lines per image
    1 + rows * 8
}

/// Get the number of images that fit per row based on available width
///
/// # Arguments
///
/// * `width` - Available width in characters
///
/// # Returns
///
/// Number of images per row (1-3)
pub fn images_per_row(width: u16) -> usize {
    let images_per_row = width.saturating_sub(4) / 30;
    images_per_row.clamp(1, 3) as usize
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
    fn test_render_image_header() {
        let (mut terminal, area) = create_test_terminal();
        let result = terminal.draw(|f| {
            let _ = render_image_header(f, area, 3, '│', Color::Cyan);
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_image_header_single() {
        let (mut terminal, area) = create_test_terminal();
        let result = terminal.draw(|f| {
            let _ = render_image_header(f, area, 1, '│', Color::Cyan);
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_single_image_preview_with_preview() {
        let (mut terminal, area) = create_test_terminal();
        let img = ImageAttachment {
            id: "test-id".to_string(),
            path: Some("/path/to/image.png".to_string()),
            mime_type: "image/png".to_string(),
            data_base64: None,
            preview: Some("ASCII preview\nline 2\nline 3".to_string()),
            width: None,
            height: None,
        };

        let result = terminal.draw(|f| {
            let _ = render_single_image_preview(f, area, &img, '│', Color::Cyan);
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_single_image_preview_no_preview() {
        let (mut terminal, area) = create_test_terminal();
        let img = ImageAttachment {
            id: "test-id".to_string(),
            path: Some("/path/to/image.png".to_string()),
            mime_type: "image/png".to_string(),
            data_base64: None,
            preview: None,
            width: None,
            height: None,
        };

        let result = terminal.draw(|f| {
            let _ = render_single_image_preview(f, area, &img, '│', Color::Cyan);
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_single_image_preview_no_path() {
        let (mut terminal, area) = create_test_terminal();
        let img = ImageAttachment {
            id: "test-id".to_string(),
            path: None,
            mime_type: "image/png".to_string(),
            data_base64: None,
            preview: None,
            width: None,
            height: None,
        };

        let result = terminal.draw(|f| {
            let _ = render_single_image_preview(f, area, &img, '│', Color::Cyan);
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_calculate_image_height_none() {
        let height = calculate_image_height(false, 0);
        assert_eq!(height, 0);
    }

    #[test]
    fn test_calculate_image_height_single() {
        let height = calculate_image_height(true, 1);
        assert_eq!(height, 9); // 1 + 1 * 8
    }

    #[test]
    fn test_calculate_image_height_three() {
        let height = calculate_image_height(true, 3);
        assert_eq!(height, 9); // 1 + 1 * 8 (all on one row)
    }

    #[test]
    fn test_calculate_image_height_four() {
        let height = calculate_image_height(true, 4);
        assert_eq!(height, 17); // 1 + 2 * 8 (two rows)
    }

    #[test]
    fn test_calculate_image_height_seven() {
        let height = calculate_image_height(true, 7);
        assert_eq!(height, 25); // 1 + 3 * 8 (three rows)
    }

    #[test]
    fn test_images_per_row_narrow() {
        let count = images_per_row(30);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_images_per_row_medium() {
        let count = images_per_row(70);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_images_per_row_wide() {
        let count = images_per_row(100);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_images_per_row_very_wide() {
        let count = images_per_row(200);
        assert_eq!(count, 3); // Max 3 per row
    }

    #[test]
    fn test_images_per_row_very_narrow() {
        let count = images_per_row(10);
        assert_eq!(count, 1); // Min 1 per row
    }
}
