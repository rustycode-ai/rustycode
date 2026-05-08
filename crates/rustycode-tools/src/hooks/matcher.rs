//! Tool name matcher for hook filtering
//!
//! Supports patterns from Claude Code / Codex hook configs:
//! - `"Edit|Write|Bash"` — pipe-separated tool names
//! - `"mcp__filesystem__.*"` — regex patterns
//! - `"*"` or `""` — match all tools
//! - `"Bash"` — single tool name (case-insensitive)

use anyhow::Result;
use regex::Regex;
use std::sync::OnceLock;

/// Matches tool names against a pattern from hook config.
#[derive(Debug, Clone)]
pub struct ToolMatcher {
    /// Compiled regex for the pattern. `None` means match everything.
    regex: Option<Regex>,
    /// The original pattern string.
    pattern: String,
}

/// Regex that matches pipe-separated bare names like `Edit|Write|Bash`.
/// Matches sequences of word chars separated by pipes with no regex metacharacters.
static BARE_NAME_RE: OnceLock<Regex> = OnceLock::new();

fn bare_name_regex() -> &'static Regex {
    BARE_NAME_RE.get_or_init(|| Regex::new(r"^[A-Za-z0-9_|]+$").unwrap())
}

impl ToolMatcher {
    /// Parse a matcher pattern from hook config.
    ///
    /// Supports:
    /// - `"Edit|Write|Bash"` — exact tool names (case-insensitive)
    /// - `"mcp__fs__.*"` — full regex
    /// - `"*"` or `""` — match all tools
    pub fn new(pattern: &str) -> Result<Self> {
        let trimmed = pattern.trim();

        // Empty or "*" matches everything
        if trimmed.is_empty() || trimmed == "*" {
            return Ok(Self {
                regex: None,
                pattern: trimmed.to_string(),
            });
        }

        // If the pattern is just pipe-separated bare names (e.g., "Edit|Write|Bash"),
        // convert to case-insensitive alternation regex.
        let regex = if bare_name_regex().is_match(trimmed) {
            let alternatives: Vec<&str> = trimmed.split('|').collect();
            // Build a case-insensitive alternation: (?i)^(edit|write|bash)$
            let joined = alternatives
                .iter()
                .map(|s| regex::escape(s.trim()))
                .collect::<Vec<_>>()
                .join("|");
            Regex::new(&format!("(?i)^({joined})$"))?
        } else {
            // Treat as a full regex pattern
            Regex::new(trimmed)?
        };

        Ok(Self {
            regex: Some(regex),
            pattern: trimmed.to_string(),
        })
    }

    /// Create a matcher that matches everything.
    pub fn match_all() -> Self {
        Self {
            regex: None,
            pattern: "*".to_string(),
        }
    }

    /// Check if a tool name matches this pattern.
    pub fn matches(&self, tool_name: &str) -> bool {
        match &self.regex {
            None => true,
            Some(re) => re.is_match(tool_name),
        }
    }

    /// The original pattern string.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_all_with_star() {
        let m = ToolMatcher::new("*").unwrap();
        assert!(m.matches("Edit"));
        assert!(m.matches("bash"));
        assert!(m.matches("mcp__fs__read"));
    }

    #[test]
    fn matches_all_with_empty() {
        let m = ToolMatcher::new("").unwrap();
        assert!(m.matches("Edit"));
        assert!(m.matches("anything"));
    }

    #[test]
    fn matches_single_tool_case_insensitive() {
        let m = ToolMatcher::new("Bash").unwrap();
        assert!(m.matches("Bash"));
        assert!(m.matches("bash"));
        assert!(m.matches("BASH"));
        assert!(!m.matches("Edit"));
    }

    #[test]
    fn matches_pipe_separated_tools() {
        let m = ToolMatcher::new("Edit|Write|Bash").unwrap();
        assert!(m.matches("Edit"));
        assert!(m.matches("Write"));
        assert!(m.matches("Bash"));
        assert!(m.matches("edit"));
        assert!(m.matches("write"));
        assert!(m.matches("bash"));
        assert!(!m.matches("Read"));
        assert!(!m.matches("Grep"));
    }

    #[test]
    fn matches_regex_pattern() {
        let m = ToolMatcher::new("mcp__filesystem__.*").unwrap();
        assert!(m.matches("mcp__filesystem__read"));
        assert!(m.matches("mcp__filesystem__write"));
        assert!(!m.matches("mcp__git__status"));
    }

    #[test]
    fn match_all_static() {
        let m = ToolMatcher::match_all();
        assert!(m.matches("anything"));
        assert!(m.matches("Edit"));
    }

    #[test]
    fn pattern_preserved() {
        let m = ToolMatcher::new("Edit|Write").unwrap();
        assert_eq!(m.pattern(), "Edit|Write");
    }

    #[test]
    fn invalid_regex_returns_error() {
        assert!(ToolMatcher::new("[invalid").is_err());
    }

    #[test]
    fn matches_with_whitespace() {
        let m = ToolMatcher::new("  Edit|Write  ").unwrap();
        assert!(m.matches("Edit"));
        assert!(m.matches("Write"));
    }

    #[test]
    fn pipe_separated_with_underscores() {
        let m = ToolMatcher::new("read_file|write_file").unwrap();
        assert!(m.matches("read_file"));
        assert!(m.matches("WRITE_FILE"));
        assert!(!m.matches("edit_file"));
    }
}
