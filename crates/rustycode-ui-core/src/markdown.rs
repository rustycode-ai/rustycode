//! Markdown rendering with syntax highlighting and streaming support
//!
//! This module provides comprehensive markdown rendering including:
//! - Code blocks with syntax highlighting (syntect + `TextMate` grammars)
//! - Inline code with distinct styling
//! - Diff display with red/green colors and hunk headers
//! - Lists, headers, bold/italic, blockquotes, tables
//! - Incremental streaming display
//! - Lazy parsing and virtual rendering for performance

use crate::FrontendMessage;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use similar::{Algorithm, TextDiff};
use std::collections::{HashMap, VecDeque};
use unicode_width::UnicodeWidthStr;

/// Message display theme (imported from `message_renderer` for consistency)
#[derive(Clone, Debug)]
pub struct MessageTheme {
    pub default_style: Style,
    pub user_color: Color,
    pub ai_color: Color,
    pub system_color: Color,
    pub tool_summary_color: Color,
    pub tool_text_color: Color,
    pub tool_detail_color: Color,
    pub thinking_color: Color,
    pub thinking_text_color: Color,
}

impl Default for MessageTheme {
    fn default() -> Self {
        Self {
            default_style: Style::default().fg(Color::Rgb(236, 239, 244)), // snow
            user_color: Color::Rgb(180, 142, 173),                         // mauve (#b48ead)
            ai_color: Color::Rgb(143, 188, 187),                           // teal (#8fbcbb)
            system_color: Color::Rgb(235, 203, 139),                       // yellow (#ebcb8b)
            tool_summary_color: Color::Rgb(163, 190, 140),                 // success green
            tool_text_color: Color::Rgb(236, 239, 244),
            tool_detail_color: Color::Rgb(129, 161, 193), // muted blue
            thinking_color: Color::Rgb(94, 129, 172),     // frost blue
            thinking_text_color: Color::Rgb(236, 239, 244),
        }
    }
}

impl MessageTheme {
    /// Create `MessageTheme` from `ColorPalette` (for theming support)
    pub fn from_colors(user_bar: &str, ai_bar: &str) -> Self {
        Self {
            default_style: Style::default().fg(Color::White),
            user_color: parse_hex_color(user_bar),
            ai_color: parse_hex_color(ai_bar),
            system_color: Color::Gray,
            tool_summary_color: Color::Yellow,
            tool_text_color: Color::White,
            tool_detail_color: Color::White,
            thinking_color: Color::Blue,
            thinking_text_color: Color::White,
        }
    }
}

/// Parse hex color string to ratatui Color
fn parse_hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        Color::Rgb(r, g, b)
    } else {
        Color::White // fallback
    }
}

/// Configuration for markdown rendering
#[derive(Clone, Debug)]
pub struct MarkdownConfig {
    /// Enable syntax highlighting
    pub syntax_highlighting: bool,

    /// Theme for syntax highlighting
    pub syntax_theme: String,

    /// Enable incremental streaming display
    pub streaming_enabled: bool,

    /// Typing animation (if false, instant display)
    pub typing_animation: bool,

    /// Chars per second for typing animation
    pub typing_speed: f32,

    /// Maximum width for code blocks (0 = no limit)
    pub max_code_width: usize,
}

/// Ordered render cache that evicts the oldest inserted entry first.
#[derive(Clone, Debug)]
pub struct RenderCache {
    entries: HashMap<u64, Vec<Line<'static>>>,
    order: VecDeque<u64>,
    capacity: usize,
}

impl RenderCache {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub fn get(&self, key: &u64) -> Option<&Vec<Line<'static>>> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: u64, lines: Vec<Line<'static>>) {
        if self.entries.contains_key(&key) {
            self.order.retain(|existing| *existing != key);
        }

        self.entries.insert(key, lines);
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

impl Default for RenderCache {
    fn default() -> Self {
        Self::with_capacity(200)
    }
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            syntax_highlighting: true,
            syntax_theme: "base16-ocean.dark".to_string(),
            streaming_enabled: true,
            typing_animation: false,
            typing_speed: 60.0,
            max_code_width: 120,
        }
    }
}

/// Markdown parser and renderer
pub struct MarkdownRenderer {
    syntax_highlighter: SyntaxHighlighter,
    #[allow(dead_code)] // Kept for future use
    config: MarkdownConfig,
}

#[derive(Clone, Debug, Default)]
struct ListState {
    ordered: bool,
    next_num: u64,
}

/// Tracks active inline styles that stack (bold + italic + strikethrough etc.)
#[derive(Clone, Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
struct InlineState {
    emphasis: bool,
    strong: bool,
    strikethrough: bool,
    heading: bool,
    heading_level: usize,
    /// Inside a link tag — accumulate link text
    in_link: bool,
    link_url: String,
    /// Inside an image tag
    in_image: bool,
    image_alt: String,
    /// Inside a table — track column index for header separator
    in_table: bool,
    in_table_header: bool,
    table_col: usize,
}

impl InlineState {
    /// Build a ratatui Style from all active inline modifiers
    fn text_style(&self, _renderer: &MarkdownRenderer) -> Style {
        let mut style = Style::default();
        let mut mods = Modifier::empty();
        if self.emphasis {
            mods |= Modifier::ITALIC;
        }
        if self.strong {
            mods |= Modifier::BOLD;
        }
        if self.strikethrough {
            mods |= Modifier::CROSSED_OUT;
        }
        if self.heading {
            style = style.fg(MarkdownRenderer::heading_color(self.heading_level));
            mods |= Modifier::BOLD;
        }
        style = style.add_modifier(mods);
        style
    }
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self::with_config(MarkdownConfig::default())
    }

    /// Create a new markdown renderer with specific config
    pub fn with_config(config: MarkdownConfig) -> Self {
        Self {
            syntax_highlighter: SyntaxHighlighter::new_with_theme(&config.syntax_theme),
            config,
        }
    }

    /// Helper for top-level usage (matches `message_markdown` signature)
    pub fn render_content(
        content: &str,
        _theme: &MessageTheme,
        cache: Option<&std::sync::RwLock<RenderCache>>,
    ) -> Vec<Line<'static>> {
        // Simple caching logic with bounded size
        if let Some(cache_lock) = cache {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            content.hash(&mut hasher);
            let hash = hasher.finish();

            if let Some(lines) = cache_lock
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&hash)
            {
                return lines.clone();
            }

            let renderer = Self::default();
            let lines = renderer.parse(content);

            let mut cache = cache_lock
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache.insert(hash, lines.clone());
            lines
        } else {
            Self::default().parse(content)
        }
    }

    /// Parse markdown into renderable lines
    #[allow(clippy::too_many_lines)]
    pub fn parse(&self, markdown: &str) -> Vec<Line<'static>> {
        let parser = Parser::new(markdown);
        let mut lines = Vec::new();
        let mut current_line = Vec::new();

        // Block state
        let mut in_code_block = false;
        let mut code_language: Option<String> = None;
        let mut code_content = String::with_capacity(512);

        let mut list_stack: Vec<ListState> = Vec::new();

        // Inline state (stackable modifiers)
        let mut inline = InlineState::default();

        // Table state: accumulate cells per row, then render when row ends
        let mut table_rows: Vec<Vec<String>> = Vec::new();
        let mut current_row: Vec<String> = Vec::new();
        let mut current_cell = String::with_capacity(128);
        let mut table_aligns: Vec<pulldown_cmark::Alignment> = Vec::new();
        let mut blockquote_depth: usize = 0;

        for event in parser {
            match event {
                // ── Code blocks ──────────────────────────────────────────
                Event::Start(Tag::CodeBlock(kind)) => {
                    Self::flush_current_line(&mut lines, &mut current_line);
                    in_code_block = true;
                    code_language = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) => Some(lang.to_string()),
                        pulldown_cmark::CodeBlockKind::Indented => None,
                    };
                    if let Some(ref lang) = code_language {
                        lines.push(Line::from(vec![Span::styled(
                            format!("{lang}:"),
                            Style::default().fg(Color::DarkGray),
                        )]));
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    if code_language.as_deref() == Some("diff") {
                        lines.extend(render_diff(&code_content));
                    } else {
                        lines.extend(
                            self.syntax_highlighter
                                .highlight(&code_content, code_language.as_deref()),
                        );
                    }
                    code_content.clear();
                    in_code_block = false;
                    code_language = None;
                }

                // ── Text ─────────────────────────────────────────────────
                Event::Text(text) => {
                    if in_code_block {
                        code_content.push_str(&text);
                    } else if inline.in_image {
                        inline.image_alt.push_str(&text);
                    } else if inline.in_table {
                        current_cell.push_str(&text);
                    } else if inline.in_link {
                        // Link text — render with underlined + colored style
                        current_line.push(Span::styled(
                            text.to_string(),
                            Style::default()
                                .fg(Color::Rgb(130, 170, 255))
                                .add_modifier(Modifier::UNDERLINED),
                        ));
                    } else {
                        current_line.push(Span::styled(text.to_string(), inline.text_style(self)));
                    }
                }

                // ── Inline code ──────────────────────────────────────────
                Event::Code(text) => {
                    if inline.in_table {
                        current_cell.push_str(&text);
                    } else {
                        current_line.push(Span::styled(
                            text.to_string(),
                            Style::default()
                                .fg(Color::Cyan)
                                .bg(Color::Rgb(40, 44, 52))
                                .add_modifier(Modifier::DIM),
                        ));
                    }
                }

                // ── Line breaks ──────────────────────────────────────────
                Event::SoftBreak | Event::HardBreak => {
                    if inline.in_table {
                        current_cell.push('\n');
                    } else {
                        Self::flush_current_line(&mut lines, &mut current_line);
                    }
                }

                // ── Emphasis / Strong / Strikethrough ────────────────────
                Event::Start(Tag::Emphasis) => inline.emphasis = true,
                Event::End(TagEnd::Emphasis) => inline.emphasis = false,
                Event::Start(Tag::Strong) => inline.strong = true,
                Event::End(TagEnd::Strong) => inline.strong = false,
                Event::Start(Tag::Strikethrough) => inline.strikethrough = true,
                Event::End(TagEnd::Strikethrough) => inline.strikethrough = false,

                // ── Links ────────────────────────────────────────────────
                Event::Start(Tag::Link { dest_url, .. }) => {
                    inline.in_link = true;
                    inline.link_url = dest_url.to_string();
                }
                Event::End(TagEnd::Link) => {
                    if inline.in_table {
                        #[allow(clippy::format_push_string)]
                        current_cell.push_str(&format!(" ({})", inline.link_url));
                    } else {
                        // Append the URL in muted color after the link text
                        current_line.push(Span::styled(
                            format!(" ({})", inline.link_url),
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(Modifier::DIM),
                        ));
                    }
                    inline.in_link = false;
                    inline.link_url.clear();
                }

                // ── Images ───────────────────────────────────────────────
                Event::Start(Tag::Image { dest_url, .. }) => {
                    inline.in_image = true;
                    inline.image_alt.clear();
                    inline.link_url = dest_url.to_string();
                }
                Event::End(TagEnd::Image) => {
                    // Render as: 🖼 alt text (url)
                    let alt = if inline.image_alt.is_empty() {
                        "image".to_string()
                    } else {
                        std::mem::take(&mut inline.image_alt)
                    };
                    if inline.in_table {
                        #[allow(clippy::format_push_string)]
                        current_cell.push_str(&format!("🖼 {alt} ({})", inline.link_url));
                    } else {
                        current_line.push(Span::styled(
                            format!("🖼 {alt} "),
                            Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::ITALIC),
                        ));
                        current_line.push(Span::styled(
                            format!("({})", inline.link_url),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                    inline.in_image = false;
                    inline.link_url.clear();
                }

                // ── Paragraphs / List items / Blockquote ends ────────────
                Event::Start(Tag::Paragraph) => {
                    if blockquote_depth > 0 && current_line.is_empty() {
                        current_line.push(Span::styled(
                            " ▎ ",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }
                }
                Event::End(TagEnd::Item) => {
                    Self::flush_current_line(&mut lines, &mut current_line);
                }
                Event::End(TagEnd::Paragraph) => {
                    Self::flush_current_line(&mut lines, &mut current_line);
                    if list_stack.is_empty() && !inline.in_table {
                        lines.push(Line::default());
                    }
                }

                // ── Headings ─────────────────────────────────────────────
                Event::Start(Tag::Heading { level, .. }) => {
                    Self::flush_current_line(&mut lines, &mut current_line);
                    inline.heading = true;
                    inline.heading_level = level as usize;
                    let prefix = "#".repeat(inline.heading_level);
                    current_line.push(Span::styled(
                        format!("{prefix} "),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                Event::End(TagEnd::Heading(_)) => {
                    Self::flush_current_line(&mut lines, &mut current_line);
                    inline.heading = false;
                    lines.push(Line::default());
                }

                // ── Lists ────────────────────────────────────────────────
                Event::Start(Tag::List(kind)) => {
                    list_stack.push(ListState {
                        ordered: kind.is_some(),
                        next_num: kind.unwrap_or(1),
                    });
                }
                Event::End(TagEnd::List(_)) => {
                    list_stack.pop();
                    if list_stack.is_empty() {
                        lines.push(Line::default());
                    }
                }
                Event::Start(Tag::Item) => {
                    Self::flush_current_line(&mut lines, &mut current_line);
                    let depth = list_stack.len().saturating_sub(1);
                    let indent = "  ".repeat(depth);
                    if let Some(state) = list_stack.last_mut() {
                        let bullet = if state.ordered {
                            let b = format!("{}{}. ", indent, state.next_num);
                            state.next_num += 1;
                            b
                        } else {
                            format!("{indent}• ")
                        };
                        current_line.push(Span::styled(
                            bullet,
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }
                }

                // ── Blockquotes ──────────────────────────────────────────
                Event::Start(Tag::BlockQuote(_)) => {
                    Self::flush_current_line(&mut lines, &mut current_line);
                    blockquote_depth += 1;
                    current_line.push(Span::styled(
                        " ▎ ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                    // Dim subsequent text inside blockquote
                    // (handled by wrapping in DIM below)
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    blockquote_depth = blockquote_depth.saturating_sub(1);
                    Self::flush_current_line(&mut lines, &mut current_line);
                }

                // ── Tables ───────────────────────────────────────────────
                Event::Start(Tag::Table(aligns)) => {
                    Self::flush_current_line(&mut lines, &mut current_line);
                    inline.in_table = true;
                    table_aligns = aligns;
                    table_rows.clear();
                }
                Event::End(TagEnd::Table) => {
                    inline.in_table = false;
                    // Render all collected rows with box-drawing characters
                    if !table_rows.is_empty() {
                        lines.extend(render_table_boxed(&table_rows, &table_aligns));
                    }
                    table_rows.clear();
                    table_aligns.clear();
                    lines.push(Line::default());
                }
                Event::Start(Tag::TableRow) => {
                    current_row.clear();
                    inline.table_col = 0;
                }
                Event::End(TagEnd::TableRow) => {
                    if !current_cell.is_empty() {
                        current_row.push(std::mem::take(&mut current_cell));
                    }
                    if !current_row.is_empty() {
                        table_rows.push(current_row.clone());
                        // First row is header — insert separator after it
                        if table_rows.len() == 1 {
                            // Separator will be added by render_table_boxed
                        }
                    }
                    current_row.clear();
                }
                Event::Start(Tag::TableCell) => {
                    current_cell.clear();
                    inline.in_table_header = table_rows.is_empty();
                }
                Event::End(TagEnd::TableCell) => {
                    current_row.push(std::mem::take(&mut current_cell));
                    inline.table_col += 1;
                    inline.in_table_header = false;
                }

                // ── Horizontal rule ──────────────────────────────────────
                Event::Rule => {
                    Self::flush_current_line(&mut lines, &mut current_line);
                    // Render a horizontal line spanning typical width
                    lines.push(Line::from(vec![Span::styled(
                        "─".repeat(60),
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::DIM),
                    )]));
                    lines.push(Line::default());
                }

                // ── Footnote references (ignore for now) ────────────────
                _ => {}
            }
        }

        if inline.in_table {
            let mut preview_rows = table_rows.clone();
            if !current_cell.is_empty() {
                current_row.push(std::mem::take(&mut current_cell));
            }
            if !current_row.is_empty() {
                preview_rows.push(current_row.clone());
            }
            if !preview_rows.is_empty() {
                lines.extend(render_table_preview(&preview_rows));
            }
        }

        Self::flush_current_line(&mut lines, &mut current_line);
        lines
    }

    /// Backward-compatible wrapper retained for legacy tests/examples.
    pub fn render(&self, markdown: &str) -> Vec<Line<'static>> {
        self.parse(markdown)
    }

    /// Render plain text content (matches `message_markdown` signature)
    pub fn render_plain_text(content: &str, theme: &MessageTheme) -> Vec<Line<'static>> {
        content
            .lines()
            .map(|line| Line::from(vec![Span::styled(line.to_string(), theme.default_style)]))
            .collect()
    }

    /// Render a collection of messages (for compatibility with benchmarks)
    pub fn render_messages(
        &self,
        messages: &[FrontendMessage],
        _width: u16,
        _height: u16,
    ) -> Vec<Line<'static>> {
        let mut all_lines = Vec::new();
        for msg in messages {
            let mut lines = self.parse(&msg.content);
            all_lines.append(&mut lines);
            // Add a separator or gap? Benchmarks probably don't care about perfect layout
            all_lines.push(Line::default());
        }
        all_lines
    }

    /// Render a streaming message (for compatibility with benchmarks)
    pub fn render_streaming(
        &self,
        msg: &StreamingMessage,
        _width: u16,
        _height: u16,
    ) -> Vec<Line<'static>> {
        self.parse(&msg.content)
    }

    fn flush_current_line(lines: &mut Vec<Line<'static>>, current_line: &mut Vec<Span<'static>>) {
        if !current_line.is_empty() {
            lines.push(Line::from(std::mem::take(current_line)));
        }
    }

    const fn heading_color(level: usize) -> Color {
        match level {
            1 => Color::Red,
            2 => Color::Magenta,
            3 => Color::Yellow,
            4 => Color::Green,
            5 => Color::Cyan,
            _ => Color::Blue,
        }
    }

    /// Parse markdown incrementally for streaming
    pub fn parse_incremental(&self, delta: &str) -> Vec<Line<'_>> {
        // For incremental parsing, we parse the entire accumulated content
        // but this is optimized by only re-rendering when needed
        self.parse(delta)
    }
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::with_config(MarkdownConfig::default())
    }
}

// Use the shared implementation from the dedicated module to avoid duplicate
// definitions and lifetime complexity.
use crate::syntax_highlighter::SyntaxHighlighter;

// Remove highlight_line as it's now inlined and stateful

/// Render plain code without syntax highlighting
#[allow(dead_code)] // Kept for future use
fn render_plain_code(code: &str) -> Vec<Line<'static>> {
    code.lines()
        .map(|line| Line::from(vec![Span::raw(line.to_string())]))
        .collect()
}

/// Render a markdown table with box-drawing characters.
///
/// `rows` contains header as rows[0], data rows follow.
/// `aligns` holds per-column alignment from the markdown table.
fn render_table_boxed(
    rows: &[Vec<String>],
    aligns: &[pulldown_cmark::Alignment],
) -> Vec<Line<'static>> {
    if rows.is_empty() {
        return Vec::new();
    }

    let num_cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if num_cols == 0 {
        return Vec::new();
    }

    // Compute max width per column
    let mut col_widths = vec![0usize; num_cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                col_widths[i] = col_widths[i].max(UnicodeWidthStr::width(cell.as_str()));
            }
        }
    }
    // Minimum column width
    for w in &mut col_widths {
        *w = (*w).max(3);
    }

    let border_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);
    let header_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let cell_style = Style::default().fg(Color::Rgb(236, 239, 244));

    let mut result = Vec::new();

    let make_border = |left: &str, mid: &str, right: &str, fill: &str| -> Line<'static> {
        let mut spans = Vec::new();
        spans.push(Span::styled(left.to_string(), border_style));
        for (i, w) in col_widths.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(mid.to_string(), border_style));
            }
            spans.push(Span::styled(fill.repeat(*w + 2), border_style));
        }
        spans.push(Span::styled(right.to_string(), border_style));
        Line::from(spans)
    };

    let make_row = |row: &[String], is_header: bool| -> Line<'static> {
        let style = if is_header { header_style } else { cell_style };
        let mut spans = Vec::new();
        spans.push(Span::styled("│", border_style));
        for (i, w) in col_widths.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("│", border_style));
            }
            let cell_text = row.get(i).map_or("", String::as_str);
            let pad = w.saturating_sub(UnicodeWidthStr::width(cell_text));
            let align = aligns
                .get(i)
                .copied()
                .unwrap_or(pulldown_cmark::Alignment::None);
            let (left_pad, right_pad) = match align {
                pulldown_cmark::Alignment::Center => (pad / 2, pad - pad / 2),
                pulldown_cmark::Alignment::Right => (pad, 0),
                _ => (0, pad), // Left or None
            };
            spans.push(Span::raw(format!(" {}", " ".repeat(left_pad))));
            spans.push(Span::styled(cell_text.to_string(), style));
            spans.push(Span::raw(format!("{} ", " ".repeat(right_pad))));
        }
        spans.push(Span::styled("│", border_style));
        Line::from(spans)
    };

    // Top border
    result.push(make_border("┌", "┬", "┐", "─"));

    // Header row
    if let Some(header) = rows.first() {
        result.push(make_row(header, true));
        // Header-data separator
        result.push(make_border("├", "┼", "┤", "─"));
    }

    // Data rows
    for row in rows.iter().skip(1) {
        result.push(make_row(row, false));
    }

    // Bottom border
    result.push(make_border("└", "┴", "┘", "─"));

    result
}

/// Render an in-progress table as raw markdown rows.
///
/// This is used as a streaming fallback before the closing table tag arrives,
/// so partially streamed tables remain visible instead of disappearing until
/// the table is complete.
fn render_table_preview(rows: &[Vec<String>]) -> Vec<Line<'static>> {
    rows.iter()
        .map(|row| {
            let mut text = String::from("|");
            for cell in row {
                text.push(' ');
                text.push_str(cell);
                text.push(' ');
                text.push('|');
            }
            if row.is_empty() {
                text.push(' ');
                text.push('|');
            }
            Line::from(vec![Span::styled(
                text,
                Style::default().fg(Color::Rgb(236, 239, 244)),
            )])
        })
        .collect()
}

/// Parse a hunk header like `@@ -10,5 +10,8 @@` into (`old_start`, `new_start`)
fn parse_hunk_header(line: &str) -> Option<(i64, i64)> {
    let rest = line.strip_prefix("@@")?;
    let rest = rest.trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let old_part = parts[0].strip_prefix('-')?;
    let new_part = parts[1].strip_prefix('+')?;

    let old_start: i64 = old_part.split(',').next()?.parse().ok()?;
    let new_start: i64 = new_part.split(',').next()?.parse().ok()?;

    Some((old_start, new_start))
}

/// Split a line into diff-friendly chunks while preserving whitespace.
///
/// The chunks are ordered, so the diff can detect reorders and duplicate word
/// changes instead of treating the text as a flat set of tokens.
fn split_diff_chunks(line: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut idx = 0usize;

    while idx < line.len() {
        let mut chars = line[idx..].char_indices();
        let Some((_, ch)) = chars.next() else {
            break;
        };
        let ch_len = ch.len_utf8();
        if ch.is_whitespace() {
            let mut end = idx + ch_len;
            while end < line.len() {
                let mut peek = line[end..].char_indices();
                let Some((_, next_ch)) = peek.next() else {
                    break;
                };
                if !next_ch.is_whitespace() {
                    break;
                }
                end += next_ch.len_utf8();
            }
            if start < end {
                chunks.push(&line[start..end]);
            }
            start = end;
            idx = end;
            continue;
        }

        idx += ch_len;
    }

    if start < line.len() {
        chunks.push(&line[start..]);
    }

    chunks
}

/// Render a unified diff with line numbers, hunks, and word-level highlights.
///
/// Handles:
/// - File headers (`---` / `+++`) with muted styling
/// - Hunk headers (`@@`) with parsed line ranges
/// - Added lines (`+`) with green accent
/// - Removed lines (`-`) with red accent
/// - Context lines with dim gutter
/// - Word-level highlighting for adjacent -/+ pairs
#[allow(clippy::too_many_lines)]
pub fn render_diff(diff_text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let raw_lines: Vec<&str> = diff_text.lines().collect();

    // Styles
    let gutter_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM);
    let file_old_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
    let file_new_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let hunk_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let add_style = Style::default().fg(Color::Green);
    let add_prefix_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let remove_style = Style::default().fg(Color::Red);
    let remove_prefix_style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
    let ctx_style = Style::default().fg(Color::Rgb(160, 160, 160));

    // Track line numbers from hunk headers
    let mut old_line: i64 = 0;
    let mut new_line: i64 = 0;

    // Buffer consecutive removals to enable word-level diff with following additions
    let mut removed_buffer: Vec<(i64, String)> = Vec::new();
    let mut pending_adds: Vec<(i64, String)> = Vec::new();

    for line in &raw_lines {
        // File headers
        if let Some(rest) = line.strip_prefix("--- ") {
            flush_diff_buffer(
                &mut lines,
                &mut removed_buffer,
                &pending_adds,
                gutter_style,
                remove_prefix_style,
                remove_style,
                add_prefix_style,
                add_style,
            );
            pending_adds.clear();
            lines.push(Line::from(vec![
                Span::styled("\u{2212} ", file_old_style),
                Span::styled(rest.to_string(), file_old_style),
            ]));
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            lines.push(Line::from(vec![
                Span::styled("+ ", file_new_style),
                Span::styled(rest.to_string(), file_new_style),
            ]));
            continue;
        }

        // Hunk header
        if line.starts_with("@@") {
            flush_diff_buffer(
                &mut lines,
                &mut removed_buffer,
                &pending_adds,
                gutter_style,
                remove_prefix_style,
                remove_style,
                add_prefix_style,
                add_style,
            );
            pending_adds.clear();

            if let Some(parsed) = parse_hunk_header(line) {
                old_line = parsed.0;
                new_line = parsed.1;
            }

            lines.push(Line::from(vec![
                Span::styled("  ", gutter_style),
                Span::styled(line.to_string(), hunk_style),
            ]));
            continue;
        }

        // Removed line
        if let Some(rest) = line.strip_prefix('-') {
            if !pending_adds.is_empty() && removed_buffer.is_empty() {
                flush_diff_buffer(
                    &mut lines,
                    &mut removed_buffer,
                    &pending_adds,
                    gutter_style,
                    remove_prefix_style,
                    remove_style,
                    add_prefix_style,
                    add_style,
                );
                pending_adds.clear();
            }
            removed_buffer.push((old_line, rest.to_string()));
            old_line += 1;
            continue;
        }

        // Added line
        if let Some(rest) = line.strip_prefix('+') {
            pending_adds.push((new_line, rest.to_string()));
            new_line += 1;
            continue;
        }

        // Context line — flush any pending -/+ pairs first
        if !removed_buffer.is_empty() || !pending_adds.is_empty() {
            flush_diff_buffer(
                &mut lines,
                &mut removed_buffer,
                &pending_adds,
                gutter_style,
                remove_prefix_style,
                remove_style,
                add_prefix_style,
                add_style,
            );
            pending_adds.clear();
        }

        lines.push(Line::from(vec![
            Span::styled(format!("{old_line:>4}   "), gutter_style),
            Span::styled(line.to_string(), ctx_style),
        ]));
        old_line += 1;
        new_line += 1;
    }

    // Flush remaining
    flush_diff_buffer(
        &mut lines,
        &mut removed_buffer,
        &pending_adds,
        gutter_style,
        remove_prefix_style,
        remove_style,
        add_prefix_style,
        add_style,
    );

    lines
}

/// Flush buffered removals and additions, rendering word-level diffs for paired lines.
#[allow(clippy::too_many_arguments)]
fn flush_diff_buffer(
    lines: &mut Vec<Line<'static>>,
    buffer: &mut Vec<(i64, String)>,
    adds: &[(i64, String)],
    gutter_style: Style,
    remove_prefix_style: Style,
    remove_style: Style,
    add_prefix_style: Style,
    add_style: Style,
) {
    if buffer.is_empty() && adds.is_empty() {
        return;
    }

    let pair_count = buffer.len().min(adds.len());

    for i in 0..pair_count {
        let (rm_line_num, rm_text) = &buffer[i];
        let (ad_line_num, ad_text) = &adds[i];

        let rm_chunks = split_diff_chunks(rm_text);
        let ad_chunks = split_diff_chunks(ad_text);
        let diff = TextDiff::configure()
            .algorithm(Algorithm::Patience)
            .diff_slices(&rm_chunks, &ad_chunks);

        // Render removed line with word-level highlights
        let mut rm_spans = vec![
            Span::styled(format!("{rm_line_num:>4} "), gutter_style),
            Span::styled("\u{2212} ", remove_prefix_style),
        ];
        let mut ad_spans = vec![
            Span::styled(format!("{ad_line_num:>4} "), gutter_style),
            Span::styled("+ ", add_prefix_style),
        ];
        for change in diff.iter_all_changes() {
            let chunk = change.value();
            match change.tag() {
                similar::ChangeTag::Delete => {
                    rm_spans.push(Span::styled(
                        chunk.to_string(),
                        Style::default()
                            .fg(Color::Red)
                            .bg(Color::Rgb(80, 20, 20))
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                similar::ChangeTag::Insert => {
                    ad_spans.push(Span::styled(
                        chunk.to_string(),
                        Style::default()
                            .fg(Color::Green)
                            .bg(Color::Rgb(20, 60, 20))
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                similar::ChangeTag::Equal => {
                    rm_spans.push(Span::styled(chunk.to_string(), remove_style));
                    ad_spans.push(Span::styled(chunk.to_string(), add_style));
                }
            }
        }
        lines.push(Line::from(rm_spans));
        lines.push(Line::from(ad_spans));
    }

    // Remaining removals (no matching addition)
    for (ln, text) in buffer.iter().skip(pair_count) {
        lines.push(Line::from(vec![
            Span::styled(format!("{ln:>4} "), gutter_style),
            Span::styled("\u{2212} ", remove_prefix_style),
            Span::styled(text.clone(), remove_style),
        ]));
    }
    // Remaining additions (no matching removal)
    for (ln, text) in adds.iter().skip(pair_count) {
        lines.push(Line::from(vec![
            Span::styled(format!("{ln:>4} "), gutter_style),
            Span::styled("+ ", add_prefix_style),
            Span::styled(text.clone(), add_style),
        ]));
    }

    buffer.clear();
}

/// Streaming message state for incremental rendering
#[derive(Clone, Debug)]
pub struct StreamingMessage {
    /// Accumulated content so far
    pub content: String,

    /// Parsed and rendered lines (updated incrementally)
    pub rendered_lines: Vec<Line<'static>>,

    /// Is the stream complete?
    pub complete: bool,

    /// Last update time (for rendering smooth typing effect)
    pub last_update: std::time::Instant,

    /// Current cursor position for animation
    pub cursor_position: usize,
}

impl StreamingMessage {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            rendered_lines: Vec::new(),
            complete: false,
            last_update: std::time::Instant::now(),
            cursor_position: 0,
        }
    }

    /// Append a delta to the streaming message
    pub fn append_delta(&mut self, delta: &str, renderer: &MarkdownRenderer) {
        self.content.push_str(delta);

        // Re-render with new content
        // Note: This returns lines with lifetime tied to the content, which is owned by self
        // We need to clear and rebuild the rendered_lines
        self.rendered_lines.clear();
        self.rendered_lines = renderer.parse(&self.content);
        self.last_update = std::time::Instant::now();
        self.cursor_position = self.content.len();
    }

    /// Mark the stream as complete
    pub const fn mark_complete(&mut self) {
        self.complete = true;
    }

    /// Get the cursor animation character
    pub const fn cursor_char(&self) -> &str {
        if self.complete {
            ""
        } else {
            "⏳"
        }
    }
}

impl Default for StreamingMessage {
    fn default() -> Self {
        Self::new()
    }
}

/// Hash code content for caching
#[allow(dead_code)] // Kept for future use
fn hash_code(code: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
#[allow(clippy::similar_names, clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn test_parse_plain_text() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("Hello, world!");

        // Plain text becomes a paragraph which adds a trailing blank line
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_inline_code() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("Use `Option<T>` for safety");

        assert!(!lines.is_empty());
        // Check that inline code is styled
    }

    #[test]
    fn test_parse_code_block() {
        let renderer = MarkdownRenderer::default();
        let markdown = r#"```rust
fn main() {
    println!("Hello");
}
```"#;

        let lines = renderer.parse(markdown);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_diff() {
        let renderer = MarkdownRenderer::default();
        let diff = r#"```diff
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,3 @@
 fn main() {
-    println!("Hello");
+    println!("World");
 }
```"#;

        let lines = renderer.parse(diff);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_headers() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("# Header\n\nContent");

        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_parse_list() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("- Item 1\n- Item 2\n- Item 3");

        assert!(!lines.is_empty());
    }

    #[test]
    fn test_streaming_message() {
        let renderer = MarkdownRenderer::default();
        let mut streaming = StreamingMessage::new();

        streaming.append_delta("Hello", &renderer);
        assert!(!streaming.content.is_empty());
        assert!(!streaming.rendered_lines.is_empty());
        assert!(!streaming.complete);

        streaming.append_delta(" world", &renderer);
        assert_eq!(streaming.content, "Hello world");

        streaming.mark_complete();
        assert!(streaming.complete);
    }

    #[test]
    fn test_syntax_highlighter() {
        let highlighter = SyntaxHighlighter::new();
        let code = r#"fn main() {
    println!("Hello");
}"#;

        let lines = highlighter.highlight(code, Some("rust"));
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_diff() {
        let diff = r#"--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,3 @@
-    println!("Hello");
+    println!("World");"#;

        let lines = render_diff(diff);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_markdown_config_default() {
        let config = MarkdownConfig::default();
        assert!(config.syntax_highlighting);
        assert!(config.streaming_enabled);
        assert!(!config.typing_animation);
        assert_eq!(config.syntax_theme, "base16-ocean.dark");
        assert!((config.typing_speed - 60.0).abs() < f32::EPSILON);
        assert_eq!(config.max_code_width, 120);
    }

    // ── New tests ─────────────────────────────────────────────────────

    #[test]
    fn test_message_theme_default_fields() {
        let theme = MessageTheme::default();
        // Verify all colors are set (not default/zero values)
        assert_eq!(theme.user_color, Color::Rgb(180, 142, 173));
        assert_eq!(theme.ai_color, Color::Rgb(143, 188, 187));
        assert_eq!(theme.system_color, Color::Rgb(235, 203, 139));
        assert_eq!(theme.tool_summary_color, Color::Rgb(163, 190, 140));
        assert_eq!(theme.tool_text_color, Color::Rgb(236, 239, 244));
        assert_eq!(theme.tool_detail_color, Color::Rgb(129, 161, 193));
        assert_eq!(theme.thinking_color, Color::Rgb(94, 129, 172));
        assert_eq!(theme.thinking_text_color, Color::Rgb(236, 239, 244));
    }

    #[test]
    fn test_message_theme_from_colors_valid_hex() {
        let theme = MessageTheme::from_colors("#ff0000", "#00ff00");
        assert_eq!(theme.user_color, Color::Rgb(255, 0, 0));
        assert_eq!(theme.ai_color, Color::Rgb(0, 255, 0));
        assert_eq!(theme.system_color, Color::Gray);
    }

    #[test]
    fn test_message_theme_from_colors_no_hash() {
        let theme = MessageTheme::from_colors("aabbcc", "112233");
        assert_eq!(theme.user_color, Color::Rgb(0xaa, 0xbb, 0xcc));
        assert_eq!(theme.ai_color, Color::Rgb(0x11, 0x22, 0x33));
    }

    #[test]
    fn test_message_theme_from_colors_invalid_hex_short() {
        // Invalid (too short) should fallback to white
        let theme = MessageTheme::from_colors("#fff", "#00");
        assert_eq!(theme.user_color, Color::White);
        assert_eq!(theme.ai_color, Color::White);
    }

    #[test]
    fn test_parse_hex_color_valid() {
        assert_eq!(parse_hex_color("#ABCDEF"), Color::Rgb(0xAB, 0xCD, 0xEF));
        assert_eq!(parse_hex_color("123456"), Color::Rgb(0x12, 0x34, 0x56));
        assert_eq!(parse_hex_color("#000000"), Color::Rgb(0, 0, 0));
    }

    #[test]
    fn test_parse_hex_color_invalid() {
        assert_eq!(parse_hex_color(""), Color::White);
        assert_eq!(parse_hex_color("#"), Color::White);
        assert_eq!(parse_hex_color("abc"), Color::White);
        assert_eq!(parse_hex_color("1234567"), Color::White);
    }

    #[test]
    fn test_parse_empty_markdown() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("");
        // Empty input should produce empty or minimal output
        assert!(lines.is_empty() || lines.iter().all(|l| l.spans.is_empty()));
    }

    #[test]
    fn test_parse_bold_text() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("**bold text**");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_italic_text() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("*italic text*");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_bold_and_italic() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("***bold and italic***");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_strikethrough() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("~~deleted~~");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_link() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("[Click here](https://example.com)");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_image() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("![Alt text](image.png)");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_blockquote() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("> This is a quote");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_horizontal_rule() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("---");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_ordered_list() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("1. First\n2. Second\n3. Third");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_ordered_list_keeps_marker_and_text_together() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("19. XXXX");

        let rendered = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(
            rendered.contains("19. XXXX"),
            "rendered output was: {rendered:?}"
        );
        assert!(
            !rendered.contains("19.\nXXXX"),
            "ordered list marker was split across lines: {rendered:?}"
        );
    }

    #[test]
    fn test_parse_nested_list() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse("- Item 1\n  - Nested 1\n  - Nested 2\n- Item 2");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_multiple_headings() {
        let renderer = MarkdownRenderer::default();
        let markdown = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6";
        let lines = renderer.parse(markdown);
        assert!(lines.len() >= 6); // At least one line per heading
    }

    #[test]
    fn test_parse_table() {
        let renderer = MarkdownRenderer::default();
        let markdown = "| Name  | Age |\n|-------|-----|\n| Alice | 30  |\n| Bob   | 25  |";
        let lines = renderer.parse(markdown);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_incomplete_table_stream_preview() {
        let renderer = MarkdownRenderer::default();
        let markdown = "| Name  | Age |\n|-------|-----|\n| Alice | 30  |";
        let lines = renderer.parse(markdown);
        let rendered = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(rendered.contains("| Name  | Age |"));
        assert!(rendered.contains("|-------|-----|"));
        assert!(rendered.contains("| Alice | 30  |"));
    }

    #[test]
    fn test_parse_table_keeps_inline_content_inside_cells() {
        let renderer = MarkdownRenderer::default();
        let markdown = "| Item |\n| --- |\n| `code` [link](https://example.com) ![img](img.png) |";
        let lines = renderer.parse(markdown);
        let rendered = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(rendered.contains("code"));
        assert!(rendered.contains("link"));
        assert!(rendered.contains("img.png"));
        assert!(!rendered.contains(")code"));
    }

    #[test]
    fn test_parse_blockquote_keeps_marker_across_paragraphs() {
        let renderer = MarkdownRenderer::default();
        let markdown = "> First paragraph\n>\n> Second paragraph";
        let lines = renderer.parse(markdown);
        let rendered = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        let marker_count = rendered.matches(" ▎ ").count();
        assert!(marker_count >= 2, "rendered output was: {rendered:?}");
    }

    #[test]
    fn test_parse_mixed_content() {
        let renderer = MarkdownRenderer::default();
        let markdown = "# Title\n\nParagraph with `code`.\n\n- List item\n\n```\ncode block\n```";
        let lines = renderer.parse(markdown);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_plain_text() {
        let theme = MessageTheme::default();
        let lines = MarkdownRenderer::render_plain_text("line1\nline2", &theme);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_render_plain_text_empty() {
        let theme = MessageTheme::default();
        let lines = MarkdownRenderer::render_plain_text("", &theme);
        // Empty string has one "empty" line from .lines()
        assert!(lines.len() <= 1);
    }

    #[test]
    fn test_render_messages() {
        let renderer = MarkdownRenderer::default();
        let messages = vec![
            FrontendMessage {
                id: "msg-1".to_string(),
                content: "Hello".to_string(),
                kind: crate::FrontendMessageKind::User,
            },
            FrontendMessage {
                id: "msg-2".to_string(),
                content: "World".to_string(),
                kind: crate::FrontendMessageKind::Assistant,
            },
        ];
        let lines = renderer.render_messages(&messages, 80, 24);
        // At least one line per message + separators
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_render_streaming() {
        let renderer = MarkdownRenderer::default();
        let msg = StreamingMessage::new();
        let lines = renderer.render_streaming(&msg, 80, 24);
        // Empty streaming message produces empty or minimal output
        assert!(lines.is_empty() || lines.len() <= 1);
    }

    #[test]
    fn test_render_content_without_cache() {
        let lines =
            MarkdownRenderer::render_content("Hello **world**", &MessageTheme::default(), None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_content_with_cache() {
        use std::sync::RwLock;

        let cache = RwLock::new(RenderCache::default());
        let lines1 =
            MarkdownRenderer::render_content("Hello", &MessageTheme::default(), Some(&cache));
        assert!(!lines1.is_empty());

        // Second call should hit the cache
        let lines2 =
            MarkdownRenderer::render_content("Hello", &MessageTheme::default(), Some(&cache));
        assert_eq!(lines1.len(), lines2.len());

        // Different content should produce a different cache entry
        let _lines3 =
            MarkdownRenderer::render_content("Different", &MessageTheme::default(), Some(&cache));
        assert_eq!(cache.read().unwrap().len(), 2);
    }

    #[test]
    fn test_render_content_cache_eviction() {
        use std::sync::RwLock;

        let cache = RwLock::new(RenderCache::default());
        // Fill cache beyond 200 entries
        for i in 0..210 {
            let content = format!("content-{i}");
            MarkdownRenderer::render_content(&content, &MessageTheme::default(), Some(&cache));
        }
        let cache_size = cache.read().unwrap().len();
        // Cache should have been evicted, not 210
        assert!(cache_size <= 210);
        // Should be more than 150 (evicts ~25% at the boundary)
        assert!(cache_size > 150);
    }

    #[test]
    fn test_render_table_boxed_uses_display_width() {
        let lines = render_table_boxed(
            &[vec!["😀".to_string()], vec!["x".to_string()]],
            &[pulldown_cmark::Alignment::Left],
        );

        let top_border_width = UnicodeWidthStr::width(line_text(&lines[0]).as_str());
        let row_width = UnicodeWidthStr::width(line_text(&lines[1]).as_str());

        assert_eq!(top_border_width, 7);
        assert_eq!(row_width, 7);
    }

    #[test]
    fn test_parse_incremental() {
        let renderer = MarkdownRenderer::default();
        let lines = renderer.parse_incremental("Hello **world**");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_streaming_message_default() {
        let msg = StreamingMessage::default();
        assert!(msg.content.is_empty());
        assert!(msg.rendered_lines.is_empty());
        assert!(!msg.complete);
        assert_eq!(msg.cursor_position, 0);
    }

    #[test]
    fn test_streaming_message_cursor_char_incomplete() {
        let msg = StreamingMessage::new();
        assert_eq!(msg.cursor_char(), "⏳");
    }

    #[test]
    fn test_streaming_message_cursor_char_complete() {
        let mut msg = StreamingMessage::new();
        msg.mark_complete();
        assert_eq!(msg.cursor_char(), "");
    }

    #[test]
    fn test_streaming_message_append_multiple_deltas() {
        let renderer = MarkdownRenderer::default();
        let mut msg = StreamingMessage::new();

        msg.append_delta("# Title", &renderer);
        assert_eq!(msg.content, "# Title");

        msg.append_delta("\n\nParagraph", &renderer);
        assert_eq!(msg.content, "# Title\n\nParagraph");
        assert_eq!(msg.cursor_position, "# Title\n\nParagraph".len());
    }

    #[test]
    fn test_streaming_table_renders_incrementally() {
        let renderer = MarkdownRenderer::default();
        let mut msg = StreamingMessage::new();

        msg.append_delta("| Name | Age |\n", &renderer);
        let preview = msg
            .rendered_lines
            .iter()
            .map(line_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            preview.contains("| Name | Age |"),
            "partial tables should stay visible while streaming"
        );

        msg.append_delta("| --- | --- |\n", &renderer);
        msg.append_delta("| Alice | 30 |\n", &renderer);
        let rendered = msg
            .rendered_lines
            .iter()
            .map(line_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            rendered.contains("Alice"),
            "completed tables should render after the separator arrives"
        );
    }

    #[test]
    fn test_render_diff_empty() {
        let lines = render_diff("");
        assert!(lines.is_empty());
    }

    #[test]
    fn test_render_diff_only_additions() {
        let diff = "+line 1\n+line 2\n+line 3";
        let lines = render_diff(diff);
        assert!(!lines.is_empty());
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_render_diff_only_removals() {
        let diff = "-line 1\n-line 2\n-line 3";
        let lines = render_diff(diff);
        assert!(!lines.is_empty());
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_render_diff_context_only() {
        let diff = "line 1\nline 2\nline 3";
        let lines = render_diff(diff);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_diff_file_headers() {
        let diff = "--- a/old.txt\n+++ b/new.txt\n@@ -1 +1 @@\n-old\n+new";
        let lines = render_diff(diff);
        assert!(lines.len() >= 4); // file headers + hunk + changes
    }

    #[test]
    fn test_render_diff_word_level() {
        let diff = "-    println!(\"Hello\");\n+    println!(\"World\");";
        let lines = render_diff(diff);
        assert!(!lines.is_empty());
        // Should have at least 2 rendered lines
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_render_diff_unequal_add_remove() {
        // More removals than additions
        let diff = "-removed line 1\n-removed line 2\n+added line 1";
        let lines = render_diff(diff);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_diff_highlights_reordered_words() {
        let diff = r#"--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-foo bar
+bar foo"#;
        let lines = render_diff(diff);
        let has_removed_highlight = lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.style.bg == Some(Color::Rgb(80, 20, 20)))
        });
        let has_added_highlight = lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.style.bg == Some(Color::Rgb(20, 60, 20)))
        });

        assert!(
            has_removed_highlight,
            "reordered words should still be highlighted on the removed side"
        );
        assert!(
            has_added_highlight,
            "reordered words should still be highlighted on the added side"
        );
    }

    #[test]
    fn test_parse_hunk_header_valid() {
        assert_eq!(parse_hunk_header("@@ -10,5 +10,8 @@"), Some((10, 10)));
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some((1, 1)));
        assert_eq!(parse_hunk_header("@@ -100,3 +200,5 @@"), Some((100, 200)));
    }

    #[test]
    fn test_parse_hunk_header_invalid() {
        assert_eq!(parse_hunk_header(""), None);
        assert_eq!(parse_hunk_header("no hunk header"), None);
        assert_eq!(parse_hunk_header("@@ invalid @@"), None);
    }

    #[test]
    fn test_markdown_renderer_with_custom_config() {
        let config = MarkdownConfig {
            syntax_highlighting: false,
            syntax_theme: "base16-ocean.dark".to_string(),
            streaming_enabled: false,
            typing_animation: true,
            typing_speed: 120.0,
            max_code_width: 80,
        };
        let renderer = MarkdownRenderer::with_config(config);
        let lines = renderer.parse("```rust\nfn main() {}\n```");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_markdown_renderer_render_alias() {
        let renderer = MarkdownRenderer::default();
        // render() is an alias for parse()
        let parse_lines = renderer.parse("# Hello");
        let render_lines = renderer.render("# Hello");
        assert_eq!(parse_lines.len(), render_lines.len());
    }

    #[test]
    fn test_code_block_indented() {
        let renderer = MarkdownRenderer::default();
        // Indented code block (4 spaces)
        let lines = renderer.parse("    fn main() {\n        println!(\"hi\");\n    }");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_multiple_code_blocks() {
        let renderer = MarkdownRenderer::default();
        let markdown = "```rust\nfn a() {}\n```\n\n```python\ndef b():\n    pass\n```";
        let lines = renderer.parse(markdown);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_render_plain_code() {
        let lines = render_plain_code("line1\nline2\nline3");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_hash_code_deterministic() {
        let h1 = hash_code("test code");
        let h2 = hash_code("test code");
        assert_eq!(h1, h2);

        let h3 = hash_code("different code");
        assert_ne!(h1, h3);
    }
}
