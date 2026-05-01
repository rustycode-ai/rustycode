//! Code diff rendering utilities
//!
//! Provides multiple diff rendering modes: unified, side-by-side, and hunk-based
//! using the similar crate for diff computation.

// Complete implementation - used by brutalist renderer for /diff command output

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use similar::{Algorithm, TextDiff};

/// Heuristically detect raw git diff / patch content.
///
/// This keeps diff output readable even when it arrives without a fenced
/// `diff` code block.
pub fn looks_like_git_diff(diff_text: &str) -> bool {
    let trimmed = diff_text.trim_start();
    if trimmed.starts_with("diff --git")
        || trimmed.starts_with("@@ ")
        || trimmed.starts_with("--- ")
        || trimmed.starts_with("+++ ")
    {
        return true;
    }

    trimmed.contains("\n--- ")
        && trimmed.contains("\n+++ ")
        && (trimmed.contains("\n@@ ") || trimmed.contains("\nindex "))
}

/// Code diff renderer
pub struct DiffRenderer {
    _priv: (),
}

impl DiffRenderer {
    /// Create a new diff renderer
    pub fn new() -> Self {
        Self { _priv: () }
    }

    /// Render a unified diff
    pub fn render_unified_diff<'a>(
        &self,
        old: &'a str,
        new: &'a str,
        file_path: &'a str,
    ) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        // Add header
        lines.push(Line::from(vec![Span::styled(
            format!(" diff: {} ", file_path),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]));

        // Fast path for very large inputs
        const LARGE_DIFF_LINE_THRESHOLD: usize = 4000;
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();

        if old_lines.len() + new_lines.len() >= LARGE_DIFF_LINE_THRESHOLD {
            return self.render_large_diff_fast(old_lines, new_lines, lines);
        }

        // Full diff using similar crate
        let diff = TextDiff::configure()
            .algorithm(Algorithm::Patience)
            .diff_lines(old, new);

        let mut old_line_num = 1;
        let mut new_line_num = 1;

        for change in diff.iter_all_changes() {
            let (sign, line_num_str, style) = match change.tag() {
                similar::ChangeTag::Delete => (
                    "-",
                    format!("{:4}", old_line_num),
                    Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
                ),
                similar::ChangeTag::Insert => (
                    "+",
                    format!("{:4}", new_line_num),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                similar::ChangeTag::Equal => (
                    " ",
                    format!("{:4}", old_line_num),
                    Style::default().fg(Color::DarkGray),
                ),
            };

            lines.push(Line::from(vec![
                Span::styled(line_num_str, style),
                Span::styled(" ", Style::default()),
                Span::styled(sign, style),
                Span::styled(" ", Style::default()),
                Span::styled(change.value().to_string(), style),
            ]));

            match change.tag() {
                similar::ChangeTag::Delete => old_line_num += 1,
                similar::ChangeTag::Insert => new_line_num += 1,
                similar::ChangeTag::Equal => {
                    old_line_num += 1;
                    new_line_num += 1;
                }
            }
        }

        lines
    }

    /// Render side-by-side diff
    pub fn render_side_by_side(&self, old: &str, new: &str) -> Vec<Line<'_>> {
        let diff = TextDiff::configure()
            .algorithm(Algorithm::Patience)
            .diff_lines(old, new);

        let mut lines = Vec::new();

        // Header
        lines.push(Line::from(vec![
            Span::styled(" OLD ", Style::default().fg(Color::Black).bg(Color::Red)),
            Span::raw("  "),
            Span::styled(" NEW ", Style::default().fg(Color::Black).bg(Color::Green)),
        ]));

        // Separator
        lines.push(Line::from(vec![
            Span::styled("─".repeat(40), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled("─".repeat(40), Style::default().fg(Color::DarkGray)),
        ]));

        for change in diff.iter_all_changes() {
            match change.tag() {
                similar::ChangeTag::Delete => {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("─ {}", change.value()),
                            Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
                        ),
                        Span::raw("  ".repeat(42)),
                    ]));
                }
                similar::ChangeTag::Insert => {
                    lines.push(Line::from(vec![
                        Span::raw("  ".repeat(42)),
                        Span::styled(
                            format!("+ {}", change.value()),
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
                similar::ChangeTag::Equal => {
                    let val = change.value().to_string();
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {}", val),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::raw("  "),
                        Span::styled(
                            format!("  {}", val),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }
            }
        }

        lines
    }

    /// Render hunk-based diff (like git diff)
    pub fn render_hunk_diff(&self, old: &str, new: &str, file_path: &str) -> Vec<Line<'_>> {
        let diff = TextDiff::configure()
            .algorithm(Algorithm::Patience)
            .diff_lines(old, new);

        let mut lines = Vec::new();

        // File header (like git diff)
        lines.push(Line::from(vec![Span::styled(
            format!("diff --git a/{} b/{}", file_path, file_path),
            Style::default().fg(Color::Cyan),
        )]));

        lines.push(Line::from(vec![Span::styled(
            format!("--- a/{}", file_path),
            Style::default().fg(Color::Red),
        )]));

        lines.push(Line::from(vec![Span::styled(
            format!("+++ b/{}", file_path),
            Style::default().fg(Color::Green),
        )]));

        // Count changes
        let mut deletions = 0;
        let mut insertions = 0;
        let mut total_changes = 0;

        for change in diff.iter_all_changes() {
            match change.tag() {
                similar::ChangeTag::Delete => deletions += 1,
                similar::ChangeTag::Insert => insertions += 1,
                similar::ChangeTag::Equal => {}
            }
            match change.tag() {
                similar::ChangeTag::Delete | similar::ChangeTag::Insert => total_changes += 1,
                similar::ChangeTag::Equal => {}
            }
        }

        lines.push(Line::from(vec![Span::styled(
            format!(
                "changed: {} line(s), {} deletion(s), {} insertion(s)",
                total_changes, deletions, insertions
            ),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )]));

        // Content with line numbers
        let mut old_line_num = 1;

        for change in diff.iter_all_changes() {
            match change.tag() {
                similar::ChangeTag::Delete => {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{:4}", old_line_num),
                            Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
                        ),
                        Span::styled(
                            format!(" -{}", change.value()),
                            Style::default().fg(Color::Red).add_modifier(Modifier::DIM),
                        ),
                    ]));
                    old_line_num += 1;
                }
                similar::ChangeTag::Insert => {
                    lines.push(Line::from(vec![
                        Span::raw("    ".to_string()),
                        Span::styled(
                            format!(" +{}", change.value()),
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
                similar::ChangeTag::Equal => {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{:4}", old_line_num),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            format!("  {}", change.value()),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                    old_line_num += 1;
                }
            }
        }

        lines
    }

    /// Render large diff with optimized fast path
    fn render_large_diff_fast<'a>(
        &self,
        old_lines: Vec<&'a str>,
        new_lines: Vec<&'a str>,
        mut lines: Vec<Line<'a>>,
    ) -> Vec<Line<'a>> {
        let mut old_line_num = 1usize;
        let mut new_line_num = 1usize;

        let min_len = old_lines.len().min(new_lines.len());
        let mut prefix = 0usize;

        // Find common prefix
        while prefix < min_len && old_lines[prefix] == new_lines[prefix] {
            prefix += 1;
        }

        // Find common suffix
        let mut suffix = 0usize;
        while suffix < (min_len - prefix)
            && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
        {
            suffix += 1;
        }

        // Common prefix
        for line in &old_lines[..prefix] {
            let style = Style::default().fg(Color::DarkGray);
            lines.push(Line::from(vec![
                Span::styled(format!("{:4}", old_line_num), style),
                Span::styled(" ", Style::default()),
                Span::styled(" ", style),
                Span::styled(" ", Style::default()),
                Span::styled((*line).to_string(), style),
            ]));
            old_line_num += 1;
            new_line_num += 1;
        }

        let old_mid_end = old_lines.len() - suffix;
        let new_mid_end = new_lines.len() - suffix;
        let overlap = (old_mid_end - prefix).min(new_mid_end - prefix);

        // Changed section
        for i in 0..overlap {
            let old_style = Style::default().fg(Color::Red).add_modifier(Modifier::DIM);
            let new_style = Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD);

            lines.push(Line::from(vec![
                Span::styled(format!("{:4}", old_line_num), old_style),
                Span::styled(" ", Style::default()),
                Span::styled("-", old_style),
                Span::styled(" ", Style::default()),
                Span::styled(old_lines[prefix + i].to_string(), old_style),
            ]));
            old_line_num += 1;

            lines.push(Line::from(vec![
                Span::styled(format!("{:4}", new_line_num), new_style),
                Span::styled(" ", Style::default()),
                Span::styled("+", new_style),
                Span::styled(" ", Style::default()),
                Span::styled(new_lines[prefix + i].to_string(), new_style),
            ]));
            new_line_num += 1;
        }

        // Old-only section
        for line in &old_lines[(prefix + overlap)..old_mid_end] {
            let style = Style::default().fg(Color::Red).add_modifier(Modifier::DIM);
            lines.push(Line::from(vec![
                Span::styled(format!("{:4}", old_line_num), style),
                Span::styled(" ", Style::default()),
                Span::styled("-", style),
                Span::styled(" ", Style::default()),
                Span::styled((*line).to_string(), style),
            ]));
            old_line_num += 1;
        }

        // New-only section
        for line in &new_lines[(prefix + overlap)..new_mid_end] {
            let style = Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD);
            lines.push(Line::from(vec![
                Span::styled(format!("{:4}", new_line_num), style),
                Span::styled(" ", Style::default()),
                Span::styled("+", style),
                Span::styled(" ", Style::default()),
                Span::styled((*line).to_string(), style),
            ]));
            new_line_num += 1;
        }

        // Common suffix
        for line in &old_lines[old_mid_end..] {
            let style = Style::default().fg(Color::DarkGray);
            lines.push(Line::from(vec![
                Span::styled(format!("{:4}", old_line_num), style),
                Span::styled(" ", Style::default()),
                Span::styled(" ", style),
                Span::styled(" ", Style::default()),
                Span::styled((*line).to_string(), style),
            ]));
            old_line_num += 1;
        }

        lines
    }
}

impl Default for DiffRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Render git diff with proper coloring
pub fn render_diff(diff_text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for line in diff_text.lines() {
        if line.starts_with("diff --git") {
            // Diff header
            lines.push(Line::from(vec![Span::styled(
                line.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]));
        } else if line.starts_with("index ") {
            // Hash line
            lines.push(Line::from(vec![Span::styled(
                line.to_string(),
                Style::default().fg(Color::DarkGray),
            )]));
        } else if line.starts_with("--- ") || line.starts_with("+++ ") {
            // File paths
            lines.push(Line::from(vec![Span::styled(
                line.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));
        } else if line.starts_with("@@ ") {
            // Hunk header
            lines.push(Line::from(vec![Span::styled(
                line.to_string(),
                Style::default().fg(Color::Magenta),
            )]));
        } else if let Some(rest) = line.strip_prefix('+') {
            // Added lines (green)
            let content = rest.to_string();
            lines.push(Line::from(vec![
                Span::styled("+".to_string(), Style::default().fg(Color::Green)),
                Span::styled(content, Style::default().fg(Color::Green)),
            ]));
        } else if let Some(rest) = line.strip_prefix('-') {
            // Removed lines (red)
            let content = rest.to_string();
            lines.push(Line::from(vec![
                Span::styled("-".to_string(), Style::default().fg(Color::Red)),
                Span::styled(content, Style::default().fg(Color::Red)),
            ]));
        } else {
            // Context lines
            lines.push(Line::from(vec![Span::styled(
                line.to_string(),
                Style::default().fg(Color::White),
            )]));
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_renderer_new() {
        let renderer = DiffRenderer::new();
        // Just verify it creates
        assert_eq!(renderer._priv, ());
    }

    #[test]
    fn test_looks_like_git_diff() {
        assert!(looks_like_git_diff(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@"
        ));
        assert!(looks_like_git_diff("--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@"));
        assert!(!looks_like_git_diff("plain markdown text"));
    }

    #[test]
    fn test_render_unified_diff_basic() {
        let renderer = DiffRenderer::new();
        let old = "line1\nline2";
        let new = "line1\nmodified";
        let lines = renderer.render_unified_diff(old, new, "test.txt");

        assert!(!lines.is_empty());
        assert!(lines[0].to_string().contains("diff:"));
    }

    #[test]
    fn test_render_side_by_side() {
        let renderer = DiffRenderer::new();
        let old = "old";
        let new = "new";
        let lines = renderer.render_side_by_side(old, new);

        // Header + separator + delete line + insert line = 4
        assert!(lines.len() >= 3);
        assert!(lines[0].to_string().contains("OLD"));
        assert!(lines[0].to_string().contains("NEW"));
    }

    #[test]
    fn test_render_hunk_diff() {
        let renderer = DiffRenderer::new();
        let old = "a\nb\nc";
        let new = "a\nmodified\nc";
        let lines = renderer.render_hunk_diff(old, new, "test.txt");

        assert!(!lines.is_empty());
        assert!(lines[0].to_string().contains("diff --git"));
    }

    #[test]
    fn test_render_diff_git_format() {
        let diff = "diff --git a/test.txt b/test.txt\n--- a/test.txt\n+++ b/test.txt\n@@ -1 +1 @@\n-old\n+new";
        let lines = render_diff(diff);

        assert!(lines.len() > 2);
        assert!(lines[0].to_string().contains("diff --git"));
        assert!(lines[1].to_string().contains("---"));
        assert!(lines[2].to_string().contains("+++"));
    }

    #[test]
    fn test_render_diff_context_lines() {
        let diff = " context line\n--- a/file\n+++ b/file\n context line";
        let lines = render_diff(diff);

        // Should handle context lines (not starting with + or -)
        let context_lines: Vec<_> = lines
            .iter()
            .filter(|l| {
                let s = l.to_string();
                !s.starts_with('+')
                    && !s.starts_with('-')
                    && !s.contains("---")
                    && !s.contains("+++")
            })
            .collect();

        assert!(!context_lines.is_empty());
    }
}
