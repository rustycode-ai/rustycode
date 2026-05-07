//! Project-scoped memory directory layout.
//!
//! Lives alongside existing memory artifacts in `.rustycode/projects/{id}/memory/`:
//!
//! ```text
//! .rustycode/projects/{id}/memory/
//! ├── memory.yaml            # existing: confidence-scored entries
//! ├── MEMORY.md              # existing + generated: searchable index
//! ├── memory_summary.md      # compact summary → injected into system prompt
//! ├── rollout_summaries/     # per-session recaps
//! ├── topics/                # existing: topic files
//! └── extensions/ad_hoc/notes/  # user-requested memory updates
//! ```

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Compact summary file injected into the system prompt (5000 token limit).
pub const MEMORY_SUMMARY_FILE: &str = "memory_summary.md";

/// Per-session rollout summary directory.
pub const ROLLOUT_SUMMARIES_DIR: &str = "rollout_summaries";

/// Extensions directory for ad-hoc user memory updates.
pub const EXTENSIONS_DIR: &str = "extensions";

/// Ad-hoc notes subdirectory.
pub const AD_HOC_NOTES_DIR: &str = "ad_hoc/notes";

/// Maximum tokens for memory_summary.md content.
pub const SUMMARY_TOKEN_LIMIT: usize = 5_000;

/// Rough chars-per-token estimate for truncation (conservative).
const CHARS_PER_TOKEN: usize = 4;

// --- Path helpers ---

/// Path to `memory_summary.md`.
pub fn memory_summary_path(mem_dir: &Path) -> PathBuf {
    mem_dir.join(MEMORY_SUMMARY_FILE)
}

/// Path to `rollout_summaries/`.
pub fn rollout_summaries_dir(mem_dir: &Path) -> PathBuf {
    mem_dir.join(ROLLOUT_SUMMARIES_DIR)
}

/// Path to `extensions/ad_hoc/notes/`.
pub fn ad_hoc_notes_dir(mem_dir: &Path) -> PathBuf {
    mem_dir.join(EXTENSIONS_DIR).join(AD_HOC_NOTES_DIR)
}

/// Create the directory layout and seed files.
///
/// Idempotent — safe to call on every startup. Creates missing directories
/// and seeds `memory_summary.md` only when absent.
pub fn ensure_layout(mem_dir: &Path) -> Result<()> {
    let dirs = [
        mem_dir.to_path_buf(),
        rollout_summaries_dir(mem_dir),
        ad_hoc_notes_dir(mem_dir),
    ];

    for dir in &dirs {
        if !dir.exists() {
            debug!("creating memory directory {}", dir.display());
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create memory directory {}", dir.display()))?;
        }
    }

    seed_file(
        &memory_summary_path(mem_dir),
        "# Memory Summary\n\nNo memories yet.\n",
    )?;

    Ok(())
}

/// Create a file with initial content only if it doesn't exist.
fn seed_file(path: &PathBuf, content: &str) -> Result<()> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

/// Read `memory_summary.md`, truncated to the token limit.
pub fn read_memory_summary(mem_dir: &Path) -> Result<String> {
    let path = memory_summary_path(mem_dir);
    if !path.exists() {
        return Ok(String::new());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(truncate_to_token_limit(&content))
}

/// Truncate a string to approximately `SUMMARY_TOKEN_LIMIT` tokens.
fn truncate_to_token_limit(content: &str) -> String {
    let max_chars = SUMMARY_TOKEN_LIMIT * CHARS_PER_TOKEN;
    if content.chars().count() <= max_chars {
        return content.to_string();
    }

    let truncated: String = content.chars().take(max_chars).collect();
    if let Some(last_newline) = truncated.rfind('\n') {
        let mut s = truncated[..last_newline].to_string();
        s.push_str("\n\n[... truncated ...]");
        s
    } else {
        let mut s = truncated;
        s.push_str("\n\n[... truncated ...]");
        s
    }
}

/// Write `memory_summary.md` atomically.
pub fn write_memory_summary(mem_dir: &Path, content: &str) -> Result<()> {
    ensure_layout(mem_dir)?;
    atomic_write(&memory_summary_path(mem_dir), content)
}

/// Atomic write: temp file + rename.
fn atomic_write(path: &PathBuf, content: &str) -> Result<()> {
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, content)
        .with_context(|| format!("failed to write temp file {}", tmp_path.display()))?;
    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e).with_context(|| format!("failed to rename to {}", path.display()));
    }
    Ok(())
}

/// List rollout summary files sorted by name.
pub fn list_rollout_summaries(mem_dir: &Path) -> Result<Vec<PathBuf>> {
    let dir = rollout_summaries_dir(mem_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect();

    files.sort();
    Ok(files)
}

/// Generate memory_summary.md from vector-memory entries.
///
/// Takes a slice of (content, memory_type, confidence) tuples and produces
/// a compact markdown summary. This is the bridge between
/// `rustycode-vector-memory` and the system prompt injection.
pub fn generate_summary(entries: &[(String, String, f32)]) -> String {
    if entries.is_empty() {
        return "# Memory Summary\n\nNo memories yet.\n".to_string();
    }

    let mut sections: std::collections::BTreeMap<&str, Vec<(f32, &str)>> =
        std::collections::BTreeMap::new();

    for (content, mem_type, confidence) in entries {
        sections
            .entry(mem_type.as_str())
            .or_default()
            .push((*confidence, content.as_str()));
    }

    let mut out = String::from("# Memory Summary\n\n");

    for (type_name, mut items) in sections {
        // Sort by confidence descending.
        items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        out.push_str("## ");
        out.push_str(&capitalize(type_name));
        out.push_str("\n\n");
        for (confidence, content) in items {
            let line = content.lines().next().unwrap_or(content);
            let trimmed = if line.len() > 120 {
                let end = line.floor_char_boundary(117);
                format!("{}...", &line[..end])
            } else {
                line.to_string()
            };
            out.push_str("- ");
            out.push_str(&trimmed);
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!(" _({:.0}%)_\n", confidence * 100.0),
            );
        }
        out.push('\n');
    }

    out
}

/// Capitalize the first letter of a snake_case string for display.
fn capitalize(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for c in s.chars() {
        if c == '_' {
            result.push(' ');
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_preserves_content_under_limit() {
        let content = "short content";
        assert_eq!(truncate_to_token_limit(content), content);
    }

    #[test]
    fn truncate_at_newline_boundary() {
        let line = "x".repeat(100);
        let mut content = String::new();
        for _ in 0..(SUMMARY_TOKEN_LIMIT * CHARS_PER_TOKEN / 100 + 10) {
            content.push_str(&line);
            content.push('\n');
        }

        let truncated = truncate_to_token_limit(&content);
        assert!(truncated.contains("[... truncated ...]"));
        assert!(truncated.len() < content.len());
    }

    #[test]
    fn path_helpers_use_mem_dir() {
        let dir = PathBuf::from("/tmp/test-memory");
        assert_eq!(memory_summary_path(&dir), dir.join(MEMORY_SUMMARY_FILE));
        assert_eq!(rollout_summaries_dir(&dir), dir.join(ROLLOUT_SUMMARIES_DIR));
        assert_eq!(
            ad_hoc_notes_dir(&dir),
            dir.join("extensions").join("ad_hoc/notes")
        );
    }

    #[test]
    fn generate_summary_empty() {
        let s = generate_summary(&[]);
        assert!(s.contains("No memories yet"));
    }

    #[test]
    fn generate_summary_groups_by_type() {
        let entries = vec![
            (
                "Always run tests before committing.".to_string(),
                "learnings".to_string(),
                0.9,
            ),
            (
                "Use tokio for async operations.".to_string(),
                "learnings".to_string(),
                0.7,
            ),
            (
                "Auth module uses JWT tokens.".to_string(),
                "code_patterns".to_string(),
                0.8,
            ),
        ];
        let s = generate_summary(&entries);
        assert!(s.contains("## Learnings"));
        assert!(s.contains("## Code Patterns"));
        assert!(s.contains("Always run tests"));
        assert!(s.contains("90%"));
    }

    #[test]
    fn capitalize_snake_case() {
        assert_eq!(capitalize("code_patterns"), "Code Patterns");
        assert_eq!(capitalize("learnings"), "Learnings");
        assert_eq!(capitalize("task_traces"), "Task Traces");
    }
}
