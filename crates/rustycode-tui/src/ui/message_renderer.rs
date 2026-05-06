//! Message rendering coordinator
//!
//! This module coordinates the rendering of messages to the terminal UI,
//! delegating specialized rendering tasks to dedicated submodules.

use super::animator::AnimationFrame;
use super::message_image::{
    calculate_image_height, images_per_row, render_image_header, render_single_image_preview,
};
use super::message_thinking::{render_thinking_content, render_thinking_header};
use super::message_types::{ExpansionLevel, Message, ToolExecution, ToolStatus};
use super::spinner::Spinner;
use crate::app::render::shared::estimate_line_count_wrapped;
use crate::theme::ThemeColors;
use anyhow::Result as anyhowResult;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    Frame,
};
pub use rustycode_ui_core::{MarkdownRenderer, MessageTheme, RenderCache};
use std::collections::{HashMap, VecDeque};
use unicode_width::UnicodeWidthStr;

// MessageTheme removed as it is now defined in ui::markdown

// MESSAGE RENDERER

/// Message renderer - handles hierarchical display
pub struct MessageRenderer {
    pub show_thinking: bool,
    pub show_tools: bool,
    /// Current animation frame for spinners
    pub anim_frame: AnimationFrame,
    /// Cache of rendered markdown lines (content_hash -> lines)
    /// Use RwLock for interior mutability since render methods take &self
    render_cache: std::sync::RwLock<RenderCache>,
    /// Cache of estimated message heights keyed by message identity/content.
    layout_cache: std::sync::RwLock<MessageLayoutCache>,
}

impl Default for MessageRenderer {
    fn default() -> Self {
        Self {
            show_thinking: false,
            show_tools: true,
            anim_frame: AnimationFrame::default(),
            render_cache: std::sync::RwLock::new(RenderCache::default()),
            layout_cache: std::sync::RwLock::new(MessageLayoutCache::default()),
        }
    }
}

impl MessageRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new message renderer with animation frame
    pub fn with_animation(anim_frame: AnimationFrame) -> Self {
        Self {
            show_thinking: false,
            show_tools: true,
            anim_frame,
            render_cache: std::sync::RwLock::new(RenderCache::default()),
            layout_cache: std::sync::RwLock::new(MessageLayoutCache::default()),
        }
    }

    /// Invalidate the render cache (call on terminal resize since
    /// line wrapping depends on terminal width).
    pub fn invalidate_cache(&self) {
        if let Ok(mut cache) = self.render_cache.write() {
            cache.clear();
        }
        if let Ok(mut cache) = self.layout_cache.write() {
            cache.clear();
        }
    }

    /// Update the animation frame
    pub fn update_animation(&mut self, frame: AnimationFrame) {
        self.anim_frame = frame;
    }

    /// Render a message
    pub fn render_message(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        theme: &MessageTheme,
        theme_colors: Option<&ThemeColors>,
    ) -> anyhowResult<()> {
        let (pipe_char, pipe_color) = theme_colors
            .map(|tc| tc.message_pipe_style(&message.role))
            .unwrap_or_else(|| message.pipe_style());

        // Calculate image height for positioning
        let image_height = if message.has_images() {
            calculate_image_height(true, message.image_count())
        } else {
            0
        };

        let mut current_y = area.y;

        // Render images first (at the top)
        if message.has_images() && current_y < area.bottom() {
            let image_area = Rect {
                x: area.x,
                y: current_y,
                width: area.width,
                height: image_height as u16,
            };
            self.render_image_previews(f, image_area, message, pipe_char, pipe_color)?;
            current_y += image_height as u16;
        }

        // Render main message content
        let content_area = Rect {
            x: area.x,
            y: current_y,
            width: area.width,
            height: (area.bottom() - current_y).max(1),
        };
        self.render_content(f, content_area, message, pipe_char, pipe_color, theme)?;

        // Update current_y after content
        current_y += self.calculate_content_height(message, area.width as usize) as u16;

        // If assistant message has tools, render inline summary
        if message.has_tools() && self.show_tools && current_y < area.bottom() {
            let tool_area = Rect {
                x: area.x,
                y: current_y,
                width: area.width,
                height: (area.bottom() - current_y).min(10), // Max 10 lines for tools
            };
            self.render_tool_summary(
                f,
                tool_area,
                message,
                pipe_char,
                pipe_color,
                theme,
                theme_colors,
            )?;
            current_y += self.calculate_tool_height(message) as u16;
        }

        // If thinking is present and user wants to see it
        if message.has_thinking() && self.show_thinking && current_y < area.bottom() {
            let thinking_area = Rect {
                x: area.x,
                y: current_y,
                width: area.width,
                height: (area.bottom() - current_y).min(10),
            };
            self.render_thinking(f, thinking_area, message, pipe_char, pipe_color, theme)?;
        }

        Ok(())
    }

    /// Render image previews
    fn render_image_previews(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        pipe: char,
        color: Color,
    ) -> anyhowResult<()> {
        let images = message
            .metadata
            .images
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No images found in message metadata"))?;

        // Render header
        render_image_header(f, area, images.len(), pipe, color)?;

        // Render up to 3 images per row
        let images_per_row = images_per_row(area.width);

        for (i, img) in images.iter().enumerate() {
            let row = i / images_per_row;
            let col = i % images_per_row;

            let img_width = area.width / images_per_row as u16;
            let img_x = area.x + (col as u16 * img_width);
            let img_y = area.y + 1 + (row as u16 * 8); // 8 lines per image

            if img_y + 8 > area.bottom() {
                break; // No more space
            }

            let img_area = Rect {
                x: img_x,
                y: img_y,
                width: img_width,
                height: 8,
            };

            render_single_image_preview(f, img_area, img, pipe, color)?;
        }

        Ok(())
    }

    /// Render the main message content
    fn render_content(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        _pipe: char,
        _color: Color, // Keep for signature compatibility, but use theme instead
        theme: &MessageTheme,
    ) -> anyhowResult<()> {
        // Use themed colors for pipe
        let pipe_color = match message.role {
            crate::ui::message::MessageRole::User => theme.user_color,
            crate::ui::message::MessageRole::Assistant => theme.ai_color,
            crate::ui::message::MessageRole::System => theme.system_color,
        };

        // Check if message is collapsed
        if message.collapsed {
            // Show collapsed state: just first line or summary
            let first_line = message.content.lines().next().unwrap_or("");
            let preview = if first_line.chars().count() > 60 {
                let s: String = first_line.chars().take(57).collect();
                format!("{}...", s)
            } else {
                first_line.to_string()
            };

            let all_lines = vec![Line::from(vec![
                // Keep the visual indent without emitting a copyable gutter glyph.
                Span::styled("  ", Style::default().fg(pipe_color)),
                Span::styled(preview, Style::default().fg(Color::DarkGray)),
                Span::styled(" (collapsed)", Style::default().fg(Color::DarkGray)),
            ])];

            let paragraph = ratatui::widgets::Paragraph::new(all_lines)
                .style(theme.default_style)
                .wrap(ratatui::widgets::Wrap { trim: false });

            f.render_widget(paragraph, area);
            return Ok(());
        }

        // Route raw git diffs to the dedicated diff renderer so file changes
        // stay readable even when they arrive without a fenced diff block.
        let rendered_lines = if crate::ui::diff_renderer::looks_like_git_diff(&message.content) {
            crate::ui::diff_renderer::render_diff(&message.content)
        } else {
            MarkdownRenderer::render_content(&message.content, theme, Some(&self.render_cache))
        };

        // Keep a blank gutter so mouse selection stays on the message content.
        let rendered_lines = rendered_lines
            .into_iter()
            .map(|line| {
                let mut spans = Vec::with_capacity(line.spans.len() + 1);
                spans.push(Span::raw(" "));
                spans.extend(line.spans.iter().cloned());
                Line::from(spans)
            })
            .collect::<Vec<_>>();

        let paragraph = ratatui::widgets::Paragraph::new(rendered_lines)
            .style(theme.default_style)
            .wrap(ratatui::widgets::Wrap { trim: false });

        f.render_widget(paragraph, area);

        Ok(())
    }

    /// Render tool summary
    fn render_tool_summary(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        pipe: char,
        color: Color,
        theme: &MessageTheme,
        theme_colors: Option<&ThemeColors>,
    ) -> anyhowResult<()> {
        let tools = message
            .tool_executions
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No tool executions found in message"))?;
        let total = tools.len();
        let complete = message.completed_tool_count();
        let failed = message.failed_tool_count();
        let total_bytes = message.total_tool_output_size();

        // Format size
        let size_str = if total_bytes < 1024 {
            format!("{}b", total_bytes)
        } else if total_bytes < 1024 * 1024 {
            format!("{:.1}kb", total_bytes as f64 / 1024.0)
        } else {
            format!("{:.1}mb", total_bytes as f64 / (1024.0 * 1024.0))
        };

        // Build header line
        let header_text = format!(
            "🔧 Executed: {} {} {}{} [▾] {}",
            total,
            if total == 1 { "tool" } else { "tools" },
            if failed > 0 {
                format!("({} failed)", failed)
            } else {
                String::new()
            },
            if complete == total && total > 0 {
                " ✅".to_string()
            } else if complete > 0 {
                format!(" ({}/{})", complete, total)
            } else {
                String::new()
            },
            size_str
        );

        let header_line = Line::from(vec![
            Span::styled(format!("{} ", pipe), Style::default().fg(color)),
            Span::styled(header_text, Style::default().fg(theme.tool_summary_color)),
        ]);

        let paragraph = ratatui::widgets::Paragraph::new(header_line);
        f.render_widget(paragraph, area);

        // If expanded, render tool list
        if message.tools_expansion != ExpansionLevel::Collapsed && area.height > 2 {
            self.render_tool_list(f, area, message, pipe, color, theme, theme_colors)?;
        }

        Ok(())
    }

    /// Render an individual tool execution as a card
    fn render_tool_card(
        &self,
        tool: &crate::ui::message::ToolExecution,
        index: usize,
        is_focused: bool,
        pipe: char,
        pipe_color: Color,
        theme: &MessageTheme,
        theme_colors: Option<&ThemeColors>,
    ) -> Line<'static> {
        let tool_color = theme_colors
            .map(|tc| tc.tool_status_color(&tool.status))
            .unwrap_or_else(|| tool.status.color());
        let status_icon = if tool.status == ToolStatus::Running {
            Spinner::working(&tool.name)
                .render_char(&self.anim_frame, None)
                .content
                .to_string()
        } else {
            tool.status.icon().to_string()
        };

        let style = if is_focused {
            Style::default()
                .bg(Color::Rgb(50, 50, 50))
                .fg(theme.tool_text_color)
        } else {
            Style::default().fg(theme.tool_text_color)
        };

        Line::from(vec![
            Span::styled(format!("{} ╎ ", pipe), Style::default().fg(pipe_color)),
            Span::styled(
                format!(" {} ", status_icon),
                Style::default().bg(tool_color).fg(Color::Black),
            ),
            Span::styled(format!(" [{}] {} ", index + 1, tool.result_summary), style),
            Span::styled(
                tool.size_summary().to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    }

    /// Render expanded tool list
    fn render_tool_list(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        pipe: char,
        color: Color,
        theme: &MessageTheme,
        theme_colors: Option<&ThemeColors>,
    ) -> anyhowResult<()> {
        let tools = message
            .tool_executions
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No tool executions found in message"))?;

        // In non-deep mode, only show the last 5 tools to avoid flooding the display
        const VISIBLE_TOOL_LIMIT: usize = 5;
        let is_deep = message.tools_expansion == ExpansionLevel::Deep;
        let (skip_count, visible_tools): (usize, Vec<_>) =
            if is_deep || tools.len() <= VISIBLE_TOOL_LIMIT {
                (0, tools.iter().enumerate().collect())
            } else {
                let skip = tools.len() - VISIBLE_TOOL_LIMIT;
                (skip, tools.iter().enumerate().skip(skip).collect())
            };

        let mut lines = vec![];

        // Add header border
        lines.push(Line::from(vec![
            Span::styled(format!("{} ┌", pipe), Style::default().fg(color)),
            Span::styled(
                "─".repeat(area.width.saturating_sub(4) as usize),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("┐", Style::default().fg(Color::DarkGray)),
        ]));

        // Show "... and N more" for skipped tools
        if skip_count > 0 {
            lines.push(Line::from(vec![
                Span::styled(format!("{} ╎ ", pipe), Style::default().fg(color)),
                Span::styled(
                    format!(
                        "  ... and {} earlier tool{}",
                        skip_count,
                        if skip_count > 1 { "s" } else { "" }
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            lines.push(Line::from(vec![Span::styled(
                format!("{} ╎ ", pipe),
                Style::default().fg(color),
            )]));
        }

        // Add each visible tool as a card
        for (idx, (i, tool)) in visible_tools.iter().enumerate() {
            let is_focused = message.focused_tool_index == Some(*i);

            if is_deep && is_focused {
                lines.push(self.render_tool_details(tool, pipe, color, theme)?);
            } else {
                lines.push(self.render_tool_card(
                    tool,
                    *i,
                    is_focused,
                    pipe,
                    color,
                    theme,
                    theme_colors,
                ));
            }

            // Add thin spacer
            if idx < visible_tools.len() - 1 {
                lines.push(Line::from(vec![Span::styled(
                    format!("{} ╎ ", pipe),
                    Style::default().fg(color),
                )]));
            }
        }

        // Add footer border
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

    /// Render detailed tool output
    fn render_tool_details(
        &self,
        tool: &ToolExecution,
        pipe: char,
        color: Color,
        theme: &MessageTheme,
    ) -> anyhowResult<Line<'_>> {
        let no_output = "(no output)".to_string();
        let detailed_output = tool.detailed_output.as_ref().unwrap_or(&no_output);

        // Truncate if too long
        let max_len = 200;
        let output =
            if <str as unicode_width::UnicodeWidthStr>::width(detailed_output.as_str()) > max_len {
                let truncated = crate::app::render::brutalist_helpers::truncate_to_display_width(
                    detailed_output,
                    max_len,
                );
                format!("{}...", truncated)
            } else {
                detailed_output.clone()
            };

        Ok(Line::from(vec![
            Span::styled(format!("{} │ ┌", pipe), Style::default().fg(color)),
            Span::styled(
                format!(" {} ", output),
                Style::default().fg(theme.tool_detail_color),
            ),
            Span::styled("┐", Style::default().fg(Color::DarkGray)),
        ]))
    }

    /// Render thinking block
    fn render_thinking(
        &self,
        f: &mut Frame,
        area: Rect,
        message: &Message,
        pipe: char,
        color: Color,
        theme: &MessageTheme,
    ) -> anyhowResult<()> {
        let thinking = message
            .thinking
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No thinking content found in message"))?;

        // Render header
        render_thinking_header(f, area, thinking, pipe, color, theme)?;

        // If expanded, render thinking content
        if message.thinking_expansion != ExpansionLevel::Collapsed {
            render_thinking_content(f, area, thinking, pipe, color, theme)?;
        }

        Ok(())
    }

    /// Calculate content height accounting for wrapping
    fn calculate_content_height(&self, message: &Message, content_width: usize) -> usize {
        let line_count = message.content.lines().count();

        let width = content_width.max(1);
        let gutter_indent = 1;
        let estimated_wraps = message
            .content
            .lines()
            .map(|line| {
                let display_width = line.width();
                if display_width <= width {
                    0
                } else {
                    let first_line_capacity = width;
                    let continuation_width = width.saturating_sub(gutter_indent).max(1);
                    let remaining = display_width.saturating_sub(first_line_capacity);
                    remaining.div_ceil(continuation_width)
                }
            })
            .sum::<usize>();

        let total_lines = line_count + estimated_wraps;

        // Add 1 for header (if message has one)
        total_lines
            + if message.role == crate::ui::message::MessageRole::System {
                0
            } else {
                1
            }
    }

    /// Calculate tool area height
    fn calculate_tool_height(&self, message: &Message) -> usize {
        if !message.has_tools() {
            return 0;
        }

        let total_tools = message.tool_count();
        const VISIBLE_LIMIT: usize = 5;

        match message.tools_expansion {
            ExpansionLevel::Collapsed => 1, // Just header
            ExpansionLevel::Expanded => {
                let visible = total_tools.min(VISIBLE_LIMIT);
                let skipped = total_tools.saturating_sub(VISIBLE_LIMIT);
                // Header border + (ellipsis line + spacer if skipped) + visible tools + spacers + footer border
                1 + if skipped > 0 { 2 } else { 0 }
                    + visible
                    + visible.saturating_sub(1) // spacers between tools
                    + 1
            }
            ExpansionLevel::Deep => {
                // Header border + all tools (with detail for focused) + detail extras + footer border
                1 + total_tools + 4 + 1
            }
        }
    }

    /// Render plain text content without markdown parsing (for streaming)
    pub fn render_plain_text(&self, content: &str, theme: &MessageTheme) -> Vec<Line<'static>> {
        MarkdownRenderer::render_plain_text(content, theme)
    }

    /// Render markdown content with syntax highlighting and diff support
    /// Uses caching to avoid re-parsing the same content multiple times.
    pub fn render_markdown_content(
        &self,
        content: &str,
        theme: &MessageTheme,
    ) -> Vec<Line<'static>> {
        MarkdownRenderer::render_content(content, theme, Some(&self.render_cache))
    }

    /// Estimate the rendered height of a message for a given content width.
    ///
    /// This keeps historical messages cheap to measure while the active
    /// streaming assistant message changes on every chunk.
    pub fn estimate_message_height(&self, message: &Message, content_width: usize) -> usize {
        let key = Self::layout_cache_key(message, content_width);

        if let Ok(cache) = self.layout_cache.read() {
            if let Some(height) = cache.get(&key) {
                return height;
            }
        }

        let height = Self::compute_message_height(message, content_width);

        if let Ok(mut cache) = self.layout_cache.write() {
            cache.insert(key, height);
        }

        height
    }

    fn layout_cache_key(message: &Message, content_width: usize) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        message.id.hash(&mut hasher);
        content_width.hash(&mut hasher);
        message.collapsed.hash(&mut hasher);
        match message.role {
            crate::ui::message::MessageRole::User => 0u8.hash(&mut hasher),
            crate::ui::message::MessageRole::Assistant => 1u8.hash(&mut hasher),
            crate::ui::message::MessageRole::System => 2u8.hash(&mut hasher),
        }
        message.content.hash(&mut hasher);
        if let Some(thinking) = &message.thinking {
            thinking.hash(&mut hasher);
        }
        match message.tools_expansion {
            ExpansionLevel::Collapsed => 0u8.hash(&mut hasher),
            ExpansionLevel::Expanded => 1u8.hash(&mut hasher),
            ExpansionLevel::Deep => 2u8.hash(&mut hasher),
        }
        if let Some(tools) = &message.tool_executions {
            tools.len().hash(&mut hasher);
        }

        hasher.finish()
    }

    fn compute_message_height(message: &Message, content_width: usize) -> usize {
        if message.collapsed {
            return 1;
        }

        let width = content_width.max(1);
        estimate_line_count_wrapped(&message.content, width).max(1)
            + message
                .thinking
                .as_ref()
                .map(|thinking| estimate_line_count_wrapped(thinking, width) + 4)
                .unwrap_or(0)
            + message
                .tool_executions
                .as_ref()
                .map(|tools| tools.len() + 2)
                .unwrap_or(0)
    }
}

#[derive(Debug, Clone)]
struct MessageLayoutCache {
    entries: HashMap<u64, usize>,
    order: VecDeque<u64>,
    capacity: usize,
}

impl MessageLayoutCache {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn get(&self, key: &u64) -> Option<usize> {
        self.entries.get(key).copied()
    }

    fn insert(&mut self, key: u64, height: usize) {
        if self.entries.contains_key(&key) {
            self.order.retain(|existing| *existing != key);
        }

        self.entries.insert(key, height);
        self.order.push_back(key);

        while self.entries.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }
    }
}

impl Default for MessageLayoutCache {
    fn default() -> Self {
        Self::with_capacity(512)
    }
}

// TESTS

#[cfg(test)]
mod tests {
    use super::super::message_types::MessageRole;
    use super::*;

    #[test]
    fn test_message_renderer_default() {
        let renderer = MessageRenderer::default();
        assert!(!renderer.show_thinking);
        assert!(renderer.show_tools);
    }

    #[test]
    fn test_message_renderer_new() {
        let renderer = MessageRenderer::new();
        assert!(!renderer.show_thinking);
        assert!(renderer.show_tools);
    }

    #[test]
    fn test_message_renderer_with_animation() {
        let anim_frame = AnimationFrame::default();
        let renderer = MessageRenderer::with_animation(anim_frame);
        assert!(!renderer.show_thinking);
        assert!(renderer.show_tools);
    }

    #[test]
    fn test_message_renderer_update_animation() {
        let mut renderer = MessageRenderer::new();
        let anim_frame = AnimationFrame::default();
        renderer.update_animation(anim_frame);
        // Animation frame updated
    }

    #[test]
    fn test_message_theme_default() {
        let theme = MessageTheme::default();
        // Default colors are mauve/purple for user, teal for AI, yellow for system (midnight-rust theme)
        assert_eq!(theme.user_color, Color::Rgb(180, 142, 173));
        assert_eq!(theme.ai_color, Color::Rgb(143, 188, 187));
        assert_eq!(theme.system_color, Color::Rgb(235, 203, 139));
    }

    #[test]
    fn test_calculate_content_height() {
        let renderer = MessageRenderer::new();
        let message = Message::new(MessageRole::Assistant, "Line 1\nLine 2\nLine 3".to_string());
        let height = renderer.calculate_content_height(&message, 80);
        assert!(height >= 4); // 3 lines + 1 for header
    }

    #[test]
    fn test_calculate_content_height_empty() {
        let renderer = MessageRenderer::new();
        let message = Message::new(MessageRole::Assistant, String::new());
        let height = renderer.calculate_content_height(&message, 80);
        assert!(height >= 1); // At least header
    }

    #[test]
    fn test_calculate_content_height_unicode_exact_fit() {
        let renderer = MessageRenderer::new();
        let content = "汉".repeat(10);
        let message = Message::new(MessageRole::Assistant, content);

        // Ten CJK characters have a display width of 20. They should fit
        // exactly in a 20-cell viewport without being treated as wrapped.
        let height = renderer.calculate_content_height(&message, 20);
        assert_eq!(height, 2);
    }

    #[test]
    fn test_calculate_tool_height_no_tools() {
        let renderer = MessageRenderer::new();
        let message = Message::new(MessageRole::Assistant, String::new());
        let height = renderer.calculate_tool_height(&message);
        assert_eq!(height, 0);
    }

    #[test]
    fn test_calculate_tool_height_collapsed() {
        let renderer = MessageRenderer::new();
        let tool = ToolExecution::new("tool_1".to_string(), "test".to_string(), "Done".to_string());
        let mut message = Message::new(MessageRole::Assistant, String::new());
        message.tool_executions = Some(vec![tool]);
        message.tools_expansion = ExpansionLevel::Collapsed;
        let height = renderer.calculate_tool_height(&message);
        assert_eq!(height, 1);
    }

    #[test]
    fn test_calculate_tool_height_expanded() {
        let renderer = MessageRenderer::new();
        let tool1 = ToolExecution::new(
            "tool_1".to_string(),
            "test1".to_string(),
            "Done1".to_string(),
        );
        let tool2 = ToolExecution::new(
            "tool_2".to_string(),
            "test2".to_string(),
            "Done2".to_string(),
        );
        let mut message = Message::new(MessageRole::Assistant, String::new());
        message.tool_executions = Some(vec![tool1, tool2]);
        message.tools_expansion = ExpansionLevel::Expanded;
        let height = renderer.calculate_tool_height(&message);
        // Header + border + 2 tools + border
        assert_eq!(height, 5);
    }

    #[test]
    fn test_calculate_tool_height_deep() {
        let renderer = MessageRenderer::new();
        let tool = ToolExecution::new("tool_1".to_string(), "test".to_string(), "Done".to_string());
        let mut message = Message::new(MessageRole::Assistant, String::new());
        message.tool_executions = Some(vec![tool]);
        message.tools_expansion = ExpansionLevel::Deep;
        let height = renderer.calculate_tool_height(&message);
        // Header + border + tool + detail + border
        assert!(height >= 7);
    }

    #[test]
    fn test_render_plain_text() {
        let renderer = MessageRenderer::new();
        let theme = MessageTheme::default();
        let lines = renderer.render_plain_text("Hello, world!", &theme);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_markdown_content() {
        let renderer = MessageRenderer::new();
        let theme = MessageTheme::default();
        let lines = renderer.render_markdown_content("# Test", &theme);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_markdown_content_cache() {
        let renderer = MessageRenderer::new();
        let theme = MessageTheme::default();
        let content = "# Test Heading\n\nSome content";

        // First call - cache miss
        let lines1 = renderer.render_markdown_content(content, &theme);
        // Second call - cache hit
        let lines2 = renderer.render_markdown_content(content, &theme);

        assert_eq!(lines1.len(), lines2.len());
    }

    #[test]
    fn test_render_tool_details_uses_display_width() {
        let renderer = MessageRenderer::new();
        let theme = MessageTheme::default();
        let mut tool = ToolExecution::new("tool_1".to_string(), "test".to_string(), "".to_string());
        tool.complete(Some("汉".repeat(100)));

        let line = renderer
            .render_tool_details(&tool, '▌', theme.tool_summary_color, &theme)
            .expect("tool details should render");
        let text = line.to_string();

        assert!(
            !text.contains("..."),
            "wide text should not be truncated by byte length: {text:?}"
        );
        assert!(
            text.contains(&"汉".repeat(100)),
            "full wide text should be preserved when it fits display width"
        );
    }
}
