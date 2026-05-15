//! Pre-loaded source context zone for the session context block.
//!
//! Provides a [`ContextZone`] implementation that holds pre-loaded file snippets,
//! allowing the agent to start with relevant source code already in context
//! rather than wasting turns discovering and reading files.

use super::context_block::ContextZone;

/// A pre-loaded source file snippet.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSnippet {
    /// Relative path from workspace root.
    pub path: String,
    /// File content (possibly truncated).
    pub content: String,
    /// Total lines in the file.
    pub total_lines: usize,
    /// Lines shown in the snippet.
    pub shown_lines: usize,
}

/// A [`ContextZone`] holding pre-loaded source file snippets.
///
/// Renders as a markdown section with file headers, suitable for inclusion
/// in the agent's initial context. The zone never reports stale (pre-loaded
/// content doesn't change at runtime).
pub struct SourceContextZone {
    snippets: Vec<FileSnippet>,
    cached_render: Option<String>,
}

impl SourceContextZone {
    /// Create an empty source context zone.
    pub fn empty() -> Self {
        Self {
            snippets: Vec::new(),
            cached_render: None,
        }
    }

    /// Create a source context zone with the given snippets.
    pub fn new(snippets: Vec<FileSnippet>) -> Self {
        Self {
            snippets,
            cached_render: None,
        }
    }

    /// Returns true if there are no snippets.
    pub fn is_empty(&self) -> bool {
        self.snippets.is_empty()
    }

    /// Returns the number of snippets.
    pub fn len(&self) -> usize {
        self.snippets.len()
    }

    fn render_internal(&self) -> String {
        if self.snippets.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        parts.push(format!(
            "## Pre-loaded Source Files ({} files)\n",
            self.snippets.len()
        ));
        parts.push(
            "These files are ALREADY loaded — do NOT waste turns re-reading them. \
             Start by tracing the code path through these files.\n"
                .to_string(),
        );

        for snippet in &self.snippets {
            parts.push(format!(
                "### {} ({} lines)\n```\n{}\n```",
                snippet.path, snippet.total_lines, snippet.content
            ));
        }

        parts.join("\n\n")
    }
}

impl ContextZone for SourceContextZone {
    fn render(&self) -> String {
        self.cached_render
            .clone()
            .unwrap_or_else(|| self.render_internal())
    }

    fn is_stale(&self) -> bool {
        false
    }

    fn estimated_tokens(&self) -> usize {
        rustycode_protocol::estimate_tokens(&self.render())
    }

    fn name(&self) -> &str {
        "source-context"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snippet(path: &str, content: &str, total: usize, shown: usize) -> FileSnippet {
        FileSnippet {
            path: path.to_string(),
            content: content.to_string(),
            total_lines: total,
            shown_lines: shown,
        }
    }

    #[test]
    fn empty_zone_renders_empty_string() {
        let zone = SourceContextZone::empty();
        assert!(zone.render().is_empty());
        assert!(zone.is_empty());
        assert_eq!(zone.len(), 0);
    }

    #[test]
    fn zone_with_snippets_renders_markdown() {
        let zone = SourceContextZone::new(vec![
            sample_snippet("src/auth.rs", "fn login() {}", 10, 10),
            sample_snippet("src/middleware.rs", "fn check() {}", 20, 15),
        ]);

        let rendered = zone.render();
        assert!(rendered.contains("Pre-loaded Source Files (2 files)"));
        assert!(rendered.contains("### src/auth.rs (10 lines)"));
        assert!(rendered.contains("fn login() {}"));
        assert!(rendered.contains("### src/middleware.rs (20 lines)"));
        assert!(rendered.contains("fn check() {}"));
        assert!(rendered.contains("ALREADY loaded"));
        assert!(!zone.is_empty());
        assert_eq!(zone.len(), 2);
    }

    #[test]
    fn zone_name_is_source_context() {
        let zone = SourceContextZone::empty();
        assert_eq!(zone.name(), "source-context");
    }

    #[test]
    fn zone_never_stale() {
        let zone =
            SourceContextZone::new(vec![sample_snippet("src/main.rs", "fn main() {}", 1, 1)]);
        assert!(!zone.is_stale());
    }

    #[test]
    fn token_estimate_is_positive_for_content() {
        let zone = SourceContextZone::new(vec![sample_snippet(
            "src/lib.rs",
            "pub fn hello() -> &str { \"hello\" }",
            5,
            5,
        )]);
        assert!(zone.estimated_tokens() > 0);
    }

    #[test]
    fn token_estimate_zero_for_empty() {
        let zone = SourceContextZone::empty();
        assert_eq!(zone.estimated_tokens(), 0);
    }

    #[test]
    fn snippet_equality() {
        let a = sample_snippet("a.rs", "fn a()", 1, 1);
        let b = sample_snippet("a.rs", "fn a()", 1, 1);
        let c = sample_snippet("b.rs", "fn b()", 1, 1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
