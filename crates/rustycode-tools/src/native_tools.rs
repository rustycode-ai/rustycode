//! Native tool implementations for cross-platform support.
//!
//! Replaces shell-dependent operations (`ls`, `grep`, `cat`) with native
//! Rust implementations to avoid dependencies on host environment binaries.

use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Native `cat` implementation
pub fn native_cat(path: &Path) -> Result<String> {
    fs::read_to_string(path).context(format!("Failed to read file: {path:?}"))
}

/// Native `ls` implementation
pub fn native_ls(path: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(path)
        .max_depth(1)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        files.push(entry.path().to_string_lossy().into_owned());
    }
    Ok(files)
}

/// Native `grep` implementation
pub fn native_grep(path: &Path, pattern: &str) -> Result<String> {
    let re = Regex::new(pattern).context("Invalid regex pattern")?;
    let content = native_cat(path)?;
    let matches: Vec<String> = content
        .lines()
        .filter(|line| re.is_match(line))
        .map(ToString::to_string)
        .collect();

    Ok(matches.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_native_cat_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();
        let content = native_cat(&file_path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_native_cat_missing_file() {
        let result = native_cat(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_native_cat_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("empty.txt");
        fs::write(&file_path, "").unwrap();
        let content = native_cat(&file_path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_native_ls_lists_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        fs::write(dir.path().join("b.txt"), "").unwrap();

        let files = native_ls(dir.path()).unwrap();
        // Should contain at least the directory itself plus the two files
        assert!(files.len() >= 3);
        assert!(files.iter().any(|f| f.contains("a.txt")));
        assert!(files.iter().any(|f| f.contains("b.txt")));
    }

    #[test]
    fn test_native_ls_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let files = native_ls(dir.path()).unwrap();
        // Just the directory itself
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_native_ls_nonexistent_directory() {
        let result = native_ls(Path::new("/nonexistent/dir"));
        // WalkDir returns an error entry, which filter_map drops,
        // resulting in an empty vec (not an error)
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_native_grep_finds_matching_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("data.txt");
        fs::write(&file_path, "foo bar\nbaz foo\nqux").unwrap();

        let matches = native_grep(&file_path, "foo").unwrap();
        assert_eq!(matches, "foo bar\nbaz foo");
    }

    #[test]
    fn test_native_grep_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("data.txt");
        fs::write(&file_path, "hello\nworld").unwrap();

        let matches = native_grep(&file_path, "xyz").unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_native_grep_invalid_regex() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("data.txt");
        fs::write(&file_path, "test").unwrap();

        let result = native_grep(&file_path, "[invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_native_grep_missing_file() {
        let result = native_grep(Path::new("/nonexistent.txt"), "pattern");
        assert!(result.is_err());
    }

    #[test]
    fn test_native_grep_regex_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("data.txt");
        fs::write(&file_path, "price: $10.00\nprice: $20.00\ntotal").unwrap();

        let matches = native_grep(&file_path, r"\$\d+\.\d{2}").unwrap();
        assert_eq!(matches.lines().count(), 2);
    }
}
