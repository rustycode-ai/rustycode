//! MEMORY.md index layer: compact always-loaded index for agent memory.
//!
//! The index is a structured markdown file capped at 200 lines containing
//! pointers to topic files, recent decisions, and session notes. It is loaded
//! at session start and provides the entry point for all memory retrieval.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write};
use std::fs;
use std::path::Path;
use tracing::debug;

/// Maximum number of lines the MEMORY.md index may contain.
pub const MAX_INDEX_LINES: usize = 200;

/// Maximum number of recent decisions retained.
const MAX_RECENT_DECISIONS: usize = 20;

/// Maximum number of session notes retained.
const MAX_SESSION_NOTES: usize = 15;

/// A reference to a topic file in the memory index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopicRef {
    /// Human-readable name for the topic (e.g., "API Patterns").
    pub name: String,
    /// Relative file path from the memory directory (e.g., "topics/api-patterns.md").
    pub file_path: String,
    /// One-line description of what this topic covers.
    pub description: String,
    /// Keywords that trigger loading of this topic.
    pub keywords: Vec<String>,
}

/// A recorded decision from a past session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decision {
    /// ISO 8601 date string (e.g., "2026-04-25").
    pub date: String,
    /// One-line description of the decision.
    pub description: String,
    /// Why this decision was made.
    pub rationale: String,
}

/// A note summarizing a completed session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionNote {
    /// ISO 8601 date string.
    pub date: String,
    /// One-line summary of what happened.
    pub summary: String,
    /// The single most important takeaway.
    pub key_learning: String,
}

/// The top-level MEMORY.md index structure.
///
/// Serialized as markdown with sections for topics, decisions, and session notes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryIndex {
    /// Brief description of active project context (1-3 lines).
    pub active_context: String,
    /// References to topic files, loaded on demand.
    pub topic_refs: Vec<TopicRef>,
    /// Recent decisions worth preserving.
    pub recent_decisions: Vec<Decision>,
    /// Summaries of recent sessions.
    pub session_notes: Vec<SessionNote>,
}

impl MemoryIndex {
    /// Create an empty index with default active context.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active_context: String::new(),
            topic_refs: Vec::new(),
            recent_decisions: Vec::new(),
            session_notes: Vec::new(),
        }
    }

    /// Count the total number of lines this index would occupy when rendered
    /// as markdown.
    pub fn line_count(&self) -> usize {
        self.to_markdown().lines().count()
    }

    /// Render the index as a markdown string.
    pub fn to_markdown(&self) -> String {
        let mut out = String::with_capacity(2048);

        out.push_str("# Memory Index\n\n");

        out.push_str("## Active Context\n\n");
        out.push_str(&self.active_context);
        out.push('\n');

        out.push_str("\n## Topic Index\n\n");
        if self.topic_refs.is_empty() {
            out.push_str("_No topics indexed._\n");
        } else {
            for tr in &self.topic_refs {
                let _ = writeln!(
                    out,
                    "- **{}** (`{}`) — {} [keywords: {}]",
                    tr.name,
                    tr.file_path,
                    tr.description,
                    tr.keywords.join(", ")
                );
            }
        }

        out.push_str("\n## Recent Decisions\n\n");
        if self.recent_decisions.is_empty() {
            out.push_str("_No decisions recorded._\n");
        } else {
            for d in &self.recent_decisions {
                let _ = writeln!(out, "- **{}**: {} — {}", d.date, d.description, d.rationale);
            }
        }

        out.push_str("\n## Session Notes\n\n");
        if self.session_notes.is_empty() {
            out.push_str("_No session notes._\n");
        } else {
            for n in &self.session_notes {
                let _ = writeln!(
                    out,
                    "- **{}**: {} _Key: {}_",
                    n.date, n.summary, n.key_learning
                );
            }
        }

        out
    }

    /// Load the memory index from a MEMORY.md file.
    ///
    /// Returns a default empty index if the file does not exist.
    /// Malformed sections are skipped with debug-level logging.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read memory index {}", path.display()))?;

        if content.trim().is_empty() {
            return Ok(Self::new());
        }

        let mut idx = Self::new();
        let mut current_section: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("## ") {
                current_section = Some(trimmed.strip_prefix("## ").unwrap_or(trimmed).to_string());
                continue;
            }

            if trimmed.starts_with("# ") || trimmed.is_empty() || trimmed.starts_with('_') {
                continue;
            }

            let section = match &current_section {
                Some(s) => s.as_str(),
                None => continue,
            };

            match section {
                "Active Context" => {
                    if !idx.active_context.is_empty() {
                        idx.active_context.push('\n');
                    }
                    idx.active_context.push_str(trimmed);
                }
                "Topic Index" => {
                    if let Some(tr) = parse_topic_ref(trimmed) {
                        idx.topic_refs.push(tr);
                    } else {
                        debug!("skipping malformed topic ref line: {trimmed}");
                    }
                }
                "Recent Decisions" => {
                    if let Some(d) = parse_decision(trimmed) {
                        idx.recent_decisions.push(d);
                    } else {
                        debug!("skipping malformed decision line: {trimmed}");
                    }
                }
                "Session Notes" => {
                    if let Some(n) = parse_session_note(trimmed) {
                        idx.session_notes.push(n);
                    } else {
                        debug!("skipping malformed session note line: {trimmed}");
                    }
                }
                _ => {
                    debug!("ignoring unknown section: {section}");
                }
            }
        }

        Ok(idx)
    }

    /// Save the memory index to a MEMORY.md file.
    ///
    /// Uses atomic write (temp file + rename) to avoid corruption.
    /// Enforces the 200-line cap by truncating sections in priority order.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let mut to_save = self.clone();
        to_save.enforce_line_cap();

        let content = to_save.to_markdown();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let tmp_path = path.with_extension("md.tmp");
        fs::write(&tmp_path, &content)
            .with_context(|| format!("Failed to write temp file {}", tmp_path.display()))?;

        if let Err(e) = fs::rename(&tmp_path, path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(e)
                .with_context(|| format!("Failed to rename temp file to {}", path.display()));
        }

        Ok(())
    }

    /// Add a decision to the index, enforcing the cap.
    pub fn add_decision(&mut self, decision: Decision) {
        self.recent_decisions.push(decision);
        if self.recent_decisions.len() > MAX_RECENT_DECISIONS {
            let excess = self.recent_decisions.len() - MAX_RECENT_DECISIONS;
            self.recent_decisions.drain(..excess);
        }
    }

    /// Add a session note to the index, enforcing the cap.
    pub fn add_session_note(&mut self, note: SessionNote) {
        self.session_notes.push(note);
        if self.session_notes.len() > MAX_SESSION_NOTES {
            let excess = self.session_notes.len() - MAX_SESSION_NOTES;
            self.session_notes.drain(..excess);
        }
    }

    /// Enforce the overall 200-line cap by truncating sections in priority order.
    ///
    /// Truncation order (first truncated = lowest priority):
    /// 1. `session_notes` — easiest to lose
    /// 2. `recent_decisions` — more valuable
    /// 3. `topic_refs` — most valuable, truncated last
    pub fn enforce_line_cap(&mut self) {
        for _ in 0..10 {
            if self.line_count() <= MAX_INDEX_LINES {
                return;
            }

            if !self.session_notes.is_empty() {
                self.session_notes.drain(0..1);
                continue;
            }

            if !self.recent_decisions.is_empty() {
                self.recent_decisions.drain(0..1);
                continue;
            }

            if !self.topic_refs.is_empty() {
                self.topic_refs.drain(0..1);
                continue;
            }

            break;
        }
    }
}

impl Default for MemoryIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MemoryIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_markdown())
    }
}

/// Parse a topic reference line like:
/// `- **API Patterns** (`topics/api.md`) — REST patterns [keywords: api, rest]`
fn parse_topic_ref(line: &str) -> Option<TopicRef> {
    let line = line.strip_prefix("- **")?;

    let close_idx = line.find("** ")?;
    let name = line[..close_idx].to_string();
    let rest = &line[close_idx + 3..];

    let bt_start = rest.find('`')?;
    let bt_end = rest[bt_start + 1..].find('`')?;
    let file_path = rest[bt_start + 1..bt_start + 1 + bt_end].to_string();
    let after_path = &rest[bt_start + 1 + bt_end + 1..];

    let (desc_start, marker_len) = after_path
        .find("— ")
        .map(|i| (i, "— ".len()))
        .or_else(|| after_path.find("- ").map(|i| (i, "- ".len())))?;
    let after_desc_marker = &after_path[desc_start + marker_len..];

    let (description, keywords) = if let Some(kw_start) = after_desc_marker.find("[keywords: ") {
        let desc = after_desc_marker[..kw_start].trim().to_string();
        let kw_str = &after_desc_marker[kw_start + 11..];
        let kw_end = kw_str.find(']')?;
        let keywords = kw_str[..kw_end]
            .split(',')
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect();
        (desc, keywords)
    } else {
        (after_desc_marker.trim().to_string(), Vec::new())
    };

    Some(TopicRef {
        name,
        file_path,
        description,
        keywords,
    })
}

/// Parse a decision line like:
/// `- **2026-04-25**: Use MEMORY.md — Markdown is human-readable`
fn parse_decision(line: &str) -> Option<Decision> {
    let line = line.strip_prefix("- **")?;
    let close_idx = line.find("**")?;
    let date = line[..close_idx].to_string();
    let rest = &line[close_idx + 2..];

    let rest = rest.strip_prefix(": ")?;

    if let Some(sep_idx) = rest.find(" — ") {
        Some(Decision {
            date,
            description: rest[..sep_idx].to_string(),
            rationale: rest[sep_idx + " — ".len()..].to_string(),
        })
    } else {
        Some(Decision {
            date,
            description: rest.to_string(),
            rationale: String::new(),
        })
    }
}

/// Parse a session note line like:
/// `- **2026-04-24**: Completed compaction _Key: Middle-out works better_`
#[allow(clippy::option_if_let_else)]
fn parse_session_note(line: &str) -> Option<SessionNote> {
    let line = line.strip_prefix("- **")?;
    let close_idx = line.find("**")?;
    let date = line[..close_idx].to_string();
    let rest = &line[close_idx + 2..];

    let rest = rest.strip_prefix(": ")?;

    let kw_start = rest.find("_Key: ");
    let summary = match kw_start {
        Some(pos) => rest[..pos].trim().to_string(),
        None => rest.trim().to_string(),
    };
    let key_learning = kw_start
        .map(|pos| {
            let after = &rest[pos + 6..];
            match after.find('_') {
                Some(end) => after[..end].to_string(),
                None => String::new(),
            }
        })
        .unwrap_or_default();

    Some(SessionNote {
        date,
        summary,
        key_learning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_dir() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-memory-index-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn memory_index_new_is_empty() {
        let idx = MemoryIndex::new();
        assert!(idx.active_context.is_empty());
        assert!(idx.topic_refs.is_empty());
        assert!(idx.recent_decisions.is_empty());
        assert!(idx.session_notes.is_empty());
    }

    #[test]
    fn memory_index_default_matches_new() {
        let a = MemoryIndex::new();
        let b = MemoryIndex::default();
        assert_eq!(a, b);
    }

    #[test]
    fn topic_ref_serialization_roundtrip() {
        let tr = TopicRef {
            name: "API Patterns".to_string(),
            file_path: "topics/api-patterns.md".to_string(),
            description: "REST and GraphQL patterns used in the project".to_string(),
            keywords: vec!["api".to_string(), "rest".to_string(), "graphql".to_string()],
        };
        let yaml = serde_yaml::to_string(&tr).unwrap();
        let decoded: TopicRef = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(decoded, tr);
    }

    #[test]
    fn decision_serialization_roundtrip() {
        let d = Decision {
            date: "2026-04-25".to_string(),
            description: "Use SQLite for session storage".to_string(),
            rationale: "Simpler deployment than Postgres for local-first agent".to_string(),
        };
        let yaml = serde_yaml::to_string(&d).unwrap();
        let decoded: Decision = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(decoded, d);
    }

    #[test]
    fn session_note_serialization_roundtrip() {
        let n = SessionNote {
            date: "2026-04-25".to_string(),
            summary: "Implemented three-layer memory model".to_string(),
            key_learning: "200-line cap requires aggressive pruning".to_string(),
        };
        let yaml = serde_yaml::to_string(&n).unwrap();
        let decoded: SessionNote = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(decoded, n);
    }

    #[test]
    fn to_markdown_empty_index() {
        let idx = MemoryIndex::new();
        let md = idx.to_markdown();
        assert!(md.contains("# Memory Index"));
        assert!(md.contains("## Active Context"));
        assert!(md.contains("## Topic Index"));
        assert!(md.contains("## Recent Decisions"));
        assert!(md.contains("## Session Notes"));
        assert!(md.contains("_No topics indexed._"));
        assert!(md.contains("_No decisions recorded._"));
        assert!(md.contains("_No session notes._"));
    }

    #[test]
    fn to_markdown_with_data() {
        let idx = MemoryIndex {
            active_context: "Working on memory architecture".to_string(),
            topic_refs: vec![TopicRef {
                name: "API Patterns".to_string(),
                file_path: "topics/api.md".to_string(),
                description: "REST patterns".to_string(),
                keywords: vec!["api".to_string()],
            }],
            recent_decisions: vec![Decision {
                date: "2026-04-25".to_string(),
                description: "Use MEMORY.md as index".to_string(),
                rationale: "Markdown is human-readable".to_string(),
            }],
            session_notes: vec![SessionNote {
                date: "2026-04-24".to_string(),
                summary: "Completed compaction upgrade".to_string(),
                key_learning: "Middle-out removal preserves context".to_string(),
            }],
        };
        let md = idx.to_markdown();
        assert!(md.contains("Working on memory architecture"));
        assert!(md.contains("API Patterns"));
        assert!(md.contains("topics/api.md"));
        assert!(md.contains("Use MEMORY.md as index"));
        assert!(md.contains("Completed compaction upgrade"));
    }

    #[test]
    fn line_count_empty_index() {
        let idx = MemoryIndex::new();
        assert!(idx.line_count() < 20);
    }

    #[test]
    fn line_count_stays_under_cap_with_data() {
        let mut idx = MemoryIndex::new();
        idx.active_context = "Test context".to_string();
        for i in 0..20 {
            idx.topic_refs.push(TopicRef {
                name: format!("Topic {i}"),
                file_path: format!("topics/t{i}.md"),
                description: format!("Description for topic {i}"),
                keywords: vec![format!("kw{i}")],
            });
        }
        assert!(
            idx.line_count() < MAX_INDEX_LINES,
            "Index with 20 topics should be under {MAX_INDEX_LINES} lines, got {}",
            idx.line_count()
        );
    }

    #[test]
    fn display_trait_matches_to_markdown() {
        let idx = MemoryIndex {
            active_context: "test".to_string(),
            topic_refs: Vec::new(),
            recent_decisions: Vec::new(),
            session_notes: Vec::new(),
        };
        assert_eq!(format!("{idx}"), idx.to_markdown());
    }

    // ── Load From File Tests ────────────────────────────────────────────────

    #[test]
    fn load_from_file_parses_well_formed() {
        let dir = temp_dir();
        let path = dir.join("MEMORY.md");
        std::fs::write(
            &path,
            r#"# Memory Index

## Active Context

Working on the memory architecture phase.

## Topic Index

- **API Patterns** (`topics/api.md`) — REST and GraphQL patterns [keywords: api, rest]

## Recent Decisions

- **2026-04-25**: Use MEMORY.md as index — Markdown is human-readable

## Session Notes

- **2026-04-24**: Completed compaction _Key: Middle-out works better_
"#,
        )
        .unwrap();

        let idx = MemoryIndex::load_from_file(&path).unwrap();
        assert!(idx
            .active_context
            .contains("Working on the memory architecture"));
        assert_eq!(idx.topic_refs.len(), 1);
        assert_eq!(idx.topic_refs[0].name, "API Patterns");
        assert_eq!(idx.topic_refs[0].file_path, "topics/api.md");
        assert_eq!(idx.topic_refs[0].keywords, vec!["api", "rest"]);
        assert_eq!(idx.recent_decisions.len(), 1);
        assert_eq!(idx.recent_decisions[0].date, "2026-04-25");
        assert_eq!(idx.session_notes.len(), 1);
        assert_eq!(idx.session_notes[0].key_learning, "Middle-out works better");
    }

    #[test]
    fn load_from_file_missing_returns_default() {
        let dir = temp_dir();
        let path = dir.join("MEMORY.md");
        let idx = MemoryIndex::load_from_file(&path).unwrap();
        assert!(idx.active_context.is_empty());
        assert!(idx.topic_refs.is_empty());
        assert!(idx.recent_decisions.is_empty());
        assert!(idx.session_notes.is_empty());
    }

    #[test]
    fn load_from_file_malformed_recovers() {
        let dir = temp_dir();
        let path = dir.join("MEMORY.md");
        std::fs::write(
            &path,
            r#"# Memory Index

## Active Context

Some context.

## Topic Index

garbage line with no dash prefix

- **Good Topic** (`topics/good.md`) — A valid topic [keywords: good]

## Recent Decisions

not a valid decision line either

## Session Notes

also not valid
"#,
        )
        .unwrap();

        let idx = MemoryIndex::load_from_file(&path).unwrap();
        assert_eq!(idx.topic_refs.len(), 1);
        assert_eq!(idx.topic_refs[0].name, "Good Topic");
        assert!(idx.recent_decisions.is_empty());
        assert!(idx.session_notes.is_empty());
    }

    #[test]
    fn load_from_file_extra_sections_ignored() {
        let dir = temp_dir();
        let path = dir.join("MEMORY.md");
        std::fs::write(
            &path,
            r#"# Memory Index

## Active Context

Context here.

## Unknown Section

This section is not recognized.

## Topic Index

- **Known** (`topics/known.md`) — Known topic [keywords: known]

## Another Unknown

More unrecognized content.
"#,
        )
        .unwrap();

        let idx = MemoryIndex::load_from_file(&path).unwrap();
        assert_eq!(idx.topic_refs.len(), 1);
        assert_eq!(idx.topic_refs[0].name, "Known");
    }

    #[test]
    fn load_from_file_empty_file_returns_default() {
        let dir = temp_dir();
        let path = dir.join("MEMORY.md");
        std::fs::write(&path, "").unwrap();

        let idx = MemoryIndex::load_from_file(&path).unwrap();
        assert_eq!(idx, MemoryIndex::new());
    }

    #[test]
    fn load_from_file_topic_ref_no_keywords() {
        let dir = temp_dir();
        let path = dir.join("MEMORY.md");
        std::fs::write(
            &path,
            r#"# Memory Index

## Active Context

Ctx

## Topic Index

- **No Keywords** (`topics/nk.md`) — Topic without keywords section
"#,
        )
        .unwrap();

        let idx = MemoryIndex::load_from_file(&path).unwrap();
        assert_eq!(idx.topic_refs.len(), 1);
        assert_eq!(idx.topic_refs[0].keywords, Vec::<String>::new());
    }

    // ── Save / Writer Tests ────────────────────────────────────────────────

    #[test]
    fn save_and_load_roundtrip() {
        let dir = temp_dir();
        let path = dir.join("MEMORY.md");

        let original = MemoryIndex {
            active_context: "Working on memory".to_string(),
            topic_refs: vec![TopicRef {
                name: "Test".to_string(),
                file_path: "topics/test.md".to_string(),
                description: "Test topic".to_string(),
                keywords: vec!["test".to_string()],
            }],
            recent_decisions: vec![Decision {
                date: "2026-04-25".to_string(),
                description: "Use markdown".to_string(),
                rationale: "Human-readable".to_string(),
            }],
            session_notes: vec![SessionNote {
                date: "2026-04-24".to_string(),
                summary: "Built index types".to_string(),
                key_learning: "Keep it simple".to_string(),
            }],
        };

        original.save_to_file(&path).unwrap();
        let loaded = MemoryIndex::load_from_file(&path).unwrap();

        assert_eq!(loaded.active_context, original.active_context);
        assert_eq!(loaded.topic_refs, original.topic_refs);
        assert_eq!(loaded.recent_decisions, original.recent_decisions);
        assert_eq!(loaded.session_notes, original.session_notes);
    }

    #[test]
    fn save_atomic_no_leftover_tmp() {
        let dir = temp_dir();
        let path = dir.join("MEMORY.md");

        let idx = MemoryIndex::new();
        idx.save_to_file(&path).unwrap();

        let tmp_path = dir.join("MEMORY.md.tmp");
        assert!(!tmp_path.exists(), "temp file should be cleaned up");
        assert!(path.exists());
    }

    #[test]
    fn save_enforces_line_cap() {
        let dir = temp_dir();
        let path = dir.join("MEMORY.md");

        let mut idx = MemoryIndex::new();
        idx.active_context = "Testing cap".to_string();

        for i in 0..100 {
            idx.recent_decisions.push(Decision {
                date: format!("2026-04-{:02}", (i % 28) + 1),
                description: format!("Decision number {i} with a moderately long description"),
                rationale: format!("Rationale for decision {i} explaining why"),
            });
        }

        idx.save_to_file(&path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let line_count = content.lines().count();
        assert!(
            line_count <= MAX_INDEX_LINES,
            "Index should be capped at {MAX_INDEX_LINES} lines, got {line_count}",
        );
    }

    #[test]
    fn add_decision_appends_and_enforces_cap() {
        let mut idx = MemoryIndex::new();
        idx.active_context = "Test".to_string();

        for i in 0..25 {
            idx.add_decision(Decision {
                date: format!("2026-04-{:02}", i + 1),
                description: format!("Decision {i}"),
                rationale: format!("Because {i}"),
            });
        }

        assert!(
            idx.recent_decisions.len() <= MAX_RECENT_DECISIONS,
            "decisions should be capped at {MAX_RECENT_DECISIONS}, got {}",
            idx.recent_decisions.len(),
        );
        let last = idx.recent_decisions.last().unwrap();
        assert_eq!(last.description, "Decision 24");
    }

    #[test]
    fn add_session_note_appends_and_enforces_cap() {
        let mut idx = MemoryIndex::new();

        for i in 0..20 {
            idx.add_session_note(SessionNote {
                date: format!("2026-04-{:02}", i + 1),
                summary: format!("Session {i}"),
                key_learning: format!("Learned {i}"),
            });
        }

        assert!(
            idx.session_notes.len() <= MAX_SESSION_NOTES,
            "session notes should be capped at {MAX_SESSION_NOTES}, got {}",
            idx.session_notes.len(),
        );
        let last = idx.session_notes.last().unwrap();
        assert_eq!(last.summary, "Session 19");
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = temp_dir();
        let nested = dir.join("deep").join("path").join("MEMORY.md");

        let idx = MemoryIndex::new();
        idx.save_to_file(&nested).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn cap_enforcement_truncates_in_order() {
        let mut idx = MemoryIndex::new();
        idx.active_context = "x".to_string();

        for i in 0..50 {
            idx.topic_refs.push(TopicRef {
                name: format!("T{i}"),
                file_path: format!("topics/t{i}.md"),
                description: format!("Topic {i} description"),
                keywords: vec![format!("kw{i}")],
            });
            idx.recent_decisions.push(Decision {
                date: format!("2026-01-{:02}", i + 1),
                description: format!("Decision {i}"),
                rationale: format!("Rationale {i}"),
            });
            idx.session_notes.push(SessionNote {
                date: format!("2026-01-{:02}", i + 1),
                summary: format!("Session {i}"),
                key_learning: format!("Learning {i}"),
            });
        }

        idx.enforce_line_cap();

        let total_lines = idx.line_count();
        assert!(
            total_lines <= MAX_INDEX_LINES,
            "After enforcement, index should be under {MAX_INDEX_LINES} lines, got {total_lines}",
        );
    }

    #[test]
    fn trim_drains_session_notes_first() {
        let mut idx = MemoryIndex::new();
        // Each note is 1 markdown line. Need 200+ to exceed cap.
        for i in 0..300 {
            idx.session_notes.push(SessionNote {
                date: "2026-01-01".to_string(),
                summary: format!("Note {i} summary text"),
                key_learning: format!("Learning {i}"),
            });
        }
        let notes_before = idx.session_notes.len();
        let lines_before = idx.line_count();
        assert!(
            lines_before > MAX_INDEX_LINES,
            "Need more than {MAX_INDEX_LINES} lines, got {lines_before}"
        );
        idx.enforce_line_cap();
        assert!(
            idx.session_notes.len() < notes_before,
            "Should have removed some notes"
        );
        assert!(
            idx.line_count() < lines_before,
            "Should have reduced line count"
        );
    }

    #[test]
    fn trim_falls_through_to_decisions_when_notes_empty() {
        let mut idx = MemoryIndex::new();
        // No session notes, fill decisions to exceed cap
        for i in 0..300 {
            idx.recent_decisions.push(Decision {
                date: "2026-01-01".to_string(),
                description: format!("Decision {i}"),
                rationale: format!("Reason {i}"),
            });
        }
        let decisions_before = idx.recent_decisions.len();
        let lines_before = idx.line_count();
        assert!(
            lines_before > MAX_INDEX_LINES,
            "Need more than {MAX_INDEX_LINES} lines, got {lines_before}"
        );
        idx.enforce_line_cap();
        assert!(
            idx.recent_decisions.len() < decisions_before,
            "Should have removed some decisions"
        );
        assert!(idx.line_count() < lines_before);
    }
}
