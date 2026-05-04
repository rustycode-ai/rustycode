impl PolishedRenderer {
    /// Render input area with label and keyboard hints
    pub fn render_input(
        &self,
        tui: &mut TUI,
        frame: &mut Frame,
        area: Rect,
    ) {
        let is_multiline =
            tui.input_handler.state.mode == crate::ui::input::InputMode::MultiLine;
        let (label_area, input_area, hints_area) = self.input_areas(area);
        self.render_input_label(tui, frame, label_area, is_multiline);
        self.render_input_lines(tui, frame, input_area, is_multiline);
        self.render_input_hints(tui, frame, area, hints_area, is_multiline);
    }

    fn input_areas(
        &self,
        area: Rect,
    ) -> (
        Rect,
        Rect,
        Rect,
    ) {
        let label_area = Rect::new(area.x, area.y, area.width, 1);
        let input_area = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(2),
        );
        let hints_area = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(1),
            area.width,
            1,
        );
        (label_area, input_area, hints_area)
    }

    fn render_input_label(
        &self,
        tui: &TUI,
        frame: &mut Frame,
        label_area: Rect,
        is_multiline: bool,
    ) {
        use ratatui::text::Line;
        use ratatui::widgets::Paragraph;

        let label_spans = self.input_label_spans(tui, is_multiline);
        frame.render_widget(Paragraph::new(Line::from(label_spans)), label_area);
    }

    fn input_label_spans(
        &self,
        tui: &TUI,
        is_multiline: bool,
    ) -> Vec<ratatui::text::Span<'static>> {
        use ratatui::style::{Color, Style};
        use ratatui::text::Span;

        if tui.is_streaming {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let anim_frame = tui.animator.current_frame();
            let frame_idx = (anim_frame.progress_frame / 5) % frames.len();
            let msg_idx = anim_frame.progress_frame / 8;

            let mut spans = vec![
                Span::styled("│", Style::default().fg(Color::DarkGray)),
                Span::styled(" ", Style::default().fg(Color::White)),
                Span::styled(
                    format!(
                        "{} {}",
                        frames[frame_idx],
                        crate::app::thinking_messages::get_thinking_message(msg_idx)
                    ),
                    Style::default().fg(Color::Rgb(255, 200, 80)),
                ),
                Span::styled("  Ctrl+C cancel", Style::default().fg(Color::DarkGray)),
            ];
            spans.push(if tui.queued_message.is_some() {
                Span::styled("  📝 1 queued", Style::default().fg(Color::Rgb(180, 180, 255)))
            } else {
                Span::styled("  type to queue", Style::default().fg(Color::Rgb(120, 120, 140)))
            });
            spans
        } else {
            let mut spans = vec![
                Span::styled("│", Style::default().fg(Color::DarkGray)),
                Span::styled(" ", Style::default().fg(Color::White)),
                Span::styled(
                    if is_multiline { "📄 Multi-line" } else { "📝 Single-line" },
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(" ", Style::default().fg(Color::DarkGray)),
            ];
            let img_count = tui.input_handler.state.images.len();
            if img_count > 0 {
                spans.push(Span::styled(
                    format!(" 🖼 {} ", img_count),
                    Style::default().fg(Color::Rgb(255, 200, 80)),
                ));
            }
            let (rs_query, _rs_match, rs_total) = tui.input_handler.reverse_search_info();
            if !rs_query.is_empty() {
                spans.push(Span::styled(
                    format!(" 🔍 '{}/{} ", rs_query, rs_total),
                    Style::default().fg(Color::Rgb(255, 200, 80)),
                ));
            } else {
                let (hist_pos, hist_total) = tui.input_handler.history_position();
                if hist_pos > 0 {
                    spans.push(Span::styled(
                        format!(" 📜 {}/{} ", hist_pos, hist_total),
                        Style::default().fg(Color::Rgb(120, 160, 200)),
                    ));
                }
            }
            let char_count: usize = tui.input_handler.state.all_text().chars().count();
            if char_count > 500 {
                let count_color = if char_count > 5000 {
                    Color::Red
                } else if char_count > 2000 {
                    Color::Yellow
                } else {
                    Color::DarkGray
                };
                let fmt_count = if char_count >= 10_000 {
                    format!("{:.1}k", char_count as f64 / 1000.0)
                } else {
                    char_count.to_string()
                };
                spans.push(Span::styled(
                    format!(" {} chars", fmt_count),
                    Style::default().fg(count_color),
                ));
            }
            spans
        }
    }

    fn render_input_lines(
        &self,
        tui: &TUI,
        frame: &mut Frame,
        input_area: Rect,
        is_multiline: bool,
    ) {
        use ratatui::widgets::Paragraph;

        let lines = self.visible_input_lines(tui, input_area.height as usize, input_area.width as usize, is_multiline);
        frame.render_widget(Paragraph::new(lines), input_area);
    }

    fn visible_input_lines(
        &self,
        tui: &TUI,
        max_display_lines: usize,
        area_width: usize,
        is_multiline: bool,
    ) -> Vec<ratatui::text::Line<'static>> {
        use ratatui::text::Line;

        let state = &tui.input_handler.state;
        let lines = &state.lines;
        let cursor_row = state.cursor_row.min(lines.len().saturating_sub(1));
        let cursor_col = state.cursor_col;
        let start_row = if is_multiline && cursor_row >= max_display_lines {
            cursor_row - max_display_lines + 1
        } else {
            0
        };

        let mut display_lines = Vec::new();
        for (row_idx, row) in lines.iter().enumerate().skip(start_row).take(max_display_lines) {
            display_lines.push(Line::from(self.input_line_spans(
                tui,
                row_idx,
                row,
                cursor_row,
                cursor_col,
                area_width,
                is_multiline,
            )));
        }

        if display_lines.is_empty() {
            display_lines.push(Line::from(self.empty_input_line_spans(tui)));
        }

        display_lines
    }

    fn input_line_spans(
        &self,
        tui: &TUI,
        row_idx: usize,
        row: &str,
        cursor_row: usize,
        cursor_col: usize,
        area_width: usize,
        is_multiline: bool,
    ) -> Vec<ratatui::text::Span<'static>> {
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::Span;

        let mut spans = Vec::new();
        let prefix_width: usize;
        if is_multiline {
            let label = format!("{:>2} ", row_idx + 1);
            prefix_width = 3;
            spans.push(Span::styled(label, Style::default().fg(Color::DarkGray)));
        } else {
            prefix_width = 2;
            spans.push(Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))));
            spans.push(Span::raw(" "));
        }

        // Visible width for text content (area minus prefix and small padding)
        let text_visible = area_width.saturating_sub(prefix_width).saturating_sub(2);

        let state = &tui.input_handler.state;
        let has_sel = state.has_selection();

        // Apply horizontal scroll offset for the cursor row
        if row_idx == cursor_row {
            // Compute scroll offset to keep cursor visible within text_visible columns
            let cursor_display = state.cursor_display_col();
            let min_offset = cursor_display.saturating_sub(text_visible.saturating_sub(1));
            let offset = state.display_offset.clamp(min_offset, cursor_display);

            // Get byte offset for the scroll position
            let scroll_byte = if offset > 0 {
                use unicode_segmentation::UnicodeSegmentation;
                let mut col = 0;
                let mut byte_pos = 0;
                for (bi, g) in row.grapheme_indices(true) {
                    if col >= offset {
                        byte_pos = bi;
                        break;
                    }
                    col += crate::unicode::display_width(g);
                    byte_pos = bi + g.len();
                }
                byte_pos
            } else {
                0
            };

            let col = row.floor_char_boundary(cursor_col.min(row.len()));
            let visible_row = if scroll_byte > 0 && scroll_byte <= row.len() {
                &row[scroll_byte.min(row.len())..]
            } else {
                row
            };
            let adjusted_cursor = col.saturating_sub(scroll_byte.min(col));
            let clamped = visible_row.floor_char_boundary(adjusted_cursor.min(visible_row.len()));

            if has_sel {
                // Selection-aware rendering: highlight selected bytes with REVERSED
                let sel_style = Style::default().add_modifier(Modifier::REVERSED);
                let sel_range = state.get_selection_range();
                let (sel_start_row, _sel_start_col, sel_end_row, _sel_end_col) =
                    sel_range.unwrap_or((0, 0, 0, 0));

                let (before, rest) = visible_row.split_at(clamped);

                if !before.is_empty() {
                    if row_idx > sel_start_row && row_idx < sel_end_row {
                        spans.push(Span::styled(before.to_string(), sel_style));
                    } else {
                        spans.push(Span::raw(before.to_string()));
                    }
                }

                // Cursor character + after
                if let Some(ch) = rest.chars().next() {
                    let after = &rest[ch.len_utf8()..];
                    let cursor_on = (tui.animator.frame_count() / 2).is_multiple_of(2);
                    let cursor_style = if cursor_on {
                        Style::default().fg(Color::Black).bg(Color::White)
                    } else {
                        Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 80))
                    };
                    spans.push(Span::styled(ch.to_string(), cursor_style));
                    if !after.is_empty() {
                        spans.push(Span::raw(after.to_string()));
                    }
                } else if row.is_empty() && !is_multiline && !tui.is_streaming {
                    spans.push(self.cursor_span(tui));
                    spans.push(Span::styled(
                        self.input_placeholder(tui),
                        Style::default().fg(Color::Rgb(80, 80, 100)),
                    ));
                } else {
                    spans.push(self.cursor_span(tui));
                }
            } else {
                // Original non-selection rendering
                let (before, rest) = visible_row.split_at(clamped);

                if !before.is_empty() {
                    spans.push(Span::raw(before.to_string()));
                }

                if let Some(ch) = rest.chars().next() {
                    let after = &rest[ch.len_utf8()..];
                    let cursor_on = (tui.animator.frame_count() / 2).is_multiple_of(2);
                    let cursor_style = if cursor_on {
                        Style::default().fg(Color::Black).bg(Color::White)
                    } else {
                        Style::default().fg(Color::White).bg(Color::Rgb(60, 60, 80))
                    };
                    spans.push(Span::styled(ch.to_string(), cursor_style));
                    if !after.is_empty() {
                        spans.push(Span::raw(after.to_string()));
                    }
                } else if row.is_empty() && !is_multiline && !tui.is_streaming {
                    spans.push(self.cursor_span(tui));
                    spans.push(Span::styled(
                        self.input_placeholder(tui),
                        Style::default().fg(Color::Rgb(80, 80, 100)),
                    ));
                } else {
                    spans.push(self.cursor_span(tui));
                }
            }
        } else if has_sel {
            // Non-cursor row with selection: highlight entire line if within selection
            let sel_style = Style::default().add_modifier(Modifier::REVERSED);
            let sel_range = state.get_selection_range();
            let (sel_start_row, _, sel_end_row, _) = sel_range.unwrap_or((0, 0, 0, 0));

            if row_idx > sel_start_row && row_idx < sel_end_row {
                spans.push(Span::styled(row.to_string(), sel_style));
            } else {
                spans.push(Span::raw(row.to_string()));
            }
        } else {
            spans.push(Span::raw(row.to_string()));
        }

        spans
    }

    fn empty_input_line_spans(
        &self,
        tui: &TUI,
    ) -> Vec<ratatui::text::Span<'static>> {
        use ratatui::style::{Color, Style};
        use ratatui::text::Span;

        let mut spans = vec![
            Span::styled("❯", Style::default().fg(Color::Rgb(220, 80, 100))),
            Span::raw(" "),
            self.cursor_span(tui),
        ];

        let placeholder = self.input_placeholder(tui);
        if !placeholder.is_empty() {
            spans.push(Span::styled(
                placeholder,
                Style::default().fg(Color::Rgb(80, 80, 100)),
            ));
        }
        spans
    }

    fn cursor_span(&self, tui: &TUI) -> ratatui::text::Span<'static> {
        use ratatui::style::{Color, Style};
        use ratatui::text::Span;

        let cursor_visible = (tui.animator.frame_count() / 2).is_multiple_of(2);
        if cursor_visible {
            Span::styled("▏", Style::default().fg(Color::White))
        } else {
            Span::styled("▏", Style::default().fg(Color::DarkGray))
        }
    }

    fn input_placeholder(&self, tui: &TUI) -> &'static str {
        if tui.is_streaming {
            ""
        } else if tui.messages.is_empty() {
            " Ask me anything..."
        } else {
            " Message..."
        }
    }

    fn render_input_hints(
        &self,
        tui: &TUI,
        frame: &mut Frame,
        area: Rect,
        hints_area: Rect,
        is_multiline: bool,
    ) {
        use ratatui::text::Line;
        use ratatui::widgets::Paragraph;

        let send_hint = if tui.is_streaming {
            "⏎ Queue".to_string()
        } else {
            "⏎ Send".to_string()
        };
        let mode_hint = if is_multiline {
            "Ctrl+J".to_string()
        } else {
            String::new()
        };
        let scroll_hint = if tui.user_scrolled {
            "Home/End = top/bottom".to_string()
        } else {
            String::new()
        };

        let hints = if area.width > 70 {
            self.wide_input_hints(tui, area, send_hint, mode_hint, scroll_hint)
        } else {
            self.narrow_input_hints(area, send_hint)
        };

        frame.render_widget(Paragraph::new(Line::from(hints)), hints_area);
    }

    fn wide_input_hints(
        &self,
        tui: &TUI,
        area: Rect,
        send_hint: String,
        mode_hint: String,
        scroll_hint: String,
    ) -> Vec<ratatui::text::Span<'static>> {
        use ratatui::style::{Color, Style};
        use ratatui::text::Span;

        let hints_text = if tui.is_streaming {
            "Ctrl+C cancel · ↑↓ scroll"
        } else {
            "Ctrl+A/E nav · Ctrl+U/D scroll · Ctrl+X edit · Ctrl+R search"
        };
        let spacer_len = area.width.saturating_sub(68) as usize;
        let scroll_hint_len = scroll_hint.len();

        let mut hints = vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(hints_text, Style::default().fg(Color::DarkGray)),
        ];

        if !scroll_hint.is_empty() {
            hints.push(Span::raw(" ".repeat(spacer_len.min(4))));
            hints.push(Span::raw(scroll_hint.to_string()));
            hints.push(Span::raw(
                " ".repeat(spacer_len.saturating_sub(scroll_hint_len + 4)),
            ));
        } else {
            hints.push(Span::raw(" ".repeat(spacer_len)));
        }

        hints.push(Span::styled(send_hint, Style::default().fg(Color::Green)));
        hints.push(Span::raw("  "));
        hints.push(Span::styled(mode_hint, Style::default().fg(Color::DarkGray)));
        hints.push(Span::styled(" │", Style::default().fg(Color::DarkGray)));
        hints
    }

    fn narrow_input_hints(
        &self,
        area: Rect,
        send_hint: String,
    ) -> Vec<ratatui::text::Span<'static>> {
        use ratatui::style::{Color, Style};
        use ratatui::text::Span;

        vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::raw(" ".repeat(area.width.saturating_sub(12) as usize)),
            Span::styled(send_hint, Style::default().fg(Color::Green)),
            Span::styled(" │", Style::default().fg(Color::DarkGray)),
        ]
    }
}
