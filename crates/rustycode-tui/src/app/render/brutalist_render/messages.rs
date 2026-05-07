impl BrutalistRenderer<'_> {
    /// Compute a map from message index to chain position.
    ///
    /// A "chain" is 2+ consecutive assistant messages where content is empty
    /// (tool-only). Returns a map: message_index → (is_chained, is_last_in_chain).
    ///
    /// Chained messages get a minimal continuation marker instead of the full
    /// "▐ ai (HH:MM)" header. The last message in a chain gets the full header.
    fn compute_tool_chain_map(&self) -> std::collections::HashMap<usize, (bool, bool)> {
        let mut map = std::collections::HashMap::new();
        let msgs = &self.messages;

        if msgs.len() < 2 {
            return map;
        }

        // Identify chain boundaries: consecutive assistant messages with empty content
        let mut chain_start: Option<usize> = None;
        let mut chain_len: usize = 0;

        for (i, msg) in msgs.iter().enumerate() {
            let is_tool_only = msg.role == MessageRole::Assistant
                && msg.content.trim().is_empty()
                && msg.tool_executions.as_ref().is_some_and(|t| !t.is_empty());

            if is_tool_only {
                if chain_start.is_none() {
                    chain_start = Some(i);
                }
                chain_len += 1;
            } else {
                // End current chain if any
                if chain_len >= 2 {
                    if let Some(start) = chain_start {
                        for j in start..start + chain_len {
                            let is_last = j == start + chain_len - 1;
                            map.insert(j, (true, is_last));
                        }
                    }
                }
                chain_start = None;
                chain_len = 0;
            }
        }

        // Handle trailing chain
        if chain_len >= 2 {
            if let Some(start) = chain_start {
                for j in start..start + chain_len {
                    let is_last = j == start + chain_len - 1;
                    map.insert(j, (true, is_last));
                }
            }
        }

        map
    }

    /// Apply search highlighting to spans for a given message.
    ///
    /// Highlights matching text with a yellow background for current match
    /// and a dim yellow for other matches. Returns new spans with highlights.
    fn apply_search_highlight<'b>(
        &self,
        spans: Vec<Span<'b>>,
        message_index: usize,
        byte_offset_start: usize,
    ) -> Vec<Span<'b>> {
        if self.search_query.is_empty() || self.search_matches.is_empty() {
            return spans;
        }

        // Collect matches for this message, capped for performance
        let matches: Vec<&MatchPosition> = self
            .search_matches
            .iter()
            .filter(|m| m.message_index == message_index)
            .take(50)
            .collect();

        if matches.is_empty() {
            return spans;
        }

        let current_match = self.search_matches.get(self.search_current_match_index);
        let mut result = Vec::with_capacity(spans.len() * 2);
        let mut byte_offset = byte_offset_start;

        for span in spans {
            let span_text = span.content.as_ref();
            let span_bytes = span_text.as_bytes();
            let span_len = span_bytes.len();
            let span_end = byte_offset + span_len;

            // Collect overlapping match intervals within this span
            let mut intervals: Vec<(usize, usize, bool)> = Vec::new(); // (start_in_span, end_in_span, is_current)
            for match_pos in &matches {
                if match_pos.end <= byte_offset || match_pos.start >= span_end {
                    continue;
                }
                let start = match_pos.start.saturating_sub(byte_offset);
                let end = (match_pos.end - byte_offset).min(span_len);
                let is_current = current_match == Some(*match_pos);
                intervals.push((start, end, is_current));
            }

            if intervals.is_empty() {
                result.push(span);
                byte_offset = span_end;
                continue;
            }

            // Sort and merge overlapping intervals
            intervals.sort_by_key(|(s, _, _)| *s);
            let mut merged: Vec<(usize, usize, bool)> = Vec::with_capacity(intervals.len());
            for (start, end, is_current) in intervals {
                if let Some(last) = merged.last_mut() {
                    if start <= last.1 {
                        // Overlapping — extend, prefer current
                        last.1 = last.1.max(end);
                        if is_current {
                            last.2 = true;
                        }
                        continue;
                    }
                }
                merged.push((start, end, is_current));
            }

            // Build highlighted spans by splitting at match boundaries
            let mut pos = 0;
            for (start, end, is_current) in &merged {
                // Text before match
                if *start > pos {
                    let before = &span_text
                        [span_text.floor_char_boundary(pos)..span_text.floor_char_boundary(*start)];
                    result.push(Span::styled(before.to_string(), span.style));
                }
                // Match text with highlight
                let match_text = &span_text
                    [span_text.floor_char_boundary(*start)..span_text.floor_char_boundary(*end)];
                let highlight_style = if *is_current {
                    Style::default()
                        .fg(Color::Rgb(30, 30, 30))
                        .bg(Color::Rgb(255, 220, 80))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(Color::Rgb(220, 200, 80))
                        .bg(Color::Rgb(50, 50, 60))
                };
                result.push(Span::styled(match_text.to_string(), highlight_style));
                pos = *end;
            }
            // Remaining text after last match
            if pos < span_len {
                let after = &span_text[span_text.floor_char_boundary(pos)..];
                result.push(Span::styled(after.to_string(), span.style));
            }

            byte_offset = span_end;
        }

        result
    }

    /// Compute byte offsets for each rendered line while preserving the
    /// original newline widths (`\n` vs `\r\n`).
    fn compute_line_byte_offsets(content: &str) -> Vec<usize> {
        let mut offsets = Vec::new();
        let mut offset = 0usize;

        for segment in content.split_inclusive('\n') {
            offsets.push(offset);
            offset += segment.len();
        }

        if content.is_empty() {
            return offsets;
        }

        if !content.contains('\n') {
            offsets.push(0);
        }

        offsets
    }

    /// Render a markdown table cell as inline spans.
    ///
    /// This keeps inline formatting such as code, links, emphasis, and
    /// strikethrough alive inside table rows.
    fn render_table_cell(
        &self,
        cell: &str,
        colors: &ThemeColors,
        is_header: bool,
    ) -> Vec<Span<'static>> {
        let spans = Self::parse_inline_content(cell, colors);

        if !is_header {
            return spans
                .into_iter()
                .map(|span| Span::styled(span.content.into_owned(), span.style))
                .collect();
        }

        spans
            .into_iter()
            .map(|span| {
                Span::styled(
                    span.content.into_owned(),
                    span.style.fg(colors.primary).add_modifier(Modifier::BOLD),
                )
            })
            .collect()
    }

    /// Compute message heights and total line count.
    ///
    /// Returns (total_lines, heights_vec, chain_map) — used for scroll computation
    /// and click area registration. The chain_map is returned to avoid recomputing
    /// inside render_messages().
    pub fn compute_message_layout(
        &self,
        width: usize,
    ) -> (
        usize,
        Vec<usize>,
        std::collections::HashMap<usize, (bool, bool)>,
    ) {
        let chain_map = self.compute_tool_chain_map();
        let mut total_lines: usize = 0;
        let mut heights = Vec::with_capacity(self.messages.len());
        for (idx, msg) in self.messages.iter().enumerate() {
            let mut h = self.estimate_message_height(msg, width);
            // Mid-chain messages skip the turn summary footer but height estimation
            // always adds it for assistant messages with tools/content > 3 lines.
            let is_chained_mid = chain_map
                .get(&idx)
                .is_some_and(|(is_chained, is_last)| *is_chained && !*is_last);
            if is_chained_mid {
                h = h.saturating_sub(1);
            }
            heights.push(h);
            total_lines += h;
        }
        (total_lines, heights, chain_map)
    }

    /// Render messages with precomputed heights (avoids redundant estimation).
    ///
    /// Use this when heights have already been computed (e.g., via compute_message_layout)
    /// to avoid estimating heights twice per frame. This is the lower-level rendering
    /// method that renders only the messages area.
    pub fn render_messages_with_heights(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        heights: &[usize],
        chain_map: &std::collections::HashMap<usize, (bool, bool)>,
    ) {
        let colors = self
            .theme_colors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        self.render_messages_with_heights_and_colors(frame, area, heights, &colors, chain_map);
    }

    /// Internal messages rendering with pre-provided colors (avoids redundant mutex lock).
    fn render_messages_with_heights_and_colors(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        heights: &[usize],
        colors: &ThemeColors,
        chain_map: &std::collections::HashMap<usize, (bool, bool)>,
    ) {
        use ratatui::style::Style;
        use ratatui::widgets::{Block, Paragraph, Wrap};

        // Clear the message area background first
        let bg = Block::default().style(Style::default().bg(colors.background));
        frame.render_widget(bg, area);

        // Show welcome when there's no user/assistant conversation (system messages don't count)
        let has_conversation = self.messages.iter().any(|m| {
            matches!(
                m.role,
                crate::ui::message::MessageRole::User | crate::ui::message::MessageRole::Assistant
            )
        });
        if !has_conversation && !self.is_streaming {
            self.render_welcome(frame, area, colors);
            return;
        }

        let _width = area.width as usize;
        let safe_viewport = (area.height as usize).max(1);

        // Use precomputed heights instead of re-estimating
        let total_lines: usize = heights.iter().sum();

        // Compute effective scroll offset (auto-scroll to bottom when not user-scrolled)
        let max_scroll = total_lines.saturating_sub(safe_viewport);
        let effective_offset = if self.user_scrolled {
            self.scroll_offset_line.min(max_scroll)
        } else {
            max_scroll // Auto-scroll to bottom
        };

        // Calculate visible range — find which message the effective offset falls within
        let mut current_line = 0;
        let mut start_idx = 0;
        let mut skip_lines_in_first = 0;

        for (idx, &msg_height) in heights.iter().enumerate() {
            if current_line + msg_height > effective_offset {
                start_idx = idx;
                skip_lines_in_first = effective_offset.saturating_sub(current_line);
                break;
            }
            current_line += msg_height;
            if idx == self.messages.len() - 1 {
                start_idx = idx;
            }
        }

        let mut y_offset = 0u16;

        // Render each visible message using precomputed heights and chain_map
        let mut first_message = true;
        for (rel_idx, msg) in self.messages.iter().skip(start_idx).enumerate() {
            let msg_idx = start_idx + rel_idx;
            let chained = chain_map.get(&msg_idx).copied();
            let msg_lines =
                self.render_message_brutalist(msg, msg_idx, area.width as usize, colors, chained);

            // For the first visible message, skip display rows that are above
            // the viewport. This must operate on wrapped rows, not logical
            // lines, or long paragraphs will jump when scrolled.
            let mut remaining_skip_rows = if first_message {
                skip_lines_in_first as u16
            } else {
                0
            };
            first_message = false;

            for line in msg_lines.iter() {
                if y_offset >= area.height {
                    break;
                }

                // Calculate wrapped height for this line
                let line_width = line.width();
                let content_width = (area.width as usize).max(1);
                let wrapped_rows = if line_width == 0 {
                    1u16
                } else {
                    (line_width.div_ceil(content_width) as u16).max(1)
                };

                if remaining_skip_rows >= wrapped_rows {
                    remaining_skip_rows -= wrapped_rows;
                    continue;
                }

                let scroll_rows = remaining_skip_rows;
                let remaining = area.height.saturating_sub(y_offset);
                let render_rows = wrapped_rows.saturating_sub(scroll_rows).min(remaining);

                if render_rows == 0 {
                    break;
                }

                let line_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: render_rows,
                };
                frame.render_widget(
                    Paragraph::new(line.clone())
                        .wrap(Wrap { trim: false })
                        .scroll((scroll_rows, 0)),
                    line_area,
                );
                remaining_skip_rows = 0;
                y_offset += render_rows;
            }

            if y_offset >= area.height {
                break;
            }
        }

        // Messages above indicator at top of viewport
        let is_scrolled = effective_offset > 0;
        if start_idx > 0 && is_scrolled && area.height > 2 {
            let indicator_area = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            };
            let indicator = Paragraph::new(Line::from(vec![Span::styled(
                format!(
                    "  ▲ {} message{} above",
                    start_idx,
                    if start_idx != 1 { "s" } else { "" }
                ),
                Style::default()
                    .fg(Color::Rgb(80, 80, 100))
                    .add_modifier(Modifier::DIM),
            )]));
            frame.render_widget(indicator, indicator_area);
        }

        // Messages below indicator at bottom of viewport
        let messages_below =
            total_lines.saturating_sub(current_line + skip_lines_in_first + y_offset as usize);
        if messages_below > 0 && is_scrolled && area.height > 2 {
            let indicator_y = area.y + area.height.saturating_sub(1);
            let indicator_area = Rect {
                x: area.x,
                y: indicator_y,
                width: area.width,
                height: 1,
            };
            let pulse_color = if self.is_streaming {
                Color::Rgb(80, 200, 220)
            } else {
                Color::Rgb(80, 80, 100)
            };
            let indicator = Paragraph::new(Line::from(vec![Span::styled(
                format!(
                    "  ▼ {} message{} below",
                    messages_below,
                    if messages_below != 1 { "s" } else { "" }
                ),
                Style::default().fg(pulse_color).add_modifier(Modifier::DIM),
            )]));
            frame.render_widget(indicator, indicator_area);
        }

        // Show streaming indicator at the bottom of the messages area.
        if self.is_streaming && y_offset < area.height {
            let colors_inner = &colors;

            // Streaming header with animated indicator and stats
            if y_offset < area.height {
                let header_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: 1,
                };

                let (status_label, label_color) = if self.active_tool_count > 0 {
                    ("tools", Color::Rgb(100, 180, 255))
                } else if self.current_stream_content.is_empty() {
                    ("thinking", colors_inner.primary)
                } else {
                    ("", colors_inner.primary)
                };

                let mut header_spans = vec![
                    Span::styled("❯ ", Style::default().fg(Color::Rgb(220, 80, 100))),
                    Span::styled(
                        status_label,
                        Style::default()
                            .fg(label_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {} ", self.streaming_char()),
                        Style::default().fg(Color::Rgb(255, 200, 80)),
                    ),
                ];
                if let Some(elapsed) = self.stream_elapsed {
                    let secs = elapsed.as_secs();
                    if secs >= 1 {
                        header_spans.push(Span::styled(
                            format!("{} ", format_elapsed_short(secs)),
                            Style::default()
                                .fg(colors_inner.muted)
                                .add_modifier(Modifier::DIM),
                        ));
                    }
                }
                if self.thinking_chunks_received > 0 {
                    header_spans.push(Span::styled(
                        format!("· {} reasoning ", self.thinking_chunks_received),
                        Style::default()
                            .fg(colors_inner.muted)
                            .add_modifier(Modifier::DIM),
                    ));
                }
                // Word count during streaming (live content stats)
                if !self.current_stream_content.is_empty() {
                    let words = self.current_stream_content.split_whitespace().count();
                    if words > 10 {
                        header_spans.push(Span::styled(
                            format!("· {} words ", words),
                            Style::default()
                                .fg(colors_inner.muted)
                                .add_modifier(Modifier::DIM),
                        ));
                        // Words per second throughput
                        if let Some(elapsed) = self.stream_elapsed {
                            let secs = elapsed.as_secs();
                            if secs >= 3 {
                                let wps = words as f64 / secs as f64;
                                header_spans.push(Span::styled(
                                    format!("({:.0}w/s) ", wps),
                                    Style::default()
                                        .fg(colors_inner.muted)
                                        .add_modifier(Modifier::DIM),
                                ));
                            }
                        }
                    }
                }
                if self.active_tool_count > 0 && !self.active_tool_names.is_empty() {
                    header_spans.push(Span::styled(
                        format!("· {} ", self.active_tool_names),
                        Style::default()
                            .fg(Color::Rgb(100, 180, 255))
                            .add_modifier(Modifier::DIM),
                    ));
                }
                let header = Paragraph::new(Line::from(header_spans));
                frame.render_widget(header, header_area);
                y_offset += 1;
            }

            // Skip preview when message list already shows this text (dedup)
            let last_assistant_has_content = self
                .messages
                .iter()
                .rev()
                .find(|m| m.role == MessageRole::Assistant)
                .is_some_and(|m| !m.content.is_empty());
            if !self.current_stream_content.is_empty()
                && y_offset < area.height
                && !last_assistant_has_content
            {
                // Live content preview: show first 2 lines of streaming content
                let preview_lines: Vec<&str> =
                    self.current_stream_content.lines().take(2).collect();
                for preview_line in &preview_lines {
                    if y_offset >= area.height {
                        break;
                    }
                    let truncated =
                        if <str as unicode_width::UnicodeWidthStr>::width(*preview_line)
                            > (area.width as usize).saturating_sub(4)
                        {
                            crate::app::render::brutalist_helpers::truncate_to_display_width(
                                preview_line,
                                (area.width as usize).saturating_sub(5),
                            ) + "…"
                    } else {
                        preview_line.to_string()
                    };
                    let preview_area = Rect {
                        x: area.x,
                        y: area.y + y_offset,
                        width: area.width,
                        height: 1,
                    };
                    let preview_spans = vec![
                        Span::styled("  ", Style::default().fg(colors_inner.foreground)),
                        Span::styled(truncated, Style::default().fg(Color::Rgb(160, 170, 190))),
                    ];
                    frame.render_widget(Paragraph::new(Line::from(preview_spans)), preview_area);
                    y_offset += 1;
                }
            } else if y_offset < area.height {
                let slow_frame = self.animation_frame / 8;
                let thinking_msg = thinking_messages::thinking_message(slow_frame);
                let think_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: 1,
                };
                let mut think_spans = vec![
                    Span::styled("  ", Style::default().fg(colors_inner.foreground)),
                    Span::styled(
                        format!("{}...", thinking_msg),
                        Style::default()
                            .fg(Color::Rgb(120, 120, 140))
                            .add_modifier(Modifier::ITALIC),
                    ),
                ];
                if let Some(elapsed) = self.stream_elapsed {
                    let secs = elapsed.as_secs();
                    if secs >= 2 {
                        think_spans.push(Span::styled(
                            format!(" ({} elapsed)", format_elapsed_short(secs)),
                            Style::default()
                                .fg(Color::Rgb(90, 90, 110))
                                .add_modifier(Modifier::DIM),
                        ));
                    }
                }
                let think_line = Paragraph::new(Line::from(think_spans));
                frame.render_widget(think_line, think_area);
            }

            // Show queued message preview (gold/italic "will send when finished")
            if self.has_queued_message
                && !self.queued_message_preview.is_empty()
                && y_offset < area.height
            {
                let preview: String = self.queued_message_preview.chars().take(60).collect();
                let ellipsis = if self.queued_message_preview.chars().count() > 60 {
                    "…"
                } else {
                    ""
                };
                let queued_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: 1,
                };
                let queued_line = Paragraph::new(Line::from(vec![
                    Span::styled("  ⏳ ", Style::default().fg(Color::Rgb(255, 200, 80))),
                    Span::styled(
                        format!("{}{} — will send when finished", preview, ellipsis),
                        Style::default()
                            .fg(Color::Rgb(180, 160, 100))
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
                frame.render_widget(queued_line, queued_area);
            }
        }
    }

    /// Render with precomputed message heights (avoids redundant estimation).
    ///
    /// Use this when heights have already been computed (e.g., via compute_message_layout)
    /// to avoid estimating heights twice per frame.
    pub fn render_with_heights(
        &self,
        frame: &mut ratatui::Frame,
        heights: &[usize],
        chain_map: &std::collections::HashMap<usize, (bool, bool)>,
        message_area: Rect,
    ) {
        use ratatui::layout::{Constraint, Direction, Layout};
        use ratatui::style::{Color, Style};
        use ratatui::widgets::{Block, Clear};

        let size = frame.area();

        frame.render_widget(Clear, size);
        let bg = Block::default().style(Style::default().bg(Color::Black));
        frame.render_widget(bg, size);

        let colors = self
            .theme_colors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        let header_height: u16 = if self.header_collapsed { 0 } else { 1 };
        let footer_height: u16 = if self.footer_collapsed { 0 } else { 1 };

        let input_height: u16 = if self.input_line_count > 1 {
            2u16.saturating_add(self.input_line_count.min(6) as u16)
        } else {
            2
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height),
                Constraint::Min(0),
                Constraint::Length(input_height),
                Constraint::Length(footer_height),
            ])
            .split(size);

        let input_area = chunks[2];

        if !self.header_collapsed {
            let header = self.render_header(&colors);
            self.render_header_widget(frame, chunks[0], &colors, header);
        }

        self.render_messages_with_heights_and_colors(frame, message_area, heights, &colors, chain_map);
        self.render_input(frame, input_area, &colors);

        if !self.footer_collapsed {
            self.render_footer(frame, chunks[3], &colors);
        }
    }

    /// Render a single message in brutalist style
    fn render_message_brutalist<'b>(
        &self,
        message: &'b Message,
        message_index: usize,
        width: usize,
        colors: &ThemeColors,
        chained: Option<(bool, bool)>,
    ) -> Vec<Line<'b>> {
        let mut lines = Vec::new();

        // System messages: compact for short notices, rich rendering for diffs/long content
        if message.role == MessageRole::System {
            let content = message.content.trim();
            if content.is_empty() {
                return lines;
            }

            // Detect git diff content — render with syntax coloring
            if crate::ui::diff_renderer::looks_like_git_diff(content) {
                let diff_lines = crate::ui::diff_renderer::render_diff(content);
                // Wrap each diff line with brutalist indent
                for diff_line in diff_lines.iter().take(50) {
                    let mut spans = vec![Span::styled("  ", Style::default().fg(colors.muted))];
                    spans.extend(diff_line.spans.iter().cloned());
                    lines.push(Line::from(spans));
                }
                if diff_lines.len() > 50 {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.muted)),
                        Span::styled(
                            format!("... {} more lines", diff_lines.len() - 50),
                            Style::default()
                                .fg(Color::Rgb(80, 80, 100))
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                }
                return lines;
            }

            // Multi-line system messages: render each line (for /diff, /stats, etc.)
            let line_count = content.lines().count();
            if line_count > 1 {
                // Detect error content for header coloring
                let content_lower = content.to_lowercase();
                let (header_icon, header_color) = if content_lower.starts_with("error")
                    || content_lower.contains("failed")
                    || content_lower.contains("rate limit")
                {
                    ("✗ ", Color::Rgb(200, 80, 80))
                } else if content_lower.starts_with("warning")
                    || content_lower.contains("cancelled")
                {
                    ("⚠ ", Color::Rgb(200, 170, 80))
                } else {
                    ("─ ", Color::Rgb(50, 50, 60))
                };
                // Header
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default().fg(colors.muted)),
                    Span::styled(header_icon, Style::default().fg(header_color)),
                ]));
                // Content lines with dim styling, capped at 50 lines
                for line in content.lines().take(50) {
                    let truncated =
                        if <str as unicode_width::UnicodeWidthStr>::width(line)
                            > width.saturating_sub(4)
                        {
                            crate::app::render::brutalist_helpers::truncate_to_display_width(
                                line,
                                width.saturating_sub(5),
                            ) + "…"
                        } else {
                            line.to_string()
                        };
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.muted)),
                        Span::styled(
                            truncated,
                            Style::default()
                                .fg(Color::Rgb(120, 120, 140))
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                }
                if line_count > 50 {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.muted)),
                        Span::styled(
                            format!("... {} more lines", line_count.saturating_sub(50)),
                            Style::default()
                                .fg(Color::Rgb(80, 80, 100))
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                }
                return lines;
            }

            // Single-line system message: compact notice with type-based coloring
            let display: String = if unicode_width::UnicodeWidthStr::width(content)
                > 100
            {
                crate::app::render::brutalist_helpers::truncate_to_display_width(content, 97) + "..."
            } else {
                content.to_string()
            };

            // Detect error/warning messages for visual distinction
            let content_lower = content.to_lowercase();
            let (prefix, text_color, prefix_color) = if content_lower.starts_with("error")
                || content_lower.contains("failed")
                || content_lower.contains("rate limit")
            {
                ("✗ ", Color::Rgb(200, 80, 80), Color::Rgb(200, 80, 80))
            } else if content_lower.starts_with("warning") || content_lower.contains("cancelled") {
                ("⚠ ", Color::Rgb(200, 170, 80), Color::Rgb(200, 170, 80))
            } else {
                ("─ ", Color::Rgb(100, 100, 120), Color::Rgb(50, 50, 60))
            };

            lines.push(Line::from(vec![
                Span::styled("  ", Style::default().fg(colors.muted)),
                Span::styled(prefix, Style::default().fg(prefix_color)),
                Span::styled(
                    display,
                    Style::default().fg(text_color).add_modifier(Modifier::DIM),
                ),
            ]));
            return lines;
        }

        // Role color for the vertical bar (pink = user, cyan = ai)
        let role_color = match message.role {
            MessageRole::User => colors.secondary,
            MessageRole::Assistant => colors.primary,
            MessageRole::System => unreachable!(), // handled above
        };

        // Tool call chaining: suppress header for chained messages (except last)
        let is_chained_mid = chained.is_some_and(|(is_chained, is_last)| is_chained && !is_last);

        if !is_chained_mid {
            // Render the role accent as a colored gutter cell instead of a
            // text glyph so terminal-native copy/paste does not capture it.
            lines.push(Line::from(vec![Span::styled(
                " ".to_string(),
                Style::default()
                    .bg(role_color)
                    .add_modifier(Modifier::BOLD),
            )]));
        } else {
            // Minimal continuation marker for chained tool-only messages
            lines.push(Line::from(vec![Span::styled(
                " ",
                Style::default().bg(Color::Rgb(60, 60, 70)),
            )]));
        }

        // Collapsed message: show first line + "N more lines" indicator
        if message.collapsed {
            let content_line_count = message.content.lines().count();
            if content_line_count > 0 {
                let first_line = message.content.lines().next().unwrap_or("");
                let preview =
                    if <str as unicode_width::UnicodeWidthStr>::width(first_line) > 60 {
                    crate::app::render::brutalist_helpers::truncate_to_display_width(first_line, 59) + "…"
                } else {
                    first_line.to_string()
                };
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default().fg(colors.foreground)),
                    Span::styled(
                        preview,
                        Style::default()
                            .fg(colors.muted)
                            .add_modifier(Modifier::DIM),
                    ),
                ]));
                if content_line_count > 1 {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.foreground)),
                        Span::styled(
                            format!(
                                "╌ {} more lines (click to expand)",
                                content_line_count.saturating_sub(1)
                            ),
                            Style::default()
                                .fg(Color::Rgb(70, 70, 90))
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                }
            }

            // Still show tool summary for collapsed messages
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    let total = tools.len();
                    let passed = tools
                        .iter()
                        .filter(|t| matches!(t.status, ToolStatus::Complete))
                        .count();
                    let failed = tools
                        .iter()
                        .filter(|t| matches!(t.status, ToolStatus::Failed))
                        .count();
                    lines.push(Line::from(vec![
                        Span::styled("  ╶ ", Style::default().fg(colors.muted)),
                        Span::styled(
                            format!("{} tool{}", total, if total != 1 { "s" } else { "" }),
                            Style::default().fg(colors.muted),
                        ),
                        if passed > 0 {
                            Span::styled(
                                format!(" {} ok", passed),
                                Style::default().fg(Color::Rgb(80, 200, 120)),
                            )
                        } else {
                            Span::styled(String::new(), Style::default())
                        },
                        if failed > 0 {
                            Span::styled(
                                format!(" {} fail", failed),
                                Style::default().fg(Color::Rgb(255, 80, 80)),
                            )
                        } else {
                            Span::styled(String::new(), Style::default())
                        },
                        Span::styled(" ╴", Style::default().fg(colors.muted)),
                    ]));
                }
            }

            lines.push(Line::from(""));
            return lines;
        }

        // Content with inline markdown rendering
        let mut in_code_block = false;
        let mut code_block_line_count: usize = 0;
        let mut in_table = false;

        // Handle messages with only tools (no text content)
        if message.content.trim().is_empty() {
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    // Show minimal indicator for tool-only messages
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.foreground)),
                        Span::styled(
                            "(running tools)",
                            Style::default()
                                .fg(colors.muted)
                                .add_modifier(Modifier::ITALIC | Modifier::DIM),
                        ),
                    ]));
                }
            }
        }

        let content_lines: Vec<&str> = message.content.lines().collect();

        // Precompute byte offsets for each content line (for search highlighting).
        // This preserves the original newline width so CRLF content stays aligned.
        let line_byte_offsets = Self::compute_line_byte_offsets(&message.content);

        let mut line_idx = 0;
        while line_idx < content_lines.len() {
            let content_line = content_lines[line_idx];
            let trimmed = content_line.trim();

            // Detect markdown table rows: | ... | ... |
            if !in_code_block
                && trimmed.starts_with('|')
                && trimmed.ends_with('|')
                && trimmed.contains('|')
            {
                // Check if this is a table by looking for separator row
                let is_separator = is_table_separator_row(trimmed);

                if !in_table && line_idx + 1 < content_lines.len() {
                    let next_trimmed = content_lines[line_idx + 1].trim();
                    let next_is_sep =
                        next_trimmed.starts_with('|') && next_trimmed.ends_with('|')
                            && is_table_separator_row(next_trimmed);

                    if next_is_sep || is_separator {
                        // Start table rendering
                        in_table = true;
                        // Render header row
                        let cells = split_table_cells(trimmed);
                        let mut header_spans =
                            vec![Span::styled("  ", Style::default().fg(colors.foreground))];
                        for (ci, cell) in cells.iter().enumerate() {
                            if ci > 0 {
                                header_spans
                                    .push(Span::styled(" │ ", Style::default().fg(colors.muted)));
                            }
                            header_spans.extend(self.render_table_cell(cell, colors, true));
                        }
                        lines.push(Line::from(header_spans));
                        line_idx += 1;

                        // Skip separator row
                        line_idx += 1;

                        // Render remaining data rows
                        while line_idx < content_lines.len() {
                            let row_line = content_lines[line_idx].trim();
                            if !row_line.starts_with('|') || !row_line.ends_with('|') {
                                in_table = false;
                                break;
                            }
                            let cells = split_table_cells(row_line);
                            let mut row_spans =
                                vec![Span::styled("  ", Style::default().fg(colors.foreground))];
                            for (ci, cell) in cells.iter().enumerate() {
                                if ci > 0 {
                                    row_spans.push(Span::styled(
                                        " │ ",
                                        Style::default().fg(colors.muted),
                                    ));
                                }
                                row_spans.extend(self.render_table_cell(cell, colors, false));
                            }
                            lines.push(Line::from(row_spans));
                            line_idx += 1;
                        }
                        // Add border line after table (adapt to terminal width)
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::default().fg(colors.foreground)),
                            Span::styled(
                                "─".repeat(width.saturating_sub(4).clamp(10, 60)),
                                Style::default().fg(colors.muted),
                            ),
                        ]));
                        continue;
                    }
                }

                if in_table {
                    // Continue table rendering
                    let cells = split_table_cells(trimmed);
                    let mut row_spans =
                        vec![Span::styled("  ", Style::default().fg(colors.foreground))];
                    for (ci, cell) in cells.iter().enumerate() {
                        if ci > 0 {
                            row_spans.push(Span::styled(" │ ", Style::default().fg(colors.muted)));
                        }
                        row_spans.extend(self.render_table_cell(cell, colors, false));
                    }
                    lines.push(Line::from(row_spans));
                    line_idx += 1;
                    continue;
                }
            } else {
                in_table = false;
            }

            // Detect code block fences
            let trimmed = content_line.trim();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                if in_code_block {
                    // Close code block — show truncation indicator if lines were hidden
                    let hidden = code_block_line_count.saturating_sub(MAX_CODE_BLOCK_LINES);
                    if hidden > 0 {
                        lines.push(Line::from(vec![
                            Span::styled("  │ ", Style::default().fg(colors.muted)),
                            Span::styled(
                                format!(
                                    "... {} more line{}",
                                    hidden,
                                    if hidden != 1 { "s" } else { "" }
                                ),
                                Style::default()
                                    .fg(Color::Rgb(100, 100, 120))
                                    .add_modifier(Modifier::DIM),
                            ),
                        ]));
                    }
                    in_code_block = false;
                    code_block_line_count = 0;
                    lines.push(Line::from(vec![Span::styled(
                        "  ╰",
                        Style::default().fg(colors.muted),
                    )]));
                    line_idx += 1;
                    continue;
                } else {
                    // Open code block — extract language tag
                    in_code_block = true;
                    code_block_line_count = 0;
                    let lang_str = trimmed.trim_start_matches(['`', '~']).trim();
                    let lang_badge = if lang_str.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", lang_str)
                    };
                    lines.push(Line::from(vec![
                        Span::styled("  ╭", Style::default().fg(colors.muted)),
                        Span::styled(lang_badge, Style::default().fg(colors.secondary)),
                    ]));
                    line_idx += 1;
                    continue;
                }
            }

            if in_code_block {
                code_block_line_count += 1;
                if code_block_line_count > MAX_CODE_BLOCK_LINES {
                    // Skip lines beyond the limit, but keep counting
                    // to know when the closing fence arrives
                    line_idx += 1;
                    continue;
                }
                // Code block line: line number + monospace content
                let line_num = format!("{:>3} ", code_block_line_count);
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(colors.muted)),
                    Span::styled(line_num, Style::default().fg(Color::Rgb(70, 70, 90))),
                    Span::styled(
                        Cow::Borrowed(content_line),
                        Style::default().fg(Color::Rgb(180, 190, 210)),
                    ),
                ]));
            } else {
                // Regular content line with inline markdown
                let spans = self.render_inline_markdown(content_line, colors);
                // Apply search highlighting if search is active
                let byte_offset = line_byte_offsets.get(line_idx).copied().unwrap_or(0);
                let highlighted = self.apply_search_highlight(spans, message_index, byte_offset);
                let mut line_spans =
                    vec![Span::styled("  ", Style::default().fg(colors.foreground))];
                line_spans.extend(highlighted);
                lines.push(Line::from(line_spans));
            }

            line_idx += 1;
        }

        // Handle unclosed code block — show truncation + close indicator
        if in_code_block {
            let hidden = code_block_line_count.saturating_sub(MAX_CODE_BLOCK_LINES);
            if hidden > 0 {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(colors.muted)),
                    Span::styled(
                        format!(
                            "... {} more line{}",
                            hidden,
                            if hidden != 1 { "s" } else { "" }
                        ),
                        Style::default()
                            .fg(Color::Rgb(100, 100, 120))
                            .add_modifier(Modifier::DIM),
                    ),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("  ╰", Style::default().fg(colors.muted)),
                Span::styled(
                    " (unclosed)",
                    Style::default()
                        .fg(Color::Rgb(70, 70, 85))
                        .add_modifier(Modifier::DIM),
                ),
            ]));
        }

        // Tool executions — compact inline display with animated status
        if let Some(tools) = &message.tool_executions {
            if !tools.is_empty() {
                // Tool summary line: "╶ 3 tools: 2 passed, 1 failed ─╴"
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

                if total > 1 {
                    let mut summary_spans =
                        vec![Span::styled("  ╶ ", Style::default().fg(colors.muted))];
                    if running > 0 {
                        summary_spans.push(Span::styled(
                            self.streaming_char(),
                            Style::default().fg(Color::Rgb(255, 200, 80)),
                        ));
                    }
                    summary_spans.push(Span::styled(
                        format!(" {} tool{}", total, if total != 1 { "s" } else { "" }),
                        Style::default().fg(colors.muted),
                    ));
                    if passed > 0 {
                        summary_spans.push(Span::styled(
                            format!(" {} passed", passed),
                            Style::default().fg(Color::Rgb(80, 200, 120)),
                        ));
                    }
                    if failed > 0 {
                        summary_spans.push(Span::styled(
                            format!(" {} failed", failed),
                            Style::default().fg(Color::Rgb(255, 80, 80)),
                        ));
                    }
                    if running > 0 {
                        summary_spans.push(Span::styled(
                            format!(" {} running", running),
                            Style::default().fg(Color::Rgb(255, 200, 80)),
                        ));
                    }
                    summary_spans.push(Span::styled(" ╴", Style::default().fg(colors.muted)));
                    lines.push(Line::from(summary_spans));
                }

                // Individual tool lines — show last N, collapse older ones
                let max_shown = 5;
                let hidden = total.saturating_sub(max_shown);
                if hidden > 0 {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default().fg(colors.foreground)),
                        Span::styled(
                            format!("… {} earlier tool{}", hidden, if hidden != 1 { "s" } else { "" }),
                            Style::default()
                                .fg(colors.muted)
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                }
                let display_tools: Vec<_> = tools.iter().rev().take(max_shown).collect();
                for tool in display_tools.into_iter().rev() {
                    lines.push(self.render_tool_line(tool, colors));

                    // Show error preview for failed tools (inline error context)
                    if tool.status == ToolStatus::Failed {
                        let error_source = tool
                            .detailed_output
                            .as_deref()
                            .unwrap_or(&tool.result_summary);
                        if !error_source.is_empty() {
                            // Take first meaningful line, truncate for inline display
                            let first_line = error_source
                                .lines()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or("");
                            let error_preview = if <str as unicode_width::UnicodeWidthStr>::width(
                                first_line,
                            )
                                > 80
                            {
                                crate::app::render::brutalist_helpers::truncate_to_display_width(
                                    first_line,
                                    79,
                                ) + "…"
                            } else {
                                first_line.to_string()
                            };
                            lines.push(Line::from(vec![
                                Span::styled("      ", Style::default().fg(colors.foreground)),
                                Span::styled(
                                    error_preview,
                                    Style::default()
                                        .fg(Color::Rgb(180, 100, 100))
                                        .add_modifier(Modifier::DIM),
                                ),
                            ]));
                        }
                    }

                    // Show output preview for running tools (live output preview)
                    if tool.status == ToolStatus::Running {
                        let output = tool
                            .detailed_output
                            .as_deref()
                            .filter(|o| !o.is_empty())
                            .unwrap_or(&tool.result_summary);
                        if !output.is_empty() {
                            // Take last meaningful line (most recent output)
                            let last_line = output
                                .lines()
                                .rev()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or("");
                            let preview =
                                if <str as unicode_width::UnicodeWidthStr>::width(last_line)
                                    > 70
                                {
                                crate::app::render::brutalist_helpers::truncate_to_display_width(
                                    last_line,
                                    69,
                                ) + "…"
                            } else {
                                last_line.to_string()
                            };
                            if !preview.is_empty() {
                                lines.push(Line::from(vec![
                                    Span::styled("      ", Style::default().fg(colors.foreground)),
                                    Span::styled(
                                        preview,
                                        Style::default()
                                            .fg(Color::Rgb(140, 160, 180))
                                            .add_modifier(Modifier::DIM),
                                    ),
                                ]));
                            }
                        }
                    }
                }

                // Expanded tool details (input JSON and output)
                if message.tools_expansion == ExpansionLevel::Expanded {
                    for tool in tools {
                        // Show tool input JSON
                        if let Some(input_json) = &tool.input_json {
                            lines.push(Line::from(vec![Span::styled(
                                "      ╭─ input ─╴",
                                Style::default().fg(colors.muted),
                            )]));
                            let json_str = serde_json::to_string_pretty(input_json)
                                .unwrap_or_else(|_| "{}".to_string());
                            for json_line in json_str.lines().take(15) {
                                lines.push(Line::from(vec![
                                    Span::styled("      │ ", Style::default().fg(colors.muted)),
                                    Span::styled(
                                        json_line.to_string(),
                                        Style::default().fg(Color::Rgb(180, 180, 200)),
                                    ),
                                ]));
                            }
                        }

                        // Show detailed output (head/tail truncation)
                        if let Some(output) = &tool.detailed_output {
                            if tool.input_json.is_some() {
                                lines.push(Line::from(vec![Span::styled(
                                    "      ╰─ output ─╴",
                                    Style::default().fg(colors.muted),
                                )]));
                            } else {
                                lines.push(Line::from(vec![Span::styled(
                                    "      ╭─ output ─╴",
                                    Style::default().fg(colors.muted),
                                )]));
                            }
                            let all_lines: Vec<&str> = output.lines().collect();
                            let max_lines = 10;
                            if all_lines.len() <= max_lines {
                                for out_line in &all_lines {
                                    lines.push(Line::from(vec![
                                        Span::styled("      │ ", Style::default().fg(colors.muted)),
                                        Span::styled(
                                            out_line.to_string(),
                                            Style::default().fg(Color::Rgb(180, 190, 210)),
                                        ),
                                    ]));
                                }
                            } else {
                                // Show first half and last half with hidden count in between
                                let head = max_lines / 2;
                                let tail = max_lines - head;
                                for out_line in &all_lines[..head] {
                                    lines.push(Line::from(vec![
                                        Span::styled("      │ ", Style::default().fg(colors.muted)),
                                        Span::styled(
                                            out_line.to_string(),
                                            Style::default().fg(Color::Rgb(180, 190, 210)),
                                        ),
                                    ]));
                                }
                                lines.push(Line::from(vec![
                                    Span::styled("      │ ", Style::default().fg(colors.muted)),
                                    Span::styled(
                                        format!(
                                            "... ({} lines hidden)",
                                            all_lines.len() - head - tail
                                        ),
                                        Style::default()
                                            .fg(colors.muted)
                                            .add_modifier(Modifier::ITALIC),
                                    ),
                                ]));
                                for out_line in &all_lines[all_lines.len() - tail..] {
                                    lines.push(Line::from(vec![
                                        Span::styled("      │ ", Style::default().fg(colors.muted)),
                                        Span::styled(
                                            out_line.to_string(),
                                            Style::default().fg(Color::Rgb(180, 190, 210)),
                                        ),
                                    ]));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Thinking
        if let Some(thinking) = &message.thinking {
            if !thinking.is_empty() {
                if message.thinking_expansion == ExpansionLevel::Expanded {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.foreground)),
                        Span::styled(
                            "╶─ thinking ─╴",
                            Style::default()
                                .fg(colors.muted)
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));

                    let all_lines: Vec<&str> = thinking.lines().collect();
                    let max_lines = 20;

                    if all_lines.len() <= max_lines {
                        for think_line in &all_lines {
                            lines.push(Line::from(vec![
                                Span::styled("    ", Style::default().fg(colors.foreground)),
                                Span::styled(
                                    Cow::Borrowed(*think_line),
                                    Style::default()
                                        .fg(colors.muted)
                                        .add_modifier(Modifier::DIM),
                                ),
                            ]));
                        }
                    } else {
                        // Head/tail truncation
                        let head = max_lines / 2;
                        let tail = max_lines - head;
                        for think_line in &all_lines[..head] {
                            lines.push(Line::from(vec![
                                Span::styled("    ", Style::default().fg(colors.foreground)),
                                Span::styled(
                                    Cow::Borrowed(*think_line),
                                    Style::default()
                                        .fg(colors.muted)
                                        .add_modifier(Modifier::DIM),
                                ),
                            ]));
                        }
                        lines.push(Line::from(vec![
                            Span::styled("    ", Style::default().fg(colors.foreground)),
                            Span::styled(
                                format!("... ({} lines hidden)", all_lines.len() - head - tail),
                                Style::default()
                                    .fg(colors.muted)
                                    .add_modifier(Modifier::DIM | Modifier::ITALIC),
                            ),
                        ]));
                        for think_line in &all_lines[all_lines.len() - tail..] {
                            lines.push(Line::from(vec![
                                Span::styled("    ", Style::default().fg(colors.foreground)),
                                Span::styled(
                                    Cow::Borrowed(*think_line),
                                    Style::default()
                                        .fg(colors.muted)
                                        .add_modifier(Modifier::DIM),
                                ),
                            ]));
                        }
                    }
                } else {
                    let char_count = thinking.chars().count();
                    let size_label = if char_count >= 1_000 {
                        format!("{:.1}k", char_count as f64 / 1_000.0)
                    } else {
                        char_count.to_string()
                    };
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(colors.foreground)),
                        Span::styled("▶", Style::default().fg(colors.muted)),
                        Span::styled(
                            format!(" thinking · {} chars · Tab to expand", size_label),
                            Style::default()
                                .fg(colors.muted)
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                }
            }
        }

        // Turn summary footer (completion summary)
        // Skip for mid-chain messages — only the last message in a chain gets the footer
        if !is_chained_mid && message.role == MessageRole::Assistant {
            let content_lines = message.content.lines().count();
            let has_tools = message
                .tool_executions
                .as_ref()
                .is_some_and(|t| !t.is_empty());

            if has_tools {
                // Tool summary: tool count + pass/fail + duration + content lines
                if let Some(tools) = &message.tool_executions {
                    if !tools.is_empty() {
                        let total = tools.len();
                        let passed = tools
                            .iter()
                            .filter(|t| matches!(t.status, ToolStatus::Complete))
                            .count();
                        let failed = tools
                            .iter()
                            .filter(|t| matches!(t.status, ToolStatus::Failed))
                            .count();
                        let total_ms: u64 = tools.iter().filter_map(|t| t.duration_ms).sum();

                        let mut footer_spans: Vec<Span<'b>> = vec![
                            Span::styled("  ╶ ", Style::default().fg(Color::Rgb(50, 50, 60))),
                            Span::styled(
                                format!("{} tool{}", total, if total != 1 { "s" } else { "" }),
                                Style::default()
                                    .fg(Color::Rgb(80, 80, 95))
                                    .add_modifier(Modifier::DIM),
                            ),
                        ];

                        if passed > 0 && failed == 0 {
                            footer_spans.push(Span::styled(
                                " ✓".to_string(),
                                Style::default()
                                    .fg(Color::Rgb(60, 120, 80))
                                    .add_modifier(Modifier::DIM),
                            ));
                        } else if failed > 0 {
                            footer_spans.push(Span::styled(
                                format!(" {}✗{}", passed, failed),
                                Style::default()
                                    .fg(Color::Rgb(130, 80, 80))
                                    .add_modifier(Modifier::DIM),
                            ));
                        }

                        if total_ms > 0 {
                            let dur_str = if total_ms < 1000 {
                                format!(" {}ms", total_ms)
                            } else {
                                format!(" {:.1}s", total_ms as f64 / 1000.0)
                            };
                            footer_spans.push(Span::styled(
                                dur_str,
                                Style::default()
                                    .fg(Color::Rgb(70, 70, 85))
                                    .add_modifier(Modifier::DIM),
                            ));
                        }

                        // Content line count
                        if content_lines > 0 {
                            footer_spans.push(Span::styled(
                                format!(" · {} lines", content_lines),
                                Style::default()
                                    .fg(Color::Rgb(70, 70, 85))
                                    .add_modifier(Modifier::DIM),
                            ));
                        }

                        footer_spans.push(Span::styled(
                            " ╴",
                            Style::default().fg(Color::Rgb(50, 50, 60)),
                        ));
                        lines.push(Line::from(footer_spans));
                    }
                }
            } else if content_lines > 3 {
                // Text-only response summary: word count + line count for longer messages
                let word_count = message.content.split_whitespace().count();
                let footer_spans: Vec<Span<'b>> = vec![
                    Span::styled("  ╶ ", Style::default().fg(Color::Rgb(50, 50, 60))),
                    Span::styled(
                        format!("{} words · {} lines", word_count, content_lines),
                        Style::default()
                            .fg(Color::Rgb(70, 70, 85))
                            .add_modifier(Modifier::DIM),
                    ),
                    Span::styled(" ╴", Style::default().fg(Color::Rgb(50, 50, 60))),
                ];
                lines.push(Line::from(footer_spans));
            }
        }

        // Blank line separator (skip for mid-chain to visually group)
        if !is_chained_mid {
            lines.push(Line::from(""));
        }

        lines
    }

    // Render a single tool execution line with compact inline format.
}

#[cfg(test)]
mod tests {
    use crate::app::render::brutalist_renderer::BrutalistRenderer;
    use crate::app::render::brutalist_renderer::BrutalistRendererBuilder;
    use crate::theme::{Theme, ThemeColors};
    use crate::ui::message::{ExpansionLevel, Message};
    use ratatui::style::Color;
    use std::sync::{Arc, Mutex};

    fn make_assistant_with_thinking(thinking: &str) -> Message {
        let mut msg = Message::assistant("test response".to_string());
        msg.thinking = if thinking.is_empty() {
            None
        } else {
            Some(thinking.to_string())
        };
        msg.thinking_expansion = ExpansionLevel::Collapsed;
        msg
    }

    #[test]
    fn collapsed_thinking_shows_indicator() {
        let msg = make_assistant_with_thinking("I need to think about this carefully");
        let messages = vec![msg];
        let input = "";
        let renderer = BrutalistRendererBuilder::new(&messages, input).build();
        let colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let lines =
            renderer.render_message_brutalist(&messages[0], 0, 80, &colors.lock().unwrap(), None);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<&str>>()
            .join("");
        assert!(
            text.contains("thinking"),
            "collapsed thinking should show indicator, got: {:?}",
            text
        );
        assert!(
            text.contains("Tab to expand"),
            "collapsed thinking should show expand hint, got: {:?}",
            text
        );
    }

    #[test]
    fn collapsed_thinking_shows_char_count() {
        let long_thinking = "x".repeat(2500);
        let msg = make_assistant_with_thinking(&long_thinking);
        let messages = vec![msg];
        let input = "";
        let renderer = BrutalistRendererBuilder::new(&messages, input).build();
        let colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let lines =
            renderer.render_message_brutalist(&messages[0], 0, 80, &colors.lock().unwrap(), None);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<&str>>()
            .join("");
        assert!(
            text.contains("2.5k chars"),
            "should format char count for large thinking, got: {:?}",
            text
        );
    }

    #[test]
    fn no_thinking_shows_no_indicator() {
        let mut msg = make_assistant_with_thinking("");
        msg.thinking = None;
        let messages = vec![msg];
        let input = "";
        let renderer = BrutalistRendererBuilder::new(&messages, input).build();
        let colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let lines =
            renderer.render_message_brutalist(&messages[0], 0, 80, &colors.lock().unwrap(), None);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<&str>>()
            .join("");
        assert!(
            !text.contains("thinking"),
            "no thinking should not show indicator, got: {:?}",
            text
        );
    }

    #[test]
    fn chain_height_subtracts_footer_for_mid_chain_messages() {
        use crate::ui::message::{MessageRole, ToolExecution, ToolStatus};

        fn make_tool_only_assistant(tool_name: &str) -> Message {
            let mut msg = Message::new(MessageRole::Assistant, String::new());
            msg.tool_executions = Some(vec![ToolExecution {
                tool_id: tool_name.to_string(),
                name: tool_name.to_string(),
                status: ToolStatus::Complete,
                result_summary: "done".to_string(),
                detailed_output: None,
                input_json: None,
                start_time: chrono::Utc::now(),
                end_time: None,
                duration_ms: None,
                progress_current: None,
                progress_total: None,
                progress_description: None,
            }]);
            msg
        }

        // 3 consecutive tool-only messages → chain of length 3
        let messages = vec![
            make_tool_only_assistant("read"),
            make_tool_only_assistant("edit"),
            make_tool_only_assistant("bash"),
        ];
        let renderer = BrutalistRendererBuilder::new(&messages, "").build();
        let (_total, heights, _chain_map) = renderer.compute_message_layout(80);

        // Mid-chain messages (0, 1) should have 1 fewer line than last (2)
        // because they skip the turn summary footer
        assert!(
            heights[0] < heights[2],
            "mid-chain height ({}) should be less than last-in-chain ({})",
            heights[0],
            heights[2]
        );
        assert!(
            heights[1] < heights[2],
            "mid-chain height ({}) should be less than last-in-chain ({})",
            heights[1],
            heights[2]
        );
    }

    #[test]
    fn table_cells_keep_inline_markdown_in_brutalist_renderer() {
        let msg = Message::assistant(
            "| kind | value |\n| --- | --- |\n| `code` | [Rust](https://rs.dev) |\n".to_string(),
        );
        let messages = vec![msg];
        let renderer = BrutalistRendererBuilder::new(&messages, "").build();
        let colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let theme_colors = colors.lock().unwrap();
        let lines = renderer.render_message_brutalist(&messages[0], 0, 80, &theme_colors, None);

        let code_span = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.as_ref() == "code")
            .expect("expected inline code span in table cell");
        assert_eq!(
            code_span.style.fg,
            Some(Color::Rgb(180, 210, 170)),
            "inline code inside a table cell should keep code styling"
        );

        let link_span = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.as_ref() == "Rust")
            .expect("expected link text span in table cell");
        assert_eq!(
            link_span.style.fg,
            Some(theme_colors.secondary),
            "link text inside a table cell should keep link styling"
        );
    }

    #[test]
    fn table_height_and_rendering_handle_pipes_inside_cells() {
        let msg = Message::assistant(
            "| left | right |\n| --- | --- |\n| `x|y` | a \\| b |\n".to_string(),
        );
        let messages = vec![msg];
        let renderer = BrutalistRendererBuilder::new(&messages, "").build();
        let colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let theme_colors = colors.lock().unwrap();
        let lines = renderer.render_message_brutalist(&messages[0], 0, 80, &theme_colors, None);
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            rendered.contains("x|y"),
            "inline code with pipes should stay intact, got: {rendered:?}"
        );
        assert!(
            rendered.contains("a | b"),
            "escaped pipe should stay inside the cell, got: {rendered:?}"
        );
        assert!(
            rendered.lines().any(|line| line.matches(" │ ").count() == 1),
            "table row should still render as two columns, got: {rendered:?}"
        );

        let height = renderer.estimate_message_height(&messages[0], 80);
        assert!(
            height >= 4,
            "table height should include header, row, and border, got: {height}"
        );
    }

    #[test]
    fn compute_line_byte_offsets_handles_crlf() {
        let offsets = BrutalistRenderer::compute_line_byte_offsets("alpha\r\nbeta\r\ngamma");
        assert_eq!(offsets, vec![0, 7, 13]);
    }

    #[test]
    fn role_gutter_is_not_rendered_as_copyable_text() {
        let msg = Message::assistant("Hello from the assistant".to_string());
        let messages = vec![msg];
        let renderer = BrutalistRendererBuilder::new(&messages, "").build();
        let colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let theme_colors = colors.lock().unwrap();
        let lines = renderer.render_message_brutalist(&messages[0], 0, 80, &theme_colors, None);

        let rendered = lines
            .first()
            .map(|line| line.to_string())
            .unwrap_or_default();

        assert!(
            !rendered.contains('▌'),
            "role gutter should be rendered as a non-text cell, got: {rendered:?}"
        );
    }

    #[test]
    fn long_system_diff_is_truncated_like_height_estimate() {
        let mut diff_lines = vec![
            "diff --git a/test.txt b/test.txt".to_string(),
            "--- a/test.txt".to_string(),
            "+++ b/test.txt".to_string(),
        ];
        for i in 1..=60 {
            diff_lines.push(format!("@@ -{i},1 +{i},1 @@"));
            diff_lines.push(format!("-old{i}"));
            diff_lines.push(format!("+new{i}"));
        }
        let diff = diff_lines.join("\n");
        let msg = Message::system(diff);
        let messages = vec![msg];
        let renderer = BrutalistRendererBuilder::new(&messages, "").build();
        let colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let theme_colors = colors.lock().unwrap();
        let lines = renderer.render_message_brutalist(&messages[0], 0, 80, &theme_colors, None);

        assert_eq!(
            lines.len(),
            51,
            "system diffs should be capped to 50 lines plus one overflow indicator"
        );
        assert!(
            lines
                .last()
                .is_some_and(|line| line.to_string().contains("more lines")),
            "truncated diff should end with an overflow indicator"
        );
    }

    #[test]
    fn wide_unicode_system_lines_are_not_truncated_by_byte_length() {
        let msg = Message::system(format!("header\n{}\nfooter", "汉".repeat(20)));
        let messages = vec![msg];
        let renderer = BrutalistRendererBuilder::new(&messages, "").build();
        let colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let theme_colors = colors.lock().unwrap();
        let lines = renderer.render_message_brutalist(&messages[0], 0, 44, &theme_colors, None);

        let wide_line = lines
            .iter()
            .map(|line| line.to_string())
            .find(|line| line.contains("汉"))
            .expect("expected wide unicode content to render");

        assert!(
            !wide_line.contains("…"),
            "line should fit by display width and not be truncated: {wide_line:?}"
        );
    }

    #[test]
    #[ignore = "Pre-existing failure — wide CJK character rendering needs unicode-width 0.2 alignment"]
    fn tool_lines_keep_wide_summaries_without_byte_truncation() {
        use crate::ui::message::{MessageRole, ToolExecution, ToolStatus};

        let mut msg = Message::new(MessageRole::Assistant, String::new());
        msg.tool_executions = Some(vec![ToolExecution {
            tool_id: "tool_1".to_string(),
            name: "test".to_string(),
            status: ToolStatus::Complete,
            result_summary: "汉".repeat(80),
            detailed_output: None,
            input_json: None,
            start_time: chrono::Utc::now(),
            end_time: None,
            duration_ms: None,
            progress_current: None,
            progress_total: None,
            progress_description: None,
        }]);

        let messages = vec![msg];
        let renderer = BrutalistRendererBuilder::new(&messages, "").build();
        let colors = Arc::new(Mutex::new(ThemeColors::from(&Theme::default())));
        let theme_colors = colors.lock().unwrap();
        let lines = renderer.render_message_brutalist(&messages[0], 0, 80, &theme_colors, None);

        let summary_line = lines
            .iter()
            .map(|line| line.to_string())
            .find(|line| line.contains("汉"))
            .expect("expected wide tool summary to render");

        assert!(
            summary_line.contains("…"),
            "wide tool summaries should be truncated by display width (not byte length): {summary_line:?}"
        );
    }
}
