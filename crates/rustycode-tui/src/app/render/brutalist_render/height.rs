impl BrutalistRenderer<'_> {
    /// Estimate message height for scrolling
    pub fn estimate_message_height(&self, message: &Message, width: usize) -> usize {
        let pipe_display_width =
            unicode_width::UnicodeWidthChar::width('▌').unwrap_or(1);
        let content_prefix_width = pipe_display_width + 1;
        let wrap_rows = |display_width: usize, prefix_width: usize| -> usize {
            (display_width + prefix_width).div_ceil(width.max(1)).max(1)
        };

        // System messages: compact for single-line, multi-line block for diffs/stats
        if message.role == MessageRole::System {
            let content = message.content.trim();
            if content.is_empty() {
                return 0;
            }
            // Diff content: one line per diff line
            if content.starts_with("diff --git") {
                let line_count = content.lines().count();
                return line_count.min(50) + if line_count > 50 { 1 } else { 0 };
            }
            // Multi-line content: header + content lines
            let line_count = content.lines().count();
            if line_count > 1 {
                let capped = line_count.min(50);
                return 1 + capped + if line_count > 50 { 1 } else { 0 }; // header + lines + overflow indicator
            }
            // Single-line notice
            return 1;
        }

        let role_height = 1;

        // Collapsed messages: role header + first line + "N more" indicator + tools + separator
        if message.collapsed {
            let content_line_count = message.content.lines().count();
            let mut height = role_height; // role header
            if content_line_count > 0 {
                height += 1; // first line preview
                if content_line_count > 1 {
                    height += 1; // "N more lines" indicator
                }
            }
            // Tool summary line
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    height += 1; // tool summary
                }
            }
            height += 1; // separator
            return height;
        }

        // Calculate content height accounting for code block line limits
        // and markdown table rendering.
        let content_lines = if message.content.is_empty() {
            0
        } else {
            let content_lines_vec: Vec<&str> = message.content.lines().collect();
            let mut in_code = false;
            let mut code_lines: usize = 0;
            let mut in_table = false;
            let mut total: usize = 0;
            let mut line_idx = 0;

            while line_idx < content_lines_vec.len() {
                let line = content_lines_vec[line_idx];
                let trimmed = line.trim();

                // Code block fences
                if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                    in_table = false;
                    if in_code {
                        let hidden = code_lines.saturating_sub(MAX_CODE_BLOCK_LINES);
                        if hidden > 0 {
                            total += 1;
                        } // "... N more lines"
                        in_code = false;
                        code_lines = 0;
                        total += 1; // closing fence
                    } else {
                        in_code = true;
                        code_lines = 0;
                        total += 1; // opening fence
                    }
                    line_idx += 1;
                    continue;
                }

                if in_code {
                    code_lines += 1;
                    if code_lines <= MAX_CODE_BLOCK_LINES {
                        total += wrap_rows(line.width(), 8);
                    }
                    line_idx += 1;
                    continue;
                }

                // Markdown table detection: match the renderer's table logic
                // Tables render as: header row + skip separator + data rows + border
                if trimmed.starts_with('|')
                    && trimmed.ends_with('|')
                    && trimmed.contains('|')
                    && !in_table
                {
                    let is_separator = is_table_separator_row(trimmed);

                    if !is_separator && line_idx + 1 < content_lines_vec.len() {
                        let next_trimmed = content_lines_vec[line_idx + 1].trim();
                        let next_is_sep =
                            next_trimmed.starts_with('|')
                                && next_trimmed.ends_with('|')
                                && is_table_separator_row(next_trimmed);

                        if next_is_sep {
                            // Start of table: header row (1) + skip separator
                            total += 1; // header row
                            line_idx += 2; // skip header + separator

                            // Count data rows
                            while line_idx < content_lines_vec.len() {
                                let row_line = content_lines_vec[line_idx].trim();
                                if !row_line.starts_with('|') || !row_line.ends_with('|') {
                                    break;
                                }
                                total += 1; // data row
                                line_idx += 1;
                            }
                            total += 1; // border line after table
                            in_table = false;
                            continue;
                        }
                    }

                    // Standalone separator row (part of a table not caught above)
                    if is_separator {
                        line_idx += 1;
                        continue; // separators are not rendered
                    }
                }

                // Regular content line
                in_table = false;
                let line_width = line.width();
                total += wrap_rows(line_width, content_prefix_width);
                line_idx += 1;
            }

            if in_code {
                let hidden = code_lines.saturating_sub(MAX_CODE_BLOCK_LINES);
                if hidden > 0 {
                    total += 1;
                } // "... N more lines"
                total += 1; // "╰ (unclosed)" close indicator
            }
            total
        };

        let mut height = role_height + content_lines;

        // Tool-only messages get "(running tools)" indicator
        if message.content.trim().is_empty() {
            if let Some(tools) = &message.tool_executions {
                if !tools.is_empty() {
                    height += 1;
                }
            }
        }

        // Add tools
        if let Some(tools) = &message.tool_executions {
            if tools.len() > 1 {
                height += 1; // Summary line (╶ N tools: X passed ... ╴)
            }
            height += tools.len(); // One line per tool
                                   // Error preview lines for failed tools
            let failed_with_output = tools
                .iter()
                .filter(|t| {
                    t.status == ToolStatus::Failed
                        && (t
                            .detailed_output
                            .as_ref()
                            .is_some_and(|o| !o.trim().is_empty())
                            || !t.result_summary.is_empty())
                })
                .count();
            height += failed_with_output;

            // Output preview lines for running tools
            let running_with_output = tools
                .iter()
                .filter(|t| {
                    t.status == ToolStatus::Running
                        && (t
                            .detailed_output
                            .as_ref()
                            .is_some_and(|o| !o.trim().is_empty())
                            || !t.result_summary.is_empty())
                })
                .count();
            height += running_with_output;

            if message.tools_expansion == ExpansionLevel::Expanded {
                for tool in tools {
                    // Add header lines for input/output
                    if tool.input_json.is_some() {
                        height += 1; // input header
                        height += 15; // JSON content (max)
                    }
                    if let Some(output) = &tool.detailed_output {
                        height += 1; // output header
                        let out_lines = output.lines().count();
                        if out_lines <= 10 {
                            height += out_lines;
                        } else {
                            // Head/tail truncation: head + hidden + tail = max_lines + 1
                            height += 11; // 10 lines + 1 hidden indicator
                        }
                    }
                }
            }
        }

        // Add thinking (capped at 20 display lines when expanded, single indicator when collapsed)
        if let Some(thinking) = &message.thinking {
            if !thinking.is_empty() {
                if message.thinking_expansion == ExpansionLevel::Expanded {
                    let total_think_lines = thinking.lines().count();
                    height += 1;
                    for think_line in thinking.lines().take(20) {
                        height += wrap_rows(think_line.width(), 4);
                    }
                    if total_think_lines > 20 {
                        height += 1; // hidden indicator line
                    }
                } else {
                    height += 1; // collapsed indicator line
                }
            }
        }

        // Turn summary footer
        if message.role == MessageRole::Assistant {
            let has_tools = message
                .tool_executions
                .as_ref()
                .is_some_and(|t| !t.is_empty());
            if has_tools {
                height += 1; // Tool summary line
            } else if message.content.lines().count() > 3 {
                height += 1; // Text-only summary line (word count + lines)
            }
        }

        // Add separator
        height += 1;

        height
    }
}

#[cfg(test)]
mod height_tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn estimate_message_height_accounts_for_code_block_prefix_width() {
        let renderer =
            crate::app::render::brutalist_renderer::BrutalistRendererBuilder::new(&[], "").build();
        let message =
            crate::ui::message::Message::assistant(format!("```rust\n{}\n```", "a".repeat(40)));

        let height = renderer.estimate_message_height(&message, 20);

        assert_eq!(
            height, 7,
            "code block wrapping should include the visible gutter and line numbers"
        );
    }
}
