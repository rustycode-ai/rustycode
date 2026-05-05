impl<'a> BrutalistRenderer<'a> {
    fn render_header(&self, colors: &ThemeColors) -> Line<'a> {
        let status_str = if self.is_streaming {
            if self.active_tool_count > 0 && !self.active_tool_names.is_empty() {
                self.active_tool_names.as_str()
            } else if self.active_tool_count > 0 {
                "tools"
            } else {
                thinking_messages::thinking_message(self.animation_frame / 60)
            }
        } else {
            "ready"
        };

        let mut spans = vec![
            Span::styled("╺─", Style::default().fg(colors.muted)),
            Span::styled(
                "rc",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("─", Style::default().fg(colors.muted)),
        ];

        let has_recent_error = self.messages.iter().rev().take(5).any(|m| {
            m.role == MessageRole::System && {
                let c = m.content.to_lowercase();
                c.starts_with("error") || c.contains("failed") || c.contains("rate limit")
            }
        });
        let status_color = if has_recent_error && !self.is_streaming {
            Color::Rgb(255, 100, 100)
        } else if self.is_streaming {
            Color::Rgb(255, 200, 80)
        } else {
            Color::Rgb(80, 200, 120)
        };
        let status_prefix = if has_recent_error && !self.is_streaming {
            "✗ "
        } else {
            ""
        };
        spans.push(Span::styled(
            format!(" {}{} ", status_prefix, status_str),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::DIM),
        ));

        if self.active_tool_count > 0 {
            spans.push(Span::styled(
                format!("│ ⚡{} ", self.active_tool_count),
                Style::default().fg(Color::Rgb(255, 200, 80)),
            ));
        }

        if !self.current_model.is_empty() {
            let model_short = self
                .current_model
                .rsplit('/')
                .next()
                .unwrap_or(self.current_model);
            spans.push(Span::styled(
                format!("│ {} ", model_short),
                Style::default()
                    .fg(Color::Rgb(120, 140, 180))
                    .add_modifier(Modifier::DIM),
            ));
        }

        // Streaming animation
        if self.is_streaming {
            spans.push(Span::styled(
                self.streaming_char(),
                Style::default().fg(Color::Rgb(255, 200, 80)),
            ));
            // Show live elapsed time during streaming (Goose pattern)
            if let Some(elapsed) = self.stream_elapsed {
                let secs = elapsed.as_secs();
                if secs >= 1 {
                    spans.push(Span::styled(
                        format!(" {}", format_elapsed_short(secs)),
                        Style::default()
                            .fg(Color::Rgb(255, 200, 80))
                            .add_modifier(Modifier::DIM),
                    ));
                }
            }
        } else if let Some(dur) = self.last_response_duration {
            // Show last response time when idle (Goose pattern: response timing)
            let secs = dur.as_secs();
            if secs > 0 {
                spans.push(Span::styled(
                    format!("│ {} ", format_elapsed_short(secs)),
                    Style::default()
                        .fg(colors.muted)
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        // Session duration (Goose pattern: show total session time)
        if let Some(start) = self.session_start {
            let elapsed = start.elapsed().as_secs();
            if elapsed >= 60 {
                spans.push(Span::styled(
                    format!("│ {} ", format_elapsed_short(elapsed)),
                    Style::default()
                        .fg(Color::Rgb(90, 100, 120))
                        .add_modifier(Modifier::DIM),
                ));
            }
        }

        Line::from(spans)
    }

    /// Get animated streaming character
    fn streaming_char(&self) -> &'static str {
        // Pulse animation: ◐ ◑ ◒ ◓
        const FRAMES: &[&str] = &["◐", "◑", "◒", "◓"];
        FRAMES[self.animation_frame % FRAMES.len()]
    }

    /// Render header widget
    fn render_header_widget(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        colors: &ThemeColors,
        content: Line<'a>,
    ) {
        use ratatui::layout::Alignment;
        use ratatui::widgets::Paragraph;

        // Fill rest with light separator
        let width = area.width as usize;
        let content_width = content.width();
        let remaining = width.saturating_sub(content_width);

        let mut spans = content.spans.clone();

        if remaining > 0 {
            spans.push(Span::styled(
                "─".repeat(remaining),
                Style::default().fg(colors.muted),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

}
