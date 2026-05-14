impl BrutalistRenderer<'_> {
    fn render_welcome(&self, frame: &mut ratatui::Frame, area: Rect, colors: &ThemeColors) {
        use ratatui::layout::Alignment;
        use ratatui::widgets::Paragraph;

        let available_height = area.height as usize;
        let welcome_lines = 8;
        let top_padding = available_height.saturating_sub(welcome_lines) / 2;

        let mut welcome = Vec::new();

        for _ in 0..top_padding {
            welcome.push(Line::from(""));
        }

        welcome.push(Line::from(vec![
            Span::styled("  ", Style::default().fg(colors.foreground)),
            Span::styled("╶─ ", Style::default().fg(colors.muted)),
            Span::styled(
                "RustyCode",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " — autonomous development framework",
                Style::default().fg(colors.foreground),
            ),
            Span::styled(" ─╴", Style::default().fg(colors.muted)),
        ]));

        welcome.push(Line::from(""));

        if !self.current_model.is_empty() {
            let model_short = self
                .current_model
                .rsplit('/')
                .next()
                .unwrap_or(self.current_model);
            welcome.push(Line::from(vec![
                Span::styled("    ", Style::default().fg(colors.foreground)),
                Span::styled("model ", Style::default().fg(colors.muted)),
                Span::styled(
                    model_short.to_string(),
                    Style::default().fg(colors.secondary),
                ),
            ]));
        }

        {
            let cwd = if self.cwd.as_os_str().is_empty() {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            } else {
                self.cwd.clone()
            };
            let cwd_str = cwd.display().to_string();
            let display = if let Ok(home) = std::env::var("HOME") {
                if cwd_str.starts_with(&home) {
                    format!("~{}", &cwd_str[home.len()..])
                } else {
                    cwd_str
                }
            } else {
                cwd_str
            };
            let path_display = if display.len() > 50 {
                let mut start = display.len().saturating_sub(47);
                while !display.is_char_boundary(start) {
                    start += 1;
                }
                format!("…{}", &display[start..])
            } else {
                display
            };

            let git_branch: Option<&str> = if self.git_branch.is_empty() {
                None
            } else {
                Some(self.git_branch)
            };

            let mut cwd_spans = vec![
                Span::styled("    ", Style::default().fg(colors.foreground)),
                Span::styled("cwd ", Style::default().fg(colors.muted)),
                Span::styled(path_display, Style::default().fg(Color::Rgb(140, 150, 170))),
            ];
            if let Some(branch) = git_branch {
                cwd_spans.push(Span::styled(
                    " · ",
                    Style::default().fg(Color::Rgb(60, 60, 70)),
                ));
                cwd_spans.push(Span::styled(branch, Style::default().fg(Color::Rgb(100, 180, 140))));
            }
            welcome.push(Line::from(cwd_spans));
        }

        welcome.push(Line::from(""));

        welcome.push(Line::from(vec![
            Span::styled("    ", Style::default().fg(colors.foreground)),
            Span::styled(
                "/",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" commands  ", Style::default().fg(colors.muted)),
            Span::styled(
                "F1",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" help  ", Style::default().fg(colors.muted)),
            Span::styled(
                "Ctrl+R",
                Style::default()
                    .fg(Color::Rgb(100, 140, 180))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" search  ", Style::default().fg(colors.muted)),
            Span::styled(
                "Ctrl+K",
                Style::default()
                    .fg(Color::Rgb(100, 140, 180))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" / Ctrl+Shift+P palette", Style::default().fg(colors.muted)),
        ]));

        welcome.push(Line::from(""));

        welcome.push(Line::from(vec![
            Span::styled("    ", Style::default().fg(colors.foreground)),
            Span::styled(
                "type a message and press ",
                Style::default().fg(colors.muted),
            ),
            Span::styled(
                "enter",
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to start", Style::default().fg(colors.muted)),
        ]));

        if !self.api_key_warning.is_empty() {
            welcome.push(Line::from(""));
            welcome.push(Line::from(vec![
                Span::styled("    ", Style::default().fg(colors.foreground)),
                Span::styled(
                    self.api_key_warning.to_string(),
                    Style::default().fg(Color::Rgb(255, 200, 80)),
                ),
            ]));
        }

        let paragraph = Paragraph::new(welcome).alignment(Alignment::Left);
        frame.render_widget(paragraph, area);
    }

}

#[cfg(test)]
mod welcome_tests {
    // Regression: the old `&display[display.len().saturating_sub(47)..]` panicked when
    // the byte offset landed inside a multi-byte character. The fix walks forward to a
    // char boundary. This helper replicates the same pattern for isolated testing.
    fn truncate_path_display(display: &str, max_len: usize, ellipsis_reserve: usize) -> String {
        if display.len() > max_len {
            let mut start = display.len().saturating_sub(max_len - ellipsis_reserve);
            while !display.is_char_boundary(start) {
                start += 1;
            }
            format!("…{}", &display[start..])
        } else {
            display.to_string()
        }
    }

    #[test]
    fn test_truncate_multi_byte_cjk_path_does_not_panic() {
        let cjk_segment = "世界";
        let mut path = "/home/user/projects/".to_string();
        while path.len() <= 50 {
            path.push_str(cjk_segment);
        }
        assert!(path.len() > 50);

        let truncated = truncate_path_display(&path, 50, 3);
        assert!(!truncated.is_empty());
        assert!(truncated.starts_with('…'));
    }

    #[test]
    fn test_truncate_short_path_unchanged() {
        let path = "/home/user/proj";
        let result = truncate_path_display(path, 50, 3);
        assert_eq!(result, path);
    }

    #[test]
    fn test_truncate_ascii_path_preserves_content() {
        let path = "/very/long/directory/path/that/exceeds/fifty/characters/total";
        assert!(path.len() > 50);

        let truncated = truncate_path_display(path, 50, 3);
        assert!(truncated.starts_with('…'));
        assert!(truncated.len() < path.len());
        assert!(path.ends_with(truncated.trim_start_matches('…')));
    }

    #[test]
    fn test_truncate_all_multi_byte_string() {
        let s: String = "世界".repeat(20);
        assert!(s.len() > 50);

        let truncated = truncate_path_display(&s, 50, 3);
        assert!(truncated.starts_with('…'));
        assert!(!truncated.is_empty());
    }
}
