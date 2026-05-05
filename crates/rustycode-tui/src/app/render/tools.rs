/// Render tool panel (right side) showing tool execution progress and history
pub fn render_tool_panel(
    tui: &mut TUI,
    frame: &mut Frame,
    area: Rect,
) {
    use ratatui::style::Color;
    use ratatui::style::Style;
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Clear, Paragraph, Wrap};

    // If showing detailed result, render a full-screen overlay
    if tui.tool_panel.showing_tool_result {
        if let Some(selected_idx) = tui.tool_panel.tool_panel_selected_index {
            if selected_idx < tui.tool_panel.tool_panel_history.len() {
                // Clone required: render_tool_result_detail needs &mut TUI,
                // so we can't hold a reference into tool_panel_history simultaneously.
                let tool = tui.tool_panel.tool_panel_history[selected_idx].clone();
                render_tool_result_detail(tui, frame, area, &tool);
                return;
            }
        }
    }

    let mut lines = Vec::new();

    // Title with help hint
    let tool_count = tui.tool_panel.tool_panel_history.len();
    lines.push(Line::from(vec![
        Span::styled(
            "● Tool Panel",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::styled(
            format!(" ↑↓ Enter Ctrl+C [{}]", tool_count),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    // Goose-inspired stats line: total | running | done | fail
    if !tui.tool_panel.tool_panel_history.is_empty() {
        let running = tui
            .tool_panel.tool_panel_history
            .iter()
            .filter(|t| matches!(t.status, crate::ui::message::ToolStatus::Running))
            .count();
        let completed = tui
            .tool_panel.tool_panel_history
            .iter()
            .filter(|t| matches!(t.status, crate::ui::message::ToolStatus::Complete))
            .count();
        let failed = tui
            .tool_panel.tool_panel_history
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    crate::ui::message::ToolStatus::Failed
                        | crate::ui::message::ToolStatus::Cancelled
                )
            })
            .count();

        let mut stats = vec![
            Span::styled(
                format!(" {}", tool_count),
                Style::default().fg(Color::White),
            ),
            Span::styled(" total", Style::default().fg(Color::DarkGray)),
        ];
        if running > 0 {
            stats.push(Span::styled(
                format!(" │ ◐ {}", running),
                Style::default().fg(Color::Yellow),
            ));
        }
        if completed > 0 {
            stats.push(Span::styled(
                format!(" │ ● {}", completed),
                Style::default().fg(Color::Green),
            ));
        }
        if failed > 0 {
            stats.push(Span::styled(
                format!(" │ ✗ {}", failed),
                Style::default().fg(Color::Red),
            ));
        }
        lines.push(Line::from(stats));
    }

    lines.push(Line::from(""));

    // Group tools by status for section headers
    let running_tools: Vec<_> = tui
        .tool_panel.tool_panel_history
        .iter()
        .filter(|t| t.status == crate::ui::message::ToolStatus::Running)
        .collect();
    let completed_tools: Vec<_> = tui
        .tool_panel.tool_panel_history
        .iter()
        .filter(|t| t.status == crate::ui::message::ToolStatus::Complete)
        .collect();
    let failed_tools: Vec<_> = tui
        .tool_panel.tool_panel_history
        .iter()
        .filter(|t| {
            t.status == crate::ui::message::ToolStatus::Failed
                || t.status == crate::ui::message::ToolStatus::Cancelled
        })
        .collect();

    // Use tool_panel_history directly for display — navigation indices
    // match this order, so selection highlight stays in sync with up/down keys
    if tui.tool_panel.tool_panel_history.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "No tool history yet",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        // Show section headers if we have multiple sections
        let show_sections =
            !running_tools.is_empty() && (!completed_tools.is_empty() || !failed_tools.is_empty());

        for (idx, tool) in tui.tool_panel.tool_panel_history.iter().enumerate() {
            let is_selected = tui.tool_panel.tool_panel_selected_index == Some(idx);
            let has_detail = tool.detailed_output.is_some() || !tool.result_summary.is_empty();

            // Add section headers based on tool status transitions
            if show_sections {
                if idx == 0 && !running_tools.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        "▸ Running",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )]));
                }
                // Show section header when transitioning from running to completed
                if idx > 0
                    && matches!(
                        tui.tool_panel.tool_panel_history[idx - 1].status,
                        crate::ui::message::ToolStatus::Running
                    )
                    && matches!(tool.status, crate::ui::message::ToolStatus::Complete)
                {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        "✓ Completed",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )]));
                }
                // Show section header when transitioning to failed
                if idx > 0
                    && !matches!(
                        tui.tool_panel.tool_panel_history[idx - 1].status,
                        crate::ui::message::ToolStatus::Failed
                            | crate::ui::message::ToolStatus::Cancelled
                    )
                    && matches!(
                        tool.status,
                        crate::ui::message::ToolStatus::Failed
                            | crate::ui::message::ToolStatus::Cancelled
                    )
                {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        "✗ Failed/Cancelled",
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    )]));
                }
            }

            let (status_icon, status_color) = match tool.status {
                crate::ui::message::ToolStatus::Running => ("◐", Color::Yellow),
                crate::ui::message::ToolStatus::Complete => ("●", Color::Green),
                crate::ui::message::ToolStatus::Failed => ("✗", Color::Red),
                crate::ui::message::ToolStatus::Cancelled => ("⚠", Color::Rgb(200, 150, 50)),
            };

            // Format timestamp
            let time_str = if let Some(end_time) = tool.end_time {
                format!("{}", end_time.format("%H:%M:%S"))
            } else {
                format!("{}", tool.start_time.format("%H:%M:%S"))
            };

            // Truncate tool name if very long (e.g., MCP tool names)
            let display_name = if tool.name.len() > 30 {
                format!("{}…", &tool.name[..tool.name.floor_char_boundary(29)])
            } else {
                tool.name.clone()
            };

            let mut spans = vec![
                // Selection indicator
                if is_selected {
                    Span::styled("►", Style::default().fg(Color::Cyan))
                } else {
                    Span::raw(" ")
                },
                Span::raw(" "),
                // Status icon
                Span::styled(status_icon, Style::default().fg(status_color)),
                Span::raw(" "),
                // Tool kind icon (goose-inspired)
                Span::styled(
                    tool_kind_icon(&tool.name),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(" "),
                // Tool name
                Span::styled(
                    display_name,
                    if is_selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(ratatui::style::Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
                // Timestamp
                Span::raw(" "),
                Span::styled(time_str, Style::default().fg(Color::DarkGray)),
            ];

            // Add indicator if detailed output is available
            if has_detail {
                spans.push(Span::styled(
                    " [details]",
                    Style::default().fg(Color::DarkGray),
                ));
            }

            // Add elapsed time if available
            if let Some(duration) = tool.duration_ms {
                let elapsed_str = if duration < 1000 {
                    format!(" {}ms", duration)
                } else {
                    format!(" {:.1}s", duration as f64 / 1000.0)
                };
                spans.push(Span::styled(
                    elapsed_str,
                    Style::default().fg(Color::DarkGray),
                ));
            } else if tool.status == crate::ui::message::ToolStatus::Running {
                // Show running time for active tools
                let elapsed_ms = chrono::offset::Utc::now()
                    .signed_duration_since(tool.start_time)
                    .num_milliseconds()
                    .max(0) as u64;
                let elapsed_str = if elapsed_ms < 1000 {
                    format!(" {}ms", elapsed_ms)
                } else {
                    format!(" {:.1}s", elapsed_ms as f64 / 1000.0)
                };
                spans.push(Span::styled(
                    elapsed_str,
                    Style::default().fg(Color::DarkGray),
                ));
            }

            lines.push(Line::from(spans));

            // Goose-inspired result preview line (compact 1-line summary)
            if is_selected && !tool.result_summary.is_empty() {
                let preview = crate::app::tool_output_format::output_summary(&tool.result_summary);
                let preview_display = if crate::unicode::display_width(&preview) > 60 {
                    format!("  {}", crate::unicode::truncate_display(&preview, 60))
                } else {
                    format!("  {}", preview)
                };
                lines.push(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(preview_display, Style::default().fg(Color::DarkGray)),
                ]));
            }
        }

        // Add total duration footer when all tools are done
        if running_tools.is_empty() && !tui.tool_panel.tool_panel_history.is_empty() {
            let total_ms: u64 = tui
                .tool_panel.tool_panel_history
                .iter()
                .filter_map(|t| t.duration_ms)
                .sum();
            let completed_count = tui
                .tool_panel.tool_panel_history
                .iter()
                .filter(|t| matches!(t.status, crate::ui::message::ToolStatus::Complete))
                .count();
            let failed_count = tui
                .tool_panel.tool_panel_history
                .iter()
                .filter(|t| {
                    matches!(
                        t.status,
                        crate::ui::message::ToolStatus::Failed
                            | crate::ui::message::ToolStatus::Cancelled
                    )
                })
                .count();

            lines.push(Line::from(""));

            // Success rate (goose pattern)
            if completed_count + failed_count > 0 {
                let rate =
                    (completed_count as f64 / (completed_count + failed_count) as f64) * 100.0;
                let rate_color = if rate >= 80.0 {
                    Color::Green
                } else if rate >= 50.0 {
                    Color::Yellow
                } else {
                    Color::Red
                };
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(format!("{:.0}% ok", rate), Style::default().fg(rate_color)),
                    Span::styled(
                        format!(" ({}/{})", completed_count, completed_count + failed_count),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }

            if total_ms > 0 {
                let dur_str = if total_ms < 1000 {
                    format!("  Total: {}ms", total_ms)
                } else {
                    format!("  Total: {:.1}s", total_ms as f64 / 1000.0)
                };
                lines.push(Line::from(vec![Span::styled(
                    dur_str,
                    Style::default().fg(Color::DarkGray),
                )]));
            }
        }
    }

    // Clear the area first to prevent content bleed-through
    frame.render_widget(Clear, area);

    // Brutalist-style rendering: heavy top border + left border, no surrounding box
    let mut brutalist_lines = Vec::new();

    // Top border with title
    let title = " Tools ";
    let side_space = (area.width as usize).saturating_sub(title.len() + 2);
    let left_pad = side_space / 2;
    let right_pad = side_space - left_pad;
    let top_border = format!(
        "╺{}{}{}╸",
        "━".repeat(left_pad),
        title,
        "━".repeat(right_pad),
    );
    brutalist_lines.push(Line::from(Span::styled(
        top_border,
        Style::default().fg(Color::Cyan),
    )));

    // Wrap each content line with brutalist left border
    for line in &lines {
        let mut spans = vec![Span::styled("▐ ", Style::default().fg(Color::Cyan))];
        spans.extend(line.spans.iter().cloned());
        brutalist_lines.push(Line::from(spans));
    }

    // Bottom border
    let bottom_border = format!("╺{}╸", "━".repeat((area.width as usize).saturating_sub(2)));
    brutalist_lines.push(Line::from(Span::styled(
        bottom_border,
        Style::default().fg(Color::DarkGray),
    )));

    let paragraph = Paragraph::new(brutalist_lines).wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

/// Render detailed tool result (full-screen overlay)
fn render_tool_result_detail(
    tui: &mut TUI,
    frame: &mut Frame,
    area: Rect,
    tool: &crate::ui::message::ToolExecution,
) {
    use ratatui::layout::Alignment;
    use ratatui::style::Color;
    use ratatui::style::Style;
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

    // Clear the area first
    frame.render_widget(Clear, area);

    let mut lines = Vec::new();

    // Title with tool name
    let (status_icon, status_color) = match tool.status {
        crate::ui::message::ToolStatus::Running => ("◐", Color::Yellow),
        crate::ui::message::ToolStatus::Complete => ("●", Color::Green),
        crate::ui::message::ToolStatus::Failed => ("✗", Color::Red),
        crate::ui::message::ToolStatus::Cancelled => ("⚠", Color::Rgb(200, 150, 50)),
    };

    // Title with tool name (truncated if very long)
    let display_name = if tool.name.len() > 40 {
        format!("{}…", &tool.name[..tool.name.floor_char_boundary(39)])
    } else {
        tool.name.clone()
    };
    lines.push(Line::from(vec![
        Span::styled(status_icon, Style::default().fg(status_color)),
        Span::raw(" "),
        Span::styled(
            display_name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
    ]));

    // Summary line
    lines.push(Line::from(vec![Span::styled(
        &tool.result_summary,
        Style::default().fg(Color::White),
    )]));
    lines.push(Line::from(""));

    // Duration info — show completed or live elapsed for running tools
    let duration_str = if let Some(duration) = tool.duration_ms {
        if duration < 1000 {
            format!("Duration: {}ms", duration)
        } else {
            format!("Duration: {:.2}s", duration as f64 / 1000.0)
        }
    } else if tool.status == crate::ui::message::ToolStatus::Running {
        let elapsed_ms = chrono::offset::Utc::now()
            .signed_duration_since(tool.start_time)
            .num_milliseconds()
            .max(0) as u64;
        if elapsed_ms < 1000 {
            format!("Elapsed: {}ms...", elapsed_ms)
        } else {
            format!("Elapsed: {:.1}s...", elapsed_ms as f64 / 1000.0)
        }
    } else {
        String::new()
    };
    if !duration_str.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            duration_str,
            Style::default().fg(Color::DarkGray),
        )]));
        lines.push(Line::from(""));
    }

    // Input parameters section (Goose pattern: show what the tool was called with)
    if let Some(input_json) = &tool.input_json {
        lines.push(Line::from(vec![Span::styled(
            "Input:",
            Style::default()
                .fg(Color::Rgb(100, 180, 255))
                .add_modifier(ratatui::style::Modifier::BOLD),
        )]));

        let json_str =
            serde_json::to_string_pretty(input_json).unwrap_or_else(|_| "{}".to_string());
        let max_input_lines = 12;
        for (i, json_line) in json_str.lines().enumerate() {
            if i >= max_input_lines {
                let remaining = json_str.lines().count().saturating_sub(max_input_lines);
                lines.push(Line::from(vec![Span::styled(
                    format!("  ... {} more lines", remaining),
                    Style::default().fg(Color::DarkGray),
                )]));
                break;
            }
            lines.push(Line::from(vec![Span::styled(
                format!("  {}", json_line),
                Style::default().fg(Color::Rgb(160, 170, 190)),
            )]));
        }
        lines.push(Line::from(""));
    }

    // Error section for failed tools — prominent red header
    if tool.status == crate::ui::message::ToolStatus::Failed {
        let error_source = tool
            .detailed_output
            .as_deref()
            .unwrap_or(&tool.result_summary);
        if !error_source.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "Error:",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )]));
            // Show first 5 lines of error with red tint
            for line in error_source.lines().take(5) {
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}", line),
                    Style::default().fg(Color::Rgb(200, 120, 120)),
                )]));
            }
            let remaining = error_source.lines().count().saturating_sub(5);
            if remaining > 0 {
                lines.push(Line::from(vec![Span::styled(
                    format!("  ... {} more lines", remaining),
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            lines.push(Line::from(""));
        }
    }

    // Detailed output section (skip for failed tools — already shown in error section)
    let show_output_section = tool.status != crate::ui::message::ToolStatus::Failed
        || tool
            .detailed_output
            .as_ref()
            .is_some_and(|o| !o.is_empty() && o != &tool.result_summary);

    if !show_output_section {
        // For failed tools where error already shown, just show fallback summary
        if tool.detailed_output.is_none()
            && !tool.result_summary.is_empty()
            && tool.status != crate::ui::message::ToolStatus::Failed
        {
            lines.push(Line::from(vec![Span::styled(
                tool.result_summary.clone(),
                Style::default().fg(Color::Gray),
            )]));
        }
    } else if let Some(output) = &tool.detailed_output {
        // Output header
        lines.push(Line::from(vec![Span::styled(
            "Output:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )]));
        lines.push(Line::from(""));
        // Goose-inspired truncation: show first+last with toggle support
        let display_output: String = if tui.tool_panel.tool_result_show_full || output.len() <= 5000 {
            output.clone()
        } else {
            // Head+tail truncation: show first ~2500 and last ~2000 chars
            // This is more useful than just showing the beginning because
            // the end often contains the actual result or error message

            // Find clean boundaries
            let head_end = output.chars().take(2500).map(|c| c.len_utf8()).sum::<usize>();
            let tail_offset = output.chars().rev().take(2000).map(|c| c.len_utf8()).sum::<usize>();
            let tail_start = output.len().saturating_sub(tail_offset);
            // Ensure tail_start is on a char boundary
            let tail_start = if output.is_char_boundary(tail_start) {
                tail_start
            } else {
                output.ceil_char_boundary(tail_start)
            };

            // Find good line boundaries for clean cuts
            let head_cut = output[..head_end]
                .rfind('\n')
                .map(|pos| pos + 1)
                .unwrap_or(head_end);

            let tail_cut = output[tail_start..]
                .find('\n')
                .map(|pos| tail_start + pos + 1)
                .unwrap_or(tail_start);

            let head_part = &output[..head_cut];
            let tail_part = &output[tail_cut..];

            let hidden_lines = output[head_cut..tail_cut].lines().count();

            let mut result = head_part.to_string();

            // Close any open code fences in head
            let open_fences = result.matches("```").count();
            if open_fences % 2 != 0 {
                result.push_str("\n```\n");
            }

            // Show truncation indicator
            result.push_str(&format!(
                "\n\n  ⋮ {} lines hidden — press F to show full output\n\n",
                hidden_lines
            ));

            // Open code fence if tail starts inside one
            let tail_fences = tail_part.matches("```").count();
            if tail_fences % 2 != 0 {
                result.push_str("```\n");
            }

            result.push_str(tail_part);

            // Close any remaining open fences
            let total_fences = result.matches("```").count();
            if total_fences % 2 != 0 {
                result.push_str("\n```\n");
            }

            result
        };

        // Render output with syntax highlighting via markdown renderer
        let theme = rustycode_ui_core::MessageTheme::default();
        let rendered = tui
            .message_renderer
            .render_markdown_content(&display_output, &theme);

        // Scroll support: skip lines above the scroll offset
        let max_display_lines = if tui.tool_panel.tool_result_show_full {
            usize::MAX
        } else {
            (tui.viewport_height * 2).max(50)
        };
        // Clamp scroll offset to prevent unbounded growth from repeated Down presses
        let total_rendered = rendered.len();
        let scroll_offset = tui
            .tool_panel.tool_result_scroll_offset
            .min(total_rendered.saturating_sub(1));
        for line in rendered
            .into_iter()
            .skip(scroll_offset)
            .take(max_display_lines)
        {
            lines.push(line);
        }
    } else if !tool.result_summary.is_empty() {
        // Fallback: show result_summary when no detailed_output
        lines.push(Line::from(vec![Span::styled(
            "Output:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![Span::styled(
            tool.result_summary.clone(),
            Style::default().fg(Color::Gray),
        )]));
    } else {
        lines.push(Line::from(vec![Span::styled(
            "No output available.",
            Style::default().fg(Color::DarkGray),
        )]));
    }

    // Help text at bottom
    lines.push(Line::from(""));
    let toggle_hint = if tui.tool_panel.tool_result_show_full {
        "F truncate"
    } else if tool
        .detailed_output
        .as_ref()
        .is_some_and(|o| o.len() > 5000)
    {
        "F full view"
    } else {
        ""
    };
    let mut help_spans = vec![
        Span::styled("[", Style::default().fg(Color::Gray)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::styled(" close", Style::default().fg(Color::Gray)),
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "↑↓ PgUp/PgDn",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::styled(" scroll", Style::default().fg(Color::Gray)),
    ];
    if !toggle_hint.is_empty() {
        help_spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        help_spans.push(Span::styled(
            toggle_hint,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
    }
    help_spans.push(Span::styled("]", Style::default().fg(Color::Gray)));
    lines.push(Line::from(help_spans));

    // Create the paragraph widget
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(status_color))
                .title(format!(" Tool Result: {} ", tool.name)),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

/// Render worker status panel (right side overlay)
pub fn render_worker_panel(
    tui: &mut TUI,
    frame: &mut Frame,
    area: Rect,
) {
    use ratatui::widgets::Clear;

    // Clear the area first to prevent content bleed-through
    frame.render_widget(Clear, area);

    // Clone the panel since render consumes self
    let panel = tui.worker_panel.clone();
    frame.render_widget(panel, area);
}
