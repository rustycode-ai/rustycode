    pub fn render_model_selector(frame: &mut Frame) {
        use crate::services::providers::all_available_models;
        use crate::app::render::shared::centered_rect;
        use ratatui::layout::Alignment;
        use ratatui::style::{Color, Style};
        use ratatui::text::Line;
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

        const MODAL_WIDTH_PCT: usize = 80;
        const MODAL_HEIGHT_PCT: usize = 70;
        const MODAL_MIN_WIDTH: usize = 20;
        const MODAL_MIN_HEIGHT: usize = 5;

        let size = frame.area();

        // Calculate modal size, clamped to minimums
        let width = ((size.width as usize * MODAL_WIDTH_PCT) / 100).max(MODAL_MIN_WIDTH).min(size.width as usize) as u16;
        let height = ((size.height as usize * MODAL_HEIGHT_PCT) / 100).max(MODAL_MIN_HEIGHT).min(size.height as usize) as u16;
        let modal_area = centered_rect(width, height, size);

        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);

        // Get available models
        let models = all_available_models();

        // Create content lines
        let mut lines = vec![
            Line::from("Available Models").style(Style::default().fg(Color::Cyan)),
            Line::from(""),
        ];

        if models.is_empty() {
            lines.push(Line::from("No models available").style(Style::default().fg(Color::Red)));
        } else {
            for (i, model) in models.iter().enumerate() {
                let shortcut = if model.shortcut.unwrap_or(0) > 0 {
                    format!(" [Ctrl+{}]", model.shortcut.unwrap_or(0))
                } else {
                    String::new()
                };
                lines.push(Line::from(format!(
                    "  {}. {}{} - {} ({}) - {} tokens, ${:.2}/M",
                    i + 1,
                    model.name,
                    shortcut,
                    model.provider,
                    model.description,
                    model.context_display(),
                    model.input_cost
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from("Switch Models:"));
            lines.push(Line::from("  /model <number>   - Switch by number"));
            lines.push(Line::from("  /model <model_id> - Switch by model ID"));
        }

        lines.push(Line::from(""));
        lines.push(Line::from("Press Esc to close").style(Style::default().fg(Color::Gray)));

        // Create the modal block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Model Selector ")
            .title_style(Style::default().fg(Color::Cyan).bold());

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, modal_area);
    }

    /// Render provider selector overlay
    pub fn render_provider_selector(frame: &mut Frame) {
        use crate::services::providers::available_providers;
        use crate::app::render::shared::centered_rect;
        use ratatui::layout::Alignment;
        use ratatui::style::{Color, Style};
        use ratatui::text::Line;
        use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

        let size = frame.area();

        // Calculate modal size (80% width, 70% height), clamped to minimums
        let width = ((size.width as usize * 80) / 100).max(20).min(size.width as usize) as u16;
        let height = ((size.height as usize * 70) / 100).max(5).min(size.height as usize) as u16;
        let modal_area = centered_rect(width, height, size);

        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);

        // Get available providers
        let providers = available_providers();

        // Create content lines
        let mut lines = vec![
            Line::from("Available Providers").style(Style::default().fg(Color::Cyan)),
            Line::from(""),
        ];

        for (i, provider) in providers.iter().enumerate() {
            let status = if provider.is_configured() {
                "✓"
            } else {
                "✗"
            };
            lines.push(Line::from(format!(
                "  {}. [{}] {} - {}",
                i + 1,
                status,
                provider.name,
                provider.description
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from("Provider Commands:"));
        lines.push(Line::from("  /provider <num>           - Switch provider"));
        lines.push(Line::from(
            "  /provider connect <num>   - Configure API key",
        ));
        lines.push(Line::from("  /provider disconnect <num>- Remove config"));
        lines.push(Line::from("  /provider validate <num>  - Test credentials"));
        lines.push(Line::from(""));
        lines.push(Line::from("Press Esc to close").style(Style::default().fg(Color::Gray)));

        // Create the modal block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Provider Management ")
            .title_style(Style::default().fg(Color::Cyan).bold());

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, modal_area);
    }

    /// Apply search highlighting to rendered lines.
    ///
    /// Performance: caps at 50 matches per message to prevent span explosion
    /// when searching for short strings in large content. Properly merges
    /// overlapping match segments to minimize Span count.
    pub fn apply_search_highlighting(
        tui: &TUI,
        lines: &[ratatui::text::Line],
        message_index: usize,
    ) -> Vec<ratatui::text::Line<'static>> {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::Span;

        /// Max matches to highlight per message (prevents span explosion)
        const MAX_MATCHES_PER_MESSAGE: usize = 50;

        // Collect matches for this message, capped
        let matches: Vec<_> = tui
            .search.search_state
            .matches
            .iter()
            .filter(|m| m.message_index == message_index)
            .take(MAX_MATCHES_PER_MESSAGE)
            .collect();

        if matches.is_empty() {
            // Return cloned lines as owned data
            return lines
                .iter()
                .map(|line| {
                    let spans: Vec<_> = line
                        .spans
                        .iter()
                        .map(|span| Span::styled(span.content.to_string(), span.style))
                        .collect();
                    ratatui::text::Line::from(spans)
                })
                .collect();
        }

        // Pre-compute current match for fast comparison
        let current_match = tui.search.search_state.current_match();

        let mut highlighted_lines = Vec::with_capacity(lines.len());
        let mut byte_offset = 0;

        for (line_idx, line) in lines.iter().enumerate() {
            let mut new_spans = Vec::new();
            let mut span_byte_offset = byte_offset;

            for span in &line.spans {
                let span_text = span.content.as_ref();
                let span_bytes = span_text.as_bytes();
                let span_len = span_bytes.len();
                let span_end = span_byte_offset + span_len;

                // Collect overlapping match intervals within this span,
                // then merge adjacent/overlapping ones to minimize spans.
                let mut intervals: Vec<(usize, usize, bool)> = Vec::new(); // (start, end, is_current)

                for match_pos in &matches {
                    if match_pos.end <= span_byte_offset || match_pos.start >= span_end {
                        continue;
                    }

                    let overlap_start = match_pos.start.saturating_sub(span_byte_offset);
                    let overlap_end = (match_pos.end - span_byte_offset).min(span_len);
                    let is_current = current_match == Some(match_pos);

                    intervals.push((overlap_start, overlap_end, is_current));
                }

                if intervals.is_empty() {
                    // No matches in this span — pass through unchanged
                    new_spans.push(Span::styled(span_text.to_string(), span.style));
                    span_byte_offset = span_end;
                    continue;
                }

                // Sort by start position, then merge overlapping intervals.
                // Current-match intervals are prioritized during merge.
                intervals.sort_by_key(|(start, _, _)| *start);

                let mut merged: Vec<(usize, usize, bool)> = Vec::with_capacity(intervals.len());
                for (start, end, is_current) in intervals {
                    if let Some(last) = merged.last_mut() {
                        // Merge if overlapping or adjacent
                        if start <= last.1 {
                            last.1 = last.1.max(end);
                            // Preserve is_current if either segment is current
                            if is_current {
                                last.2 = true;
                            }
                            continue;
                        }
                    }
                    merged.push((start, end, is_current));
                }

                // Build spans from merged intervals
                let mut pos = 0;
                for (seg_start, seg_end, is_current) in merged {
                    // Non-match segment before this interval
                    if pos < seg_start {
                        if let Ok(segment) = std::str::from_utf8(&span_bytes[pos..seg_start]) {
                            new_spans.push(Span::styled(segment.to_string(), span.style));
                        }
                    }

                    // Match segment
                    if let Ok(segment) = std::str::from_utf8(&span_bytes[seg_start..seg_end]) {
                        let highlight_style = if is_current {
                            Style::default()
                                .fg(Color::White)
                                .bg(Color::Yellow)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().bg(Color::Yellow).fg(Color::Black)
                        };
                        new_spans.push(Span::styled(segment.to_string(), highlight_style));
                    }

                    pos = seg_end;
                }

                // Remaining non-match segment
                if pos < span_len {
                    if let Ok(segment) = std::str::from_utf8(&span_bytes[pos..]) {
                        new_spans.push(Span::styled(segment.to_string(), span.style));
                    }
                }

                span_byte_offset = span_end;
            }

            highlighted_lines.push(ratatui::text::Line::from(new_spans));

            // Track byte offset for next line (+1 for newline)
            if line_idx < lines.len() - 1 {
                byte_offset = span_byte_offset + 1;
            } else {
                byte_offset = span_byte_offset;
            }
        }

        highlighted_lines
    }
