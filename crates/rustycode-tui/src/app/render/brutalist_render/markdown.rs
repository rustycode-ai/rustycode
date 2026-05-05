impl BrutalistRenderer<'_> {
    /// Render a single line with inline markdown styling.
    ///
    /// Handles: `code`, **bold**, *italic*, ~~strikethrough~~,
    /// [links](url), # headings, and > blockquotes.
    fn render_inline_markdown<'b>(&self, line: &'b str, colors: &ThemeColors) -> Vec<Span<'b>> {
        let mut spans = Vec::new();

        // Detect heading
        let heading_level = if line.starts_with("### ") {
            Some(3)
        } else if line.starts_with("## ") {
            Some(2)
        } else if line.starts_with("# ") {
            Some(1)
        } else {
            None
        };

        if let Some(level) = heading_level {
            let hashes = match level {
                1 => "# ",
                2 => "## ",
                3 => "### ",
                _ => "# ",
            };
            let content = &line[hashes.len()..];
            spans.push(Span::styled(
                hashes,
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD | Modifier::DIM),
            ));
            spans.push(Span::styled(
                Cow::Borrowed(content),
                Style::default()
                    .fg(colors.primary)
                    .add_modifier(Modifier::BOLD),
            ));
            return spans;
        }

        // Detect blockquote
        if let Some(rest) = line.strip_prefix("> ") {
            spans.push(Span::styled("▎ ", Style::default().fg(colors.muted)));
            spans.push(Span::styled(
                Cow::Borrowed(rest),
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::ITALIC),
            ));
            return spans;
        }

        // Detect unordered list items: "- item", "* item", "+ item"
        let trimmed_line = line.trim_start();
        if trimmed_line.starts_with("- ")
            || trimmed_line.starts_with("* ")
            || trimmed_line.starts_with("+ ")
        {
            let indent = line.len() - trimmed_line.len();
            let bullet_char = &trimmed_line[..1];
            let rest = &trimmed_line[2..];
            if indent > 0 {
                spans.push(Span::styled(
                    " ".repeat(indent),
                    Style::default().fg(colors.foreground),
                ));
            }
            spans.push(Span::styled(
                format!("{} ", bullet_char),
                Style::default().fg(colors.primary),
            ));
            // Parse inline markdown in the list item content
            let content_spans = Self::parse_inline_content(rest, colors);
            spans.extend(content_spans);
            return spans;
        }

        // Detect ordered list items: "1. item", "2. item", etc.
        if let Some(dot_pos) = trimmed_line.find(". ") {
            let prefix = &trimmed_line[..dot_pos];
            if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
                let indent = line.len() - trimmed_line.len();
                let rest = &trimmed_line[dot_pos + 2..];
                if indent > 0 {
                    spans.push(Span::styled(
                        " ".repeat(indent),
                        Style::default().fg(colors.foreground),
                    ));
                }
                spans.push(Span::styled(
                    format!("{}. ", prefix),
                    Style::default().fg(colors.primary),
                ));
                let content_spans = Self::parse_inline_content(rest, colors);
                spans.extend(content_spans);
                return spans;
            }
        }

        // Detect horizontal rule: --- or *** or ___
        let rule_trimmed = trimmed_line.trim();
        if (rule_trimmed.chars().all(|c| c == '-') && rule_trimmed.len() >= 3)
            || (rule_trimmed.chars().all(|c| c == '*') && rule_trimmed.len() >= 3)
            || (rule_trimmed.chars().all(|c| c == '_') && rule_trimmed.len() >= 3)
        {
            spans.push(Span::styled(
                "─".repeat(line.len().clamp(10, 60)),
                Style::default().fg(colors.muted),
            ));
            return spans;
        }

        // Inline parsing (delegates to shared method)
        Self::parse_inline_content(line, colors)
    }

    /// Parse inline markdown content: `code`, **bold**, *italic*, ~~strikethrough~~.
    /// Shared by render_inline_markdown (full lines) and list item content.
    ///
    /// Optimized: uses byte-based scanning instead of `Vec<char>` allocation,
    /// returns `Cow::Borrowed` spans to avoid string allocations, and has a
    /// fast path for lines with no markdown characters.
    fn parse_inline_content<'b>(line: &'b str, colors: &ThemeColors) -> Vec<Span<'b>> {
        let bytes = line.as_bytes();
        let len = bytes.len();

        // Fast path: no markdown characters — single borrowed span, zero allocation
        if !bytes.contains(&b'`')
            && !bytes.contains(&b'*')
            && !bytes.contains(&b'~')
            && !bytes.contains(&b'[')
        {
            return vec![Span::styled(
                Cow::Borrowed(line),
                Style::default().fg(colors.foreground),
            )];
        }

        let mut spans = Vec::new();
        let mut i = 0; // byte index

        while i < len {
            let b = bytes[i];

            // Inline code: `code` or ``code``
            if b == b'`' {
                let tick_count = count_consecutive(bytes, i, b'`');
                let search_start = i + tick_count;
                if search_start < len {
                    if let Some(close_pos) =
                        find_consecutive(&bytes[search_start..], b'`', tick_count)
                    {
                        let content_end = search_start + close_pos;
                        spans.push(Span::styled(
                            Cow::Borrowed(&line[search_start..content_end]),
                            Style::default().fg(Color::Rgb(180, 210, 170)),
                        ));
                        i = content_end + tick_count;
                        continue;
                    }
                }
                spans.push(Span::styled(
                    Cow::Borrowed(&line[i..i + 1]),
                    Style::default().fg(colors.foreground),
                ));
                i += 1;
                continue;
            }

            // Bold (**text**)
            if b == b'*' && i + 1 < len && bytes[i + 1] == b'*' {
                if let Some(end) = find_byte_pair(&bytes[i + 2..], b'*') {
                    spans.push(Span::styled(
                        Cow::Borrowed(&line[i + 2..i + 2 + end]),
                        Style::default()
                            .fg(colors.foreground)
                            .add_modifier(Modifier::BOLD),
                    ));
                    i = i + 2 + end + 2;
                    continue;
                }
            }

            // Italic (*text*)
            if b == b'*' && (i + 1 >= len || bytes[i + 1] != b'*') {
                if let Some(end) = find_byte(&bytes[i + 1..], b'*') {
                    spans.push(Span::styled(
                        Cow::Borrowed(&line[i + 1..i + 1 + end]),
                        Style::default()
                            .fg(colors.foreground)
                            .add_modifier(Modifier::ITALIC),
                    ));
                    i = i + 1 + end + 1;
                    continue;
                }
            }

            // Strikethrough (~~text~~)
            if b == b'~' && i + 1 < len && bytes[i + 1] == b'~' {
                if let Some(end) = find_byte_pair(&bytes[i + 2..], b'~') {
                    spans.push(Span::styled(
                        Cow::Borrowed(&line[i + 2..i + 2 + end]),
                        Style::default()
                            .fg(colors.muted)
                            .add_modifier(Modifier::CROSSED_OUT),
                    ));
                    i = i + 2 + end + 2;
                    continue;
                }
            }

            // Markdown links [text](url)
            if b == b'[' {
                if let Some(bracket_end) = find_byte(&bytes[i + 1..], b']') {
                    let url_start = i + 1 + bracket_end + 1;
                    if url_start < len && bytes[url_start] == b'(' {
                        if let Some(url_end) = find_byte(&bytes[url_start + 1..], b')') {
                            let link_text = &line[i + 1..i + 1 + bracket_end];
                            let url_text = &line[url_start + 1..url_start + 1 + url_end];
                            if !link_text.is_empty() {
                                spans.push(Span::styled(
                                    Cow::Borrowed(link_text),
                                    Style::default()
                                        .fg(colors.secondary)
                                        .add_modifier(Modifier::UNDERLINED),
                                ));
                            }
                            if !url_text.is_empty() && url_text.len() < 80 {
                                spans.push(Span::styled(
                                    format!("({})", url_text),
                                    Style::default()
                                        .fg(colors.muted)
                                        .add_modifier(Modifier::DIM),
                                ));
                            }
                            i = url_start + 1 + url_end + 1;
                            continue;
                        }
                    }
                }
            }

            // Plain text — accumulate until markdown character
            let start = i;
            i += 1;
            while i < len {
                let c = bytes[i];
                if c == b'`' || c == b'*' || c == b'~' || c == b'[' {
                    break;
                }
                i += 1;
            }
            spans.push(Span::styled(
                Cow::Borrowed(&line[start..i]),
                Style::default().fg(colors.foreground),
            ));
        }

        spans
    }

}
