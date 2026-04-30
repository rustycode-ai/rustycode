    /// Render search box as a single-line bar at the bottom of the messages area.
    ///
    /// Messages remain visible above with search highlights so the user can
    /// see context around matches.
    pub fn render_search_box(tui: &mut TUI, frame: &mut Frame, area: Rect) {
        use ratatui::style::{Color, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Clear, Paragraph};

        if !tui.search_state.visible {
            return;
        }

        // Render search bar at the bottom of the messages area (1 row)
        let bar_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };

        // Clear only the search bar line
        frame.render_widget(Clear, bar_area);

        // Build search box content
        let mut spans = Vec::new();

        // Label
        spans.push(Span::styled(
            "Search> ",
            Style::default().fg(Color::Cyan).bold(),
        ));

        // Query text (truncate if too long for the bar)
        let query = &tui.search_state.query;
        let remaining_width = area.width as usize;
        // Reserve space for: "Search> " (8) + cursor (1) + match info (~15) + help (~30) = ~54
        let max_query = remaining_width.saturating_sub(54).max(10);
        let display_query = if query.len() > max_query {
            // Show the end of the query (where the cursor is)
            let start = query.len().saturating_sub(max_query);
            // Walk forward to the next char boundary (safe for multi-byte UTF-8)
            let start = (start..=query.len()).find(|&i| query.is_char_boundary(i)).unwrap_or(query.len());
            format!("...{}", &query[start..])
        } else {
            query.clone()
        };
        spans.push(Span::raw(display_query));

        // Blinking cursor indicator
        spans.push(Span::styled("│", Style::default().fg(Color::Cyan)));

        // Match count
        let match_info = if tui.search_state.match_count() == 0 {
            " no matches".to_string()
        } else {
            format!(
                " {}/{}",
                tui.search_state.current_match_number(),
                tui.search_state.match_count()
            )
        };
        spans.push(Span::styled(match_info, Style::default().fg(Color::Gray)));

        // Help text
        spans.push(Span::styled(
            " ↑↓ nav  Esc close",
            Style::default().fg(Color::DarkGray),
        ));

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line)
            .block(Block::default().style(Style::default().bg(Color::Rgb(30, 30, 40))));

        frame.render_widget(paragraph, bar_area);
    }
