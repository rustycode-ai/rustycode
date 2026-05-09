/// Max display width for collapsed message first-line preview.
const COLLAPSED_PREVIEW_MAX_WIDTH: usize = 60;
/// Max content lines for expanded thinking block display.
const THINKING_MAX_DISPLAY_LINES: usize = 8;
/// Max string length before treating as a likely file path.
const PATH_DETECT_MAX_LEN: usize = 200;
/// Border character repeat count for thinking/code block frames.
const BLOCK_BORDER_WIDTH: usize = 30;

impl PolishedRenderer {
    /// Render messages area with line-based auto-scrolling
    pub fn render_messages(&self, tui: &mut TUI, frame: &mut Frame, area: Rect) {
        let debug_enabled = crate::logging::is_debug_enabled();
        let render_start = std::time::Instant::now();
        use ratatui::layout::Alignment;
        use ratatui::text::Line;
        use ratatui::widgets::Paragraph;
        use rustycode_ui_core::MessageTheme;

        // Clear previous message areas for click detection
        tui.clear_message_areas();

        // Calculate how many lines fit in viewport
        let viewport_height = area.height as usize;

        // If no user/assistant conversation yet, show helpful empty state with context
        // (System messages like "Workspace loaded" don't count as conversation)
        let has_conversation = tui.messages.iter().any(|m| {
            matches!(
                m.role,
                crate::ui::message::MessageRole::User | crate::ui::message::MessageRole::Assistant
            )
        });
        if !has_conversation {
            let center_y = area.height / 2;
            let mut lines = Vec::new();

            // Add top padding for centering
            for _ in 0..center_y.saturating_sub(5) {
                lines.push(Line::raw(""));
            }

            // ASCII art logo (compact 1-line)
            lines.push(Line::from(vec![
                ratatui::text::Span::styled(
                    "rustycode",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
                ratatui::text::Span::styled(" v0.1", Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::raw(""));

            // Context info (claw-code pattern: show model, project, branch)
            let project_name = tui
                .services
                .cwd()
                .file_name()
                .and_then(|n: &std::ffi::OsStr| n.to_str())
                .unwrap_or("unknown");
            // Stable per-session index derived from project name
            let greeting_idx = project_name
                .bytes()
                .fold(0u8, |a: u8, b: u8| a.wrapping_add(b))
                as usize;

            lines.push(Line::raw(""));
            {
                // Rotating greeting messages
                const GREETINGS: &[&str] = &[
                    "What would you like to build?",
                    "Ready to code something amazing?",
                    "What shall we create today?",
                    "Let's write some Rust!",
                    "What's on your mind?",
                    "How can I help you today?",
                    "Ready to ship some features?",
                    "What should we work on?",
                    "Let's get productive!",
                    "Your codebase awaits...",
                ];
                // Stable per session: hash project name for deterministic greeting
                let greeting = GREETINGS[greeting_idx % GREETINGS.len()];
                lines.push(Line::from(vec![ratatui::text::Span::styled(
                    greeting.to_string(),
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            lines.push(Line::from(vec![ratatui::text::Span::styled(
                {
                    // Rotate through different tips for discoverability
                    // Changes every ~5 seconds based on animation frame
                    const TIPS: &[&str] = &[
                        "? help  ·  / commands  ·  ! bash  ·  Ctrl+K / Ctrl+Shift+P palette",
                        "Ctrl+X editor  ·  Ctrl+S stash  ·  Ctrl+R search history",
                        "Shift+Up/Down = turn jump  ·  Alt+E/W expand/collapse all",
                        "Tab = toggle tools  ·  Ctrl+P tool panel  ·  Ctrl+B sessions",
                        "Ctrl+Q to quit  ·  Ctrl+C to cancel  ·  Esc to stop",
                    ];
                    let tip_idx = (greeting_idx + tui.animator.current_frame().progress_frame / 20)
                        % TIPS.len();
                    TIPS[tip_idx]
                },
                Style::default().fg(Color::DarkGray),
            )]));

            // Check if API key is missing and show a warning (cached in TUI struct)
            if !tui.api_key_warning.is_empty() {
                lines.push(Line::raw(""));
                lines.push(Line::from(vec![ratatui::text::Span::styled(
                    format!("  {}", tui.api_key_warning),
                    Style::default().fg(Color::Rgb(255, 200, 80)),
                )]));
            }

            let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
            return;
        }

        // Use default theme for rendering
        let theme = MessageTheme::default();

        // Render all messages with vertical border (using MessageRenderer)
        let mut render_chunks: Vec<(usize, Color, Vec<Line>, bool)> = Vec::new();

        // Pre-estimate total lines to determine which messages are visible.
        // This avoids expensive markdown rendering for messages entirely off-screen.
        //
        // When the user is manually browsing history, prefer correctness over
        // the skip-ahead optimization. The approximation can undershoot on
        // complex markdown/tool content and accidentally skip every visible
        // message, producing a blank viewport.
        let safe_viewport_height = viewport_height.max(1);
        // Content width for wrapped line estimation (border column + space prefix)
        let est_content_width = area.width.saturating_sub(1).max(1) as usize;

        // Compute full estimated total from ALL messages (including fast-skipped ones).
        // This is reused for both auto-scroll start calculation and last_total_lines.
        let mut full_estimated_total: usize = 0;
        {
            let mut prev_was_system = false;
            for msg in &tui.messages {
                let is_system = matches!(msg.role, crate::ui::message::MessageRole::System);
                let separator = if prev_was_system && is_system { 0 } else { 1 };
                let msg_lines = tui
                    .message_renderer
                    .estimate_message_height(msg, est_content_width)
                    + separator;
                full_estimated_total += msg_lines.max(1);
                prev_was_system = is_system;
            }
        }

        let estimated_auto_scroll_start = if !tui.view.user_scrolled {
            full_estimated_total.saturating_sub(safe_viewport_height)
        } else {
            tui.view.scroll_offset_line
        };

        // Track cumulative estimated lines to skip messages above viewport
        let mut est_cumulative: usize = 0;
        let mut all_above_viewport = true;
        let use_fast_skip = !tui.view.user_scrolled;
        // Estimate viewport end to skip messages below it
        let estimated_viewport_end = estimated_auto_scroll_start + safe_viewport_height + 10; // +10 buffer
        let mut estimated_msg_lines = Vec::with_capacity(tui.messages.len());

        for (msg_idx, msg) in tui.messages.iter().enumerate() {
            // Get vertical bar style (determines border color)
            let tc = tui.theme_colors.lock().unwrap_or_else(|e| e.into_inner());
            let (pipe_char, pipe_color) = tc.message_pipe_style(&msg.role);
            drop(tc);

            // Fast skip: estimate this message's line count and skip if entirely
            // above the viewport. This avoids expensive markdown rendering for
            // messages the user can't see (especially important in long conversations).
            let est_msg_lines = tui
                .message_renderer
                .estimate_message_height(msg, est_content_width);
            estimated_msg_lines.push(est_msg_lines);
            let prev_is_system = msg_idx > 0
                && matches!(
                    tui.messages.get(msg_idx - 1),
                    Some(m) if matches!(m.role, crate::ui::message::MessageRole::System)
                );
            let is_system = matches!(msg.role, crate::ui::message::MessageRole::System);
            let separator = if msg_idx > 0 && !(prev_is_system && is_system) {
                1
            } else {
                0
            };
            est_cumulative += separator + est_msg_lines;

            if use_fast_skip && all_above_viewport && est_cumulative < estimated_auto_scroll_start {
                // This message is entirely above the viewport — skip expensive rendering
                continue;
            }
            if use_fast_skip && est_cumulative >= estimated_auto_scroll_start {
                all_above_viewport = false;
            }

            // Skip messages well below the viewport to avoid expensive markdown rendering
            if use_fast_skip && est_cumulative > estimated_viewport_end {
                break;
            }

            // Check if message is collapsed
            if msg.collapsed {
                let first_line = msg.content.lines().next().unwrap_or("");
                // For empty content with tool executions, show tool count instead
                let preview = if first_line.is_empty() {
                    if let Some(tools) = &msg.tool_executions {
                        if !tools.is_empty() {
                            format!(
                                "{} tool{}",
                                tools.len(),
                                if tools.len() > 1 { "s" } else { "" }
                            )
                        } else {
                            // Empty content, no tools — show role-based placeholder
                            match msg.role {
                                crate::ui::message::MessageRole::User => {
                                    "(empty message)".to_string()
                                }
                                crate::ui::message::MessageRole::Assistant => {
                                    "(no content)".to_string()
                                }
                                crate::ui::message::MessageRole::System => "(system)".to_string(),
                            }
                        }
                    } else {
                        // Empty content, no tools — show role-based placeholder
                        match msg.role {
                            crate::ui::message::MessageRole::User => "(empty message)".to_string(),
                            crate::ui::message::MessageRole::Assistant => {
                                "(no content)".to_string()
                            }
                            crate::ui::message::MessageRole::System => "(system)".to_string(),
                        }
                    }
                } else if unicode_width::UnicodeWidthStr::width(first_line) > COLLAPSED_PREVIEW_MAX_WIDTH {
                    // floor_char_boundary ensures we don't slice mid-UTF-8
                    let end = first_line.floor_char_boundary(COLLAPSED_PREVIEW_MAX_WIDTH.saturating_sub(3));
                    format!("{}...", &first_line[..end])
                } else {
                    first_line.to_string()
                };

                let line = Line::from(vec![
                    ratatui::text::Span::styled(
                        format!("{} ", pipe_char),
                        Style::default().fg(pipe_color),
                    ),
                    ratatui::text::Span::styled(preview, Style::default().fg(Color::DarkGray)),
                    ratatui::text::Span::styled(
                        " (collapsed)",
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                render_chunks.push((
                    msg_idx,
                    pipe_color,
                    vec![line],
                    matches!(msg.role, crate::ui::message::MessageRole::System),
                ));
            } else {
                // Render markdown content
                let content_lines = tui
                    .message_renderer
                    .render_markdown_content(&msg.content, &theme);

                // Collect spans for this message with highlighting
                let mut lines: Vec<Line> = content_lines
                    .into_iter()
                    .map(|l| Line::from(l.spans))
                    .collect();

                // Apply search highlighting if search is active
                if !tui.search_state.query.is_empty() {
                    lines = apply_search_highlighting(tui, &lines, msg_idx);
                }

                // Append thinking block (collapsed header or expanded content)
                // Skip if thinking is empty or whitespace-only
                if let Some(thinking) = &msg.thinking {
                    if !thinking.trim().is_empty() {
                        lines.push(Line::from(""));
                        lines.extend(render_thinking_block(
                            thinking,
                            msg.thinking_expansion,
                            pipe_char,
                            pipe_color,
                        ));
                    }
                }

                // Append tool execution summary lines
                if let Some(tools) = &msg.tool_executions {
                    if !tools.is_empty() {
                        lines.push(Line::from(""));
                        if msg.content.trim().is_empty() {
                            lines.push(Line::from(vec![ratatui::text::Span::styled(
                                format!(
                                    "  🔧 {} tool{} executed",
                                    tools.len(),
                                    if tools.len() > 1 { "s" } else { "" }
                                ),
                                Style::default().fg(Color::Gray),
                            )]));
                        }
                        lines.extend(render_tool_summary(tools));
                    }
                }

                render_chunks.push((
                    msg_idx,
                    pipe_color,
                    lines,
                    matches!(msg.role, crate::ui::message::MessageRole::System),
                ));
            }
        }

        // Use line-based scroll offset
        // Account for blank separator lines between messages (N-1 messages get a separator)
        // Calculate actual rendered height including wrapped lines
        let available_width = area.width.saturating_sub(1).max(1) as usize;
        let mut total_lines: usize = 0;

        for (_, _, lines, _) in &render_chunks {
            for line in lines {
                // +1 for the space prefix added when building spaced_lines
                let line_width = line.width().saturating_add(1);
                let wrapped_rows = line_width.div_ceil(available_width).max(1);
                total_lines += wrapped_rows;
            }
        }

        // Add separator lines between messages (suppress between consecutive System messages)
        let separator_count: usize = render_chunks
            .windows(2)
            .map(|w| if w[0].3 && w[1].3 { 0 } else { 1 })
            .sum();
        total_lines += separator_count;

        tui.view.last_total_lines.set(full_estimated_total);

        // Ensure viewport_height is at least 1 to avoid division issues
        let safe_viewport_height = viewport_height.max(1);

        // Clamp scroll offset to valid range
        let max_scroll = total_lines.saturating_sub(safe_viewport_height);
        let clamped_scroll = tui.view.scroll_offset_line.min(max_scroll);

        let start_line = if tui.view.user_scrolled {
            clamped_scroll
        } else {
            total_lines.saturating_sub(safe_viewport_height)
        };

        // Render with per-message colored prefix (no borders for clean selection)
        let mut current_line = 0;
        let mut y_offset = 0u16;
        let mut rendered_any = false;
        let mut rendered_messages = 0usize;
        let mut skipped_above = 0usize;

        // Record per-message line offsets for accurate search/turn scrolling.
        // Key: indexed by msg_idx (message index), not chunk position,
        // and accounts for line wrapping (div_ceil) to match scroll coordinates.
        {
            let mut offsets = tui.message_line_offsets.borrow_mut();
            offsets.clear();
            offsets.resize(tui.messages.len(), usize::MAX);
            let mut acc = 0usize;
            for (chunk_idx, (msg_idx, _, lines, is_system)) in render_chunks.iter().enumerate() {
                let prev_is_system =
                    chunk_idx > 0 && render_chunks.get(chunk_idx - 1).is_some_and(|c| c.3);
                let separator = if chunk_idx > 0 && !(prev_is_system && *is_system) {
                    1
                } else {
                    0
                };
                acc += separator;
                if offsets[*msg_idx] == usize::MAX {
                    offsets[*msg_idx] = acc;
                }
                // Account for line wrapping — each rendered line may occupy
                // multiple terminal rows, matching how total_lines is computed.
                for line in lines {
                    let wrapped_rows = line
                        .width()
                        .saturating_add(1)
                        .div_ceil(available_width)
                        .max(1);
                    acc += wrapped_rows;
                }
            }
        }
        for (chunk_idx, (msg_idx, border_color, lines, is_system)) in
            render_chunks.iter().enumerate()
        {
            // Early exit: if we've filled the viewport, stop rendering
            if y_offset >= area.height {
                break;
            }

            let msg_height: usize =
                estimated_msg_lines
                    .get(*msg_idx)
                    .copied()
                    .unwrap_or_else(|| {
                        lines
                            .iter()
                            .map(|l| l.width().saturating_add(1).div_ceil(available_width).max(1))
                            .sum()
                    });
            let prev_is_system =
                chunk_idx > 0 && render_chunks.get(chunk_idx - 1).is_some_and(|c| c.3);
            let separator = if chunk_idx > 0 && !(prev_is_system && *is_system) {
                1
            } else {
                0
            };
            // Skip lines above scroll offset (including separator)
            if current_line + separator + msg_height <= start_line {
                current_line += separator + msg_height;
                skipped_above += 1;
                continue;
            }

            if rendered_any && separator > 0 && y_offset < area.height {
                y_offset = y_offset.saturating_add(1);
            }

            let visible_start = start_line.saturating_sub(current_line + separator);

            let visible_lines: Vec<_> = lines.iter().skip(visible_start).cloned().collect();
            let visible_count = visible_lines.len();

            if visible_count > 0 {
                // Calculate available height for this message
                let remaining = area.height.saturating_sub(y_offset);
                if remaining == 0 {
                    current_line += separator + msg_height;
                    continue;
                }

                // Add space between border and content
                let spaced_lines: Vec<_> = visible_lines
                    .iter()
                    .map(|line| {
                        let mut styled_spans = vec![ratatui::text::Span::raw(" ")];
                        styled_spans.extend(line.spans.iter().cloned());
                        Line::from(styled_spans)
                    })
                    .collect();

                // Compute actual wrapped height from spaced_lines.
                // spaced_lines already include the space prefix, so their .width()
                // is the true content width. Content area = area.width - 1 (border).
                let content_width = area.width.saturating_sub(1).max(1) as usize;
                let char_wrapped_height: u16 = spaced_lines
                    .iter()
                    .map(|l| {
                        if l.width() == 0 {
                            1u16
                        } else {
                            l.width().div_ceil(content_width).max(1) as u16
                        }
                    })
                    .fold(0u16, |acc, rows| acc.saturating_add(rows));
                // +2 buffer: div_ceil assumes char wrapping but ratatui uses word wrapping,
                // which can produce extra rows. The buffer ensures the Clear widget covers
                // all rendered rows, preventing stale content from previous frames.
                let wrapped_height = char_wrapped_height.saturating_add(2);
                let render_height = remaining.min(wrapped_height);

                // Create the full message area (border column + content)
                let msg_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: render_height,
                };

                // Render border in its own 1-char-wide column so terminal
                // mouse selection does NOT capture the border character.
                let border_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: 1,
                    height: render_height,
                };
                let content_area = Rect {
                    x: area.x + 1,
                    y: area.y + y_offset,
                    width: area.width.saturating_sub(1),
                    height: render_height,
                };

                // Clear both areas
                frame.render_widget(Clear, msg_area);

                // Render border column separately (just the left border, no content)
                let border_block = Block::default()
                    .borders(ratatui::widgets::Borders::LEFT)
                    .border_style(Style::default().fg(*border_color));
                frame.render_widget(border_block, border_area);

                // Render content without any border block
                let paragraph = Paragraph::new(spaced_lines)
                    .alignment(Alignment::Left)
                    .block(Block::default())
                    .wrap(Wrap { trim: false });
                frame.render_widget(paragraph, content_area);

                // Register click area for this message (content area only, excludes border)
                tui.register_message_area(*msg_idx, content_area);

                y_offset = y_offset.saturating_add(render_height);
                rendered_any = true;
                rendered_messages += 1;
            }

            current_line += separator + msg_height;
        }

        // Show queued message indicator at bottom when auto-scrolled
        // (dimmed preview of queued message)
        if !tui.view.user_scrolled {
            if let Some(queued) = &tui.streaming.queued_message {
                if y_offset < area.height.saturating_sub(2) {
                    const MAX_PREVIEW_WIDTH: usize = 80;
                    let full_width = unicode_width::UnicodeWidthStr::width(queued.as_str());
                    let (preview, ellipsis) = if full_width > MAX_PREVIEW_WIDTH {
                        let mut width = 0usize;
                        let truncated: String = queued
                            .chars()
                            .take_while(|c| {
                                let cw = unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
                                if width + cw > MAX_PREVIEW_WIDTH - 3 {
                                    false
                                } else {
                                    width += cw;
                                    true
                                }
                            })
                            .collect();
                        (truncated, "...")
                    } else {
                        (queued.clone(), "")
                    };
                    let queued_line = Line::from(vec![
                        ratatui::text::Span::styled(
                            " ⏳ ",
                            Style::default().fg(Color::Rgb(180, 180, 255)),
                        ),
                        ratatui::text::Span::styled(
                            format!("Queued: {}{}", preview, ellipsis),
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(ratatui::style::Modifier::DIM),
                        ),
                    ]);
                    let queued_area = Rect {
                        x: area.x,
                        y: area.y + y_offset.saturating_add(1),
                        width: area.width,
                        height: 1,
                    };
                    frame.render_widget(
                        Paragraph::new(vec![queued_line]).alignment(Alignment::Left),
                        queued_area,
                    );
                }
            }
        }

        // Viewport overflow indicators
        let overflows = total_lines > safe_viewport_height;
        if overflows && tui.view.user_scrolled && area.height > 2 {
            let above = start_line;
            let below = total_lines.saturating_sub(start_line + safe_viewport_height);

            // Top indicator
            if above > 0 {
                let indicator = format!(" ▲ {} more (↑)", above);
                let top_line = Line::from(vec![ratatui::text::Span::styled(
                    indicator,
                    Style::default().fg(Color::DarkGray),
                )]);
                let top_area = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: 1,
                };
                frame.render_widget(Clear, top_area);
                frame.render_widget(
                    Paragraph::new(vec![top_line]).alignment(Alignment::Left),
                    top_area,
                );
            }

            // Bottom indicator — more prominent, clickable
            if below > 0 && (y_offset as usize) < area.height as usize {
                let anim_frame = tui.animator.current_frame();
                let is_streaming = tui.streaming.is_streaming;
                // Use a brighter color when streaming to attract attention
                let indicator_color = if is_streaming {
                    let pulse = (anim_frame.progress_frame / 10).is_multiple_of(2);
                    if pulse {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    }
                } else {
                    Color::DarkGray
                };
                let indicator = if below < 100 {
                    format!(" ▼ {} lines below · End to jump", below)
                } else {
                    format!(
                        " ▼ ~{} lines below · End to jump",
                        (below as f64 / 10.0).round() as usize * 10
                    )
                };
                let bottom_line = Line::from(vec![ratatui::text::Span::styled(
                    indicator,
                    Style::default().fg(indicator_color),
                )]);
                let bottom_area = Rect {
                    x: area.x,
                    y: area.y + area.height.saturating_sub(1),
                    width: area.width,
                    height: 1,
                };
                frame.render_widget(Clear, bottom_area);
                frame.render_widget(
                    Paragraph::new(vec![bottom_line]).alignment(Alignment::Left),
                    bottom_area,
                );
            }
        }

        // Turn indicator when viewing a past turn
        // Shows "turn X/Y" when user navigated to a historical message
        if tui.view.user_scrolled {
            let total_turns = tui
                .messages
                .iter()
                .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
                .count();
            if total_turns > 1 {
                // Find which turn the selected message belongs to
                // Defensive bounds check: messages may have been pruned between
                // renders, making selected_message stale. The .min() clamp handles
                // non-empty cases; the is_empty() check prevents ..=0 panic.
                let safe_end = tui
                    .view
                    .selected_message
                    .min(tui.messages.len().saturating_sub(1));
                if tui.messages.is_empty() {
                    return;
                }
                let current_turn = tui.messages[..=safe_end]
                    .iter()
                    .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
                    .count();
                let is_latest = tui.view.selected_message >= tui.messages.len().saturating_sub(1);
                if !is_latest && current_turn > 0 {
                    let turn_text =
                        format!(" ◈ turn {}/{} — shift+↓ return ", current_turn, total_turns);
                    let turn_line = Line::from(vec![ratatui::text::Span::styled(
                        turn_text,
                        Style::default()
                            .fg(Color::Rgb(255, 200, 80))
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )]);
                    let turn_area = Rect {
                        x: area.x,
                        y: area.y + area.height.saturating_sub(2),
                        width: area.width,
                        height: 1,
                    };
                    frame.render_widget(Clear, turn_area);
                    frame.render_widget(
                        Paragraph::new(vec![turn_line]).alignment(Alignment::Center),
                        turn_area,
                    );
                }
            }
        }

        if debug_enabled {
            let elapsed = render_start.elapsed();
            if elapsed > std::time::Duration::from_millis(2) {
                crate::debug_log!(
                    "Polished message render ran long: messages={} render_chunks={} rendered={} skipped_above={} total_lines={} viewport_height={} elapsed_ms={}",
                    tui.messages.len(),
                    render_chunks.len(),
                    rendered_messages,
                    skipped_above,
                    total_lines,
                    viewport_height,
                    elapsed.as_millis()
                );
            }
        }
    }
}

/// Render a compact tool execution summary for a message.
///
/// Claw-code inspired: shows context-aware tool info with file paths,
/// line counts, and semantic formatting per tool type.
fn render_tool_summary(
    tools: &[crate::ui::message::ToolExecution],
) -> Vec<ratatui::text::Line<'static>> {
    use crate::ui::message::ToolStatus;
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    let mut lines = Vec::new();
    let total = tools.len();
    let passed = tools
        .iter()
        .filter(|t| matches!(t.status, ToolStatus::Complete))
        .count();
    let failed = tools
        .iter()
        .filter(|t| matches!(t.status, ToolStatus::Failed))
        .count();
    let running = tools
        .iter()
        .filter(|t| matches!(t.status, ToolStatus::Running))
        .count();

    // Summary line: ╭─ 3 tools · 2 ok · 1 fail · 450ms ╮
    // Border color reflects overall status (red for failures, gold for running)
    let border_color = if failed > 0 {
        Color::Rgb(255, 80, 80) // Red border when any tool failed
    } else if running > 0 {
        Color::Rgb(255, 200, 80) // Gold border while tools are running
    } else {
        Color::DarkGray
    };
    let mut summary = vec![Span::styled("  ╭─ ", Style::default().fg(border_color))];
    if running > 0 {
        summary.push(Span::styled(
            "◐ ",
            Style::default().fg(Color::Rgb(255, 200, 80)),
        ));
    }
    summary.push(Span::styled(
        format!("{} tool{}", total, if total != 1 { "s" } else { "" }),
        Style::default().fg(Color::Gray),
    ));
    if passed > 0 {
        summary.push(Span::styled(
            format!(" · {} ok", passed),
            Style::default().fg(Color::Rgb(80, 200, 120)),
        ));
    }
    if failed > 0 {
        summary.push(Span::styled(
            format!(" · {} fail", failed),
            Style::default().fg(Color::Rgb(255, 80, 80)),
        ));
    }
    if running > 0 {
        summary.push(Span::styled(
            format!(" · {} running", running),
            Style::default().fg(Color::Rgb(255, 200, 80)),
        ));
    }
    // Show total duration when all tools are complete
    if running == 0 {
        let total_ms: u64 = tools.iter().filter_map(|t| t.duration_ms).sum();
        if total_ms > 0 {
            summary.push(Span::styled(
                format!(" · {}", format_duration(total_ms)),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    lines.push(Line::from(summary));

    // Individual tool lines with context-aware formatting
    for (i, tool) in tools.iter().enumerate() {
        let is_last = i == tools.len() - 1;
        let connector = if is_last { "  ╰─ " } else { "  │ " };

        // Connector color matches tool status
        let connector_color = match tool.status {
            ToolStatus::Failed => Color::Rgb(255, 80, 80),
            ToolStatus::Running => Color::Rgb(255, 200, 80),
            _ => Color::DarkGray,
        };

        let (icon, color) = match tool.status {
            ToolStatus::Running => ("◐", Color::Rgb(255, 200, 80)),
            ToolStatus::Complete => ("●", Color::Rgb(80, 200, 120)),
            ToolStatus::Failed => ("✗", Color::Rgb(255, 80, 80)),
            ToolStatus::Cancelled => ("⚠", Color::Rgb(200, 150, 50)),
        };

        let kind = tool_kind_icon(&tool.name);

        // Extract context-aware detail for this tool (file path, line count, etc.)
        let tool_detail = extract_tool_detail(tool);

        let mut tool_line = vec![
            Span::styled(connector, Style::default().fg(connector_color)),
            Span::styled(format!("{} ", icon), Style::default().fg(color)),
            Span::styled(
                format!("[{}] ", kind),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        // Show tool name with context detail
        if let Some(detail) = &tool_detail {
            tool_line.push(Span::styled(
                tool.name.clone(),
                Style::default().fg(Color::Gray),
            ));
            tool_line.push(Span::styled(
                format!(" {}", detail),
                Style::default().fg(Color::Rgb(180, 180, 180)),
            ));
        } else {
            tool_line.push(Span::styled(
                tool.name.clone(),
                Style::default().fg(Color::Gray),
            ));
        }

        // Duration badge
        if let Some(dur_ms) = tool.duration_ms {
            tool_line.push(Span::styled(
                format!(" {}", format_duration(dur_ms)),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ));
        }

        lines.push(Line::from(tool_line));
    }

    lines
}

/// Extract context-aware detail from a tool execution.
///
/// Claw-code pattern: for file operations, show the file path;
/// for bash, show the command; for search, show match count.
fn extract_tool_detail(tool: &crate::ui::message::ToolExecution) -> Option<String> {
    let lower = tool.name.to_lowercase();
    let summary = &tool.result_summary;
    let detailed = tool.detailed_output.as_deref();

    if lower.contains("read") || lower.contains("cat") || lower.contains("view") {
        // Try input_json for path first (contains the file that was read)
        let path_from_input = tool.input_json.as_ref().and_then(|json| {
            json.get("path")
                .or_else(|| json.get("file_path"))
                .or_else(|| json.get("file"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

        let path = path_from_input
            .or_else(|| detailed.and_then(extract_file_path))
            .or_else(|| extract_file_path(summary));

        let line_count = detailed.map(estimate_line_count).unwrap_or(0);

        if let Some(p) = path {
            return Some(if line_count > 0 {
                format!("{} ({} lines)", shorten_path(&p), line_count)
            } else {
                shorten_path(&p)
            });
        }
        return Some(safe_truncate(summary, 80));
    }
    if lower.contains("write") || lower.contains("create") {
        // Try input_json for path first
        let path_from_input = tool.input_json.as_ref().and_then(|json| {
            json.get("path")
                .or_else(|| json.get("file_path"))
                .or_else(|| json.get("file"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

        let path = path_from_input
            .or_else(|| detailed.and_then(extract_file_path))
            .or_else(|| extract_file_path(summary));

        let line_count = detailed.map(estimate_line_count).unwrap_or(0);

        if let Some(p) = path {
            return Some(if line_count > 0 {
                format!("{} ({} lines)", shorten_path(&p), line_count)
            } else {
                shorten_path(&p)
            });
        }
        return Some(safe_truncate(summary, 80));
    }
    if lower.contains("edit") || lower.contains("patch") || lower.contains("replace") {
        return Some(safe_truncate(
            &extract_file_path(summary).unwrap_or_else(|| summary.clone()),
            80,
        ));
    }
    // Bash/shell: show the command that was run
    if lower.contains("Bash") || lower.contains("exec") || lower.contains("shell") {
        if let Some(cmd) = tool
            .input_json
            .as_ref()
            .and_then(|json| json.get("command").and_then(|v| v.as_str()))
        {
            return Some(safe_truncate(cmd, 60));
        }
        if let Some(output) = &tool.detailed_output {
            let first_line = output.lines().next().unwrap_or("");
            if !first_line.is_empty() && unicode_width::UnicodeWidthStr::width(first_line) < 80 {
                return Some(first_line.to_string());
            }
        }
        return Some(safe_truncate(summary, 60));
    }
    // Search/grep: show match count
    if lower.contains("Grep") || lower.contains("search") {
        return Some(safe_truncate(summary, 80));
    }
    if lower.contains("Glob") || lower.contains("find") || lower.contains("list") {
        // Try to extract path from result_summary first (e.g., "list_directory: /path/to/dir (5 files)")
        if let Some(path) = extract_file_path(&tool.result_summary) {
            if let Some(output) = &tool.detailed_output {
                let count = estimate_line_count(output);
                if count > 0 {
                    return Some(format!("{} ({} files)", path, count));
                }
            }
            return Some(path);
        }
        // Fall back to file count if no path extracted
        if let Some(output) = &tool.detailed_output {
            let count = estimate_line_count(output);
            return Some(format!("{} files", count));
        }
    }

    None
}

/// Try to extract a file path from a tool result summary string.
fn extract_file_path(s: &str) -> Option<String> {
    // Look for common path patterns in tool summaries:
    // "read_file: src/main.rs (145b)" → "src/main.rs"
    // "write_file: src/tree.rs" → "src/tree.rs"
    // "src/main.rs" → "src/main.rs"

    // Try to extract path after colon separator
    if let Some(colon_pos) = s.find(": ") {
        let after_colon = &s[colon_pos + 2..];
        // Take up to first space that looks like metadata (e.g., "(145b)")
        let path_end = after_colon
            .find(" (")
            .or_else(|| after_colon.find(" ["))
            .unwrap_or(after_colon.len());
        let path = &after_colon[..path_end];
        if !path.is_empty() && (path.contains('/') || path.contains('.') || path.contains('\\')) {
            return Some(shorten_path(path));
        }
    }

    // Check if the whole string looks like a path
    if (s.contains('/') || s.contains('\\')) && !s.contains('\n') && s.len() < PATH_DETECT_MAX_LEN {
        return Some(shorten_path(s));
    }

    None
}

// shorten_path and tool_kind_icon are imported from super::shared at the top of this file.

/// Format duration for display — thin wrapper over the shared helper.
#[inline]
fn format_duration(ms: u64) -> String {
    format_duration_ms(ms)
}

/// Render a thinking block (collapsed header or expanded content).
///
/// Shows a compact header line with size indicator when collapsed,
/// or a bordered content section when expanded.
fn render_thinking_block(
    thinking: &str,
    expansion: crate::ui::message_types::ExpansionLevel,
    pipe_char: char,
    pipe_color: Color,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};

    let mut lines = Vec::new();

    let size = thinking.len();
    let size_str = if size == 0 {
        "empty".to_string()
    } else if size < 1024 {
        format!("{}b", size)
    } else {
        format!("{:.1}kb", size as f64 / 1024.0)
    };

    match expansion {
        crate::ui::message_types::ExpansionLevel::Collapsed => {
            // Just header: 💭 [thinking] Nkb [▾ show]
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                Span::styled(
                    format!("💭 [thinking] {} [▾ show]", size_str),
                    Style::default().fg(Color::Rgb(180, 160, 220)),
                ),
            ]));
        }
        crate::ui::message_types::ExpansionLevel::Expanded
        | crate::ui::message_types::ExpansionLevel::Deep => {
            // Header with collapse hint
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                Span::styled(
                    format!("💭 [thinking] {} [▴ hide]", size_str),
                    Style::default().fg(Color::Rgb(180, 160, 220)),
                ),
            ]));

            // Top border
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                Span::styled(
                    format!("┌{}", "─".repeat(BLOCK_BORDER_WIDTH)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));

            // Content lines (max THINKING_MAX_DISPLAY_LINES, with wrapping).
            // Single-pass iteration: collect up to max+1 lines to detect
            // overflow without iterating the full thinking string (which
            // can be 100KB+ for extended thinking models).
            let max_content_lines = THINKING_MAX_DISPLAY_LINES;
            let collected: Vec<&str> = thinking.lines().take(max_content_lines + 1).collect();
            let has_more = collected.len() > max_content_lines;

            for content_line in collected.iter().take(max_content_lines) {
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                    Span::styled("│ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        content_line.to_string(),
                        Style::default().fg(Color::Rgb(160, 150, 200)),
                    ),
                ]));
            }

            if has_more {
                // Estimate remaining lines from byte ratio to avoid full scan
                let shown_bytes: usize = collected
                    .iter()
                    .take(max_content_lines)
                    .map(|l| l.len())
                    .sum();
                let avg_line_bytes = (shown_bytes / max_content_lines.max(1)).max(1);
                let remaining_bytes = thinking.len().saturating_sub(shown_bytes);
                let estimated_remaining = remaining_bytes / avg_line_bytes;
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                    Span::styled(
                        format!("│ ... ~{} more lines", estimated_remaining),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }

            // Bottom border
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", pipe_char), Style::default().fg(pipe_color)),
                Span::styled(
                    format!("└{}", "─".repeat(30)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    lines
}

// safe_truncate is imported from super::shared at the top of this file.
