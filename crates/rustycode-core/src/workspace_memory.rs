//! Persistent workspace memory with file-based storage and relevance scanning.
//!
//! Stores markdown files with frontmatter metadata, scans for relevance on
//! query, and prunes stale entries by age.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Memory type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryType {
    User,
    Feedback,
    Project,
    Reference,
}

/// Frontmatter for a memory file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFrontmatter {
    pub name: String,
    #[serde(default = "default_description")]
    pub description: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

fn default_description() -> String {
    String::new()
}

/// A single memory entry.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub frontmatter: MemoryFrontmatter,
    pub content: String,
    pub path: PathBuf,
}

/// Manager for persistent workspace memories.
pub struct WorkspaceMemory {
    memory_dir: PathBuf,
    entries: HashMap<String, MemoryEntry>,
}

impl WorkspaceMemory {
    /// Create or load workspace memory from a directory.
    pub fn new(memory_dir: &Path) -> Result<Self> {
        fs::create_dir_all(memory_dir)
            .with_context(|| format!("Failed to create memory dir: {}", memory_dir.display()))?;

        let mut mgr = Self {
            memory_dir: memory_dir.to_path_buf(),
            entries: HashMap::new(),
        };
        mgr.reload()?;
        Ok(mgr)
    }

    /// Reload all memory entries from disk.
    pub fn reload(&mut self) -> Result<()> {
        self.entries.clear();
        let entries = fs::read_dir(&self.memory_dir)
            .with_context(|| format!("Failed to read memory dir: {}", self.memory_dir.display()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(mem) = self.load_entry(name, &path) {
                        self.entries.insert(name.to_string(), mem);
                    }
                }
            }
        }
        Ok(())
    }

    /// Write a memory entry. Creates or updates the file on disk.
    pub fn write(
        &mut self,
        name: &str,
        memory_type: MemoryType,
        description: &str,
        content: &str,
        tags: Vec<String>,
    ) -> Result<()> {
        let now = Utc::now();
        let created_at = self
            .entries
            .get(name)
            .map(|e| e.frontmatter.created_at)
            .unwrap_or(now);

        let frontmatter = MemoryFrontmatter {
            name: name.to_string(),
            description: description.to_string(),
            memory_type,
            tags,
            created_at,
            updated_at: now,
        };

        let file_content = format!(
            "---\n{}---\n\n{content}",
            serde_yaml::to_string(&frontmatter).unwrap_or_default()
        );

        let filename = format!("{name}.md");
        let path = self.memory_dir.join(&filename);
        fs::write(&path, &file_content)
            .with_context(|| format!("Failed to write memory: {}", path.display()))?;

        let entry = MemoryEntry {
            frontmatter,
            content: content.to_string(),
            path,
        };
        self.entries.insert(name.to_string(), entry);

        Ok(())
    }

    /// Read a memory entry by name.
    pub fn read(&self, name: &str) -> Option<&MemoryEntry> {
        self.entries.get(name)
    }

    /// Delete a memory entry.
    pub fn delete(&mut self, name: &str) -> Result<bool> {
        if let Some(entry) = self.entries.remove(name) {
            if let Err(e) = fs::remove_file(&entry.path) {
                tracing::warn!("Failed to delete memory file {}: {e}", entry.path.display());
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Search memories by query string (keyword matching against name, description, tags, content).
    pub fn search(&self, query: &str, limit: usize) -> Vec<&MemoryEntry> {
        let terms: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();
        if terms.is_empty() {
            return self.entries.values().take(limit).collect();
        }

        let mut scored: Vec<(i32, &MemoryEntry)> = self
            .entries
            .values()
            .filter_map(|entry| {
                let mut score = 0i32;
                let name_lower = entry.frontmatter.name.to_lowercase();
                let desc_lower = entry.frontmatter.description.to_lowercase();
                let content_lower = entry.content.to_lowercase();

                for term in &terms {
                    if name_lower.contains(term) {
                        score += 10;
                    }
                    if desc_lower.contains(term) {
                        score += 5;
                    }
                    if entry
                        .frontmatter
                        .tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(term))
                    {
                        score += 7;
                    }
                    if content_lower.contains(term) {
                        score += 2;
                    }
                }
                if score > 0 {
                    Some((score, entry))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().take(limit).map(|(_, e)| e).collect()
    }

    /// List all memory entries, optionally filtered by type.
    pub fn list(&self, memory_type: Option<MemoryType>) -> Vec<&MemoryEntry> {
        self.entries
            .values()
            .filter(|e| match memory_type {
                Some(t) => e.frontmatter.memory_type == t,
                None => true,
            })
            .collect()
    }

    /// Prune entries older than the given TTL.
    pub fn prune(&mut self, max_age_days: i64) -> usize {
        let threshold = Utc::now() - chrono::Duration::days(max_age_days);
        let to_remove: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                // User-type memories are never auto-pruned
                if entry.frontmatter.memory_type == MemoryType::User {
                    return false;
                }
                entry.frontmatter.updated_at < threshold
            })
            .map(|(name, _)| name.clone())
            .collect();

        let count = to_remove.len();
        for name in &to_remove {
            if let Err(e) = self.delete(name) {
                tracing::warn!(name = %name, error = %e, "failed to prune memory entry");
            }
        }
        count
    }

    /// Return the memory directory path.
    pub fn memory_dir(&self) -> &Path {
        &self.memory_dir
    }

    /// Return the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn load_entry(&self, name: &str, path: &Path) -> Result<MemoryEntry> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("Failed to read memory: {}", path.display()))?;

        // Parse frontmatter between --- markers
        let (frontmatter, content) = if let Some(rest) = raw.strip_prefix("---\n") {
            if let Some(end) = rest.find("\n---\n") {
                let fm_str = &rest[..end];
                let body = rest[end + 5..].trim_start();
                let fm: MemoryFrontmatter = serde_yaml::from_str(fm_str).unwrap_or_else(|e| {
                    tracing::warn!(
                        name,
                        "Failed to parse memory frontmatter: {e}, using defaults"
                    );
                    MemoryFrontmatter {
                        name: name.to_string(),
                        description: String::new(),
                        memory_type: MemoryType::Reference,
                        tags: vec![],
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    }
                });
                (fm, body.to_string())
            } else {
                (
                    MemoryFrontmatter {
                        name: name.to_string(),
                        description: String::new(),
                        memory_type: MemoryType::Reference,
                        tags: vec![],
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    },
                    raw.clone(),
                )
            }
        } else {
            (
                MemoryFrontmatter {
                    name: name.to_string(),
                    description: String::new(),
                    memory_type: MemoryType::Reference,
                    tags: vec![],
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                },
                raw.clone(),
            )
        };

        Ok(MemoryEntry {
            frontmatter,
            content,
            path: path.to_path_buf(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ws-memory-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_write_and_read() {
        let dir = temp_dir();
        let mut mgr = WorkspaceMemory::new(&dir).unwrap();

        mgr.write(
            "test-memory",
            MemoryType::Project,
            "A test memory",
            "This is test content about authentication.",
            vec!["auth".to_string()],
        )
        .unwrap();

        let entry = mgr.read("test-memory").unwrap();
        assert_eq!(entry.frontmatter.name, "test-memory");
        assert_eq!(entry.frontmatter.memory_type, MemoryType::Project);
        assert!(entry.content.contains("authentication"));
    }

    #[test]
    fn test_write_preserves_created_at() {
        let dir = temp_dir();
        let mut mgr = WorkspaceMemory::new(&dir).unwrap();

        mgr.write(
            "age-test",
            MemoryType::Project,
            "Original",
            "v1 content",
            vec![],
        )
        .unwrap();
        let original_created = mgr.read("age-test").unwrap().frontmatter.created_at;

        // Small sleep to ensure updated_at would differ
        std::thread::sleep(std::time::Duration::from_millis(10));

        mgr.write(
            "age-test",
            MemoryType::Project,
            "Updated",
            "v2 content",
            vec![],
        )
        .unwrap();
        let entry = mgr.read("age-test").unwrap();

        assert_eq!(
            entry.frontmatter.created_at, original_created,
            "created_at should be preserved across updates"
        );
        assert!(entry.frontmatter.updated_at > original_created);
        assert!(entry.content.contains("v2 content"));
    }

    #[test]
    fn test_search_relevance() {
        let dir = temp_dir();
        let mut mgr = WorkspaceMemory::new(&dir).unwrap();

        mgr.write(
            "auth-patterns",
            MemoryType::Reference,
            "Auth patterns",
            "JWT and OAuth patterns",
            vec!["auth".to_string()],
        )
        .unwrap();
        mgr.write(
            "db-setup",
            MemoryType::Reference,
            "Database setup",
            "PostgreSQL connection strings",
            vec!["database".to_string()],
        )
        .unwrap();

        let results = mgr.search("auth JWT", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].frontmatter.name, "auth-patterns");
    }

    #[test]
    fn test_delete() {
        let dir = temp_dir();
        let mut mgr = WorkspaceMemory::new(&dir).unwrap();

        mgr.write(
            "to-delete",
            MemoryType::Feedback,
            "Will be deleted",
            "content",
            vec![],
        )
        .unwrap();
        assert!(mgr.read("to-delete").is_some());
        mgr.delete("to-delete").unwrap();
        assert!(mgr.read("to-delete").is_none());
    }

    #[test]
    fn test_prune_old_entries() {
        let dir = temp_dir();
        let mut mgr = WorkspaceMemory::new(&dir).unwrap();

        // Create a feedback entry (auto-prunable)
        mgr.write(
            "old-feedback",
            MemoryType::Feedback,
            "Old feedback",
            "stale content",
            vec![],
        )
        .unwrap();
        // Create a user entry (never auto-pruned)
        mgr.write(
            "user-pref",
            MemoryType::User,
            "User preference",
            "always keep",
            vec![],
        )
        .unwrap();

        // Manually age the feedback entry
        let entry = mgr.entries.get_mut("old-feedback").unwrap();
        entry.frontmatter.updated_at = Utc::now() - chrono::Duration::days(60);

        let pruned = mgr.prune(30);
        assert_eq!(pruned, 1);
        assert!(mgr.read("old-feedback").is_none());
        assert!(mgr.read("user-pref").is_some());
    }

    #[test]
    fn test_list_by_type() {
        let dir = temp_dir();
        let mut mgr = WorkspaceMemory::new(&dir).unwrap();

        mgr.write("proj1", MemoryType::Project, "P1", "content", vec![])
            .unwrap();
        mgr.write("ref1", MemoryType::Reference, "R1", "content", vec![])
            .unwrap();
        mgr.write("proj2", MemoryType::Project, "P2", "content", vec![])
            .unwrap();

        let projects = mgr.list(Some(MemoryType::Project));
        assert_eq!(projects.len(), 2);
    }

    #[test]
    fn test_reload_from_disk() {
        let dir = temp_dir();
        let mut mgr = WorkspaceMemory::new(&dir).unwrap();
        mgr.write(
            "persistent",
            MemoryType::Reference,
            "Persists",
            "survives reload",
            vec![],
        )
        .unwrap();

        // Create a new manager pointing to the same dir
        let mgr2 = WorkspaceMemory::new(&dir).unwrap();
        assert!(mgr2.read("persistent").is_some());
    }
}
