#![allow(clippy::ref_option, clippy::format_collect)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::redundant_clone,
        clippy::float_cmp,
        clippy::needless_raw_string_hashes
    )
)]

use anyhow::{Context, Result};
use rustycode_config::domain::DomainContext;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::debug;

pub mod consolidation;
pub mod domain_topic;
pub mod index;
pub mod topic;

// Custom serialization for SystemTime as ISO 8601 string
mod system_time_serde {
    use super::*;

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_else(|e| {
                // System time is before UNIX epoch — use 0 as fallback
                tracing::warn!("system time before UNIX epoch: {e}, falling back to 0");
                std::time::Duration::ZERO
            });
        let secs = duration.as_secs();
        serializer.serialize_str(&secs.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let secs: u64 = s.parse().map_err(serde::de::Error::custom)?;
        Ok(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs))
    }

    pub mod option {
        use super::*;

        pub fn serialize<S>(time: &Option<SystemTime>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            match time {
                Some(t) => super::serialize(t, serializer),
                None => serializer.serialize_none(),
            }
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<SystemTime>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let opt = Option::<String>::deserialize(deserializer)?;
            match opt {
                Some(s) => {
                    let secs: u64 = s.parse().map_err(serde::de::Error::custom)?;
                    Ok(Some(
                        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
                    ))
                }
                None => Ok(None),
            }
        }
    }
}

/// Memory domain for categorization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum MemoryDomain {
    CodeStyle,
    Testing,
    Git,
    Debugging,
    Workflow,
    Architecture,
    ProjectSpecific,
}

/// Memory scope - project-specific or global
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum MemoryScope {
    Project,
    Global,
}

/// Source of a memory entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum MemorySource {
    SessionObservation,
    UserExplicit,
    ProjectAnalysis,
    ManualEntry,
}

/// Observation that contributed to a memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub timestamp: SystemTime,
    pub pattern_type: String,
    pub description: String,
    pub confidence_boost: f32,
}

/// Enhanced memory entry with confidence scoring and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier for this entry
    pub id: String,
    /// When this memory becomes relevant (trigger condition)
    pub trigger: String,
    /// Confidence score (0.3 - 0.9)
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    /// Domain categorization
    pub domain: MemoryDomain,
    /// Source of this memory
    pub source: MemorySource,
    /// Scope - project-specific or global
    pub scope: MemoryScope,
    /// Project ID (if project-scoped)
    pub project_id: Option<String>,
    /// Action to take when triggered
    pub action: String,
    /// Evidence that created this memory
    #[serde(default)]
    pub evidence: Vec<Observation>,
    /// When this entry was created
    #[serde(with = "system_time_serde")]
    pub created_at: SystemTime,
    /// When this entry was last used
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "system_time_serde::option")]
    pub last_used: Option<SystemTime>,
    /// How many times this entry has been used
    #[serde(default)]
    pub use_count: usize,
}

const fn default_confidence() -> f32 {
    0.5
}

/// Configuration for creating a `MemoryEntry`
pub struct MemoryEntryConfig {
    pub id: String,
    pub trigger: String,
    pub confidence: f32,
    pub domain: MemoryDomain,
    pub source: MemorySource,
    pub scope: MemoryScope,
    pub project_id: Option<String>,
    pub action: String,
}

impl MemoryEntry {
    /// Create a new memory entry
    pub fn new(config: MemoryEntryConfig) -> Self {
        let clamped_confidence = config.confidence.clamp(0.3, 0.9);

        Self {
            id: config.id,
            trigger: config.trigger,
            confidence: clamped_confidence,
            domain: config.domain,
            source: config.source,
            scope: config.scope,
            project_id: config.project_id,
            action: config.action,
            evidence: Vec::new(),
            created_at: SystemTime::now(),
            last_used: None,
            use_count: 0,
        }
    }

    /// Boost confidence when entry is used
    pub fn boost_confidence(&mut self, amount: f32) {
        self.confidence = (self.confidence + amount).min(0.9);
        self.last_used = Some(SystemTime::now());
        self.use_count = self.use_count.saturating_add(1);
    }

    /// Decay confidence if contradicted
    pub fn decay_confidence(&mut self, amount: f32) {
        self.confidence = (self.confidence - amount).max(0.0);
    }

    /// Check if this entry should be pruned
    pub fn should_prune(&self) -> bool {
        if self.confidence < 0.3 {
            if let Some(last_used) = self.last_used {
                let days_since_use = last_used.elapsed().unwrap_or_default().as_secs() / 86400;
                return days_since_use > 30;
            }
        }
        false
    }

    /// Calculate relevance score for a query
    pub fn calculate_relevance(&self, query: &str, current_domain: &MemoryDomain) -> f32 {
        let mut score = 0.0;

        // Domain match: high boost
        if &self.domain == current_domain {
            score += 0.5;
        }

        // Trigger keyword match
        if query.to_lowercase().contains(&self.trigger.to_lowercase()) {
            score += 0.3;
        }

        // Confidence weighting
        score *= self.confidence;

        // Recency boost (used recently)
        if let Some(last_used) = self.last_used {
            let days_since_use = last_used.elapsed().unwrap_or_default().as_secs() / 86400;
            if days_since_use < 7 {
                score += 0.1;
            }
        }

        score
    }
}

/// Project detection context
#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

/// Detect project context from git repository
pub fn detect_project_context(cwd: &Path) -> Option<ProjectContext> {
    // Try git remote URL first (portable across machines)
    if let Some(remote_url) = get_git_remote(cwd) {
        let project_id = hash_remote_url(&remote_url);
        return Some(ProjectContext {
            id: project_id,
            name: extract_project_name(&remote_url),
            path: cwd.to_path_buf(),
        });
    }

    // Fallback to git toplevel path (machine-specific)
    if let Some(toplevel) = get_git_toplevel(cwd) {
        let project_id = hash_path(&toplevel);
        return Some(ProjectContext {
            id: project_id,
            name: toplevel.file_name()?.to_string_lossy().to_string(),
            path: toplevel,
        });
    }

    None
}

/// Get git remote URL for repository
fn get_git_remote(cwd: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["-C", cwd.to_str()?, "remote", "get-url", "origin"])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Get git toplevel path
fn get_git_toplevel(cwd: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["-C", cwd.to_str()?, "rev-parse", "--show-toplevel"])
        .output()
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(PathBuf::from(path))
    } else {
        None
    }
}

/// Hash remote URL to create portable project ID
fn hash_remote_url(url: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let result = hasher.finalize();
    // Convert first 12 bytes to hex string
    result[..12].iter().map(|b| format!("{b:02x}")).collect()
}

/// Hash path to create machine-specific project ID
fn hash_path(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    // Convert first 12 bytes to hex string
    result[..12].iter().map(|b| format!("{b:02x}")).collect()
}

/// Extract project name from remote URL
fn extract_project_name(url: &str) -> String {
    // Remove .git suffix if present
    let url = url.trim_end_matches(".git");

    // Extract last path component
    url.split('/').next_back().unwrap_or("unknown").to_string()
}

/// Get memory directory for current context
pub fn get_memory_dir(cwd: &Path) -> PathBuf {
    if let Some(project) = detect_project_context(cwd) {
        // Project-scoped memory
        PathBuf::from(".rustycode")
            .join("projects")
            .join(&project.id)
            .join("memory")
    } else {
        // Global memory
        PathBuf::from(".rustycode").join("memory")
    }
}

/// Facade that ties together the memory index, topic loader, and consolidation.
///
/// This is the primary interface for session lifecycle management.
pub struct MemoryManager {
    memory_dir: PathBuf,
    index: index::MemoryIndex,
    topic_loader: topic::TopicLoader,
}

impl MemoryManager {
    /// Create a new `MemoryManager`, loading the existing index from disk
    /// or creating a default empty one if no index exists.
    pub fn new(memory_dir: &Path) -> Result<Self> {
        fs::create_dir_all(memory_dir).with_context(|| {
            format!("Failed to create memory directory {}", memory_dir.display())
        })?;

        let index_path = memory_dir.join("MEMORY.md");
        let index = index::MemoryIndex::load_from_file(&index_path)?;

        let topics_dir = memory_dir.join("topics");
        let topic_loader = topic::TopicLoader::new(&topics_dir);

        Ok(Self {
            memory_dir: memory_dir.to_path_buf(),
            index,
            topic_loader,
        })
    }

    /// Save the domain context as a topic file and load it by keyword.
    pub fn load_domain_context(&mut self, ctx: &DomainContext) -> Result<Option<topic::TopicFile>> {
        let topics_dir = self.memory_dir.join("topics");
        domain_topic::save_domain_topic(&topics_dir, ctx)?;
        self.topic_loader.load_by_keyword("domain")
    }

    /// Discover a domain context at the workspace root, persist it, and load it.
    pub fn load_domain_context_from_workspace(
        &mut self,
        workspace_root: &Path,
    ) -> Result<Option<topic::TopicFile>> {
        let Some(path) = DomainContext::discover(workspace_root)? else {
            return Ok(None);
        };
        let ctx = DomainContext::load_from_file(&path)?;
        self.load_domain_context(&ctx)
    }
}

/// Load memory entries from directory (supports both YAML and legacy formats)
pub fn load(memory_dir: &Path) -> Result<Vec<MemoryEntry>> {
    if !memory_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();

    // Look for memory.yaml (new format) or notes.md (legacy format)
    let yaml_path = memory_dir.join("memory.yaml");
    let legacy_path = memory_dir.join("notes.md");

    if yaml_path.exists() {
        // Load new YAML format
        let content = fs::read_to_string(&yaml_path)
            .with_context(|| format!("Failed to read memory file {}", yaml_path.display()))?;

        // Parse YAML documents separated by '---'
        for doc_str in content.split("---") {
            let doc_str = doc_str.trim();
            if doc_str.is_empty() {
                continue;
            }

            match serde_yaml::from_str::<MemoryEntry>(doc_str) {
                Ok(entry) => entries.push(entry),
                Err(e) => debug!(
                    "failed to parse memory entry YAML chunk: {} err: {}",
                    doc_str, e
                ),
            }
        }
    } else if legacy_path.exists() {
        // Convert legacy format to new format
        let legacy_content = fs::read_to_string(&legacy_path).with_context(|| {
            format!(
                "Failed to read legacy memory file {}",
                legacy_path.display()
            )
        })?;

        // Each line is a memory fact
        for (i, line) in legacy_content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Remove leading "- " if present
            let fact = line.strip_prefix("- ").unwrap_or(line);

            // Create a simple memory entry from legacy fact
            entries.push(MemoryEntry {
                id: format!("legacy-{i}"),
                trigger: fact
                    .split_whitespace()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" "),
                confidence: 0.5,
                domain: MemoryDomain::ProjectSpecific,
                source: MemorySource::ManualEntry,
                scope: MemoryScope::Global,
                project_id: None,
                action: fact.to_string(),
                evidence: Vec::new(),
                created_at: SystemTime::now(),
                last_used: None,
                use_count: 0,
            });
        }

        // Migrate to YAML format
        save_entries(&yaml_path, &entries)?;
    }

    Ok(entries)
}

/// Save memory entries to YAML file
pub fn save_entries(path: &Path, entries: &[MemoryEntry]) -> Result<()> {
    // Pre-allocate YAML content string with estimated capacity
    let mut yaml_content = String::with_capacity(entries.len() * 512);

    for entry in entries {
        yaml_content.push_str("---\n");
        yaml_content.push_str(
            &serde_yaml::to_string(entry)
                .with_context(|| format!("Failed to serialize memory entry {}", entry.id))?,
        );
        yaml_content.push('\n');
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create memory directory {}", parent.display()))?;
    }

    // Atomic write: temp file + rename to avoid corruption on crash
    let tmp_path = path.with_extension("yaml.tmp");
    fs::write(&tmp_path, &yaml_content)
        .with_context(|| format!("Failed to write temp file {}", tmp_path.display()))?;
    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e).with_context(|| format!("Failed to rename temp file to {}", path.display()));
    }
    Ok(())
}

/// Threshold that triggers consolidation when entry count exceeds it.
const CONSOLIDATION_THRESHOLD: usize = 100;

/// Add a new memory entry.
///
/// If the total entry count exceeds the consolidation threshold, runs
/// consolidation automatically.
pub fn add_entry(memory_dir: &Path, entry: MemoryEntry) -> Result<()> {
    fs::create_dir_all(memory_dir)
        .with_context(|| format!("Failed to create memory directory {}", memory_dir.display()))?;

    let yaml_path = memory_dir.join("memory.yaml");
    let mut entries = load(memory_dir)?;

    if let Some(existing) = entries.iter_mut().find(|e| e.id == entry.id) {
        *existing = entry;
    } else {
        entries.push(entry);
    }

    save_entries(&yaml_path, &entries)?;

    if entries.len() > CONSOLIDATION_THRESHOLD {
        let engine = consolidation::ConsolidationEngine::new();
        let (consolidated, _) = engine.run(entries);
        save_entries(&yaml_path, &consolidated)?;
    }

    Ok(())
}

/// Run consolidation on the memory store in the given directory.
///
/// Loads all entries, runs the consolidation pipeline, and saves the
/// result back. Returns the consolidation statistics.
pub fn consolidate(memory_dir: &Path) -> Result<consolidation::ConsolidationResult> {
    let entries = load(memory_dir)?;
    let entries_before = entries.len();

    let engine = consolidation::ConsolidationEngine::new();
    let (result_entries, mut result) = engine.run(entries);

    if result.entries_before != result.entries_after {
        let yaml_path = memory_dir.join("memory.yaml");
        save_entries(&yaml_path, &result_entries)?;
    }

    result.entries_before = entries_before;
    result.entries_after = result_entries.len();

    Ok(result)
}

/// Session context returned by `MemoryManager` for session initialization.
#[derive(Debug)]
pub struct SessionContext {
    /// The always-loaded memory index.
    pub index: index::MemoryIndex,
    /// Topic files loaded for this session (empty until explicitly requested).
    pub topic_files: Vec<topic::TopicFile>,
}

/// Facade that ties together the memory index, topic loader, and consolidation.
///
/// This is the primary interface for session lifecycle management.
impl MemoryManager {
    /// Get the session context: the index (always loaded) and any
    /// topic files that should be pre-loaded.
    pub fn get_context_for_session(&self) -> SessionContext {
        SessionContext {
            index: self.index.clone(),
            topic_files: Vec::new(),
        }
    }

    /// Called when a session ends. Records a session note and triggers
    /// consolidation if the memory store is large enough.
    pub fn on_session_end(&mut self, summary: &str, key_learning: &str) -> Result<()> {
        let today = format_date(SystemTime::now());

        self.index.add_session_note(index::SessionNote {
            date: today,
            summary: summary.to_string(),
            key_learning: key_learning.to_string(),
        });

        let index_path = self.memory_dir.join("MEMORY.md");
        self.index.save_to_file(&index_path)?;

        consolidate(&self.memory_dir)?;

        Ok(())
    }

    /// Add a decision to the index and persist it.
    pub fn add_decision(&mut self, decision: index::Decision) -> Result<()> {
        self.index.add_decision(decision);
        let index_path = self.memory_dir.join("MEMORY.md");
        self.index.save_to_file(&index_path)?;
        Ok(())
    }

    /// Load a topic file by keyword match.
    pub fn load_topic_by_keyword(&mut self, keyword: &str) -> Result<Option<topic::TopicFile>> {
        self.topic_loader.load_by_keyword(keyword)
    }

    /// Load a topic file by exact name.
    pub fn load_topic_by_name(&mut self, name: &str) -> Result<Option<topic::TopicFile>> {
        self.topic_loader.load_by_name(name)
    }

    /// Search persisted session transcripts for a keyword or phrase.
    pub fn search_transcripts(&self, query: &str) -> Result<Vec<String>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let Some(sessions_dir) = resolve_sessions_dir(&self.memory_dir) else {
            return Ok(Vec::new());
        };

        let mut matches = Vec::new();
        collect_transcript_matches(&sessions_dir, query, &mut matches)?;
        matches.sort();
        matches.dedup();
        if matches.len() > 20 {
            matches.truncate(20);
        }
        Ok(matches)
    }
}

/// Format a `SystemTime` as an approximate ISO 8601 date string (YYYY-MM-DD).
fn format_date(time: SystemTime) -> String {
    let secs = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days_since_epoch = secs / 86400;
    let year = 1970 + (days_since_epoch / 365);
    let day_of_year = days_since_epoch % 365;
    let month = (day_of_year / 30).clamp(0, 11) + 1;
    let day = (day_of_year % 30).clamp(0, 29) + 1;
    format!("{year:04}-{month:02}-{day:02}")
}

/// Resolve the shared sessions directory from a memory directory.
fn resolve_sessions_dir(memory_dir: &Path) -> Option<PathBuf> {
    memory_dir
        .ancestors()
        .find(|ancestor| ancestor.file_name().and_then(|s| s.to_str()) == Some(".rustycode"))
        .map(|root| root.join("sessions"))
}

/// Recursively collect transcript matches from JSON files under the sessions directory.
fn collect_transcript_matches(dir: &Path, query: &str, matches: &mut Vec<String>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(dir)
        .with_context(|| format!("Failed to read sessions directory {}", dir.display()))?;

    for entry in entries {
        let entry = entry.with_context(|| "Failed to read sessions directory entry")?;
        let path = entry.path();

        if path.is_dir() {
            collect_transcript_matches(&path, query, matches)?;
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read session transcript {}", path.display()))?;

        if let Some(snippet) = transcript_snippet(&content, query) {
            matches.push(format!("{}: {}", path.display(), snippet));
        }
    }

    Ok(())
}

/// Extract a compact snippet for a transcript match.
fn transcript_snippet(content: &str, query: &str) -> Option<String> {
    let query_lower = query.to_lowercase();

    for line in content.lines() {
        if line.to_lowercase().contains(&query_lower) {
            return Some(compact_snippet(line));
        }
    }

    let content_lower = content.to_lowercase();
    let pos = content_lower.find(&query_lower)?;
    let start = pos.saturating_sub(60);
    let end = (pos + query.len() + 60).min(content.len());
    let snippet = content[start..end].replace(['\n', '\r'], " ");
    Some(compact_snippet(&snippet))
}

/// Compact a potentially long line for display.
fn compact_snippet(text: &str) -> String {
    const MAX_CHARS: usize = 180;

    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }

    let mut snippet = trimmed.chars().take(MAX_CHARS - 3).collect::<String>();
    snippet.push_str("...");
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_config::domain::{AutonomyLevel, DomainContext};
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[derive(Serialize)]
    struct LegacyMemoryEntry {
        path: String,
        preview: String,
    }

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-memory-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn memory_root_with_sessions() -> PathBuf {
        let root = temp_dir().join(".rustycode");
        fs::create_dir_all(root.join("memory")).unwrap();
        fs::create_dir_all(root.join("sessions")).unwrap();
        fs::write(root.join("memory").join("MEMORY.md"), "# Memory Index\n").unwrap();
        root
    }

    fn sample_domain() -> DomainContext {
        DomainContext {
            project_name: "sample".to_string(),
            language: "rust".to_string(),
            build_commands: vec!["cargo build".to_string()],
            test_commands: vec!["cargo test".to_string()],
            architecture_rules: vec!["No unwrap".to_string()],
            preferred_patterns: vec!["builder".to_string()],
            test_strategy: Some("unit".to_string()),
            lint_config: None,
            formatter_config: None,
            autonomy_default: AutonomyLevel::L2,
            autonomy_overrides: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn loads_yaml_format() {
        let dir = temp_dir();

        // Create a YAML memory file
        let yaml_content = r#"---
id: test-entry-1
trigger: "when writing async code"
confidence: 0.8
domain: code-style
source: manual-entry
scope: global
project_id: ~
action: "Use async/await pattern for asynchronous code"
evidence: []
created_at: "1740940800"
last_used: ~
use_count: 0
"#;
        let yaml_path = dir.join("memory.yaml");
        fs::write(&yaml_path, yaml_content).unwrap();

        let entries = load(&dir).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "test-entry-1");
        assert_eq!(entries[0].trigger, "when writing async code");
        assert_eq!(entries[0].confidence, 0.8);
    }

    #[test]
    fn migrates_legacy_format() {
        let dir = temp_dir();

        // Create a legacy notes.md file
        let legacy_content = r#"# Project Notes
- User prefers async/await pattern
- Database is PostgreSQL
- Testing uses Jest
"#;
        fs::write(dir.join("notes.md"), legacy_content).unwrap();

        let entries = load(&dir).unwrap();

        // Should migrate 3 facts
        assert!(entries.len() >= 3);

        // YAML file should be created
        assert!(dir.join("memory.yaml").exists());
    }

    #[test]
    fn loads_yaml_without_last_used() {
        let dir = temp_dir();

        // Create a YAML memory file without `last_used` field
        let yaml_content = r#"---
id: test-entry-no-last-used
trigger: "when writing async code"
confidence: 0.7
domain: code-style
source: manual-entry
scope: global
project_id: ~
action: "Use async/await pattern for asynchronous code"
evidence: []
created_at: "1740940800"
use_count: 0
"#;

        let yaml_path = dir.join("memory.yaml");
        fs::write(&yaml_path, yaml_content).unwrap();

        let entries = load(&dir).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "test-entry-no-last-used");
        assert_eq!(entries[0].confidence, 0.7);
        // last_used should be None but parsing must succeed
        assert!(entries[0].last_used.is_none());
    }

    #[test]
    fn add_entry_and_load_roundtrip() {
        let dir = temp_dir();

        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "roundtrip-1".to_string(),
            trigger: "trigger me".to_string(),
            confidence: 0.6,
            domain: MemoryDomain::Testing,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "Do something".to_string(),
        });

        add_entry(&dir, entry.clone()).unwrap();

        let entries = load(&dir).unwrap();
        assert!(!entries.is_empty());
        // Find our entry
        let found = entries.iter().find(|e| e.id == entry.id);
        assert!(found.is_some());
        let f = found.unwrap();
        assert_eq!(f.trigger, "trigger me");
        assert_eq!(f.domain, MemoryDomain::Testing);
    }

    #[test]
    fn confidence_clamping() {
        let config = MemoryEntryConfig {
            id: "test".to_string(),
            trigger: "test trigger".to_string(),
            confidence: 1.5, // Too high
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "test action".to_string(),
        };
        let entry = MemoryEntry::new(config);

        assert_eq!(entry.confidence, 0.9); // Clamped to max
    }

    #[test]
    fn confidence_boost_and_decay() {
        let config = MemoryEntryConfig {
            id: "test".to_string(),
            trigger: "test trigger".to_string(),
            confidence: 0.5,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "test action".to_string(),
        };
        let mut entry = MemoryEntry::new(config);

        entry.boost_confidence(0.2);
        assert!((entry.confidence - 0.7).abs() < 0.001);
        assert_eq!(entry.use_count, 1);

        entry.decay_confidence(0.3);
        assert!((entry.confidence - 0.4).abs() < 0.001);
    }

    #[test]
    fn relevance_calculation() {
        let config = MemoryEntryConfig {
            id: "test".to_string(),
            trigger: "async code".to_string(),
            confidence: 0.8,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "Use async/await".to_string(),
        };
        let entry = MemoryEntry::new(config);

        let score = entry.calculate_relevance("writing async code", &MemoryDomain::CodeStyle);
        assert!(score > 0.0); // Should match domain and trigger
    }

    #[test]
    fn memory_domain_serde_roundtrip() {
        for domain in &[
            MemoryDomain::CodeStyle,
            MemoryDomain::Testing,
            MemoryDomain::Git,
            MemoryDomain::Debugging,
            MemoryDomain::Workflow,
            MemoryDomain::Architecture,
            MemoryDomain::ProjectSpecific,
        ] {
            let yaml = serde_yaml::to_string(domain).unwrap();
            let decoded: MemoryDomain = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(*domain, decoded);
        }
    }

    #[test]
    fn domain_topic_can_be_saved_and_loaded_by_keyword() {
        let root = memory_root_with_sessions();
        let memory_dir = root.join("memory");
        let mut manager = MemoryManager::new(&memory_dir).unwrap();

        let topic = manager.load_domain_context(&sample_domain()).unwrap();
        assert!(topic.is_some());

        let topic = topic.unwrap();
        assert_eq!(topic.name, "domain-context");
        assert!(topic.content.contains("sample"));
        assert!(topic.content.contains("cargo build"));

        let via_keyword = manager.load_topic_by_keyword("domain").unwrap();
        assert!(via_keyword.is_some());
    }

    #[test]
    fn load_domain_context_from_workspace_discovers_yaml() {
        let root = temp_dir();
        let rustycode_dir = root.join(".rustycode");
        fs::create_dir_all(&rustycode_dir).unwrap();
        fs::create_dir_all(rustycode_dir.join("memory")).unwrap();
        fs::create_dir_all(rustycode_dir.join("sessions")).unwrap();
        fs::write(
            rustycode_dir.join("memory").join("MEMORY.md"),
            "# Memory Index\n",
        )
        .unwrap();

        let domain_path = root.join("domain.yaml");
        fs::write(
            &domain_path,
            r#"project_name: discovered
language: go
build_commands:
  - go build ./...
"#,
        )
        .unwrap();

        let mut manager = MemoryManager::new(&rustycode_dir.join("memory")).unwrap();
        let topic = manager.load_domain_context_from_workspace(&root).unwrap();
        assert!(topic.is_some());
        assert!(topic.unwrap().content.contains("discovered"));
    }

    #[test]
    fn memory_scope_serde_roundtrip() {
        let yaml = serde_yaml::to_string(&MemoryScope::Project).unwrap();
        let decoded: MemoryScope = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(MemoryScope::Project, decoded);
    }

    #[test]
    fn memory_source_serde_roundtrip() {
        for source in &[
            MemorySource::SessionObservation,
            MemorySource::UserExplicit,
            MemorySource::ProjectAnalysis,
            MemorySource::ManualEntry,
        ] {
            let yaml = serde_yaml::to_string(source).unwrap();
            let decoded: MemorySource = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(*source, decoded);
        }
    }

    #[test]
    fn confidence_clamped_below_min() {
        let config = MemoryEntryConfig {
            id: "low".to_string(),
            trigger: "t".to_string(),
            confidence: 0.1, // Below min
            domain: MemoryDomain::Debugging,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Project,
            project_id: Some("proj".to_string()),
            action: "a".to_string(),
        };
        let entry = MemoryEntry::new(config);
        assert!((entry.confidence - 0.3).abs() < 0.001);
        assert_eq!(entry.project_id.as_deref(), Some("proj"));
    }

    #[test]
    fn boost_confidence_caps_at_max() {
        let config = MemoryEntryConfig {
            id: "cap".to_string(),
            trigger: "t".to_string(),
            confidence: 0.85,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        };
        let mut entry = MemoryEntry::new(config);
        entry.boost_confidence(0.5);
        assert!((entry.confidence - 0.9).abs() < 0.001); // Capped at 0.9
    }

    #[test]
    fn decay_confidence_floors_at_zero() {
        let config = MemoryEntryConfig {
            id: "floor".to_string(),
            trigger: "t".to_string(),
            confidence: 0.3,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        };
        let mut entry = MemoryEntry::new(config);
        entry.decay_confidence(1.0);
        assert!((entry.confidence - 0.0).abs() < 0.001);
    }

    #[test]
    fn relevance_no_domain_match_lower_score() {
        let config = MemoryEntryConfig {
            id: "cross".to_string(),
            trigger: "async".to_string(),
            confidence: 0.8,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        };
        let entry = MemoryEntry::new(config);
        let score_same = entry.calculate_relevance("async", &MemoryDomain::CodeStyle);
        let score_diff = entry.calculate_relevance("async", &MemoryDomain::Testing);
        assert!(score_same > score_diff);
    }

    #[test]
    fn get_memory_dir_returns_rustycode_path() {
        let cwd = Path::new("/tmp/myproject");
        let dir = get_memory_dir(cwd);
        assert!(dir.starts_with(".rustycode"));
    }

    #[test]
    fn extract_project_name_from_https_url() {
        assert_eq!(
            extract_project_name("https://github.com/user/my-project.git"),
            "my-project"
        );
    }

    #[test]
    fn extract_project_name_without_git_suffix() {
        assert_eq!(
            extract_project_name("https://github.com/user/my-project"),
            "my-project"
        );
    }

    #[test]
    fn extract_project_name_from_ssh_url() {
        assert_eq!(extract_project_name("git@github.com:user/repo.git"), "repo");
    }

    #[test]
    fn extract_project_name_single_component() {
        assert_eq!(extract_project_name("my-repo"), "my-repo");
    }

    #[test]
    fn hash_remote_url_deterministic() {
        let h1 = hash_remote_url("https://github.com/user/repo.git");
        let h2 = hash_remote_url("https://github.com/user/repo.git");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 24); // 12 bytes = 24 hex chars
    }

    #[test]
    fn hash_remote_url_different_urls() {
        let h1 = hash_remote_url("https://github.com/user/repo1.git");
        let h2 = hash_remote_url("https://github.com/user/repo2.git");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_path_deterministic() {
        let h1 = hash_path(Path::new("/tmp/project"));
        let h2 = hash_path(Path::new("/tmp/project"));
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 24);
    }

    #[test]
    fn hash_path_different_paths() {
        let h1 = hash_path(Path::new("/tmp/project-a"));
        let h2 = hash_path(Path::new("/tmp/project-b"));
        assert_ne!(h1, h2);
    }

    #[test]
    fn should_prune_low_confidence_no_last_used() {
        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "prune-test".to_string(),
            trigger: "t".to_string(),
            confidence: 0.2,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        });
        // Below threshold but no last_used — not pruned
        assert!(!entry.should_prune());
    }

    #[test]
    fn should_prune_high_confidence() {
        let mut entry = MemoryEntry::new(MemoryEntryConfig {
            id: "keep".to_string(),
            trigger: "t".to_string(),
            confidence: 0.7,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        });
        entry.last_used = Some(SystemTime::UNIX_EPOCH);
        // High confidence — never pruned regardless of last_used
        assert!(!entry.should_prune());
    }

    #[test]
    fn observation_serialization() {
        let obs = Observation {
            timestamp: SystemTime::UNIX_EPOCH,
            pattern_type: "test-pattern".to_string(),
            description: "observed something".to_string(),
            confidence_boost: 0.1,
        };
        let yaml = serde_yaml::to_string(&obs).unwrap();
        let decoded: Observation = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(decoded.pattern_type, "test-pattern");
        assert_eq!(decoded.description, "observed something");
        assert!((decoded.confidence_boost - 0.1).abs() < 0.001);
    }

    #[test]
    fn legacy_memory_entry_serialization() {
        let entry = LegacyMemoryEntry {
            path: "src/main.rs".to_string(),
            preview: "fn main() {}".to_string(),
        };
        let yaml = serde_yaml::to_string(&entry).unwrap();
        assert!(yaml.contains("src/main.rs"));
        assert!(yaml.contains("fn main()"));
    }

    #[test]
    fn add_entry_updates_existing() {
        let dir = temp_dir();

        let entry1 = MemoryEntry::new(MemoryEntryConfig {
            id: "update-test".to_string(),
            trigger: "old trigger".to_string(),
            confidence: 0.5,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "old action".to_string(),
        });
        add_entry(&dir, entry1).unwrap();

        let entry2 = MemoryEntry::new(MemoryEntryConfig {
            id: "update-test".to_string(),
            trigger: "new trigger".to_string(),
            confidence: 0.7,
            domain: MemoryDomain::Testing,
            source: MemorySource::UserExplicit,
            scope: MemoryScope::Project,
            project_id: Some("proj-1".to_string()),
            action: "new action".to_string(),
        });
        add_entry(&dir, entry2).unwrap();

        let entries = load(&dir).unwrap();
        assert_eq!(entries.len(), 1); // Updated, not duplicated
        assert_eq!(entries[0].trigger, "new trigger");
        assert_eq!(entries[0].confidence, 0.7);
        assert_eq!(entries[0].domain, MemoryDomain::Testing);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = temp_dir();
        let path = dir.join("memory.yaml");

        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "roundtrip-rt".to_string(),
            trigger: "round trip trigger".to_string(),
            confidence: 0.65,
            domain: MemoryDomain::Architecture,
            source: MemorySource::ProjectAnalysis,
            scope: MemoryScope::Project,
            project_id: Some("proj-42".to_string()),
            action: "Refactor module X".to_string(),
        });

        save_entries(&path, std::slice::from_ref(&entry)).unwrap();
        let loaded = load(&dir).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "roundtrip-rt");
        assert_eq!(loaded[0].trigger, "round trip trigger");
        assert_eq!(loaded[0].domain, MemoryDomain::Architecture);
        assert_eq!(loaded[0].source, MemorySource::ProjectAnalysis);
        assert_eq!(loaded[0].scope, MemoryScope::Project);
        assert_eq!(loaded[0].project_id.as_deref(), Some("proj-42"));
        assert_eq!(loaded[0].action, "Refactor module X");
    }

    #[test]
    fn calculate_relevance_no_trigger_match() {
        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "no-match".to_string(),
            trigger: "async code".to_string(),
            confidence: 0.8,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        });
        // Query doesn't contain trigger text
        let score = entry.calculate_relevance("database migrations", &MemoryDomain::CodeStyle);
        // Should still have domain match score but no trigger bonus
        assert!(score > 0.0);
        assert!(score < 0.5); // Domain only, no trigger
    }

    #[test]
    fn memory_entry_config_fields() {
        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "cfg-test".to_string(),
            trigger: "my trigger".to_string(),
            confidence: 0.55,
            domain: MemoryDomain::Workflow,
            source: MemorySource::SessionObservation,
            scope: MemoryScope::Global,
            project_id: None,
            action: "Run tests first".to_string(),
        });
        assert_eq!(entry.id, "cfg-test");
        assert_eq!(entry.trigger, "my trigger");
        assert!((entry.confidence - 0.55).abs() < 0.001);
        assert_eq!(entry.domain, MemoryDomain::Workflow);
        assert_eq!(entry.source, MemorySource::SessionObservation);
        assert!(entry.evidence.is_empty());
        assert_eq!(entry.use_count, 0);
        assert!(entry.last_used.is_none());
    }

    #[test]
    fn project_context_construction() {
        let ctx = ProjectContext {
            id: "abc123".to_string(),
            name: "my-project".to_string(),
            path: PathBuf::from("/home/user/my-project"),
        };
        assert_eq!(ctx.id, "abc123");
        assert_eq!(ctx.name, "my-project");
        assert_eq!(ctx.path, PathBuf::from("/home/user/my-project"));
    }

    #[test]
    fn load_empty_directory() {
        let dir = temp_dir();
        let entries = load(&dir).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn load_nonexistent_directory() {
        let dir = temp_dir().join("no-such-dir");
        let entries = load(&dir).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn boost_confidence_updates_last_used_and_count() {
        let mut entry = MemoryEntry::new(MemoryEntryConfig {
            id: "boost-track".to_string(),
            trigger: "t".to_string(),
            confidence: 0.5,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        });
        assert!(entry.last_used.is_none());
        assert_eq!(entry.use_count, 0);

        entry.boost_confidence(0.1);
        assert!(entry.last_used.is_some());
        assert_eq!(entry.use_count, 1);

        entry.boost_confidence(0.1);
        assert_eq!(entry.use_count, 2);
    }

    #[test]
    fn memory_entry_evidence_with_observations() {
        let mut entry = MemoryEntry::new(MemoryEntryConfig {
            id: "evidence-test".to_string(),
            trigger: "t".to_string(),
            confidence: 0.5,
            domain: MemoryDomain::Testing,
            source: MemorySource::SessionObservation,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        });

        entry.evidence.push(Observation {
            timestamp: SystemTime::now(),
            pattern_type: "test-failure".to_string(),
            description: "Test failed due to missing mock".to_string(),
            confidence_boost: 0.1,
        });

        assert_eq!(entry.evidence.len(), 1);
        assert_eq!(entry.evidence[0].pattern_type, "test-failure");
    }

    #[test]
    fn save_entries_creates_parent_dirs() {
        let dir = temp_dir();
        let nested = dir.join("deep").join("nested").join("path");
        let path = nested.join("memory.yaml");

        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "deep-test".to_string(),
            trigger: "t".to_string(),
            confidence: 0.5,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        });

        save_entries(&path, &[entry]).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn save_entries_empty_list() {
        let dir = temp_dir();
        let path = dir.join("memory.yaml");
        save_entries(&path, &[]).unwrap();
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn decay_confidence_floored_at_0() {
        let mut entry = MemoryEntry::new(MemoryEntryConfig {
            id: "decay-test".to_string(),
            trigger: "t".to_string(),
            confidence: 0.4,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        });
        entry.decay_confidence(0.5); // would go to -0.1
        assert!((entry.confidence - 0.0).abs() < 0.001);
    }

    #[test]
    fn calculate_relevance_domain_and_trigger() {
        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "rel-both".to_string(),
            trigger: "async code".to_string(),
            confidence: 0.8,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        });
        let score = entry.calculate_relevance("async code patterns", &MemoryDomain::CodeStyle);
        // Both domain match (0.5) and trigger match (0.3) = 0.8 * confidence 0.8 = 0.64
        assert!(score > 0.5);
    }

    #[test]
    fn hash_different_urls_different_hashes() {
        let hash1 = hash_remote_url("https://github.com/user/repo-a.git");
        let hash2 = hash_remote_url("https://github.com/user/repo-b.git");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn add_entry_to_directory() {
        let dir = temp_dir();
        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "add-test".to_string(),
            trigger: "t".to_string(),
            confidence: 0.6,
            domain: MemoryDomain::Workflow,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "Do the thing".to_string(),
        });

        add_entry(&dir, entry).unwrap();
        let entries = load(&dir).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "add-test");
    }

    #[test]
    fn detect_project_context_in_git_repo() {
        // This test runs from the rustycode repo itself, so it should detect a context
        let cwd = Path::new(".");
        let ctx = detect_project_context(cwd);
        assert!(ctx.is_some());
        let ctx = ctx.unwrap();
        assert!(!ctx.id.is_empty());
        assert!(!ctx.name.is_empty());
    }

    #[test]
    fn detect_project_context_nonexistent_dir() {
        // Non-existent directory should return None (no git context)
        let cwd = Path::new("/nonexistent/path/that/does/not/exist");
        let ctx = detect_project_context(cwd);
        assert!(ctx.is_none());
    }

    #[test]
    fn load_nonexistent_dir_returns_empty() {
        let entries = load(Path::new("/nonexistent/dir/memory")).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn save_entries_creates_deeply_nested_parent_dirs() {
        let dir = temp_dir();
        let nested = dir.join("a").join("b").join("c");
        let path = nested.join("memory.yaml");

        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "nested-test".to_string(),
            trigger: "nested".to_string(),
            confidence: 0.6,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "nested save".to_string(),
        });

        save_entries(&path, &[entry]).unwrap();
        assert!(path.exists());

        let loaded = load(&nested).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "nested-test");
    }

    #[test]
    fn save_entries_atomic_no_leftover_tmp() {
        let dir = temp_dir();
        let path = dir.join("memory.yaml");
        let entry = MemoryEntry::new(MemoryEntryConfig {
            id: "atomic-test".to_string(),
            trigger: "t".to_string(),
            confidence: 0.5,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a".to_string(),
        });

        save_entries(&path, &[entry]).unwrap();

        // No temp file should remain after successful save
        let tmp_path = dir.join("memory.yaml.tmp");
        assert!(
            !tmp_path.exists(),
            "temp file should be cleaned up after rename"
        );
        assert!(path.exists());
    }

    #[test]
    fn save_entries_overwrite_preserves_data() {
        let dir = temp_dir();
        let path = dir.join("memory.yaml");

        let entry1 = MemoryEntry::new(MemoryEntryConfig {
            id: "first".to_string(),
            trigger: "t1".to_string(),
            confidence: 0.5,
            domain: MemoryDomain::CodeStyle,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a1".to_string(),
        });

        save_entries(&path, &[entry1]).unwrap();
        assert_eq!(load(&dir).unwrap().len(), 1);

        let entry2 = MemoryEntry::new(MemoryEntryConfig {
            id: "second".to_string(),
            trigger: "t2".to_string(),
            confidence: 0.7,
            domain: MemoryDomain::Workflow,
            source: MemorySource::ManualEntry,
            scope: MemoryScope::Global,
            project_id: None,
            action: "a2".to_string(),
        });

        save_entries(&path, &[entry2]).unwrap();
        let loaded = load(&dir).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "second");
    }

    #[test]
    fn search_transcripts_finds_session_state_json() {
        let root = memory_root_with_sessions();
        let memory_dir = root.join("memory");
        let session_dir = root.join("sessions").join("session-1");
        fs::create_dir_all(&session_dir).unwrap();

        let state_json = r#"{
  "session_id": "session-1",
  "created_at": "2026-04-25T00:00:00Z",
  "last_saved": "2026-04-25T00:10:00Z",
  "messages": [
    {
      "id": "1",
      "role": "user",
      "content": "Please fix the memory architecture search"
    }
  ],
  "scroll_position": 0,
  "preferences": {
    "tools_expanded": false,
    "thinking_expanded": false
  }
}"#;
        fs::write(session_dir.join("state.json"), state_json).unwrap();

        let manager = MemoryManager::new(&memory_dir).unwrap();
        let results = manager.search_transcripts("memory architecture").unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].contains("state.json"));
        assert!(results[0].contains("memory architecture"));
    }

    #[test]
    fn search_transcripts_ignores_missing_sessions_dir() {
        let root = temp_dir().join(".rustycode");
        fs::create_dir_all(root.join("memory")).unwrap();
        fs::write(root.join("memory").join("MEMORY.md"), "# Memory Index\n").unwrap();

        let manager = MemoryManager::new(&root.join("memory")).unwrap();
        let results = manager.search_transcripts("anything").unwrap();

        assert!(results.is_empty());
    }
}
