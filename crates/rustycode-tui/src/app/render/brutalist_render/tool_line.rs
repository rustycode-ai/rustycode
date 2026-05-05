impl BrutalistRenderer<'_> {
    fn render_tool_line<'b>(&self, tool: &'b ToolExecution, colors: &ThemeColors) -> Line<'b> {
        let (icon, color) = match tool.status {
            ToolStatus::Running => {
                let frames = ["◐", "◑", "◒", "◓"];
                (
                    frames[self.animation_frame % frames.len()],
                    Color::Rgb(255, 200, 80),
                )
            }
            ToolStatus::Complete => ("●", Color::Rgb(80, 200, 120)),
            ToolStatus::Failed => ("✗", Color::Rgb(255, 80, 80)),
            ToolStatus::Cancelled => ("⚠", Color::Rgb(200, 150, 50)),
        };

        let type_icon = tool_type_icon(&tool.name);

        let mut spans = vec![
            Span::styled("    ", Style::default().fg(colors.foreground)),
            Span::styled(icon, Style::default().fg(color)),
            Span::styled(" ", Style::default().fg(colors.foreground)),
        ];
        if !type_icon.is_empty() {
            spans.push(Span::styled(
                format!("{} ", type_icon),
                Style::default()
                    .fg(Color::Rgb(100, 140, 180))
                    .add_modifier(Modifier::DIM),
            ));
        }

        let display_name = match tool.name.as_str() {
            "read_file" => "read",
            "write_file" => "write",
            "edit_file" | "apply_patch" => "edit",
            "execute_command" | "bash" => "sh",
            "list_dir" | "list_files" => "ls",
            n => n,
        };
        spans.push(Span::styled(
            display_name,
            Style::default().fg(colors.foreground),
        ));

        let key_param = extract_tool_key_param(
            &tool.name,
            tool.input_json.as_ref(),
            &tool.result_summary,
        );
        if let Some(ref kp) = key_param {
            let truncated =
                if <str as unicode_width::UnicodeWidthStr>::width(kp.as_str()) > 50 {
                    shorten_tool_param(kp, 50)
                } else {
                    kp.clone()
                };
            spans.push(Span::styled(
                format!(" {}", truncated),
                Style::default()
                    .fg(Color::Rgb(140, 150, 170))
                    .add_modifier(Modifier::DIM),
            ));
        }

        if let Some(dur_ms) = tool.duration_ms {
            spans.push(Span::styled(
                format!(
                    " {}",
                    crate::app::tool_output_format::format_duration(dur_ms)
                ),
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::DIM),
            ));
        } else if tool.status == ToolStatus::Running {
            let elapsed = Utc::now()
                .signed_duration_since(tool.start_time)
                .num_milliseconds()
                .max(0) as u64;
            spans.push(Span::styled(
                format!(
                    " {}",
                    crate::app::tool_output_format::format_duration(elapsed)
                ),
                Style::default()
                    .fg(Color::Rgb(255, 200, 80))
                    .add_modifier(Modifier::DIM),
            ));
        }

        if tool.status == ToolStatus::Running {
            if let (Some(current), Some(total), Some(desc)) = (
                tool.progress_current,
                tool.progress_total,
                &tool.progress_description,
            ) {
                let pct = if total > 0 {
                    ((current as f64 / total as f64 * 100.0).round() as u16).clamp(0, 100)
                } else {
                    0
                };
                let bar_width = 10;
                let filled = ((pct as usize * bar_width) / 100).min(bar_width);
                let empty = bar_width - filled;
                let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

                spans.push(Span::styled(
                    format!(" [{} {}%]", bar, pct),
                    Style::default().fg(Color::Rgb(100, 180, 255)),
                ));
                let desc_display = if <str as unicode_width::UnicodeWidthStr>::width(desc.as_str())
                    > 40
                {
                    format!(
                        "{}…",
                        crate::app::render::brutalist_helpers::truncate_to_display_width(desc, 39)
                    )
                } else {
                    desc.clone()
                };
                spans.push(Span::styled(
                    format!(" {}", desc_display),
                    Style::default().fg(colors.muted),
                ));
            }
        }

        Line::from(spans)
    }

}
