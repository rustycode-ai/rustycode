impl BrutalistRenderer<'_> {
    /// Render only the footer component (for integration with existing TUI)
    pub fn render_footer_area(&self, frame: &mut ratatui::Frame, area: Rect) {
        let colors = self
            .theme_colors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        self.render_footer(frame, area, &colors);
    }

    /// Render footer with status
    pub fn render_footer(&self, frame: &mut ratatui::Frame, area: Rect, colors: &ThemeColors) {
        use ratatui::layout::Alignment;
        use ratatui::widgets::Paragraph;

        // Dynamic footer status with thinking messages
        let footer_status = if self.is_streaming && self.active_tool_count > 0 {
            if self.active_tool_names.is_empty() {
                format!("{} tools running", self.active_tool_count)
            } else {
                self.active_tool_names.clone()
            }
        } else if self.is_streaming {
            if let Some(elapsed) = self.stream_elapsed {
                let words = if self.current_stream_content.is_empty() {
                    0
                } else {
                    self.current_stream_content.split_whitespace().count()
                };
                if words > 20 {
                    format!("{}s · {} words", elapsed.as_secs(), words)
                } else {
                    format!("{}s", elapsed.as_secs())
                }
            } else {
                "thinking".to_string()
            }
        } else {
            self.agent_status.to_string()
        };

        let mut spans = vec![
            Span::styled("╶─ ", Style::default().fg(colors.muted)),
            Span::styled(
                footer_status,
                Style::default().fg(colors.foreground),
            ),
            Span::styled(" ", Style::default().fg(colors.muted)),
        ];

        if self.active_tool_count > 0 {
            spans.push(Span::styled(
                format!("⚡{} ", self.active_tool_count),
                Style::default().fg(Color::Rgb(255, 200, 80)),
            ));
        }

        if !self.is_streaming {
            if let Some(dur) = self.last_response_duration {
                let ms = dur.as_millis();
                let timing_str = if ms < 1000 {
                    format!("{}ms", ms)
                } else {
                    format_elapsed_short(dur.as_secs())
                };
                spans.push(Span::styled("·", Style::default().fg(colors.muted)));
                spans.push(Span::styled(
                    format!(" {} ", timing_str),
                    Style::default()
                        .fg(colors.muted)
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        // Rate limit countdown
        if let Some(until) = self.rate_limit_until {
            let remaining = until.saturating_duration_since(Instant::now());
            let remaining_secs = remaining.as_secs();
            if remaining_secs > 0 {
                spans.push(Span::styled("│", Style::default().fg(colors.muted)));
                spans.push(Span::styled(
                    format!(" ◯ rate:{}s ", remaining_secs),
                    Style::default()
                        .fg(colors.muted)
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        // Fill with light separators and keyboard hints
        let width = area.width as usize;
        let content_width: usize = spans.iter().map(|s| s.content.width()).sum();
        let remaining = width.saturating_sub(content_width);

        if remaining > 30 {
            // Show compact keyboard hints in the fill area
            let hints = if self.is_streaming {
                "Ctrl+C stop"
            } else {
                "? help · / cmds · Ctrl+D quit"
            };
            let hint_len = hints.len();
            let sep_count = remaining.saturating_sub(hint_len + 2);
            if sep_count > 0 {
                spans.push(Span::styled(
                    "─".repeat(sep_count / 2),
                    Style::default().fg(colors.muted),
                ));
                spans.push(Span::styled(
                    format!(" {} ", hints),
                    Style::default().fg(Color::DarkGray),
                ));
                let right_sep = remaining.saturating_sub(sep_count / 2 + hint_len + 2);
                spans.push(Span::styled(
                    "─".repeat(right_sep),
                    Style::default().fg(colors.muted),
                ));
            } else {
                spans.push(Span::styled(
                    "─".repeat(remaining),
                    Style::default().fg(colors.muted),
                ));
            }
        } else if remaining > 0 {
            spans.push(Span::styled(
                "─".repeat(remaining),
                Style::default().fg(colors.muted),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

    // Render a single line with inline markdown styling.
}
