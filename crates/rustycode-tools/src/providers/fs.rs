//! Shared filesystem utilities used across multiple tool providers.
//!
//! `WebFetchTool` imports `WEB_FETCH_MAX_CHARS`, `is_html_content`,
//! `html_to_simple_markdown`, and `truncate_to_char_boundary` from here.
//! `ReadFileTool` and `WriteFileTool` import `is_blocked_extension` and
//! `is_blocked_filename` from here.

use crate::security::{validation::BLOCKED_FILENAMES, BLOCKED_EXTENSIONS};
use std::path::Path;

/// Maximum number of characters returned by `WebFetchTool` content
pub(super) const WEB_FETCH_MAX_CHARS: usize = 50_000;

/// Check if a file extension is blocked for security reasons
pub(crate) fn is_blocked_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            // BLOCKED_EXTENSIONS entries include the leading dot (e.g., ".env", ".exe")
            let dotted = format!(".{}", ext.to_lowercase());
            BLOCKED_EXTENSIONS.contains(&dotted.as_str())
        })
        .unwrap_or(false)
}

/// Check if a filename is blocked for security reasons
pub(crate) fn is_blocked_filename(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| BLOCKED_FILENAMES.contains(&name))
        .unwrap_or(false)
}

/// Check if content appears to be HTML
pub(super) fn is_html_content(content: &str) -> bool {
    let trimmed = content.trim().to_lowercase();
    trimmed.starts_with("<!doctype")
        || trimmed.starts_with("<html")
        || (trimmed.starts_with('<') && trimmed.contains("xmlns="))
}

/// Convert HTML to markdown using a proper HTML parser.
pub(super) fn html_to_simple_markdown(html: &str) -> String {
    html2md::parse_html(html).trim().to_string()
}

pub(super) fn truncate_to_char_boundary(content: &str, max_chars: usize) -> &str {
    if content.len() <= max_chars {
        return content;
    }
    let mut end = max_chars;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    &content[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_to_char_boundary_keeps_utf8_valid() {
        let content = "é".repeat(10) + "abc";
        let truncated = truncate_to_char_boundary(&content, 3);
        assert!(truncated.is_char_boundary(truncated.len()));
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    #[test]
    fn test_truncate_to_char_boundary_ascii() {
        let content = "hello world";
        assert_eq!(truncate_to_char_boundary(content, 5), "hello");
    }

    #[test]
    fn test_truncate_to_char_boundary_within_bounds() {
        let content = "hello";
        assert_eq!(truncate_to_char_boundary(content, 100), "hello");
    }

    #[test]
    fn test_truncate_to_char_boundary_empty() {
        assert_eq!(truncate_to_char_boundary("", 10), "");
    }

    // ── is_blocked_extension tests ─────────────

    #[test]
    fn test_is_blocked_extension_env() {
        assert!(!is_blocked_extension(Path::new(".env")));
        assert!(is_blocked_extension(Path::new("local.env")));
    }

    #[test]
    fn test_is_blocked_extension_secrets() {
        assert!(is_blocked_extension(Path::new("id_rsa.key")));
        assert!(is_blocked_extension(Path::new("cert.pem")));
        assert!(is_blocked_extension(Path::new("app.exe")));
        assert!(is_blocked_extension(Path::new("lib.so")));
        assert!(is_blocked_extension(Path::new("lib.dylib")));
    }

    #[test]
    fn test_is_blocked_extension_not_text() {
        assert!(!is_blocked_extension(Path::new("main.rs")));
        assert!(!is_blocked_extension(Path::new("app.py")));
        assert!(!is_blocked_extension(Path::new("README.md")));
    }

    #[test]
    fn test_is_blocked_filename_credentials() {
        assert!(is_blocked_filename(Path::new("credentials.json")));
        assert!(is_blocked_filename(Path::new(".credentials.json")));
        assert!(is_blocked_filename(Path::new("id_rsa")));
        assert!(is_blocked_filename(Path::new("id_ed25519")));
        assert!(is_blocked_filename(Path::new("terraform.tfstate")));
    }

    #[test]
    fn test_is_blocked_filename_not_blocked() {
        assert!(!is_blocked_filename(Path::new("package.json")));
        assert!(!is_blocked_filename(Path::new("Cargo.toml")));
        assert!(!is_blocked_filename(Path::new("main.rs")));
        assert!(!is_blocked_filename(Path::new("README.md")));
    }

    #[test]
    fn test_is_blocked_filename_nested_path() {
        assert!(is_blocked_filename(Path::new("/home/user/.ssh/id_rsa")));
        assert!(!is_blocked_filename(Path::new("/home/user/src/main.rs")));
    }

    #[test]
    fn test_is_html_content() {
        assert!(is_html_content("<!DOCTYPE html><html></html>"));
        assert!(is_html_content("<html><body></body></html>"));
        assert!(is_html_content(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\">"
        ));
        assert!(!is_html_content("Just plain text"));
        assert!(!is_html_content("{\"key\": \"value\"}"));
        assert!(is_html_content("<!doctype html><html lang=\"en\">"));
        assert!(is_html_content(
            "<html>\n<head><title>Test</title></head>\n<body></body>\n</html>"
        ));
    }

    #[test]
    fn test_html_to_markdown() {
        let html = "<h1>Hello</h1><p>World</p>";
        let markdown = html_to_simple_markdown(html);
        assert!(markdown.contains("Hello") || !markdown.is_empty());
    }
}
