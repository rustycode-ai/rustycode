//! Orchestra Summary Distiller — Context Optimization for Summaries
//!
//! Extracts essential structured data from SUMMARY.md files:
//! * Frontmatter parsing (provides, requires, `key_files`, etc.)
//! * Title and one-liner extraction
//! * Progressive field dropping for budget constraints
//! * Token savings calculation
//!
//! Critical for context optimization in autonomous systems.

use regex::Regex;

// SAFETY: These regex patterns are valid compile-time constants
#[allow(clippy::unwrap_used)]
static SCALAR_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^(\w[\w_]*):\s*(.+)$").unwrap());
// SAFETY: This regex pattern is a valid compile-time constant
#[allow(clippy::unwrap_used)]
static ARRAY_START_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^(\w[\w_]*):\s*(\[\])?\s*$").unwrap());
// SAFETY: This regex pattern is a valid compile-time constant
#[allow(clippy::unwrap_used)]
static ITEM_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^\s+-\s+(.+)$").unwrap());

// ─── Types ────────────────────────────────────────────────────────────────────

/// Result of distillation operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DistillationResult {
    /// Distilled content
    pub content: String,

    /// Number of summaries processed
    pub summary_count: usize,

    /// Percentage of content saved
    pub savings_percent: usize,

    /// Original character count
    pub original_chars: usize,

    /// Distilled character count
    pub distilled_chars: usize,
}

/// Parsed frontmatter from SUMMARY.md
#[derive(Debug, Clone, Default)]
struct ParsedFrontmatter {
    id: String,
    provides: Vec<String>,
    requires: Vec<String>,
    key_files: Vec<String>,
    key_decisions: Vec<String>,
    patterns_established: Vec<String>,
}

/// Distilled summary entry
#[derive(Debug, Clone)]
struct DistilledEntry {
    id: String,
    one_liner: String,
    provides: Vec<String>,
    requires: Vec<String>,
    key_files: Vec<String>,
    key_decisions: Vec<String>,
    patterns: Vec<String>,
}

// ─── Frontmatter Parsing ───────────────────────────────────────────────────────

/// Parse YAML frontmatter from SUMMARY.md
#[allow(clippy::unwrap_used)]
fn parse_frontmatter(raw: &str) -> ParsedFrontmatter {
    let mut result = ParsedFrontmatter::default();

    // Extract frontmatter block between --- markers
    let fm_match = Regex::new(r"^---\r?\n([\s\S]*?)\r?\n---").unwrap();
    let fm_block = match fm_match.captures(raw) {
        Some(caps) => caps.get(1).map_or("", |m| m.as_str()),
        None => return result,
    };

    let lines: Vec<&str> = fm_block.lines().collect();
    let mut current_key: Option<String> = None;

    for line in lines {
        // Scalar value: key: value
        if let Some(caps) = SCALAR_RE.captures(line) {
            let key = caps.get(1).map_or("", |m| m.as_str());
            let value = caps.get(2).map_or("", |m| m.as_str());
            current_key = Some(key.to_string());
            set_scalar(&mut result, key, value.trim());
            continue;
        }

        // Array-start key with empty value: key:\n  or key: []\n
        if let Some(caps) = ARRAY_START_RE.captures(line) {
            current_key = caps.get(1).map(|m| m.as_str()).map(ToString::to_string);
            continue;
        }

        // Array item:   - value
        if let Some(caps) = ITEM_RE.captures(line) {
            if let Some(key) = &current_key {
                let value = caps.get(1).map_or("", |m| m.as_str());
                push_item(&mut result, key, value.trim());
            }
        }
    }

    result
}

fn set_scalar(fm: &mut ParsedFrontmatter, key: &str, value: &str) {
    if key == "id" {
        fm.id = value.to_string();
    }
}

fn push_item(fm: &mut ParsedFrontmatter, key: &str, value: &str) {
    match key {
        "provides" => fm.provides.push(value.to_string()),
        "requires" => fm.requires.push(value.to_string()),
        "key_files" => fm.key_files.push(value.to_string()),
        "key_decisions" => fm.key_decisions.push(value.to_string()),
        "patterns_established" => fm.patterns_established.push(value.to_string()),
        _ => {}
    }
}

// ─── Body Parsing ─────────────────────────────────────────────────────────────

/// Extract title and one-liner from body content
#[allow(clippy::unwrap_used)]
fn extract_title_and_one_liner(body: &str) -> (String, String) {
    let mut title_id = String::new();
    let mut one_liner = String::new();
    let mut found_title = false;

    let title_re = Regex::new(r"^#\s+(\S+):\s*(.*)$").unwrap();

    for line in body.lines() {
        if let Some(caps) = title_re.captures(line) {
            if !found_title {
                title_id = caps.get(1).map_or("", |m| m.as_str()).to_string();
                // If the title line itself has text after "S01: ", use that as a fallback
                if let Some(title_text) = caps.get(2) {
                    let text = title_text.as_str().trim();
                    if !text.is_empty() {
                        one_liner = text.to_string();
                    }
                }
                found_title = true;
                continue;
            }
        }

        // First non-empty line after the title is the one-liner
        if found_title && one_liner.is_empty() && !line.trim().is_empty() && !line.starts_with('#')
        {
            one_liner = line.trim().to_string();
            break;
        }
    }

    (title_id, one_liner)
}

/// Get body content after frontmatter
#[allow(clippy::unwrap_used)]
fn get_body_after_frontmatter(raw: &str) -> String {
    let fm_re = Regex::new(r"^---\r?\n[\s\S]*?\r?\n---\r?\n?").unwrap();
    fm_re.captures(raw).map_or_else(
        || raw.to_string(),
        |caps| {
            let match_end = caps.get(0).map_or(0, |m| m.end());
            raw[match_end..].to_string()
        },
    )
}

// ─── Formatting ───────────────────────────────────────────────────────────────

/// Format an entry with progressive field dropping
fn format_entry_with_drop_level(entry: &DistilledEntry, drop_level: usize) -> String {
    let mut lines = Vec::new();
    lines.push(format!("## {}: {}", entry.id, entry.one_liner));

    if !entry.provides.is_empty() {
        lines.push(format!("provides: {}", entry.provides.join(", ")));
    }
    if !entry.requires.is_empty() {
        lines.push(format!("requires: {}", entry.requires.join(", ")));
    }
    if drop_level < 3 && !entry.key_files.is_empty() {
        lines.push(format!("key_files: {}", entry.key_files.join(", ")));
    }
    if drop_level < 2 && !entry.key_decisions.is_empty() {
        lines.push(format!("key_decisions: {}", entry.key_decisions.join(", ")));
    }
    if drop_level < 1 && !entry.patterns.is_empty() {
        lines.push(format!("patterns: {}", entry.patterns.join(", ")));
    }

    lines.join("\n")
}

/// Format a distilled entry
fn format_entry(entry: &DistilledEntry) -> String {
    format_entry_with_drop_level(entry, 0)
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Distill a single SUMMARY.md content into compact structured block
///
/// # Arguments
/// * `summary` - Raw SUMMARY.md content
///
/// # Returns
/// Distilled structured content
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestration::summary_distiller::*;
///
/// let summary = r#"
/// ---
/// id: T01
/// provides: [auth]
/// requires: [user_model]
/// ---
/// # T01: Implement authentication
///
/// Added login and logout functionality.
/// "#;
///
/// let distilled = distill_single(summary);
/// assert!(distilled.contains("T01"));
/// ```
pub fn distill_single(summary: &str) -> String {
    let fm = parse_frontmatter(summary);
    let body = get_body_after_frontmatter(summary);
    let (title_id, one_liner) = extract_title_and_one_liner(&body);

    let id = if !fm.id.is_empty() {
        fm.id
    } else if !title_id.is_empty() {
        title_id
    } else {
        "???".to_string()
    };

    format_entry(&DistilledEntry {
        id,
        one_liner,
        provides: fm.provides,
        requires: fm.requires,
        key_files: fm.key_files,
        key_decisions: fm.key_decisions,
        patterns: fm.patterns_established,
    })
}

/// Distill multiple SUMMARY.md contents into budget-constrained output
///
/// # Arguments
/// * `summaries` - Vector of raw SUMMARY.md contents
/// * `budget_chars` - Maximum character budget
///
/// # Returns
/// Distillation result with content and metadata
///
/// # Example
/// ```rust,no_run
/// use rustycode_orchestration::summary_distiller::*;
///
/// let summaries = vec![
///     "id: T01\n---\n# Task 1".to_string(),
///     "id: T02\n---\n# Task 2".to_string(),
/// ];
///
/// let result = distill_summaries(&summaries, 1000);
/// assert!(result.savings_percent > 0);
/// ```
pub fn distill_summaries(summaries: &[String], budget_chars: usize) -> DistillationResult {
    let original_chars: usize = summaries.iter().map(String::len).sum();

    if summaries.is_empty() {
        return DistillationResult {
            content: String::new(),
            summary_count: 0,
            savings_percent: 0,
            original_chars: 0,
            distilled_chars: 0,
        };
    }

    // Parse all entries up front
    let entries: Vec<DistilledEntry> = summaries
        .iter()
        .map(|summary| {
            let fm = parse_frontmatter(summary);
            let body = get_body_after_frontmatter(summary);
            let (title_id, one_liner) = extract_title_and_one_liner(&body);

            let id = if !fm.id.is_empty() {
                fm.id
            } else if !title_id.is_empty() {
                title_id
            } else {
                "???".to_string()
            };

            DistilledEntry {
                id,
                one_liner,
                provides: fm.provides,
                requires: fm.requires,
                key_files: fm.key_files,
                key_decisions: fm.key_decisions,
                patterns: fm.patterns_established,
            }
        })
        .collect();

    // Try progressively more aggressive dropping until it fits
    for drop_level in 0..=3 {
        let blocks: Vec<String> = entries
            .iter()
            .map(|e| format_entry_with_drop_level(e, drop_level))
            .collect();
        let content = blocks.join("\n\n");

        if content.len() <= budget_chars {
            let distilled_chars = content.len();
            return DistillationResult {
                content,
                summary_count: summaries.len(),
                savings_percent: if original_chars > 0 {
                    ((1.0 - (distilled_chars as f64 / original_chars as f64)) * 100.0).round()
                        as usize
                } else {
                    0
                },
                original_chars,
                distilled_chars,
            };
        }
    }

    // Even at max drop level it doesn't fit — truncate
    let blocks: Vec<String> = entries
        .iter()
        .map(|e| format_entry_with_drop_level(e, 3))
        .collect();
    let mut content = blocks.join("\n\n");

    if content.len() > budget_chars {
        let truncate_at = budget_chars.saturating_sub(15);
        let truncate_at = content.floor_char_boundary(truncate_at);
        content = format!("{}\n[...truncated]", &content[..truncate_at]);
    }

    let distilled_chars = content.len();
    DistillationResult {
        content,
        summary_count: summaries.len(),
        savings_percent: if original_chars > 0 {
            ((1.0 - (distilled_chars as f64 / original_chars as f64)) * 100.0).round() as usize
        } else {
            0
        },
        original_chars,
        distilled_chars,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_empty() {
        let result = parse_frontmatter("no frontmatter here");
        assert_eq!(result.id, "");
        assert!(result.provides.is_empty());
    }

    #[test]
    fn test_parse_frontmatter_basic() {
        let input = r"---
id: T01
provides:
  - auth
  - login
requires:
  - user_model
---
";
        let result = parse_frontmatter(input);
        assert_eq!(result.id, "T01");
        assert_eq!(result.provides, vec!["auth", "login"]);
        assert_eq!(result.requires, vec!["user_model"]);
    }

    #[test]
    fn test_extract_title_and_one_liner() {
        let input = r"# T01: Implement authentication

Added login and logout functionality with JWT tokens.
";
        let (id, one_liner) = extract_title_and_one_liner(input);
        assert_eq!(id, "T01");
        assert_eq!(one_liner, "Implement authentication");
    }

    #[test]
    fn test_extract_title_fallback() {
        let input = r"# T01: This is the one-liner

Some additional content.
";
        let (id, one_liner) = extract_title_and_one_liner(input);
        assert_eq!(id, "T01");
        assert_eq!(one_liner, "This is the one-liner");
    }

    #[test]
    fn test_get_body_after_frontmatter() {
        let input = r"---
id: T01
---
# Body content
";
        let body = get_body_after_frontmatter(input);
        assert!(!body.contains("---"));
        assert!(body.contains("# Body content"));
    }

    #[test]
    fn test_distill_single() {
        let input = r"---
id: T01
provides:
  - auth
requires:
  - user_model
key_files:
  - src/auth.rs
---
# T01: Implement authentication

Added login and logout functionality.
";
        let distilled = distill_single(input);
        assert!(distilled.contains("## T01"));
        assert!(distilled.contains("provides: auth"));
        assert!(distilled.contains("requires: user_model"));
        assert!(distilled.contains("key_files: src/auth.rs"));
    }

    #[test]
    fn test_distill_summaries_empty() {
        let result = distill_summaries(&[], 1000);
        assert_eq!(result.summary_count, 0);
        assert_eq!(result.savings_percent, 0);
    }

    #[test]
    fn test_distill_summaries_basic() {
        let summaries = vec![
            r"---
id: T01
provides:
  - auth
---
# T01: Task one
Content here.
"
            .to_string(),
            r"---
id: T02
requires:
  - user_model
---
# T02: Task two
More content.
"
            .to_string(),
        ];

        let result = distill_summaries(&summaries, 1000);
        assert_eq!(result.summary_count, 2);
        assert!(result.savings_percent > 0);
        assert!(result.content.contains("## T01"));
        assert!(result.content.contains("## T02"));
    }

    #[test]
    fn test_distill_summaries_budget_constraint() {
        let summaries = vec![
            r"---
id: T01
provides: auth
requires: user_model
key_files: src/auth.rs
key_decisions: use_jwt
patterns: singleton
---
# T01: Task one
Content here.
"
            .to_string(),
            r"---
id: T02
provides: api
requires: http
key_files: src/api.rs
key_decisions: restful
patterns: factory
---
# T02: Task two
More content.
"
            .to_string(),
        ];

        // Very tight budget - should drop fields
        let result = distill_summaries(&summaries, 200);
        assert_eq!(result.summary_count, 2);
        assert!(result.distilled_chars <= 210); // Allow small margin for truncation marker
    }

    #[test]
    fn test_format_entry_drop_level_0() {
        let entry = DistilledEntry {
            id: "T01".to_string(),
            one_liner: "Task one".to_string(),
            provides: vec!["auth".to_string()],
            requires: vec!["user_model".to_string()],
            key_files: vec!["src/auth.rs".to_string()],
            key_decisions: vec!["use_jwt".to_string()],
            patterns: vec!["singleton".to_string()],
        };

        let formatted = format_entry_with_drop_level(&entry, 0);
        assert!(formatted.contains("provides:"));
        assert!(formatted.contains("requires:"));
        assert!(formatted.contains("key_files:"));
        assert!(formatted.contains("key_decisions:"));
        assert!(formatted.contains("patterns:"));
    }

    #[test]
    fn test_format_entry_drop_level_1() {
        let entry = DistilledEntry {
            id: "T01".to_string(),
            one_liner: "Task one".to_string(),
            provides: vec!["auth".to_string()],
            requires: vec!["user_model".to_string()],
            key_files: vec!["src/auth.rs".to_string()],
            key_decisions: vec!["use_jwt".to_string()],
            patterns: vec!["singleton".to_string()],
        };

        let formatted = format_entry_with_drop_level(&entry, 1);
        assert!(formatted.contains("provides:"));
        assert!(formatted.contains("requires:"));
        assert!(formatted.contains("key_files:"));
        assert!(formatted.contains("key_decisions:"));
        assert!(!formatted.contains("patterns:"));
    }

    #[test]
    fn test_format_entry_drop_level_2() {
        let entry = DistilledEntry {
            id: "T01".to_string(),
            one_liner: "Task one".to_string(),
            provides: vec!["auth".to_string()],
            requires: vec!["user_model".to_string()],
            key_files: vec!["src/auth.rs".to_string()],
            key_decisions: vec!["use_jwt".to_string()],
            patterns: vec!["singleton".to_string()],
        };

        let formatted = format_entry_with_drop_level(&entry, 2);
        assert!(formatted.contains("provides:"));
        assert!(formatted.contains("requires:"));
        assert!(formatted.contains("key_files:"));
        assert!(!formatted.contains("key_decisions:"));
        assert!(!formatted.contains("patterns:"));
    }

    #[test]
    fn test_format_entry_drop_level_3() {
        let entry = DistilledEntry {
            id: "T01".to_string(),
            one_liner: "Task one".to_string(),
            provides: vec!["auth".to_string()],
            requires: vec!["user_model".to_string()],
            key_files: vec!["src/auth.rs".to_string()],
            key_decisions: vec!["use_jwt".to_string()],
            patterns: vec!["singleton".to_string()],
        };

        let formatted = format_entry_with_drop_level(&entry, 3);
        assert!(formatted.contains("provides:"));
        assert!(formatted.contains("requires:"));
        assert!(!formatted.contains("key_files:"));
        assert!(!formatted.contains("key_decisions:"));
        assert!(!formatted.contains("patterns:"));
    }
}
