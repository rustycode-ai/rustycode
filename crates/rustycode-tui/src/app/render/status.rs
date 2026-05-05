impl PolishedRenderer {
    pub fn render_status(
        &self,
        tui: &mut TUI,
        frame: &mut Frame,
        area: Rect,
    ) {
        use crate::app::plan_mode_ops::PlanModeBanner;
        use ratatui::style::Color;
        use ratatui::style::Style;
        use ratatui::text::{Line, Span};
        use ratatui::widgets::Paragraph;

        let anim_frame = tui.animator.current_frame();
        let width = area.width as usize;

        let show_context_bar = width >= 70;
        let show_cost = width >= 85;
        let show_git_branch = width >= 90;
        let show_task_counts = width >= 80;

        // Plan-mode banners take priority over other states.
        let status = if let Some(banner) = tui.plan_mode_banner.clone() {
            RenderStatus::PlanMode { banner }
        } else if tui.streaming.is_streaming {
            RenderStatus::Thinking {
                chunks_received: tui.streaming.chunks_received,
                thinking_chunks_received: tui.streaming.thinking_chunks_received,
            }
        } else if tui.ast_phase_state.is_active() {
            let ast = &tui.ast_phase_state;
            RenderStatus::AstPhase {
                phase: ast.phase.clone(),
                phase_index: ast.phase_index,
                milestones_completed: ast.milestones_completed,
                milestones_total: ast.milestones_total,
                elapsed_ms: ast.total_elapsed_ms,
            }
        } else if !tui.active_tools.is_empty() {
            let tool_names: Vec<String> = tui.active_tools.keys().take(3).cloned().collect();
            let remaining = tui.active_tools.len().saturating_sub(3);

            RenderStatus::RunningTools {
                count: tui.active_tools.len(),
                tool_names,
                remaining,
            }
        } else {
            RenderStatus::Idle
        };

        let mut spans = Vec::new();

        match status {
            RenderStatus::PlanMode { banner } => {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let frame_idx = (anim_frame.progress_frame / 5) % frames.len();

                let icon = match banner {
                    PlanModeBanner::AwaitingApproval { .. } => "⚠ ",
                    PlanModeBanner::PlanApproved { .. } => "✓ ",
                    _ => &format!("{} ", frames[frame_idx]),
                };

                spans.push(Span::styled(
                    format!("{}{}", icon, banner.title()),
                    Style::default()
                        .fg(banner.status_color())
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    banner.description(),
                    Style::default().fg(banner.status_color()),
                ));
            }
            RenderStatus::Thinking { chunks_received, thinking_chunks_received } => {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let frame_idx = (anim_frame.progress_frame / 5) % frames.len();
                let thinking_msg = crate::app::thinking_messages::get_thinking_message(
                    anim_frame.progress_frame / 60,
                );
                spans.push(Span::styled(
                    format!("{} {} ", frames[frame_idx], thinking_msg),
                    Style::default().fg(Color::Cyan),
                ));
                if thinking_chunks_received > 0 {
                    spans.push(Span::styled(
                        format!("({} reasoning) ", thinking_chunks_received),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                let _ = chunks_received;
                if let Some(dur) = tui.streaming.stream_start_time {
                    let elapsed = dur.elapsed();
                    if elapsed.as_secs() >= 2 {
                        spans.push(Span::styled(
                            format!("{}s ", elapsed.as_secs()),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }
                if !tui.streaming.current_stream_content.is_empty() {
                    let words = tui.streaming.current_stream_content.split_whitespace().count();
                    if words > 20 {
                        spans.push(Span::styled(
                            format!("· {} words ", words),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }
            }
            RenderStatus::RunningTools {
                count,
                tool_names,
                remaining,
            } => {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let frame_idx = (anim_frame.progress_frame / 5) % frames.len();
                spans.push(Span::styled(
                    format!(
                        "{} Running {} tool{}",
                        frames[frame_idx],
                        count,
                        if count > 1 { "s" } else { "" }
                    ),
                    Style::default().fg(Color::Yellow),
                ));
                if !tool_names.is_empty() {
                    let names_display = tool_names.join(", ");
                    spans.push(Span::styled(
                        format!(": {}", names_display),
                        Style::default().fg(Color::DarkGray),
                    ));
                    if remaining > 0 {
                        spans.push(Span::styled(
                            format!(" +{} more", remaining),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }

                // Show running-tool progress bar if available, otherwise latest description.
                if let Some(tool) = tui.active_tools.values().find(|tool| {
                    tool.status == crate::ui::message::ToolStatus::Running
                        && (tool.progress_current.is_some()
                            || tool.progress_total.is_some()
                            || tool.progress_description.is_some())
                }) {
                    spans.push(Span::raw(" | "));
                    if let Some(pct) = tool.progress_percent() {
                        let pct = pct.round().clamp(0.0, 100.0) as usize;
                        let bar_width = 8;
                        let filled = usize::div_ceil(pct * bar_width, 100).min(bar_width);
                        let empty = bar_width - filled;
                        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
                        spans.push(Span::styled(
                            format!("{} {}%", bar, pct),
                            Style::default().fg(Color::Rgb(100, 180, 255)),
                        ));
                    }

                    if let Some(stage) = tool.progress_description.as_deref() {
                        if tool.progress_percent().is_some() {
                            spans.push(Span::raw(" "));
                        }
                        spans.push(Span::styled(
                            stage.to_string(),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                }
            }
            RenderStatus::AstPhase {
                phase,
                phase_index,
                milestones_completed,
                milestones_total,
                elapsed_ms,
            } => {
                let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let frame_idx = (anim_frame.progress_frame / 5) % frames.len();
                let ast_color = tui.ast_phase_state.status_color();

                spans.push(Span::styled(
                    format!("{} AST ", frames[frame_idx]),
                    Style::default().fg(ast_color).add_modifier(ratatui::style::Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    format!("{}/{}:{}", phase_index + 1, 6, phase),
                    Style::default().fg(ast_color),
                ));

                if milestones_total > 0 {
                    let bar_width = 8usize;
                    let filled = usize::div_ceil(milestones_completed * bar_width, milestones_total)
                        .min(bar_width);
                    let empty = bar_width - filled;
                    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        bar,
                        Style::default().fg(Color::Rgb(100, 180, 255)),
                    ));
                }

                if elapsed_ms > 0 {
                    let secs = elapsed_ms / 1000;
                    let time_str = if secs < 60 {
                        format!("{}s", secs)
                    } else {
                        format!("{}m{}s", secs / 60, secs % 60)
                    };
                    spans.push(Span::styled(
                        format!(" {}", time_str),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
            RenderStatus::Idle => {
                spans.push(Span::styled("✓ Ready", Style::default().fg(Color::Green)));
                if let Some(dur) = tui.streaming.last_response_duration {
                    spans.push(Span::styled(
                        format!(" {}", format_response_duration(dur)),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }

        // Turn counter (goose pattern) — count user+assistant message pairs
        let turn_count = tui
            .messages
            .iter()
            .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
            .count();
        if turn_count > 0 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!(
                    "{} turn{}",
                    turn_count,
                    if turn_count != 1 { "s" } else { "" }
                ),
                Style::default().fg(Color::DarkGray),
            ));
        }

        spans.push(Span::raw(" | "));

        if let Some((scanned, total)) = tui.workspace_scan_progress {
            let pct = if total > 0 {
                ((scanned as f64 / total as f64 * 100.0).round() as u16).clamp(0, 100)
            } else {
                0
            };
            spans.push(Span::styled(
                format!("🔍 Scanning... {}% ({}/{})", pct, scanned, total),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
            spans.push(Span::raw(" | "));
        }

        if let Some(until) = tui.rate_limit.until {
            let remaining = until.saturating_duration_since(std::time::Instant::now());
            let remaining_secs = remaining.as_secs();
            if remaining_secs > 0 {
                spans.push(Span::styled(
                    format!("⏱️ Rate limit: {}s ", remaining_secs),
                    Style::default().fg(Color::Red),
                ));
                spans.push(Span::raw(" | "));
            }
        }

        // Input mode and agent mode are already shown in the input area — skip here

        let agents = tui.agent_manager.get_agents();
        let running_agents: Vec<_> = agents
            .iter()
            .filter(|a| matches!(a.status, crate::agents::AgentStatus::Running))
            .collect();

        let in_progress_tasks = tui
            .workspace_tasks
            .tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    crate::tasks::TaskStatus::InProgress
                )
            })
            .count();

        let pending_todos = tui.workspace_tasks.todos.iter().filter(|t| !t.done).count();

        if show_task_counts
            && (!running_agents.is_empty() || in_progress_tasks > 0 || pending_todos > 0)
        {
            spans.push(Span::raw(" | "));
            let mut activity_spans = Vec::new();

            if !running_agents.is_empty() {
                let elapsed =
                    if let Some(longest) = running_agents.iter().max_by_key(|a| a.elapsed_secs) {
                        format!(" ({})", longest.formatted_time())
                    } else {
                        String::new()
                    };
                activity_spans.push(Span::styled(
                    format!("🤖{}{}", running_agents.len(), elapsed),
                    Style::default().fg(Color::Yellow),
                ));
            }

            if in_progress_tasks > 0 {
                if !activity_spans.is_empty() {
                    activity_spans.push(Span::raw(" "));
                }
                activity_spans.push(Span::styled(
                    format!("🔄{}", in_progress_tasks),
                    Style::default().fg(Color::Yellow),
                ));
            }

            if pending_todos > 0 {
                if !activity_spans.is_empty() {
                    activity_spans.push(Span::raw(" "));
                }
                activity_spans.push(Span::styled(
                    format!("📋{}", pending_todos),
                    Style::default().fg(Color::White),
                ));
            }

            spans.push(Span::raw(""));
            spans.extend(activity_spans);
        }

        if show_context_bar {
            let usage_pct = (tui.context_monitor.usage_percentage() * 100.0) as usize;
            spans.push(Span::raw(" | "));
            let token_color = if usage_pct < 50 {
                Color::Green
            } else if usage_pct < 80 {
                Color::Yellow
            } else {
                Color::Red
            };
            let bar_width = 10;
            let filled = if usage_pct > 0 {
                usize::div_ceil(usage_pct * bar_width, 100).min(bar_width)
            } else {
                0
            };
            let empty = bar_width - filled;
            let bar = format!("{}{}", "━".repeat(filled), "╌".repeat(empty));
            let fmt_tokens = |n: usize| -> String {
                if n >= 1_000_000 {
                    format!("{:.1}M", n as f64 / 1_000_000.0)
                } else if n >= 1_000 {
                    format!("{:.0}k", n as f64 / 1_000.0)
                } else {
                    n.to_string()
                }
            };
            let current_tokens = tui.context_monitor.current_tokens;
            let max_tokens = tui.context_monitor.max_tokens;
            let display_model = tui
                .current_model
                .rsplit('/')
                .next()
                .map(|s| {
                    if let Some(stripped) = s.strip_prefix("claude-") {
                        stripped
                    } else {
                        s
                    }
                })
                .unwrap_or(&tui.current_model);
            spans.push(Span::styled(bar, Style::default().fg(token_color)));
            spans.push(Span::raw(" "));
            if width >= 100 && max_tokens > 0 {
                spans.push(Span::styled(
                    format!("{}/{}", fmt_tokens(current_tokens), fmt_tokens(max_tokens)),
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                spans.push(Span::styled(
                    display_model.to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }

        if show_cost && tui.token_budget.session_cost_usd > 0.0 {
            let cost_str = if tui.token_budget.session_cost_usd < 0.01 {
                format!("${:.4}", tui.token_budget.session_cost_usd)
            } else if tui.token_budget.session_cost_usd < 1.0 {
                format!("${:.3}", tui.token_budget.session_cost_usd)
            } else {
                format!("${:.2}", tui.token_budget.session_cost_usd)
            };
            spans.push(Span::raw(" "));
            spans.push(Span::styled(cost_str, Style::default().fg(Color::Yellow)));
        }

        if show_git_branch {
            if let Some(branch) = &tui.git_branch {
                let display_branch = if crate::unicode::display_width(branch) > 25 {
                    crate::unicode::truncate_display(branch, 25)
                } else {
                    branch.clone()
                };
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("| {} ", display_branch),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ));
            }
        }

        if tui.user_scrolled {
            let total = tui.last_total_lines.get();
            if total > 0 {
                let safe_viewport = tui.viewport_height.max(1);
                let max_scroll = total.saturating_sub(safe_viewport);
                if max_scroll > 0 {
                    // Clamp offset to valid range (may be stale if messages changed)
                    let offset = tui.scroll_offset_line.min(max_scroll);
                    let pos_label = if offset == 0 {
                        "Top".to_string()
                    } else if offset >= max_scroll {
                        "Bot".to_string()
                    } else {
                        let current = offset + safe_viewport;
                        format!("{}/{}", current.min(total), total)
                    };
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        pos_label,
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }

        if tui.team_handler.event_rx.is_some() {
            spans.push(Span::raw(" "));
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame_idx = (anim_frame.progress_frame / 5) % frames.len();
            let active_agent = tui.team_panel.active_agent_name();
            if let Some(agent) = active_agent {
                let display_agent = if agent.len() > 15 {
                    format!("{}…", &agent[..agent.floor_char_boundary(14)])
                } else {
                    agent.clone()
                };
                spans.push(Span::styled(
                    format!(
                        "{}TEAM {}{}",
                        frames[frame_idx],
                        tui.team_panel.current_turn(),
                        display_agent
                    ),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(
                    format!(
                        "{}TEAM T:{}/{} Tr:{:.0}%",
                        frames[frame_idx],
                        tui.team_panel.current_turn(),
                        tui.team_panel.max_turns(),
                        tui.team_panel.trust_value() * 100.0,
                    ),
                    Style::default().fg(Color::Cyan),
                ));
            }
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
    }
}

/// Format response duration for status bar (Goose pattern).
///
/// Shows human-friendly timing: "<1s", "3.2s", "1m05s"
fn format_response_duration(dur: std::time::Duration) -> String {
    let secs = dur.as_secs();
    let ms = dur.as_millis();
    if ms < 1000 {
        format!("{}ms", ms)
    } else if secs < 60 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let mins = secs / 60;
        let remain = secs % 60;
        format!("{}m{:02}s", mins, remain)
    }
}
