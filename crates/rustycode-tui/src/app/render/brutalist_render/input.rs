impl BrutalistRenderer<'_> {
    /// Render input area with context info line (Goose pattern)
    pub fn render_input(&self, frame: &mut ratatui::Frame, area: Rect, colors: &ThemeColors) {
        use ratatui::layout::Alignment;
        use ratatui::text::Text;
        use ratatui::widgets::Paragraph;

        let mode_char = match self.input_mode {
            InputMode::SingleLine => "▸",
            InputMode::MultiLine => "▪",
        };

        // Line 1: Context info bar (Goose pattern: show context near input)
        let mut info_spans = vec![Span::styled("  ", Style::default().fg(colors.muted))];

        // Context usage bar
        if self.context_usage.context_limit > 0 {
            let bar_text = self.context_usage.format_bar(10);
            let bar_color = match self.context_usage.color_level() {
                crate::app::context_usage::UsageLevel::Low => Color::Rgb(80, 200, 120),
                crate::app::context_usage::UsageLevel::Medium => Color::Rgb(255, 200, 80),
                crate::app::context_usage::UsageLevel::High => Color::Rgb(255, 80, 80),
            };
            info_spans.push(Span::styled(
                format!("ctx:{} ", bar_text),
                Style::default().fg(bar_color).add_modifier(Modifier::DIM),
            ));
        }

        // Token split (Goose pattern: show input/output breakdown)
        if self.session_input_tokens > 0 || self.session_output_tokens > 0 {
            let in_fmt = format_tokens_compact(self.session_input_tokens);
            let out_fmt = format_tokens_compact(self.session_output_tokens);
            let cache_fmt = if self.session_cache_read_tokens > 0 {
                let total_input = self.session_input_tokens + self.session_cache_read_tokens;
                let pct = (self.session_cache_read_tokens * 100)
                    .checked_div(total_input)
                    .unwrap_or(0);
                format!(" {}%", pct)
            } else {
                String::new()
            };
            info_spans.push(Span::styled(
                format!("↑{} ↓{} ", in_fmt, out_fmt),
                Style::default()
                    .fg(Color::Rgb(100, 120, 150))
                    .add_modifier(Modifier::DIM),
            ));
            if !cache_fmt.is_empty() {
                info_spans.push(Span::styled(
                    format!("c:{} ", cache_fmt.trim_start()),
                    Style::default()
                        .fg(Color::Rgb(80, 200, 120))
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        // Session cost
        if self.session_cost > 0.0 {
            let cost_str = if self.session_cost < 0.01 {
                format!("${:.4}", self.session_cost)
            } else {
                format!("${:.2}", self.session_cost)
            };
            info_spans.push(Span::styled(
                format!("{} ", cost_str),
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Fill with separator
        let info_width: usize = info_spans.iter().map(|s| s.content.width()).sum();
        let remaining = area.width as usize;
        if remaining > info_width + 20 {
            // Show readline-style hints on the info bar
            let hints = if !self.reverse_search_query.is_empty() && self.reverse_search_total > 0 {
                format!(
                    "(reverse-i-search)`{}': {}/{}",
                    self.reverse_search_query, self.reverse_search_match, self.reverse_search_total
                )
            } else if !self.reverse_search_query.is_empty() {
                format!(
                    "(reverse-i-search)`{}': no matches",
                    self.reverse_search_query
                )
            } else if self.history_position > 0 {
                format!("history {}/{}", self.history_position, self.history_total)
            } else if self.view.user_scrolled && !self.is_streaming {
                "G bottom · ↑↓ scroll".to_string()
            } else if self.is_streaming {
                if self.has_queued_message {
                    "Ctrl+C cancel · next ready".to_string()
                } else {
                    "Ctrl+C cancel".to_string()
                }
            } else if self.input_mode == InputMode::MultiLine {
                "Opt+Enter send · Enter newline".to_string()
            } else {
                "Enter send · Shift+Enter newline".to_string()
            };
            let hints_len = hints.len();
            let sep_count = remaining.saturating_sub(info_width + hints_len + 4);
            info_spans.push(Span::styled(
                "─".repeat(sep_count),
                Style::default().fg(Color::Rgb(40, 40, 50)),
            ));
            info_spans.push(Span::styled(
                format!(" {} ", hints),
                Style::default().fg(if !self.reverse_search_query.is_empty() {
                    Color::Rgb(255, 200, 80)
                } else if self.history_position > 0 {
                    Color::Rgb(120, 160, 200)
                } else {
                    Color::Rgb(70, 70, 85)
                }),
            ));
        }

        let info_line = Line::from(info_spans);

        let mut all_lines = vec![info_line];

        let cursor_bright = self.animation_frame % 8 < 4;
        let cursor_char = "▏";
        let cursor_style = if cursor_bright {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Rgb(100, 100, 120))
        };

        if self.input_text.is_empty() {
            let placeholder = if self.is_streaming {
                ""
            } else {
                " Ask me anything..."
            };
            let mut spans = vec![
                Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))),
                Span::styled(format!("{} ", mode_char), Style::default().fg(colors.muted)),
                Span::styled(cursor_char, cursor_style),
            ];
            if !placeholder.is_empty() {
                spans.push(Span::styled(
                    placeholder.to_string(),
                    Style::default().fg(colors.muted),
                ));
            }
            all_lines.push(Line::from(spans));
        } else if self.input_line_count > 1 {
            let lines: Vec<&str> = self.input_text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let line_num = format!("{:>2} ", i + 1);
                let is_cursor_line = i == self.cursor_row.min(lines.len().saturating_sub(1));

                let prefix = if i == 0 {
                    vec![
                        Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))),
                        Span::styled(format!("{} ", mode_char), Style::default().fg(colors.muted)),
                        Span::styled(
                            line_num.clone(),
                            Style::default().fg(Color::Rgb(70, 70, 85)),
                        ),
                    ]
                } else {
                    vec![
                        Span::styled("  ", Style::default().fg(colors.muted)),
                        Span::styled(
                            line_num.clone(),
                            Style::default().fg(Color::Rgb(70, 70, 85)),
                        ),
                    ]
                };

                if is_cursor_line {
                    let col = line.floor_char_boundary(self.cursor_col.min(line.len()));
                    let (before, after) = line.split_at(col);
                    let mut spans = prefix;
                    if !before.is_empty() {
                        spans.push(Span::styled(
                            before.to_string(),
                            Style::default().fg(colors.foreground),
                        ));
                    }
                    if let Some(ch) = after.chars().next() {
                        let rest = &after[ch.len_utf8()..];
                        let block_style = if cursor_bright {
                            Style::default().fg(Color::Black).bg(Color::White)
                        } else {
                            Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 80))
                        };
                        spans.push(Span::styled(ch.to_string(), block_style));
                        if !rest.is_empty() {
                            spans.push(Span::styled(
                                rest.to_string(),
                                Style::default().fg(colors.foreground),
                            ));
                        }
                    } else {
                        spans.push(Span::styled(cursor_char, cursor_style));
                    }
                    all_lines.push(Line::from(spans));
                } else {
                    let mut spans = prefix;
                    spans.push(Span::styled(
                        line.to_string(),
                        Style::default().fg(colors.foreground),
                    ));
                    all_lines.push(Line::from(spans));
                }
            }
        } else {
            let col = self.input_text.floor_char_boundary(self.cursor_col.min(self.input_text.len()));
            let (before, after) = self.input_text.split_at(col);
            let mut spans = vec![
                Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))),
                Span::styled(format!("{} ", mode_char), Style::default().fg(colors.muted)),
            ];
            if !before.is_empty() {
                spans.push(Span::styled(before, Style::default().fg(colors.foreground)));
            }
            if let Some(ch) = after.chars().next() {
                let rest = &after[ch.len_utf8()..];
                let block_style = if cursor_bright {
                    Style::default().fg(Color::Black).bg(Color::White)
                } else {
                    Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 80))
                };
                spans.push(Span::styled(ch.to_string(), block_style));
                if !rest.is_empty() {
                    spans.push(Span::styled(
                        rest.to_string(),
                        Style::default().fg(colors.foreground),
                    ));
                }
            } else {
                spans.push(Span::styled(cursor_char, cursor_style));
            }
            all_lines.push(Line::from(spans));
        }

        let text = Text::from(all_lines);
        let paragraph = Paragraph::new(text).alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

}
