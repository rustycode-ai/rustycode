//! Syntax highlighting support for `RustyCode`.
//!
//! Native builds use `syntect` for TextMate-style highlighting.
//! `WebAssembly` builds fall back to plain text so the browser target stays lean.

#[cfg(not(target_arch = "wasm32"))]
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

#[cfg(not(target_arch = "wasm32"))]
use std::sync::LazyLock;

#[cfg(not(target_arch = "wasm32"))]
use syntect::{
    easy::HighlightLines, highlighting::Theme, highlighting::ThemeSet, parsing::SyntaxSet,
};

#[cfg(not(target_arch = "wasm32"))]
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

#[cfg(not(target_arch = "wasm32"))]
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

#[cfg(not(target_arch = "wasm32"))]
pub struct SyntaxHighlighter {
    theme: &'static Theme,
}

#[cfg(target_arch = "wasm32")]
pub struct SyntaxHighlighter;

#[cfg(not(target_arch = "wasm32"))]
impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self::new_with_theme("base16-ocean.dark")
    }

    pub fn new_with_theme(theme_name: &str) -> Self {
        #[allow(clippy::expect_used)]
        let theme = THEME_SET
            .themes
            .get(theme_name)
            .or_else(|| THEME_SET.themes.get("base16-ocean.dark"))
            .or_else(|| THEME_SET.themes.values().next())
            .expect("ThemeSet::load_defaults() always contains at least one theme");

        Self { theme }
    }

    pub fn highlight(&self, code: &str, language: Option<&str>) -> Vec<Line<'static>> {
        let syntax = language
            .and_then(|lang| {
                SYNTAX_SET
                    .find_syntax_by_token(lang)
                    .or_else(|| SYNTAX_SET.find_syntax_by_extension(lang))
            })
            .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, self.theme);
        let mut lines = Vec::new();

        for (line_num, line) in code.lines().enumerate() {
            let ranges = highlighter
                .highlight_line(line, &SYNTAX_SET)
                .unwrap_or_default();

            let spans: Vec<Span<'static>> = ranges
                .into_iter()
                .map(|(style, text)| {
                    let r = style.foreground.r;
                    let g = style.foreground.g;
                    let b = style.foreground.b;
                    let fg = if r == 0 && g == 0 && b == 0 {
                        Color::Rgb(200, 200, 200)
                    } else {
                        Color::Rgb(r, g, b)
                    };

                    if (line_num < 3) && (text.contains('{') || text.contains('}')) {
                        tracing::debug!(
                            "Brace rendering: '{}' with color RGB({},{},{})",
                            text,
                            r,
                            g,
                            b
                        );
                    }

                    Span::styled(text.to_string(), Style::default().fg(fg))
                })
                .collect();

            lines.push(Line::from(spans));
        }

        lines
    }

    pub fn highlight_auto(&self, code: &str, file_hint: Option<&str>) -> Vec<Line<'static>> {
        let language = file_hint
            .and_then(Self::guess_language_from_file)
            .unwrap_or_else(|| Self::guess_language_from_content(code));

        self.highlight(code, Some(&language))
    }

    fn guess_language_from_file(filename: &str) -> Option<String> {
        let ext = std::path::Path::new(filename).extension()?.to_str()?;

        let language = match ext {
            "rs" => "rust",
            "py" => "python",
            "js" | "jsx" => "javascript",
            "ts" | "tsx" => "typescript",
            "go" => "go",
            "java" => "java",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" => "cpp",
            "cs" => "csharp",
            "php" => "php",
            "rb" => "ruby",
            "sh" => "Bash",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "json" => "json",
            "md" => "markdown",
            "sql" => "sql",
            "html" | "htm" => "html",
            "css" => "css",
            "scss" | "sass" => "scss",
            "xml" => "xml",
            "swift" => "swift",
            "kt" | "kts" => "kotlin",
            "scala" => "scala",
            "dart" => "dart",
            "lua" => "lua",
            "ps1" => "PowerShell",
            "dockerfile" => "dockerfile",
            "r" | "R" => "r",
            _ => return None,
        };
        Some(language.to_string())
    }

    fn guess_language_from_content(code: &str) -> String {
        if code.contains("fn ") && code.contains("impl ") {
            "rust".to_string()
        } else if code.contains("def ") && code.contains("import ") {
            "python".to_string()
        } else if code.contains("function ") || code.contains("const ") {
            "javascript".to_string()
        } else if code.contains("package ") && code.contains("func ") {
            "go".to_string()
        } else if code.contains("public class ") {
            "java".to_string()
        } else if code.contains("func ") && code.contains("var ") && code.contains(":=") {
            "go".to_string()
        } else if code.contains("class ") && code.contains(": ") && code.contains("def ") {
            "python".to_string()
        } else if code.contains("import ") && code.contains("@main") {
            "swift".to_string()
        } else if code.contains("fun ") && code.contains("val ") && code.contains("var ") {
            "kotlin".to_string()
        } else if code.contains("struct ") && code.contains("impl ") && code.contains("fn ") {
            "rust".to_string()
        } else {
            "plaintext".to_string()
        }
    }

    pub fn highlight_plain(&self, code: &str) -> Vec<Line<'static>> {
        code.lines()
            .map(|line| Line::from(vec![Span::raw(line.to_string())]))
            .collect()
    }
}

#[cfg(target_arch = "wasm32")]
impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self
    }

    pub fn new_with_theme(_theme_name: &str) -> Self {
        Self
    }

    pub fn highlight(&self, code: &str, _language: Option<&str>) -> Vec<Line<'static>> {
        self.highlight_plain(code)
    }

    pub fn highlight_auto(&self, code: &str, file_hint: Option<&str>) -> Vec<Line<'static>> {
        let _ = file_hint;
        self.highlight_plain(code)
    }

    pub fn highlight_plain(&self, code: &str) -> Vec<Line<'static>> {
        code.lines()
            .map(|line| Line::from(vec![Span::raw(line.to_string())]))
            .collect()
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_highlighter_new() {
        let highlighter = SyntaxHighlighter::new();
        let lines = highlighter.highlight("fn test() {}", Some("rust"));
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_guess_language_from_file() {
        assert_eq!(
            SyntaxHighlighter::guess_language_from_file("test.rs"),
            Some("rust".to_string())
        );
        assert_eq!(
            SyntaxHighlighter::guess_language_from_file("test.py"),
            Some("python".to_string())
        );
        assert_eq!(
            SyntaxHighlighter::guess_language_from_file("test.js"),
            Some("javascript".to_string())
        );
    }

    #[test]
    fn test_default_trait() {
        let h1 = SyntaxHighlighter::default();
        let h2 = SyntaxHighlighter::new();
        let lines1 = h1.highlight("fn main() {}", Some("rust"));
        let lines2 = h2.highlight("fn main() {}", Some("rust"));
        assert_eq!(lines1.len(), lines2.len());
    }

    #[test]
    fn test_new_with_valid_theme() {
        let highlighter = SyntaxHighlighter::new_with_theme("base16-ocean.dark");
        let lines = highlighter.highlight("fn main() {}", Some("rust"));
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_new_with_invalid_theme_falls_back() {
        let highlighter = SyntaxHighlighter::new_with_theme("nonexistent-theme");
        let lines = highlighter.highlight("fn main() {}", Some("rust"));
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_plain() {
        let highlighter = SyntaxHighlighter::new();
        let lines = highlighter.highlight_plain("line1\nline2");
        assert_eq!(lines.len(), 2);
    }
}
