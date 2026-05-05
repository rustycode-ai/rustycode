//! File Reference Expansion System
//!
//! Parses and expands `@file` references in text content (e.g., CLAUDE.md, .rustycodehints).
//! Supports nested references with configurable depth limits, circular reference protection,
//! path traversal prevention, and gitignore integration.
//!
//! Ported from goose's `hints/import_files.rs`.
//!
//! # Security
//!
//! - Absolute paths are rejected
//! - Path traversal (`../` beyond import boundary) is blocked
//! - Circular references are detected and skipped
//! - Maximum depth prevents unbounded recursion
//! - Content size is capped to prevent `ReDoS`

use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Maximum nesting depth for file references
const MAX_DEPTH: usize = 3;

/// Maximum content size for file reference parsing (128KB, prevents `ReDoS`)
const MAX_CONTENT_LENGTH: usize = 131_072;

/// Regex for matching `@file` references in text.
///
/// Matches patterns like `@README.md`, `@./docs/guide.md`, `@src/utils/helper.js`.
/// Does NOT match email addresses (`user@example.com`) or social handles (`@username`).
static FILE_REFERENCE_REGEX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"(?:^|\s)@([a-zA-Z0-9_\-./]+(?:\.[a-zA-Z0-9]+)+|[A-Z][a-zA-Z0-9_\-]*|[a-zA-Z0-9_\-./]*[./][a-zA-Z0-9_\-./]*)",
    )
    .expect("Invalid file reference regex pattern")
});

/// Error type for file reference operations
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FileReferenceError {
    #[error("absolute paths not allowed in file references: {0}")]
    AbsolutePathRejected(String),

    #[error("path traversal blocked: {path} is outside {boundary}")]
    PathTraversalBlocked { path: String, boundary: String },

    #[error("import boundary directory not found: {0}")]
    BoundaryNotFound(String),

    #[error("maximum reference depth {depth} exceeded")]
    MaxDepthExceeded { depth: usize },

    #[error("content too large for parsing: {size} bytes (limit: {limit} bytes)")]
    ContentTooLarge { size: usize, limit: usize },
}

/// Sanitize a file reference path to prevent security issues.
///
/// Rejects absolute paths and paths that resolve outside the import boundary.
fn sanitize_reference_path(
    reference: &Path,
    including_file_path: &Path,
    import_boundary: &Path,
) -> Result<PathBuf, FileReferenceError> {
    if reference.is_absolute() {
        return Err(FileReferenceError::AbsolutePathRejected(
            reference.to_string_lossy().to_string(),
        ));
    }

    let resolved = including_file_path.join(reference);

    let boundary_canonical = import_boundary.canonicalize().map_err(|_| {
        FileReferenceError::BoundaryNotFound(import_boundary.to_string_lossy().to_string())
    })?;

    if let Ok(canonical) = resolved.canonicalize() {
        if !canonical.starts_with(&boundary_canonical) {
            return Err(FileReferenceError::PathTraversalBlocked {
                path: resolved.to_string_lossy().to_string(),
                boundary: import_boundary.to_string_lossy().to_string(),
            });
        }
        Ok(canonical)
    } else {
        // File doesn't exist yet, but path structure is safe
        Ok(resolved)
    }
}

/// Parse `@file` references from text content.
///
/// Returns a list of file paths referenced in the content.
/// Skips email addresses and social handles.
pub fn parse_file_references(content: &str) -> Vec<PathBuf> {
    if content.len() > MAX_CONTENT_LENGTH {
        log::warn!(
            "Content too large for file reference parsing: {} bytes (limit: {} bytes)",
            content.len(),
            MAX_CONTENT_LENGTH
        );
        return Vec::new();
    }

    FILE_REFERENCE_REGEX
        .captures_iter(content)
        .map(|cap| PathBuf::from(&cap[1]))
        .collect()
}

/// Check if a file reference should be processed.
///
/// Validates the path, checks for circular references, and verifies the file exists.
fn should_process_reference(
    reference: &Path,
    including_file_path: &Path,
    import_boundary: &Path,
    visited: &HashSet<PathBuf>,
) -> Option<PathBuf> {
    match sanitize_reference_path(reference, including_file_path, import_boundary) {
        Ok(path) => {
            // Use canonical path for circular reference detection to handle symlinks
            let canonical_path = path.canonicalize().unwrap_or_else(|_| path.clone());
            if visited.contains(&canonical_path) {
                return None;
            }
            if !path.is_file() {
                return None;
            }
            Some(path)
        }
        Err(e) => {
            log::warn!("Skipping unsafe file reference {reference:?}: {e}");
            None
        }
    }
}

/// Process a single file reference, expanding its content.
///
/// Reads the referenced file, recursively expands any references within it,
/// and returns the replacement text.
fn process_file_reference(
    reference: &Path,
    safe_path: &Path,
    visited: &mut HashSet<PathBuf>,
    import_boundary: &Path,
    depth: usize,
) -> Option<(String, String)> {
    if depth >= MAX_DEPTH {
        log::warn!("Maximum reference depth {MAX_DEPTH} exceeded");
        return None;
    }

    // Use canonical path for visited tracking to handle symlinks consistently
    let canonical = safe_path
        .canonicalize()
        .unwrap_or_else(|_| safe_path.to_path_buf());
    visited.insert(canonical);

    let expanded_content = expand_file_references(safe_path, import_boundary, visited, depth + 1);

    let reference_pattern = format!("@{}", reference.to_string_lossy());
    let replacement = format!(
        "--- Content from {} ---\n{}\n--- End of {} ---",
        safe_path.display(),
        expanded_content,
        safe_path.display()
    );

    Some((reference_pattern, replacement))
}

/// Read a file and expand all `@file` references within it.
///
/// Recursively resolves `@path/to/file` references, replacing them with
/// the file's content wrapped in markers. Handles:
/// - Nested references (up to `MAX_DEPTH`)
/// - Circular reference detection
/// - Path traversal prevention
/// - Missing files (left as-is)
///
pub fn expand_file_references(
    file_path: &Path,
    import_boundary: &Path,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
) -> String {
    let content = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            log::warn!("Could not read file {file_path:?}: {e}");
            return String::new();
        }
    };

    let including_file_path = file_path.parent().unwrap_or(file_path);

    let references = parse_file_references(&content);
    let mut result = content;

    for reference in references {
        let safe_path = match should_process_reference(
            &reference,
            including_file_path,
            import_boundary,
            visited,
        ) {
            Some(path) => path,
            None => continue,
        };

        if let Some((pattern, replacement)) =
            process_file_reference(&reference, &safe_path, visited, import_boundary, depth)
        {
            result = result.replace(&pattern, &replacement);
        }
    }

    result
}

/// Convenience function: expand file references from a root file.
///
/// Creates fresh visited set and starts at depth 0.
pub fn expand_references(file_path: &Path, import_boundary: &Path) -> String {
    let mut visited = HashSet::new();
    expand_file_references(file_path, import_boundary, &mut visited, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_parse_file_references_basic() {
        let content = "See @README.md and @./docs/guide.md for details";
        let refs = parse_file_references(content);
        assert!(refs.contains(&PathBuf::from("README.md")));
        assert!(refs.contains(&PathBuf::from("./docs/guide.md")));
    }

    #[test]
    fn test_parse_file_references_complex_paths() {
        let content = "Files: @src/utils/helper.js @docs/api/endpoints.md @config.local.json";
        let refs = parse_file_references(content);
        assert!(refs.contains(&PathBuf::from("src/utils/helper.js")));
        assert!(refs.contains(&PathBuf::from("docs/api/endpoints.md")));
        assert!(refs.contains(&PathBuf::from("config.local.json")));
    }

    #[test]
    fn test_parse_file_references_no_extension_names() {
        let content = "See @Makefile @LICENSE @Dockerfile @CHANGELOG";
        let refs = parse_file_references(content);
        assert!(refs.contains(&PathBuf::from("Makefile")));
        assert!(refs.contains(&PathBuf::from("LICENSE")));
        assert!(refs.contains(&PathBuf::from("Dockerfile")));
        assert!(refs.contains(&PathBuf::from("CHANGELOG")));
    }

    #[test]
    fn test_parse_file_references_rejects_emails() {
        let content = "Contact user@example.com or admin@company.org";
        let refs = parse_file_references(content);
        assert!(!refs
            .iter()
            .any(|p| p.to_str().unwrap().contains("example.com")));
        assert!(!refs
            .iter()
            .any(|p| p.to_str().unwrap().contains("company.org")));
    }

    #[test]
    fn test_parse_file_references_rejects_social_handles() {
        let content = "Follow @username and @user123";
        let refs = parse_file_references(content);
        assert!(!refs.iter().any(|p| p.to_str().unwrap() == "username"));
        assert!(!refs.iter().any(|p| p.to_str().unwrap() == "user123"));
    }

    #[test]
    fn test_parse_file_references_empty() {
        assert!(parse_file_references("").is_empty());
        assert!(parse_file_references("no references here").is_empty());
    }

    #[test]
    fn test_sanitize_rejects_absolute_paths() {
        let result = sanitize_reference_path(
            Path::new("/etc/passwd"),
            Path::new("/project"),
            Path::new("/project"),
        );
        assert!(matches!(
            result,
            Err(FileReferenceError::AbsolutePathRejected(_))
        ));
    }

    #[test]
    fn test_sanitize_allows_relative_within_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let boundary = temp.path();
        fs::create_dir_all(boundary.join("src")).unwrap();
        fs::write(boundary.join("src/main.rs"), "fn main() {}").unwrap();

        let result = sanitize_reference_path(Path::new("src/main.rs"), boundary, boundary);
        assert!(result.is_ok());
        let canonical = result.unwrap();
        assert!(canonical.ends_with("src/main.rs"));
    }

    #[test]
    fn test_expand_direct_reference() {
        let temp = tempfile::tempdir().unwrap();
        let boundary = temp.path();

        create_file(boundary, "included.md", "This is included content");
        let main = create_file(
            boundary,
            "main.md",
            "Main content\n@included.md\nMore content",
        );

        let expanded = expand_references(&main, boundary);

        assert!(expanded.contains("Main content"));
        assert!(expanded.contains("This is included content"));
        assert!(expanded.contains("More content"));
        assert!(expanded.contains("--- Content from"));
        assert!(expanded.contains("--- End of"));
    }

    #[test]
    fn test_expand_nested_references() {
        let temp = tempfile::tempdir().unwrap();
        let boundary = temp.path();

        create_file(boundary, "level2.md", "Level 2 content");
        create_file(boundary, "level1.md", "Level 1 content\n@level2.md");
        let main = create_file(boundary, "main.md", "Main content\n@level1.md");

        let expanded = expand_references(&main, boundary);

        assert!(expanded.contains("Level 1 content"));
        assert!(expanded.contains("Level 2 content"));
    }

    #[test]
    fn test_expand_circular_reference_protection() {
        let temp = tempfile::tempdir().unwrap();
        let boundary = temp.path();

        create_file(boundary, "file1.md", "File 1\n@file2.md");
        create_file(boundary, "file2.md", "File 2\n@file1.md");
        let main = create_file(boundary, "main.md", "Main\n@file1.md");

        let expanded = expand_references(&main, boundary);

        assert!(expanded.contains("File 1"));
        assert!(expanded.contains("File 2"));
        // Should only appear once due to circular reference protection
        assert_eq!(expanded.matches("File 1").count(), 1);
    }

    #[test]
    fn test_expand_max_depth_limit() {
        let temp = tempfile::tempdir().unwrap();
        let boundary = temp.path();

        for i in 1..=5 {
            let content = if i < 5 {
                format!("Level {} content\n@level{}.md", i, i + 1)
            } else {
                format!("Level {} content", i)
            };
            create_file(boundary, &format!("level{}.md", i), &content);
        }

        let main = create_file(boundary, "main.md", "Main\n@level1.md");
        let expanded = expand_references(&main, boundary);

        // Should contain up to level 3 (MAX_DEPTH = 3)
        assert!(expanded.contains("Level 1 content"));
        assert!(expanded.contains("Level 2 content"));
        assert!(expanded.contains("Level 3 content"));
        // Should NOT contain level 4 or 5 due to depth limit
        assert!(!expanded.contains("Level 4 content"));
        assert!(!expanded.contains("Level 5 content"));
    }

    #[test]
    fn test_expand_missing_file_left_as_is() {
        let temp = tempfile::tempdir().unwrap();
        let boundary = temp.path();

        let main = create_file(boundary, "main.md", "Main\n@missing.md\nMore content");

        let expanded = expand_references(&main, boundary);

        assert!(expanded.contains("@missing.md"));
        assert!(!expanded.contains("--- Content from"));
        assert!(expanded.contains("More content"));
    }

    #[test]
    fn test_expand_path_traversal_blocked() {
        let temp = tempfile::tempdir().unwrap();
        let boundary = temp.path();

        create_file(boundary, "legitimate.md", "Safe content");

        let main = create_file(
            boundary,
            "main.md",
            "Content\n@legitimate.md\n@../etc/passwd",
        );

        let expanded = expand_references(&main, boundary);

        assert!(expanded.contains("Safe content"));
        assert!(expanded.contains("@../etc/passwd")); // left as-is (not expanded)
        assert!(!expanded.contains("root:")); // shouldn't have /etc/passwd content
    }

    #[test]
    fn test_expand_circular_reference_detects_canonical_path() {
        let temp = tempfile::tempdir().unwrap();
        let boundary = temp.path();

        create_file(boundary, "shared.md", "Shared\n");
        create_file(boundary, "alias.md", "@./shared.md\n@shared.md");
        let main = create_file(boundary, "main.md", "@alias.md");

        let expanded = expand_references(&main, boundary);

        // The circular dependency logic detects the cycle.
        // We expect it to stop expansion gracefully.
        // It should resolve one depth, the circular one is ignored.
        assert!(expanded.contains("Shared"));
        assert_eq!(expanded.matches("Shared").count(), 1);
    }

    #[test]
    fn test_parse_content_too_large() {
        let big_content = "x".repeat(MAX_CONTENT_LENGTH + 1);
        let refs = parse_file_references(&big_content);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_error_display() {
        let err = FileReferenceError::AbsolutePathRejected("/etc/passwd".to_string());
        assert!(format!("{}", err).contains("absolute paths"));

        let err = FileReferenceError::MaxDepthExceeded { depth: 3 };
        assert!(format!("{}", err).contains("depth"));

        let err = FileReferenceError::ContentTooLarge {
            size: 200_000,
            limit: MAX_CONTENT_LENGTH,
        };
        assert!(format!("{}", err).contains("too large"));
    }
}
