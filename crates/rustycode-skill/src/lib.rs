#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::similar_names,
        clippy::float_cmp,
        clippy::redundant_clone,
        clippy::uninlined_format_args
    )
)]
#![cfg_attr(test, allow(unused_mut))]

//! Skills system with caching and progressive loading support.
//!
//! This module provides:
//! - Skill discovery from SKILL.md files
//! - In-memory caching with TTL support
//! - Progressive loading: metadata-only first, full content on-demand
//! - Relevance scoring for context-aware skill selection
//!
//! # Enforceable Workflows
//!
//! The `workflows` module provides structured, enforceable workflows
//! that Claude must follow (e.g., TDD: RED->GREEN->REFACTOR).

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, trace};

/// Heading regex compiled once for all skill loading paths.
static HEADING_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"(?m)^#\s+(.+)$").expect("invalid heading regex")
});

pub mod activation;
pub mod adapter;
pub mod budget;
pub mod bundled;
pub mod checklist;
pub mod curator;
pub mod discovery;
pub mod exclusions;
pub mod gotchas;
pub mod graph;
pub mod improvement;
pub mod lifecycle;
pub mod manager;
pub mod metadata;
pub mod procedure;
pub mod quality;
pub mod registry;
pub mod scoping;
pub mod types;
pub mod watcher;
pub mod workflows;

pub use budget::{BudgetEnforcer, ContextBudget, SkillBudgetEntry};
pub use discovery::Discovery;
pub use registry::SkillRegistry;
pub use scoping::{resolve_allowed_tools, scope_from_definition, SkillToolScope};

/// Metadata about a discovered skill
#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub name: String,
    pub path: String,
    pub description: Option<String>,
}

impl From<types::SkillDefinition> for Skill {
    fn from(def: types::SkillDefinition) -> Self {
        Self {
            name: def.name,
            path: def.content_path.to_string_lossy().to_string(),
            description: Some(def.description),
        }
    }
}

/// Extended metadata for progressive loading (lightweight, ~100 bytes per skill)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    /// Keywords that trigger this skill
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Mode: "code", "plan", "debug", etc.
    pub mode: Option<String>,
    /// Priority 1-10 (lower = higher priority)
    #[serde(default = "default_priority")]
    pub priority: u8,
    /// Skill version
    #[serde(default)]
    pub version: String,
    /// Path to the skill content file
    #[serde(skip)]
    pub content_path: PathBuf,
    /// Tools this skill is allowed to use (empty = all tools)
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Effort level required
    pub effort: Option<EffortLevel>,
    /// Argument hint for display
    pub argument_hint: Option<String>,
    /// Whether user can invoke this skill directly
    #[serde(default = "default_true")]
    pub user_invocable: bool,
    /// Categories for routing
    #[serde(default)]
    pub categories: Vec<String>,
}

const fn default_priority() -> u8 {
    5
}

/// High-level effort indicator for a skill
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffortLevel {
    Low,
    Medium,
    High,
}

const fn default_true() -> bool {
    true
}

/// Cache entry with expiration tracking
#[derive(Debug, Clone)]
struct CacheEntry {
    skills: Vec<Skill>,
    cached_at: Instant,
}

impl CacheEntry {
    fn is_expired(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }
}

/// Skill manager with caching
pub struct SkillManager {
    skills_dir: PathBuf,
    cache_ttl: Duration,
    cache: RwLock<Option<CacheEntry>>,
}

impl SkillManager {
    pub async fn new(skills_dir: &Path, cache_ttl: Duration) -> Result<Self> {
        let manager = Self {
            skills_dir: skills_dir.to_path_buf(),
            cache_ttl,
            cache: RwLock::new(None),
        };
        manager.refresh_cache().await?;
        Ok(manager)
    }

    pub async fn skills(&self) -> Result<Vec<Skill>> {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.as_ref() {
            if !entry.is_expired(self.cache_ttl) {
                trace!("Returning cached skills ({} skills)", entry.skills.len());
                return Ok(entry.skills.clone());
            }
        }
        drop(cache);
        self.refresh_cache().await
    }

    pub async fn skills_refreshed(&self) -> Result<Vec<Skill>> {
        self.refresh_cache().await
    }

    async fn refresh_cache(&self) -> Result<Vec<Skill>> {
        debug!("Refreshing skills cache from {:?}", self.skills_dir);
        let skills = self.discover_skills().await?;
        let entry = CacheEntry {
            skills: skills.clone(),
            cached_at: Instant::now(),
        };
        *self.cache.write().await = Some(entry);
        debug!("Cache refreshed with {} skills", skills.len());
        Ok(skills)
    }

    async fn discover_skills(&self) -> Result<Vec<Skill>> {
        if !self.skills_dir.exists() {
            return Ok(Vec::new());
        }

        let mut skills = Vec::new();

        let mut entries = tokio::fs::read_dir(&self.skills_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                let skill_md = entry_path.join("SKILL.md");
                if skill_md.exists() {
                    if let Some(skill) = Self::load_skill(&skill_md).await {
                        skills.push(skill);
                    }
                }
            }
        }

        skills.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(skills)
    }

    async fn load_skill(path: &Path) -> Option<Skill> {
        let content = fs::read_to_string(path).await.ok()?;

        let name = HEADING_RE
            .captures(&content)
            .and_then(|captures| captures.get(1))
            .map_or_else(
                || {
                    path.parent().and_then(|p| p.file_name()).map_or_else(
                        || "unknown".to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    )
                },
                |m| m.as_str().trim().to_string(),
            );

        let description_opt = content
            .split('\n')
            .skip_while(|line: &&str| !line.starts_with('#'))
            .skip(1)
            .find(|line: &&str| !line.trim().is_empty())
            .map(|s: &str| s.trim().to_string());

        Some(Skill {
            name,
            path: path.display().to_string(),
            description: description_opt,
        })
    }

    pub async fn cached_count(&self) -> usize {
        self.cache
            .read()
            .await
            .as_ref()
            .map_or(0, |entry| entry.skills.len())
    }

    pub async fn is_cache_expired(&self) -> bool {
        self.cache
            .read()
            .await
            .as_ref()
            .is_none_or(|entry| entry.is_expired(self.cache_ttl))
    }
}

/// Progressive skill loader -- loads metadata first, content on-demand.
///
/// For 100 skills: metadata ~10KB, full content ~20MB. This loader
/// keeps metadata in memory and loads content only when needed.
pub struct ProgressiveLoader {
    skills_dir: PathBuf,
    /// Lightweight metadata (always loaded)
    metadata_cache: HashMap<String, SkillMetadata>,
    /// Full skill content (loaded on-demand)
    content_cache: HashMap<String, String>,
    /// Insertion-order keys for FIFO eviction
    content_cache_order: Vec<String>,
    /// Maximum content cache entries
    max_content_cache: usize,
}

impl ProgressiveLoader {
    pub fn new(skills_dir: PathBuf, max_content_cache: usize) -> Self {
        Self {
            skills_dir,
            metadata_cache: HashMap::new(),
            content_cache: HashMap::new(),
            content_cache_order: Vec::new(),
            max_content_cache,
        }
    }

    /// Load metadata for all discovered skills (fast, ~10KB for 100 skills)
    pub async fn load_metadata(&mut self) -> Result<Vec<&SkillMetadata>> {
        self.metadata_cache.clear();

        if !self.skills_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = fs::read_dir(&self.skills_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let entry_path = entry.path();
            if !entry_path.is_dir() {
                continue;
            }

            let skill_md = entry_path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            let Ok(content) = fs::read_to_string(&skill_md).await else {
                continue;
            };

            let name = HEADING_RE
                .captures(&content)
                .and_then(|captures| captures.get(1))
                .map_or_else(
                    || {
                        entry_path.file_name().map_or_else(
                            || "unknown".to_string(),
                            |n| n.to_string_lossy().into_owned(),
                        )
                    },
                    |m| m.as_str().trim().to_string(),
                );

            let description = content
                .lines()
                .skip_while(|line| !line.starts_with('#'))
                .skip(1)
                .find(|line| !line.trim().is_empty())
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            // Extract triggers from "Trigger:" or "Keywords:" lines
            let triggers = Self::extract_triggers(&content);

            // Extract mode from "Mode:" line
            let mode = content
                .lines()
                .find(|l| l.to_lowercase().starts_with("mode:"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string());

            self.metadata_cache.insert(
                name.clone(),
                SkillMetadata {
                    name,
                    description,
                    triggers,
                    mode,
                    priority: 5,
                    version: String::new(),
                    content_path: skill_md,
                    allowed_tools: Vec::new(),
                    effort: None,
                    argument_hint: None,
                    user_invocable: true,
                    categories: Vec::new(),
                },
            );
        }

        Ok(self.metadata_cache.values().collect())
    }

    /// Find skills relevant to the given context (metadata-based, no content loading)
    pub fn find_relevant(&self, context: &str, max_results: usize) -> Vec<&SkillMetadata> {
        let context_lower = context.to_lowercase();

        let mut scored: Vec<_> = self
            .metadata_cache
            .values()
            .map(|meta| {
                let score = Self::relevance_score(meta, &context_lower);
                (meta, score)
            })
            .filter(|(_, score)| *score > 0.5)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(max_results)
            .map(|(m, _)| m)
            .collect()
    }

    /// Load full skill content on-demand (lazy loading)
    pub async fn load_skill(&mut self, name: &str) -> Result<Option<String>> {
        // Check content cache first
        if let Some(content) = self.content_cache.get(name) {
            return Ok(Some(content.clone()));
        }

        let Some(meta) = self.metadata_cache.get(name) else {
            return Ok(None);
        };

        let content = fs::read_to_string(&meta.content_path).await?;

        // FIFO eviction: remove oldest entries first
        let excess = self
            .content_cache
            .len()
            .saturating_sub(self.max_content_cache - 1);
        if excess > 0 {
            let keys: Vec<String> = self
                .content_cache_order
                .iter()
                .take(excess)
                .cloned()
                .collect();
            for key in &keys {
                self.content_cache.remove(key);
            }
            self.content_cache_order.drain(0..keys.len());
        }

        self.content_cache_order.push(name.to_string());
        self.content_cache.insert(name.to_string(), content.clone());
        Ok(Some(content))
    }

    /// Get all loaded metadata
    pub fn all_metadata(&self) -> Vec<&SkillMetadata> {
        self.metadata_cache.values().collect()
    }

    /// Get metadata for a specific skill
    pub fn metadata(&self, name: &str) -> Option<&SkillMetadata> {
        self.metadata_cache.get(name)
    }

    /// Number of skills with loaded metadata
    pub fn metadata_count(&self) -> usize {
        self.metadata_cache.len()
    }

    /// Number of skills with loaded content
    pub fn content_count(&self) -> usize {
        self.content_cache.len()
    }

    /// Calculate relevance score for a skill given a context
    fn relevance_score(skill: &SkillMetadata, context_lower: &str) -> f32 {
        let mut score = 0.0f32;

        // Trigger keyword matching
        for trigger in &skill.triggers {
            if context_lower.contains(&trigger.to_lowercase()) {
                score += 1.0;
            }
        }

        // Name matching
        if context_lower.contains(&skill.name.to_lowercase()) {
            score += 2.0;
        }

        // Description word overlap
        for word in skill.description.to_lowercase().split_whitespace() {
            if word.len() > 3 && context_lower.contains(word) {
                score += 0.3;
            }
        }

        // Priority bonus (lower priority number = higher relevance)
        let priority_bonus = f32::from(10 - skill.priority) * 0.1;
        score += priority_bonus;

        score
    }

    /// Extract trigger keywords from skill content
    fn extract_triggers(content: &str) -> Vec<String> {
        let mut triggers = Vec::new();

        for line in content.lines() {
            let line_lower = line.to_lowercase();
            if line_lower.starts_with("trigger:")
                || line_lower.starts_with("keywords:")
                || line_lower.starts_with("triggers:")
            {
                if let Some(value) = line.split(':').nth(1) {
                    for keyword in value.split(',') {
                        let trimmed = keyword.trim().to_string();
                        if !trimmed.is_empty() {
                            triggers.push(trimmed);
                        }
                    }
                }
            }
        }

        triggers
    }
}

/// Synchronous skill discovery (legacy API)
pub fn discover(skills_dir: &Path) -> Result<Vec<Skill>> {
    // Synchronous discovery path (legacy API).
    // Perform discovery using blocking std::fs APIs to avoid creating
    // a Tokio runtime inside callers that may already be running one.
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();

    for entry in std::fs::read_dir(skills_dir)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            let skill_md = entry_path.join("SKILL.md");
            if skill_md.exists() {
                if let Ok(content) = std::fs::read_to_string(&skill_md) {
                    let name = HEADING_RE
                        .captures(&content)
                        .and_then(|captures| captures.get(1))
                        .map_or_else(
                            || {
                                skill_md.parent().and_then(|p| p.file_name()).map_or_else(
                                    || "unknown".to_string(),
                                    |n| n.to_string_lossy().into_owned(),
                                )
                            },
                            |m| m.as_str().trim().to_string(),
                        );

                    let description_opt = content
                        .split('\n')
                        .skip_while(|line: &&str| !line.starts_with('#'))
                        .skip(1)
                        .find(|line: &&str| !line.trim().is_empty())
                        .map(|s: &str| s.trim().to_string());

                    skills.push(Skill {
                        name,
                        path: skill_md.display().to_string(),
                        description: description_opt,
                    });
                }
            }
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-skill-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[tokio::test]
    async fn discovers_skill_metadata_from_markdown() {
        let dir = temp_dir();
        let skill_dir = dir.join("writer");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Writer\n\nProduces concise implementation notes.\n",
        )
        .unwrap();

        let manager = SkillManager::new(&dir, Duration::from_mins(1))
            .await
            .unwrap();
        let skills = manager.skills().await.unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "Writer");
        assert_eq!(
            skills[0].description.as_deref(),
            Some("Produces concise implementation notes.")
        );
    }

    #[tokio::test]
    async fn caches_skills_within_ttl() {
        let dir = temp_dir();
        let skill_dir = dir.join("writer");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Writer\n\nA test skill.\n").unwrap();

        let manager = SkillManager::new(&dir, Duration::from_secs(1))
            .await
            .unwrap();
        let _skills1 = manager.skills().await.unwrap();
        assert!(!manager.is_cache_expired().await);
    }

    #[tokio::test]
    async fn handles_nonexistent_directory() {
        let dir = temp_dir().join("nonexistent");
        let manager = SkillManager::new(&dir, Duration::from_mins(1))
            .await
            .unwrap();
        let skills = manager.skills().await.unwrap();
        assert_eq!(skills.len(), 0);
    }

    #[tokio::test]
    async fn loads_multiple_skills() {
        let dir = temp_dir();
        for i in 1..=3 {
            let skill_dir = dir.join(format!("skill{i}"));
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!("# Skill {i}\n\nDescription {i}.\n"),
            )
            .unwrap();
        }

        let manager = SkillManager::new(&dir, Duration::from_mins(1))
            .await
            .unwrap();
        let skills = manager.skills().await.unwrap();
        assert_eq!(skills.len(), 3);
    }

    #[test]
    fn sync_discover_finds_skills() {
        let dir = temp_dir();
        let skill_dir = dir.join("reader");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Reader\n\nReads files efficiently.\n",
        )
        .unwrap();

        let skills = discover(&dir).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "Reader");
        assert_eq!(
            skills[0].description.as_deref(),
            Some("Reads files efficiently.")
        );
    }

    #[test]
    fn sync_discover_nonexistent_dir() {
        let skills = discover(Path::new("/nonexistent/path")).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn sync_discover_ignores_files_without_skill_md() {
        let dir = temp_dir();
        let skill_dir = dir.join("noskill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("README.md"), "# Not a skill\n").unwrap();

        let skills = discover(&dir).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn sync_discover_uses_directory_name_when_no_heading() {
        let dir = temp_dir();
        let skill_dir = dir.join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "No heading here\n").unwrap();

        let skills = discover(&dir).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-skill");
    }

    #[test]
    fn sync_discover_sorts_alphabetically() {
        let dir = temp_dir();
        for name in &["charlie", "alpha", "bravo"] {
            let skill_dir = dir.join(name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!("# {name}\n\nA skill.\n"),
            )
            .unwrap();
        }

        let skills = discover(&dir).unwrap();
        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0].name, "alpha");
        assert_eq!(skills[1].name, "bravo");
        assert_eq!(skills[2].name, "charlie");
    }

    #[tokio::test]
    async fn cache_expires_after_ttl() {
        let dir = temp_dir();
        let skill_dir = dir.join("writer");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Writer\n\nA test skill.\n").unwrap();

        let manager = SkillManager::new(&dir, Duration::from_millis(50))
            .await
            .unwrap();

        assert!(!manager.is_cache_expired().await);
        assert_eq!(manager.cached_count().await, 1);

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(manager.is_cache_expired().await);
    }

    #[tokio::test]
    async fn get_skills_refreshed_bypasses_cache() {
        let dir = temp_dir();
        let skill_dir = dir.join("writer");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Writer\n\nV1.\n").unwrap();

        let manager = SkillManager::new(&dir, Duration::from_mins(1))
            .await
            .unwrap();

        // Update the skill
        fs::write(skill_dir.join("SKILL.md"), "# Writer\n\nV2.\n").unwrap();

        let refreshed = manager.skills_refreshed().await.unwrap();
        assert_eq!(refreshed[0].description.as_deref(), Some("V2."));
    }

    #[test]
    fn skill_serialization_roundtrip() {
        let skill = Skill {
            name: "test-skill".to_string(),
            path: "/some/path/SKILL.md".to_string(),
            description: Some("A test skill.".to_string()),
        };
        let json = serde_json::to_string(&skill).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "test-skill");
        assert_eq!(parsed["path"], "/some/path/SKILL.md");
        assert_eq!(parsed["description"], "A test skill.");
    }

    #[test]
    fn skill_serialization_no_description() {
        let skill = Skill {
            name: "bare".to_string(),
            path: "/bare/SKILL.md".to_string(),
            description: None,
        };
        let json = serde_json::to_string(&skill).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["description"], serde_json::Value::Null);
    }

    #[test]
    fn sync_discover_multiline_description() {
        let dir = temp_dir();
        let skill_dir = dir.join("multi");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Multi\n\nFirst paragraph is the description.\n\nSecond paragraph is not.",
        )
        .unwrap();

        let skills = discover(&dir).unwrap();
        assert_eq!(
            skills[0].description.as_deref(),
            Some("First paragraph is the description.")
        );
    }

    #[test]
    fn sync_discover_skill_with_no_description_body() {
        let dir = temp_dir();
        let skill_dir = dir.join("heading-only");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# JustAHeading\n").unwrap();

        let skills = discover(&dir).unwrap();
        assert_eq!(skills[0].name, "JustAHeading");
        assert!(skills[0].description.is_none());
    }

    #[test]
    fn sync_discover_empty_skill_md() {
        let dir = temp_dir();
        let skill_dir = dir.join("empty");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "").unwrap();

        let skills = discover(&dir).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "empty");
        assert!(skills[0].description.is_none());
    }

    #[test]
    fn sync_discover_heading_with_extra_whitespace() {
        let dir = temp_dir();
        let skill_dir = dir.join("ws-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "#   Trimmed Heading  \n\n  Description with spaces  \n",
        )
        .unwrap();

        let skills = discover(&dir).unwrap();
        assert_eq!(skills[0].name, "Trimmed Heading");
        assert_eq!(
            skills[0].description.as_deref(),
            Some("Description with spaces")
        );
    }

    #[test]
    fn sync_discover_ignores_regular_files() {
        let dir = temp_dir();
        // Put a file (not directory) in the skills dir
        fs::write(dir.join("not-a-dir.md"), "# Not a dir\n").unwrap();
        let skills = discover(&dir).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn sync_discover_nested_dir_without_skill_md() {
        let dir = temp_dir();
        let nested = dir.join("nested");
        fs::create_dir_all(nested.join("sub")).unwrap();
        // nested has no SKILL.md, nested/sub has no SKILL.md either
        fs::write(nested.join("sub").join("other.txt"), "hello").unwrap();

        let skills = discover(&dir).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn skill_path_points_to_skill_md() {
        let dir = temp_dir();
        let skill_dir = dir.join("path-test");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# PathTest\n\nTest.\n").unwrap();

        let skills = discover(&dir).unwrap();
        assert!(skills[0].path.ends_with("SKILL.md"));
        assert!(skills[0].path.contains("path-test"));
    }

    #[test]
    fn skill_debug_format() {
        let skill = Skill {
            name: "debug-test".to_string(),
            path: "/test/SKILL.md".to_string(),
            description: Some("test desc".to_string()),
        };
        let debug_str = format!("{skill:?}");
        assert!(debug_str.contains("debug-test"));
        assert!(debug_str.contains("test desc"));
    }

    #[test]
    fn skill_clone() {
        let skill = Skill {
            name: "clone-me".to_string(),
            path: "/path/SKILL.md".to_string(),
            description: Some("desc".to_string()),
        };
        let cloned = skill.clone();
        assert_eq!(cloned.name, "clone-me");
        assert_eq!(cloned.path, "/path/SKILL.md");
        assert_eq!(cloned.description, Some("desc".to_string()));
    }

    #[test]
    fn skill_serialization_roundtrip_with_description() {
        let skill = Skill {
            name: "roundtrip".to_string(),
            path: "/roundtrip/SKILL.md".to_string(),
            description: Some("full roundtrip".to_string()),
        };
        let json = serde_json::to_string(&skill).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "roundtrip");
        assert_eq!(parsed["path"], "/roundtrip/SKILL.md");
        assert_eq!(parsed["description"], "full roundtrip");
    }

    #[test]
    fn sync_discover_multiple_headings_uses_first() {
        let dir = temp_dir();
        let skill_dir = dir.join("multi-heading");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# First Heading\n\nDescription here.\n\n## Sub Heading\n\nMore content.\n",
        )
        .unwrap();

        let skills = discover(&dir).unwrap();
        assert_eq!(skills[0].name, "First Heading");
        assert_eq!(skills[0].description.as_deref(), Some("Description here."));
    }

    #[tokio::test]
    async fn cached_count_on_empty_dir() {
        let dir = temp_dir().join("empty-skills");
        let manager = SkillManager::new(&dir, Duration::from_mins(1))
            .await
            .unwrap();
        assert_eq!(manager.cached_count().await, 0);
    }

    #[tokio::test]
    async fn is_cache_expired_on_new_manager() {
        let dir = temp_dir();
        let skill_dir = dir.join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Test\n\nDesc.\n").unwrap();

        let manager = SkillManager::new(&dir, Duration::from_mins(1))
            .await
            .unwrap();
        assert!(!manager.is_cache_expired().await);
    }

    #[tokio::test]
    async fn get_skills_returns_sorted() {
        let dir = temp_dir();
        for name in &["zebra", "alpha", "middle"] {
            let skill_dir = dir.join(name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join("SKILL.md"), format!("# {name}\n\nDesc.\n")).unwrap();
        }

        let manager = SkillManager::new(&dir, Duration::from_mins(1))
            .await
            .unwrap();
        let skills = manager.skills().await.unwrap();
        assert_eq!(skills[0].name, "alpha");
        assert_eq!(skills[1].name, "middle");
        assert_eq!(skills[2].name, "zebra");
    }

    #[tokio::test]
    async fn async_discovers_skill_without_heading_uses_dir_name() {
        let dir = temp_dir();
        let skill_dir = dir.join("fallback-name");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "No heading here\n").unwrap();

        let manager = SkillManager::new(&dir, Duration::from_mins(1))
            .await
            .unwrap();
        let skills = manager.skills().await.unwrap();
        assert_eq!(skills[0].name, "fallback-name");
    }

    // Progressive loader tests

    #[tokio::test]
    async fn progressive_load_metadata() {
        let dir = temp_dir();
        let skill_dir = dir.join("writer");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Writer\n\nWrites code.\n\nTriggers: code, implement, write\n",
        )
        .unwrap();

        let mut loader = ProgressiveLoader::new(dir, 10);
        let metadata = loader.load_metadata().await.unwrap();
        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].name, "Writer");
        assert_eq!(metadata[0].description, "Writes code.");
    }

    #[tokio::test]
    async fn progressive_find_relevant() {
        let dir = temp_dir();
        let skill_dir = dir.join("tester");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Tester\n\nRuns tests.\n\nTriggers: test, verify, validate\n",
        )
        .unwrap();

        let skill_dir2 = dir.join("deployer");
        fs::create_dir_all(&skill_dir2).unwrap();
        fs::write(skill_dir2.join("SKILL.md"), "# Deployer\n\nDeploys code.\n").unwrap();

        let mut loader = ProgressiveLoader::new(dir, 10);
        loader.load_metadata().await.unwrap();

        let relevant = loader.find_relevant("write a test for the login function", 5);
        assert_eq!(relevant.len(), 1);
        assert_eq!(relevant[0].name, "Tester");
    }

    #[tokio::test]
    async fn progressive_load_skill_content() {
        let dir = temp_dir();
        let skill_dir = dir.join("writer");
        fs::create_dir_all(&skill_dir).unwrap();
        let content = "# Writer\n\nThis is the full skill content.\n\nStep 1: Do stuff\n";
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        let mut loader = ProgressiveLoader::new(dir, 10);
        loader.load_metadata().await.unwrap();

        // Content not loaded yet
        assert_eq!(loader.content_count(), 0);

        // Load on-demand
        let loaded = loader.load_skill("Writer").await.unwrap();
        assert!(loaded.is_some());
        assert!(loaded.unwrap().contains("full skill content"));
        assert_eq!(loader.content_count(), 1);
    }

    #[tokio::test]
    async fn progressive_load_skill_not_found() {
        let dir = temp_dir();
        let mut loader = ProgressiveLoader::new(dir, 10);
        loader.load_metadata().await.unwrap();

        let result = loader.load_skill("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn progressive_content_cache_eviction() {
        let dir = temp_dir();
        for i in 0..5 {
            let skill_dir = dir.join(format!("skill{i}"));
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!("# Skill{i}\n\nContent {i}\n"),
            )
            .unwrap();
        }

        let mut loader = ProgressiveLoader::new(dir, 3);
        loader.load_metadata().await.unwrap();

        // Load more than max_content_cache
        for i in 0..5 {
            loader.load_skill(&format!("Skill{i}")).await.unwrap();
        }

        assert!(loader.content_count() <= 3);
    }

    #[tokio::test]
    async fn progressive_metadata_count() {
        let dir = temp_dir();
        let mut loader = ProgressiveLoader::new(dir, 10);
        loader.load_metadata().await.unwrap();
        assert_eq!(loader.metadata_count(), 0);
    }

    #[test]
    fn progressive_extract_triggers() {
        let content = "# Test\n\nTriggers: code, write, debug\nMode: code\n";
        let triggers = ProgressiveLoader::extract_triggers(content);
        assert_eq!(triggers, vec!["code", "write", "debug"]);
    }

    #[test]
    fn progressive_extract_triggers_keywords() {
        let content = "# Test\n\nKeywords: rust, cargo\n";
        let triggers = ProgressiveLoader::extract_triggers(content);
        assert_eq!(triggers, vec!["rust", "cargo"]);
    }

    #[test]
    fn progressive_extract_triggers_empty() {
        let content = "# Test\n\nNo triggers here.\n";
        let triggers = ProgressiveLoader::extract_triggers(content);
        assert!(triggers.is_empty());
    }

    #[test]
    fn progressive_get_metadata() {
        let dir = temp_dir();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut loader = ProgressiveLoader::new(dir, 10);
        rt.block_on(async {
            loader.load_metadata().await.unwrap();
        });
        assert!(loader.metadata("nonexistent").is_none());
    }
}
