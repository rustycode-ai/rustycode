//! Rollout summary storage for session capture.
//!
//! Each completed session produces an `ExtractedMemory` containing a raw memory
//! (detailed notes) and a rollout summary (compact recap). These are persisted
//! as individual markdown files under `rollout_summaries/`.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::memdir;

/// A single extracted memory from Phase 1 LLM extraction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedMemory {
    /// Thread/session ID this was extracted from.
    pub thread_id: String,
    /// Detailed raw memory content (markdown).
    pub raw_memory: String,
    /// Compact rollout summary (1-3 lines).
    pub rollout_summary: String,
    /// Optional human-readable slug for the rollout file name.
    pub rollout_slug: Option<String>,
    /// Working directory at extraction time.
    pub cwd: String,
    /// When this memory was generated.
    pub generated_at: DateTime<Utc>,
    /// How many times this memory has been used.
    #[serde(default)]
    pub usage_count: u32,
    /// When this memory was last used.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_usage: Option<DateTime<Utc>>,
}

/// Output from Phase 1 LLM extraction (JSON schema).
#[derive(Debug, Clone, Deserialize)]
pub struct StageOneOutput {
    /// Detailed raw memory.
    pub raw_memory: String,
    /// Compact rollout summary.
    pub rollout_summary: String,
    /// Optional slug for file naming.
    pub rollout_slug: Option<String>,
}

impl ExtractedMemory {
    /// Create from Phase 1 output.
    pub fn from_stage_one(output: StageOneOutput, thread_id: &str, cwd: &str) -> Self {
        Self {
            thread_id: thread_id.to_string(),
            raw_memory: output.raw_memory,
            rollout_summary: output.rollout_summary,
            rollout_slug: output.rollout_slug,
            cwd: cwd.to_string(),
            generated_at: Utc::now(),
            usage_count: 0,
            last_usage: None,
        }
    }

    /// Generate a filename for this rollout summary.
    pub fn filename(&self) -> String {
        let timestamp = self.generated_at.format("%Y-%m-%dT%H-%M-%S");
        let id_short = &self.thread_id[..self.thread_id.len().min(6)];

        match &self.rollout_slug {
            Some(slug) => {
                let sanitized: String = slug
                    .chars()
                    .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
                    .collect();
                format!("{timestamp}-{id_short}-{sanitized}.md")
            }
            None => format!("{timestamp}-{id_short}.md"),
        }
    }

    /// Record a usage of this memory.
    pub fn record_usage(&mut self) {
        self.usage_count = self.usage_count.saturating_add(1);
        self.last_usage = Some(Utc::now());
    }

    /// Format as markdown for the rollout summary file.
    pub fn to_markdown(&self) -> String {
        format!(
            "# Rollout Summary: {}\n\n\
             **Thread:** `{}`\n\
             **CWD:** `{}`\n\
             **Generated:** {}\n\
             **Usage count:** {}\n\n\
             ## Summary\n\n{}\n\n\
             ## Raw Memory\n\n{}\n",
            self.rollout_slug.as_deref().unwrap_or("untitled"),
            self.thread_id,
            self.cwd,
            self.generated_at.to_rfc3339(),
            self.usage_count,
            self.rollout_summary,
            self.raw_memory,
        )
    }
}

/// Write a single rollout summary file.
pub fn write_rollout_summary(mem_dir: &Path, memory: &ExtractedMemory) -> Result<PathBuf> {
    memdir::ensure_layout(mem_dir)?;

    let dir = memdir::rollout_summaries_dir(mem_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create {}", dir.display()))?;

    let path = dir.join(memory.filename());
    fs::write(&path, memory.to_markdown())
        .with_context(|| format!("failed to write rollout summary {}", path.display()))?;

    Ok(path)
}

/// Load all rollout summaries from disk.
pub fn load_all_rollout_summaries(mem_dir: &Path) -> Result<Vec<ExtractedMemory>> {
    let dir = memdir::rollout_summaries_dir(mem_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut memories = Vec::new();
    let entries = fs::read_dir(&dir)
        .with_context(|| format!("failed to read {}", dir.display()))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }

        match parse_rollout_file(&path) {
            Ok(Some(m)) => memories.push(m),
            Ok(None) => {
                tracing::debug!("skipping non-memory file: {}", path.display());
            }
            Err(e) => {
                tracing::debug!("failed to parse {}: {e}", path.display());
            }
        }
    }

    memories.sort_by_key(|m| m.generated_at);
    Ok(memories)
}

/// Parse a rollout summary markdown file back into an ExtractedMemory.
fn parse_rollout_file(path: &PathBuf) -> Result<Option<ExtractedMemory>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    if !content.starts_with("# Rollout Summary:") {
        return Ok(None);
    }

    let filename = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let generated_at = parse_filename_timestamp(filename).unwrap_or_else(Utc::now);

    let thread_id = extract_field(&content, "**Thread:**")
        .unwrap_or_else(|| filename.to_string());
    let cwd = extract_field(&content, "**CWD:**")
        .unwrap_or_else(|| "/unknown".to_string());
    let usage_count: u32 = extract_field(&content, "**Usage count:**")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let rollout_slug = content
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("# Rollout Summary: "))
        .map(|s| s.trim().to_string())
        .filter(|s| s != "untitled");

    let rollout_summary = extract_section(&content, "## Summary")
        .unwrap_or_default();
    let raw_memory = extract_section(&content, "## Raw Memory")
        .unwrap_or_default();

    Ok(Some(ExtractedMemory {
        thread_id,
        raw_memory,
        rollout_summary,
        rollout_slug,
        cwd,
        generated_at,
        usage_count,
        last_usage: None,
    }))
}

fn extract_field(content: &str, prefix: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(prefix) {
            let trimmed = rest.trim();
            let trimmed = trimmed
                .strip_prefix('`')
                .and_then(|s| s.strip_suffix('`'))
                .unwrap_or(trimmed);
            return Some(trimmed.to_string());
        }
    }
    None
}

fn extract_section(content: &str, heading: &str) -> Option<String> {
    let start = content.find(heading)?;
    let after_heading = &content[start + heading.len()..];
    let end = after_heading
        .find("\n## ")
        .or_else(|| after_heading.find("\n---"))
        .unwrap_or(after_heading.len());
    Some(after_heading[..end].trim().to_string())
}

fn parse_filename_timestamp(filename: &str) -> Option<DateTime<Utc>> {
    let ts_str = filename.get(..19)?;
    let fixed = format!(
        "{}:{}:{}",
        &ts_str[..13],
        &ts_str[14..16],
        &ts_str[17..19]
    );
    DateTime::parse_from_str(&fixed, "%Y-%m-%dT%H:%M:%S")
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_with_slug() {
        let mem = ExtractedMemory {
            thread_id: "abc123def".to_string(),
            raw_memory: String::new(),
            rollout_summary: String::new(),
            rollout_slug: Some("fix auth bug".to_string()),
            cwd: "/home/user".to_string(),
            generated_at: Utc::now(),
            usage_count: 0,
            last_usage: None,
        };
        let name = mem.filename();
        assert!(name.ends_with("-fix_auth_bug.md"));
    }

    #[test]
    fn filename_without_slug() {
        let mem = ExtractedMemory {
            thread_id: "abc123".to_string(),
            raw_memory: String::new(),
            rollout_summary: String::new(),
            rollout_slug: None,
            cwd: "/home/user".to_string(),
            generated_at: Utc::now(),
            usage_count: 0,
            last_usage: None,
        };
        let name = mem.filename();
        assert!(name.ends_with("-abc123.md"));
    }

    #[test]
    fn roundtrip_markdown() {
        let original = ExtractedMemory {
            thread_id: "test123".to_string(),
            raw_memory: "Found that foo() needs bar() called first.".to_string(),
            rollout_summary: "Fixed auth bug in foo().".to_string(),
            rollout_slug: Some("auth-fix".to_string()),
            cwd: "/project".to_string(),
            generated_at: Utc::now(),
            usage_count: 3,
            last_usage: None,
        };

        let md = original.to_markdown();
        assert!(md.contains("## Summary"));
        assert!(md.contains("## Raw Memory"));
    }

    #[test]
    fn from_stage_one() {
        let output = StageOneOutput {
            raw_memory: "Detailed notes here.".to_string(),
            rollout_summary: "Quick recap.".to_string(),
            rollout_slug: Some("my-session".to_string()),
        };

        let mem = ExtractedMemory::from_stage_one(output, "thread1", "/cwd");
        assert_eq!(mem.thread_id, "thread1");
        assert_eq!(mem.cwd, "/cwd");
        assert_eq!(mem.rollout_slug.as_deref(), Some("my-session"));
    }
}
