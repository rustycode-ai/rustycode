#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)
)]

//! Vector-based memory system for rustycode agents.
//!
//! Provides semantic search over team learnings, task traces, and code patterns
//! using BGE-Small embeddings and cosine similarity for efficient similarity search.
//!
//! # Usage
//!
//! ```rust,no_run
//! use rustycode_vector_memory::{VectorMemory, MemoryType, MemoryEntry, MemoryMeta};
//! use tempfile::TempDir;
//!
//! let temp_dir = TempDir::new().unwrap();
//! let mut memory = VectorMemory::new(temp_dir.path());
//! memory.init().unwrap();
//!
//! // Add a memory
//! memory.add(
//!     "Tests live in /tests directory".to_string(),
//!     MemoryType::Learnings,
//!     MemoryMeta::default(),
//! ).unwrap();
//!
//! // Search for relevant memories
//! let results = memory.search("where are tests?", MemoryType::Learnings, 5);
//! ```

#[cfg(feature = "embedding")]
pub use fastembed;

use anyhow::{Context, Result};
use chrono::Utc;
#[cfg(feature = "embedding")]
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
#[cfg(feature = "embedding")]
use std::sync::Mutex;
use uuid::Uuid;

#[cfg(not(feature = "embedding"))]
struct Embedder;

#[cfg(not(feature = "embedding"))]
impl Embedder {
    fn new() -> Result<Self> {
        Ok(Self)
    }

    fn embed(&self, _text: &str) -> Vec<f32> {
        Vec::new()
    }
}

/// Types of memories that can be stored
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryType {
    Learnings,
    TaskTraces,
    CodePatterns,
    ToolUsage,
}

/// A single memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub metadata: MemoryMeta,
    pub created_at: String,
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
}

/// Metadata for a memory entry
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MemoryMeta {
    pub confidence: f32,
    pub source_task: Option<String>,
    pub occurrence_count: u32,
    /// Unix timestamp when this memory was created (for TTL-based pruning)
    #[serde(default)]
    pub created_timestamp: Option<i64>,
    /// Time-to-live in hours (None = permanent, 0 = expired)
    #[serde(default)]
    pub ttl_hours: Option<u64>,
}

/// Search result with relevance score
#[derive(Debug, Clone)]
pub struct MemoryResult {
    pub entry: MemoryEntry,
    pub similarity: f32,
}

/// Brute-force index with cosine similarity
struct MemoryIndex {
    id_to_pos: HashMap<String, usize>,
    embeddings: Vec<Vec<f32>>,
    entries: Vec<MemoryEntry>,
}

impl MemoryIndex {
    fn new() -> Self {
        Self {
            id_to_pos: HashMap::new(),
            embeddings: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn insert(&mut self, entry: MemoryEntry, embedding: Vec<f32>) -> usize {
        let pos = self.entries.len();
        self.id_to_pos.insert(entry.id.clone(), pos);
        self.embeddings.push(embedding);
        self.entries.push(entry);
        pos
    }

    fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<(usize, f32)> {
        let mut scores: Vec<(usize, f32)> = self
            .embeddings
            .iter()
            .enumerate()
            .filter(|(_, emb)| !emb.is_empty())
            .map(|(pos, emb)| (pos, cosine_similarity(query_embedding, emb)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);
        scores
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Embedder using BGE-Small model via fastembed
#[cfg(feature = "embedding")]
struct Embedder {
    model: Mutex<TextEmbedding>,
}

#[cfg(feature = "embedding")]
impl Embedder {
    fn new() -> Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(true),
        )
        .context("Failed to initialize BGE-Small embedding model")?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut guard = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.embed(vec![text.to_string()], None).map_or_else(
            |_| Vec::new(),
            |mut embeddings| embeddings.pop().unwrap_or_default(),
        )
    }
}

/// Vector memory using cosine similarity for search
pub struct VectorMemory {
    storage_dir: PathBuf,
    indices: HashMap<MemoryType, MemoryIndex>,
    embedder: Option<Embedder>,
}

impl VectorMemory {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let project_root = project_root.into();
        let storage_dir = project_root.join(".rustycode").join("vector_memory");
        let _ = fs::create_dir_all(&storage_dir);
        Self {
            storage_dir,
            indices: HashMap::new(),
            embedder: None,
        }
    }

    pub fn init(&mut self) -> Result<()> {
        // Initialize embedder first
        self.embedder = Some(Embedder::new()?);

        // Load indices for each memory type
        for memory_type in [
            MemoryType::Learnings,
            MemoryType::TaskTraces,
            MemoryType::CodePatterns,
            MemoryType::ToolUsage,
        ] {
            let index_path = self.index_path(memory_type);
            let index = if index_path.exists() {
                self.load_index(memory_type)?
            } else {
                MemoryIndex::new()
            };
            self.indices.insert(memory_type, index);
        }

        // Compute embeddings for all loaded entries (they weren't computed during load_index)
        self.compute_missing_embeddings();

        Ok(())
    }

    /// Compute embeddings for entries that don't have them
    fn compute_missing_embeddings(&mut self) {
        let Some(embedder) = self.embedder.as_ref() else {
            return;
        };

        for index in self.indices.values_mut() {
            for (i, entry) in index.entries.iter().enumerate() {
                if index.embeddings.get(i).is_none_or(Vec::is_empty) {
                    let embedding = embedder.embed(&entry.content);
                    if i < index.embeddings.len() {
                        index.embeddings[i] = embedding;
                    } else {
                        index.embeddings.push(embedding);
                    }
                }
            }
        }
    }

    pub fn add(
        &mut self,
        content: String,
        memory_type: MemoryType,
        metadata: MemoryMeta,
    ) -> Result<String> {
        if self.indices.is_empty() {
            self.init()?;
        }

        let Some(embedder) = self.embedder.as_ref() else {
            anyhow::bail!("Embedder not initialized. Call init() first.");
        };
        let embedding = embedder.embed(&content);

        let mut metadata = metadata;
        // Set timestamp if not provided
        if metadata.created_timestamp.is_none() {
            metadata.created_timestamp = Some(Utc::now().timestamp());
        }

        let entry = MemoryEntry {
            id: Uuid::new_v4().to_string(),
            content,
            metadata,
            created_at: Utc::now().to_rfc3339(),
            embedding: Some(embedding.clone()),
        };

        let index = self
            .indices
            .get_mut(&memory_type)
            .ok_or_else(|| anyhow::anyhow!("memory type {memory_type:?} not initialized"))?;
        index.insert(entry.clone(), embedding);
        self.save_index(memory_type)?;
        Ok(entry.id)
    }

    pub fn search(&self, query: &str, memory_type: MemoryType, top_k: usize) -> Vec<MemoryResult> {
        let Some(index) = self.indices.get(&memory_type) else {
            return Vec::new();
        };
        let Some(embedder) = self.embedder.as_ref() else {
            return Vec::new();
        };
        let query_embedding = embedder.embed(query);

        index
            .search(&query_embedding, top_k)
            .into_iter()
            .filter_map(|(pos, similarity)| {
                index.entries.get(pos).map(|entry| MemoryResult {
                    entry: entry.clone(),
                    similarity,
                })
            })
            .collect()
    }

    pub fn search_all(&self, query: &str, top_k: usize) -> HashMap<MemoryType, Vec<MemoryResult>> {
        let mut results = HashMap::new();
        for memory_type in [
            MemoryType::Learnings,
            MemoryType::TaskTraces,
            MemoryType::CodePatterns,
            MemoryType::ToolUsage,
        ] {
            let type_results = self.search(query, memory_type, top_k);
            if !type_results.is_empty() {
                results.insert(memory_type, type_results);
            }
        }
        results
    }

    pub fn remove(&mut self, memory_type: MemoryType, memory_id: &str) -> bool {
        let Some(index) = self.indices.get_mut(&memory_type) else {
            return false;
        };
        if let Some(&pos) = index.id_to_pos.get(memory_id) {
            index.id_to_pos.remove(memory_id);
            if let Some(entry) = index.entries.get_mut(pos) {
                entry.content = String::new();
            }
            if let Err(e) = self.save_index(memory_type) {
                tracing::warn!(
                    "Failed to save index after removing memory {}: {}",
                    memory_id,
                    e
                );
            }
            true
        } else {
            false
        }
    }

    pub fn consolidate(&mut self, memory_type: MemoryType, threshold: f32) -> Result<usize> {
        let Some(index) = self.indices.get_mut(&memory_type) else {
            return Ok(0);
        };

        let mut to_remove = Vec::new();

        // Clone all necessary data upfront to avoid borrow conflicts
        let entries_data: Vec<(usize, String, MemoryMeta)> = index
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.content.is_empty())
            .map(|(i, e)| (i, e.content.clone(), e.metadata.clone()))
            .collect();

        // First pass: find duplicates and compute merged metadata
        let mut merged_metadata: HashMap<usize, MemoryMeta> = HashMap::new();

        for (i_pos, content_i, meta_i) in &entries_data {
            let i = *i_pos;
            if to_remove.contains(&i) {
                continue;
            }

            let mut final_meta = meta_i.clone();

            for (j_pos, content_j, meta_j) in &entries_data {
                let j = *j_pos;
                if to_remove.contains(&j) || j <= i {
                    continue;
                }

                let similarity = text_similarity_static(content_i, content_j);
                if similarity > threshold {
                    final_meta.occurrence_count = final_meta
                        .occurrence_count
                        .saturating_add(meta_j.occurrence_count);
                    final_meta.confidence = final_meta.confidence.max(meta_j.confidence);
                    to_remove.push(j);
                }
            }

            if !to_remove.contains(&i) && final_meta != *meta_i {
                merged_metadata.insert(i, final_meta);
            }
        }

        // Second pass: apply merged metadata
        for (pos, new_meta) in merged_metadata {
            if let Some(entry) = index.entries.get_mut(pos) {
                entry.metadata = new_meta;
            }
        }

        // Third pass: clear removed entries
        for &pos in to_remove.iter().rev() {
            if let Some(entry) = index.entries.get_mut(pos) {
                entry.content = String::new();
            }
        }

        self.save_index(memory_type)?;
        Ok(to_remove.len())
    }

    pub fn count(&self, memory_type: MemoryType) -> usize {
        self.indices.get(&memory_type).map_or(0, |index| {
            index
                .entries
                .iter()
                .filter(|e| !e.content.is_empty())
                .count()
        })
    }

    /// Remove expired entries based on TTL
    /// Returns the number of entries pruned
    pub fn prune_expired(&mut self, memory_type: MemoryType) -> Result<usize> {
        let Some(index) = self.indices.get_mut(&memory_type) else {
            return Ok(0);
        };
        let now = Utc::now().timestamp();
        let mut pruned = 0;

        for entry in &mut index.entries {
            if entry.content.is_empty() {
                continue;
            }

            if let Some(ttl_hours) = entry.metadata.ttl_hours {
                if ttl_hours == 0 {
                    continue;
                } // Permanent

                let created = entry.metadata.created_timestamp.unwrap_or(0);
                let age_seconds = now - created;
                let ttl_seconds = i64::try_from(ttl_hours * 3600).unwrap_or(i64::MAX);

                if age_seconds > ttl_seconds {
                    entry.content = String::new();
                    pruned += 1;
                }
            }
        }

        if pruned > 0 {
            self.save_index(memory_type)?;
        }
        Ok(pruned)
    }

    /// Prune entries with low confidence
    pub fn prune_low_confidence(
        &mut self,
        memory_type: MemoryType,
        min_confidence: f32,
    ) -> Result<usize> {
        let Some(index) = self.indices.get_mut(&memory_type) else {
            return Ok(0);
        };
        let mut pruned = 0;

        for entry in &mut index.entries {
            if entry.content.is_empty() {
                continue;
            }

            if entry.metadata.confidence < min_confidence {
                entry.content = String::new();
                pruned += 1;
            }
        }

        if pruned > 0 {
            self.save_index(memory_type)?;
        }
        Ok(pruned)
    }

    /// Get count of non-empty entries
    pub fn count_active(&self, memory_type: MemoryType) -> usize {
        self.indices.get(&memory_type).map_or(0, |index| {
            index
                .entries
                .iter()
                .filter(|e| !e.content.is_empty())
                .count()
        })
    }

    /// Get all active (non-empty, non-expired) memories
    pub fn active(&self, memory_type: MemoryType) -> Vec<&MemoryEntry> {
        let now = Utc::now().timestamp();
        self.indices
            .get(&memory_type)
            .map(|index| {
                index
                    .entries
                    .iter()
                    .filter(|e| {
                        if e.content.is_empty() {
                            return false;
                        }

                        // Check TTL
                        if let Some(ttl_hours) = e.metadata.ttl_hours {
                            if ttl_hours > 0 {
                                let created = e.metadata.created_timestamp.unwrap_or(0);
                                let age_seconds = now - created;
                                let ttl_seconds =
                                    i64::try_from(ttl_hours * 3600).unwrap_or(i64::MAX);
                                if age_seconds > ttl_seconds {
                                    return false;
                                }
                            }
                        }
                        true
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn all(&self, memory_type: MemoryType) -> Vec<&MemoryEntry> {
        self.indices
            .get(&memory_type)
            .map(|index| {
                index
                    .entries
                    .iter()
                    .filter(|e| !e.content.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn index_path(&self, memory_type: MemoryType) -> PathBuf {
        let type_name = match memory_type {
            MemoryType::Learnings => "learnings",
            MemoryType::TaskTraces => "task_traces",
            MemoryType::CodePatterns => "code_patterns",
            MemoryType::ToolUsage => "tool_usage",
        };
        self.storage_dir.join(format!("{type_name}.json"))
    }

    fn save_index(&self, memory_type: MemoryType) -> Result<()> {
        let Some(index) = self.indices.get(&memory_type) else {
            return Ok(());
        };
        let path = self.index_path(memory_type);
        let content =
            serde_json::to_string_pretty(&index.entries).context("Failed to serialize index")?;

        // Atomic write: write to temp file then rename to avoid corruption on crash
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &content)
            .with_context(|| format!("Failed to write temp index to {}", tmp_path.display()))?;
        fs::rename(&tmp_path, &path)
            .with_context(|| format!("Failed to rename temp index to {}", path.display()))?;
        Ok(())
    }

    fn load_index(&self, memory_type: MemoryType) -> Result<MemoryIndex> {
        let path = self.index_path(memory_type);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read index from {}", path.display()))?;
        let entries: Vec<MemoryEntry> =
            serde_json::from_str(&content).context("Failed to deserialize index")?;

        let mut index = MemoryIndex::new();

        for entry in entries {
            let pos = index.entries.len();
            index.id_to_pos.insert(entry.id.clone(), pos);
            // Embeddings will be computed after init() completes
            index.embeddings.push(Vec::new());
            index.entries.push(entry);
        }
        Ok(index)
    }

    #[allow(dead_code)] // Kept for future use
    fn _text_similarity(a: &str, b: &str) -> f32 {
        text_similarity_static(a, b)
    }
}

#[allow(clippy::cast_precision_loss)] // word counts never approach 2^52
fn text_similarity_static(a: &str, b: &str) -> f32 {
    let a_words: std::collections::HashSet<_> = a.split_whitespace().collect();
    let b_words: std::collections::HashSet<_> = b.split_whitespace().collect();
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_add_and_search() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "Tests live in /tests directory".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.9,
                    source_task: Some("auth".to_string()),
                    occurrence_count: 1,
                    created_timestamp: None,
                    ttl_hours: None,
                },
            )
            .unwrap();

        let results = memory.search("where are tests?", MemoryType::Learnings, 5);
        assert!(!results.is_empty());
        assert!(results[0].entry.content.contains("Tests"));
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // First session: add memory
        {
            let mut memory1 = VectorMemory::new(temp_dir.path());
            memory1.init().unwrap();
            memory1
                .add(
                    "Persistent learning".to_string(),
                    MemoryType::Learnings,
                    MemoryMeta::default(),
                )
                .unwrap();
        }

        // Second session: load from disk and search
        {
            let mut memory2 = VectorMemory::new(temp_dir.path());
            memory2.init().unwrap();

            let results = memory2.search("persistent", MemoryType::Learnings, 5);
            assert!(!results.is_empty());
        }
    }

    #[test]
    fn test_multi_tier_memory_ttl() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let now = Utc::now().timestamp();

        // Add a short-term memory (task trace with 1 hour TTL)
        memory
            .add(
                "Task trace: attempted fix for auth bug".to_string(),
                MemoryType::TaskTraces,
                MemoryMeta {
                    confidence: 0.7,
                    source_task: Some("auth-bug-fix".to_string()),
                    occurrence_count: 1,
                    created_timestamp: Some(now - 7200), // 2 hours ago (expired)
                    ttl_hours: Some(1),
                },
            )
            .unwrap();

        // Add a long-term memory (pattern, permanent)
        memory
            .add(
                "Pattern: auth handlers return Result<Token, AuthError>".to_string(),
                MemoryType::CodePatterns,
                MemoryMeta {
                    confidence: 0.9,
                    source_task: Some("auth-refactor".to_string()),
                    occurrence_count: 3,
                    created_timestamp: Some(now - 86400), // 1 day ago
                    ttl_hours: None,                      // Permanent
                },
            )
            .unwrap();

        // Before pruning: both should exist
        assert_eq!(memory.count_active(MemoryType::TaskTraces), 1);
        assert_eq!(memory.count_active(MemoryType::CodePatterns), 1);

        // Prune expired
        let pruned = memory.prune_expired(MemoryType::TaskTraces).unwrap();
        assert_eq!(pruned, 1, "Should have pruned 1 expired task trace");

        // After pruning: task trace should be gone, pattern should remain
        assert_eq!(memory.count_active(MemoryType::TaskTraces), 0);
        assert_eq!(memory.count_active(MemoryType::CodePatterns), 1);

        // get_active should filter out expired
        let active_traces = memory.active(MemoryType::TaskTraces);
        assert!(active_traces.is_empty(), "No active task traces expected");

        let active_patterns = memory.active(MemoryType::CodePatterns);
        assert_eq!(active_patterns.len(), 1, "One active pattern expected");
    }

    #[test]
    fn test_prune_low_confidence() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        // Add low-confidence memory
        memory
            .add(
                "Low confidence learning".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.3,
                    source_task: None,
                    occurrence_count: 1,
                    created_timestamp: None,
                    ttl_hours: None,
                },
            )
            .unwrap();

        // Add high-confidence memory
        memory
            .add(
                "High confidence learning".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.9,
                    source_task: None,
                    occurrence_count: 5,
                    created_timestamp: None,
                    ttl_hours: None,
                },
            )
            .unwrap();

        // Prune low confidence (< 0.5)
        let pruned = memory
            .prune_low_confidence(MemoryType::Learnings, 0.5)
            .unwrap();
        assert_eq!(pruned, 1, "Should have pruned 1 low-confidence memory");

        // Only high-confidence should remain
        assert_eq!(memory.count_active(MemoryType::Learnings), 1);
        let results = memory.search("learning", MemoryType::Learnings, 5);
        assert!(results[0].entry.content.contains("High confidence"));
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let vec = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&vec, &vec) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 1.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_text_similarity_identical() {
        let sim = text_similarity_static("hello world", "hello world");
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_text_similarity_no_overlap() {
        let sim = text_similarity_static("alpha beta", "gamma delta");
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_text_similarity_partial() {
        let sim = text_similarity_static("hello world foo baz", "hello world bar");
        assert!(sim > 0.3 && sim < 1.0);
    }

    #[test]
    fn test_text_similarity_empty() {
        assert_eq!(text_similarity_static("", ""), 0.0);
        assert_eq!(text_similarity_static("hello", ""), 0.0);
    }

    #[test]
    fn test_memory_entry_serde_roundtrip() {
        let entry = MemoryEntry {
            id: "test-id".to_string(),
            content: "test content".to_string(),
            metadata: MemoryMeta {
                confidence: 0.8,
                source_task: Some("task-1".to_string()),
                occurrence_count: 3,
                created_timestamp: Some(1_234_567_890),
                ttl_hours: Some(24),
            },
            created_at: "2025-01-01T00:00:00Z".to_string(),
            embedding: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry.id, back.id);
        assert_eq!(entry.content, back.content);
        assert_eq!(entry.metadata, back.metadata);
    }

    #[test]
    fn test_memory_type_serde_roundtrip() {
        for mt in [
            MemoryType::Learnings,
            MemoryType::TaskTraces,
            MemoryType::CodePatterns,
            MemoryType::ToolUsage,
        ] {
            let json = serde_json::to_string(&mt).unwrap();
            let back: MemoryType = serde_json::from_str(&json).unwrap();
            assert_eq!(mt, back);
        }
    }

    #[test]
    fn test_remove_entry() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let id = memory
            .add(
                "to be removed".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();

        assert_eq!(memory.count(MemoryType::Learnings), 1);
        assert!(memory.remove(MemoryType::Learnings, &id));
        assert_eq!(memory.count_active(MemoryType::Learnings), 0);
    }

    #[test]
    fn test_remove_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        assert!(!memory.remove(MemoryType::Learnings, "fake-id"));
    }

    #[test]
    fn test_consolidate_duplicates() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "auth handler returns Result Token".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.7,
                    occurrence_count: 1,
                    ..Default::default()
                },
            )
            .unwrap();

        memory
            .add(
                "auth handler returns Result Token Error".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.8,
                    occurrence_count: 2,
                    ..Default::default()
                },
            )
            .unwrap();

        // Consolidate with high threshold
        let removed = memory.consolidate(MemoryType::Learnings, 0.3).unwrap();
        assert_eq!(removed, 1, "Should consolidate 1 duplicate");
        assert_eq!(memory.count_active(MemoryType::Learnings), 1);
    }

    #[test]
    fn test_search_all_types() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "auth learning".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "auth trace".to_string(),
                MemoryType::TaskTraces,
                MemoryMeta::default(),
            )
            .unwrap();

        let results = memory.search_all("auth", 5);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_get_all() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "entry 1".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "entry 2".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();

        let all = memory.all(MemoryType::Learnings);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_memory_result_similarity_ordering() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "Rust ownership borrowing lifetimes".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "Python list comprehension syntax".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "Rust async await futures".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();

        let results = memory.search("Rust programming", MemoryType::Learnings, 3);
        assert!(results.len() >= 2);
        // Rust-related results should score higher than Python
        assert!(results[0].similarity >= results.last().unwrap().similarity);
    }

    // --- New tests ---

    // Cosine similarity edge cases

    #[test]
    fn test_cosine_similarity_opposite_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_different_magnitudes() {
        let a = vec![1.0, 0.0];
        let b = vec![3.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_both_zero_vectors() {
        let a = vec![0.0, 0.0];
        let b = vec![0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_negative_components() {
        let a = vec![1.0, -1.0];
        let b = vec![1.0, -1.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_asymmetric_angle() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim > 0.0 && sim < 1.0);
        // cos(45 deg) = sqrt(2)/2 ~ 0.707
        assert!((sim - std::f32::consts::SQRT_2 / 2.0).abs() < 1e-5);
    }

    // Text similarity edge cases

    #[test]
    fn test_text_similarity_single_word_overlap() {
        let sim = text_similarity_static("hello world", "hello there");
        // "hello" overlaps, 3 unique words total (hello, world, there)
        assert!((sim - 0.333_333_34).abs() < 1e-5);
    }

    #[test]
    fn test_text_similarity_case_sensitive() {
        let sim = text_similarity_static("Hello World", "hello world");
        // Case-sensitive: no overlap
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_text_similarity_whitespace_handling() {
        let sim = text_similarity_static("hello   world", "hello world");
        // Extra whitespace should be handled by split_whitespace
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_text_similarity_one_empty() {
        assert_eq!(text_similarity_static("", "hello"), 0.0);
    }

    #[test]
    fn test_text_similarity_subset() {
        // "hello" is subset of "hello world"
        let sim = text_similarity_static("hello", "hello world");
        // intersection=1, union=2 => 0.5
        assert!((sim - 0.5).abs() < 1e-5);
    }

    // MemoryType tests

    #[test]
    fn test_memory_type_serde_snake_case() {
        let json = serde_json::to_string(&MemoryType::CodePatterns).unwrap();
        assert_eq!(json, "\"code_patterns\"");
        let back: MemoryType = serde_json::from_str("\"code_patterns\"").unwrap();
        assert_eq!(back, MemoryType::CodePatterns);
    }

    #[test]
    fn test_memory_type_all_variants_roundtrip() {
        let variants = vec![
            MemoryType::Learnings,
            MemoryType::TaskTraces,
            MemoryType::CodePatterns,
            MemoryType::ToolUsage,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let back: MemoryType = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, back);
        }
    }

    #[test]
    fn test_memory_type_hash_and_eq() {
        use std::collections::HashSet;
        let set: HashSet<MemoryType> = [
            MemoryType::Learnings,
            MemoryType::Learnings,
            MemoryType::CodePatterns,
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 2);
    }

    // MemoryMeta tests

    #[test]
    fn test_memory_meta_default() {
        let meta = MemoryMeta::default();
        assert_eq!(meta.confidence, 0.0);
        assert_eq!(meta.source_task, None);
        assert_eq!(meta.occurrence_count, 0);
        assert_eq!(meta.created_timestamp, None);
        assert_eq!(meta.ttl_hours, None);
    }

    #[test]
    fn test_memory_meta_serde_roundtrip() {
        let meta = MemoryMeta {
            confidence: 0.95,
            source_task: Some("debug-auth".to_string()),
            occurrence_count: 7,
            created_timestamp: Some(1_700_000_000),
            ttl_hours: Some(48),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: MemoryMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, back);
    }

    #[test]
    fn test_memory_meta_serde_missing_optional_fields() {
        // Simulate loading from JSON that lacks optional fields
        let json = r#"{"confidence":0.5,"source_task":null,"occurrence_count":1}"#;
        let meta: MemoryMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.confidence, 0.5);
        assert_eq!(meta.source_task, None);
        assert_eq!(meta.occurrence_count, 1);
        assert_eq!(meta.created_timestamp, None);
        assert_eq!(meta.ttl_hours, None);
    }

    #[test]
    fn test_memory_meta_equality() {
        let a = MemoryMeta {
            confidence: 0.5,
            source_task: None,
            occurrence_count: 1,
            created_timestamp: Some(100),
            ttl_hours: None,
        };
        let b = MemoryMeta {
            confidence: 0.5,
            source_task: None,
            occurrence_count: 1,
            created_timestamp: Some(100),
            ttl_hours: None,
        };
        assert_eq!(a, b);
    }

    // MemoryEntry tests

    #[test]
    fn test_memory_entry_embedding_skipped_in_serde() {
        let entry = MemoryEntry {
            id: "test-id".to_string(),
            content: "test".to_string(),
            metadata: MemoryMeta::default(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            embedding: Some(vec![1.0, 2.0, 3.0]),
        };
        let json = serde_json::to_string(&entry).unwrap();
        // embedding field should not appear in serialized JSON
        assert!(!json.contains("embedding"));
        let back: MemoryEntry = serde_json::from_str(&json).unwrap();
        // embedding is skipped, so it should be None after deserialization
        assert!(back.embedding.is_none());
    }

    #[test]
    fn test_memory_entry_serde_all_fields() {
        let entry = MemoryEntry {
            id: "uuid-1234".to_string(),
            content: "A code pattern for error handling".to_string(),
            metadata: MemoryMeta {
                confidence: 0.88,
                source_task: Some("refactor".to_string()),
                occurrence_count: 42,
                created_timestamp: Some(1_700_000_000),
                ttl_hours: Some(72),
            },
            created_at: "2025-06-15T12:00:00Z".to_string(),
            embedding: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry.id, back.id);
        assert_eq!(entry.content, back.content);
        assert_eq!(entry.metadata, back.metadata);
        assert_eq!(entry.created_at, back.created_at);
    }

    // VectorMemory tests

    #[test]
    fn test_new_creates_storage_dir() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join(".rustycode").join("vector_memory");
        assert!(!storage_path.exists());
        let _memory = VectorMemory::new(temp_dir.path());
        assert!(storage_path.exists());
    }

    #[test]
    fn test_search_without_init_returns_empty() {
        let temp_dir = TempDir::new().unwrap();
        let memory = VectorMemory::new(temp_dir.path());
        // No init() called, no embedder, should return empty
        let results = memory.search("test", MemoryType::Learnings, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_uninitialized_type_returns_empty() {
        let temp_dir = TempDir::new().unwrap();
        let memory = VectorMemory::new(temp_dir.path());
        // No indices populated, search should return empty
        let results = memory.search("test", MemoryType::CodePatterns, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_count_uninitialized_type() {
        let temp_dir = TempDir::new().unwrap();
        let memory = VectorMemory::new(temp_dir.path());
        assert_eq!(memory.count(MemoryType::Learnings), 0);
    }

    #[test]
    fn test_count_active_uninitialized_type() {
        let temp_dir = TempDir::new().unwrap();
        let memory = VectorMemory::new(temp_dir.path());
        assert_eq!(memory.count_active(MemoryType::Learnings), 0);
    }

    #[test]
    fn test_get_active_uninitialized_type() {
        let temp_dir = TempDir::new().unwrap();
        let memory = VectorMemory::new(temp_dir.path());
        let active = memory.active(MemoryType::Learnings);
        assert!(active.is_empty());
    }

    #[test]
    fn test_get_all_uninitialized_type() {
        let temp_dir = TempDir::new().unwrap();
        let memory = VectorMemory::new(temp_dir.path());
        let all = memory.all(MemoryType::Learnings);
        assert!(all.is_empty());
    }

    #[test]
    fn test_search_all_no_results() {
        let temp_dir = TempDir::new().unwrap();
        let memory = VectorMemory::new(temp_dir.path());
        let results = memory.search_all("test", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_add_auto_sets_timestamp() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let before = Utc::now().timestamp();
        memory
            .add(
                "timestamp test".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(), // created_timestamp is None
            )
            .unwrap();
        let after = Utc::now().timestamp();

        let all = memory.all(MemoryType::Learnings);
        assert_eq!(all.len(), 1);
        let ts = all[0].metadata.created_timestamp.unwrap();
        assert!(ts >= before && ts <= after);
    }

    #[test]
    fn test_add_preserves_existing_timestamp() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let custom_ts = 1_609_459_200; // 2021-01-01
        memory
            .add(
                "custom timestamp".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    created_timestamp: Some(custom_ts),
                    ..Default::default()
                },
            )
            .unwrap();

        let all = memory.all(MemoryType::Learnings);
        assert_eq!(all[0].metadata.created_timestamp.unwrap(), custom_ts);
    }

    #[test]
    fn test_add_auto_inits() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        // Don't call init() -- add() should auto-init when indices are empty
        let id = memory
            .add(
                "auto init test".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        assert!(!id.is_empty());
        assert_eq!(memory.count(MemoryType::Learnings), 1);
    }

    #[test]
    fn test_add_returns_unique_ids() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let id1 = memory
            .add(
                "entry 1".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        let id2 = memory
            .add(
                "entry 2".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_count_vs_count_active() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "active entry".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        assert_eq!(memory.count(MemoryType::Learnings), 1);
        assert_eq!(memory.count_active(MemoryType::Learnings), 1);

        // count and count_active should behave identically for non-removed entries
        // They differ after remove or prune, but both filter on empty content
    }

    #[test]
    fn test_count_across_types() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "learning 1".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "learning 2".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "trace 1".to_string(),
                MemoryType::TaskTraces,
                MemoryMeta::default(),
            )
            .unwrap();

        assert_eq!(memory.count(MemoryType::Learnings), 2);
        assert_eq!(memory.count(MemoryType::TaskTraces), 1);
        assert_eq!(memory.count(MemoryType::CodePatterns), 0);
        assert_eq!(memory.count(MemoryType::ToolUsage), 0);
    }

    #[test]
    fn test_search_top_k_limits_results() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        for i in 0..5 {
            memory
                .add(
                    format!("auth handler variant {i}"),
                    MemoryType::Learnings,
                    MemoryMeta::default(),
                )
                .unwrap();
        }

        let results = memory.search("auth handler", MemoryType::Learnings, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_empty_query() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "some content".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        // Empty query should still work (embedder handles it)
        let results = memory.search("", MemoryType::Learnings, 5);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_remove_clears_content_not_position() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let id1 = memory
            .add(
                "first entry".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        let _id2 = memory
            .add(
                "second entry".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();

        assert!(memory.remove(MemoryType::Learnings, &id1));
        // First entry removed, second should still be there
        assert_eq!(memory.count_active(MemoryType::Learnings), 1);
        let all = memory.all(MemoryType::Learnings);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].content, "second entry");
    }

    #[test]
    fn test_remove_wrong_type() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let id = memory
            .add(
                "learning entry".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        // Try removing from a different type
        assert!(!memory.remove(MemoryType::CodePatterns, &id));
        // Original should still exist
        assert_eq!(memory.count(MemoryType::Learnings), 1);
    }

    #[test]
    fn test_persistence_after_remove() {
        let temp_dir = TempDir::new().unwrap();

        let _id = {
            let mut mem1 = VectorMemory::new(temp_dir.path());
            mem1.init().unwrap();
            let id = mem1
                .add(
                    "to remove".to_string(),
                    MemoryType::Learnings,
                    MemoryMeta::default(),
                )
                .unwrap();
            mem1.add(
                "to keep".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
            mem1.remove(MemoryType::Learnings, &id);
            id
        };

        {
            let mut mem2 = VectorMemory::new(temp_dir.path());
            mem2.init().unwrap();
            assert_eq!(mem2.count_active(MemoryType::Learnings), 1);
            let all = mem2.all(MemoryType::Learnings);
            assert_eq!(all[0].content, "to keep");
        }
    }

    #[test]
    fn test_persistence_after_prune() {
        let temp_dir = TempDir::new().unwrap();
        let now = Utc::now().timestamp();

        {
            let mut mem1 = VectorMemory::new(temp_dir.path());
            mem1.init().unwrap();
            mem1.add(
                "expired entry".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.5,
                    created_timestamp: Some(now - 7200),
                    ttl_hours: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();
            mem1.add(
                "permanent entry".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.9,
                    ttl_hours: None,
                    ..Default::default()
                },
            )
            .unwrap();
            let pruned = mem1.prune_expired(MemoryType::Learnings).unwrap();
            assert_eq!(pruned, 1);
        }

        {
            let mut mem2 = VectorMemory::new(temp_dir.path());
            mem2.init().unwrap();
            assert_eq!(mem2.count_active(MemoryType::Learnings), 1);
            let all = mem2.all(MemoryType::Learnings);
            assert_eq!(all[0].content, "permanent entry");
        }
    }

    #[test]
    fn test_prune_expired_permanent_entries_not_pruned() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        // ttl_hours = None means permanent, should never be pruned
        let now = Utc::now().timestamp();
        memory
            .add(
                "old but permanent".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.5,
                    created_timestamp: Some(now - 999_999),
                    ttl_hours: None,
                    ..Default::default()
                },
            )
            .unwrap();

        let pruned = memory.prune_expired(MemoryType::Learnings).unwrap();
        assert_eq!(pruned, 0);
        assert_eq!(memory.count_active(MemoryType::Learnings), 1);
    }

    #[test]
    fn test_prune_expired_ttl_zero_means_permanent() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        // ttl_hours = 0 is treated as permanent per the code
        memory
            .add(
                "ttl zero entry".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.5,
                    created_timestamp: Some(Utc::now().timestamp() - 999_999),
                    ttl_hours: Some(0),
                    ..Default::default()
                },
            )
            .unwrap();

        let pruned = memory.prune_expired(MemoryType::Learnings).unwrap();
        assert_eq!(pruned, 0);
        assert_eq!(memory.count_active(MemoryType::Learnings), 1);
    }

    #[test]
    fn test_prune_expired_empty_type() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let pruned = memory.prune_expired(MemoryType::CodePatterns).unwrap();
        assert_eq!(pruned, 0);
    }

    #[test]
    fn test_prune_low_confidence_threshold_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "exact threshold".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.5,
                    ..Default::default()
                },
            )
            .unwrap();

        memory
            .add(
                "above threshold".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.5001,
                    ..Default::default()
                },
            )
            .unwrap();

        // Prune strictly less than 0.5 -- exact 0.5 stays, 0.5001 stays
        let pruned = memory
            .prune_low_confidence(MemoryType::Learnings, 0.5)
            .unwrap();
        assert_eq!(pruned, 0);
        assert_eq!(memory.count_active(MemoryType::Learnings), 2);
    }

    #[test]
    fn test_prune_low_confidence_all_above() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "high conf".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.9,
                    ..Default::default()
                },
            )
            .unwrap();

        let pruned = memory
            .prune_low_confidence(MemoryType::Learnings, 0.5)
            .unwrap();
        assert_eq!(pruned, 0);
    }

    #[test]
    fn test_prune_low_confidence_uninitialized_type() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let pruned = memory
            .prune_low_confidence(MemoryType::ToolUsage, 0.5)
            .unwrap();
        assert_eq!(pruned, 0);
    }

    #[test]
    fn test_consolidate_no_duplicates() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "completely different topic about networking".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "unrelated information about graphics".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();

        let removed = memory.consolidate(MemoryType::Learnings, 0.9).unwrap();
        assert_eq!(removed, 0, "No duplicates expected with high threshold");
        assert_eq!(memory.count_active(MemoryType::Learnings), 2);
    }

    #[test]
    fn test_consolidate_empty_type() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let removed = memory.consolidate(MemoryType::CodePatterns, 0.5).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn test_consolidate_merges_occurrence_count_and_max_confidence() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "auth handler returns Result".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.5,
                    occurrence_count: 3,
                    ..Default::default()
                },
            )
            .unwrap();

        memory
            .add(
                "auth handler returns Result Error".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.9,
                    occurrence_count: 2,
                    ..Default::default()
                },
            )
            .unwrap();

        let removed = memory.consolidate(MemoryType::Learnings, 0.3).unwrap();
        assert_eq!(removed, 1);

        let remaining = memory.all(MemoryType::Learnings);
        assert_eq!(remaining.len(), 1);
        // The surviving entry should have merged metadata
        let entry = &remaining[0];
        assert_eq!(entry.metadata.occurrence_count, 5); // 3 + 2
        assert!((entry.metadata.confidence - 0.9).abs() < 1e-5); // max(0.5, 0.9)
    }

    #[test]
    fn test_consolidate_threshold_one() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "auth handler pattern A".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "auth handler pattern B".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();

        // threshold=1.0 requires exact Jaccard match, which won't happen for different text
        let removed = memory.consolidate(MemoryType::Learnings, 1.0).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn test_get_active_expires_ttl_entries() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let now = Utc::now().timestamp();

        // Add expired entry
        memory
            .add(
                "expired".to_string(),
                MemoryType::TaskTraces,
                MemoryMeta {
                    confidence: 0.5,
                    created_timestamp: Some(now - 7200),
                    ttl_hours: Some(1),
                    ..Default::default()
                },
            )
            .unwrap();

        // Add non-expired entry
        memory
            .add(
                "active".to_string(),
                MemoryType::TaskTraces,
                MemoryMeta {
                    confidence: 0.5,
                    created_timestamp: Some(now),
                    ttl_hours: Some(24),
                    ..Default::default()
                },
            )
            .unwrap();

        // get_active filters by TTL
        let active = memory.active(MemoryType::TaskTraces);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].content, "active");
    }

    #[test]
    fn test_get_active_no_ttl_is_always_active() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "permanent".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.5,
                    created_timestamp: Some(Utc::now().timestamp() - 999_999),
                    ttl_hours: None,
                    ..Default::default()
                },
            )
            .unwrap();

        let active = memory.active(MemoryType::Learnings);
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_search_all_combines_multiple_types() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "authentication pattern".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "authentication trace log".to_string(),
                MemoryType::TaskTraces,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "authentication code snippet".to_string(),
                MemoryType::CodePatterns,
                MemoryMeta::default(),
            )
            .unwrap();

        let results = memory.search_all("authentication", 5);
        assert_eq!(results.len(), 3);
        assert!(results.contains_key(&MemoryType::Learnings));
        assert!(results.contains_key(&MemoryType::TaskTraces));
        assert!(results.contains_key(&MemoryType::CodePatterns));
    }

    #[test]
    fn test_add_multiple_types_independently() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "learning content".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "trace content".to_string(),
                MemoryType::TaskTraces,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "pattern content".to_string(),
                MemoryType::CodePatterns,
                MemoryMeta::default(),
            )
            .unwrap();
        memory
            .add(
                "tool usage content".to_string(),
                MemoryType::ToolUsage,
                MemoryMeta::default(),
            )
            .unwrap();

        assert_eq!(memory.count(MemoryType::Learnings), 1);
        assert_eq!(memory.count(MemoryType::TaskTraces), 1);
        assert_eq!(memory.count(MemoryType::CodePatterns), 1);
        assert_eq!(memory.count(MemoryType::ToolUsage), 1);
    }

    #[test]
    fn test_remove_then_re_add() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        let id = memory
            .add(
                "temporary".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        memory.remove(MemoryType::Learnings, &id);
        assert_eq!(memory.count(MemoryType::Learnings), 0);

        memory
            .add(
                "replacement".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        assert_eq!(memory.count(MemoryType::Learnings), 1);
    }

    #[test]
    fn test_search_result_has_valid_similarity() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "test content for similarity".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
        let results = memory.search("test content", MemoryType::Learnings, 5);
        assert!(!results.is_empty());
        // Similarity should be a valid f32 in [-1, 1]
        let sim = results[0].similarity;
        assert!((-1.0..=1.0).contains(&sim));
        assert!(!sim.is_nan());
    }

    #[test]
    fn test_search_result_contains_entry_data() {
        let temp_dir = TempDir::new().unwrap();
        let mut memory = VectorMemory::new(temp_dir.path());
        memory.init().unwrap();

        memory
            .add(
                "detailed learning content".to_string(),
                MemoryType::Learnings,
                MemoryMeta {
                    confidence: 0.85,
                    source_task: Some("integration-test".to_string()),
                    occurrence_count: 3,
                    ..Default::default()
                },
            )
            .unwrap();

        let results = memory.search("detailed learning", MemoryType::Learnings, 5);
        assert_eq!(results.len(), 1);
        let entry = &results[0].entry;
        assert!(entry.content.contains("detailed learning content"));
        assert!((entry.metadata.confidence - 0.85).abs() < 1e-5);
        assert_eq!(
            entry.metadata.source_task.as_deref(),
            Some("integration-test")
        );
        assert_eq!(entry.metadata.occurrence_count, 3);
        assert!(!entry.id.is_empty());
        assert!(!entry.created_at.is_empty());
    }

    #[test]
    fn test_persistence_multiple_entries() {
        let temp_dir = TempDir::new().unwrap();

        {
            let mut mem1 = VectorMemory::new(temp_dir.path());
            mem1.init().unwrap();
            mem1.add(
                "first learning".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
            mem1.add(
                "second learning".to_string(),
                MemoryType::Learnings,
                MemoryMeta::default(),
            )
            .unwrap();
            mem1.add(
                "a trace".to_string(),
                MemoryType::TaskTraces,
                MemoryMeta::default(),
            )
            .unwrap();
        }

        {
            let mut mem2 = VectorMemory::new(temp_dir.path());
            mem2.init().unwrap();
            assert_eq!(mem2.count_active(MemoryType::Learnings), 2);
            assert_eq!(mem2.count_active(MemoryType::TaskTraces), 1);
        }
    }

    #[test]
    fn test_index_path_format() {
        let temp_dir = TempDir::new().unwrap();
        let memory = VectorMemory::new(temp_dir.path());

        let expected_dir = temp_dir.path().join(".rustycode").join("vector_memory");
        assert_eq!(
            memory.index_path(MemoryType::Learnings),
            expected_dir.join("learnings.json")
        );
        assert_eq!(
            memory.index_path(MemoryType::TaskTraces),
            expected_dir.join("task_traces.json")
        );
        assert_eq!(
            memory.index_path(MemoryType::CodePatterns),
            expected_dir.join("code_patterns.json")
        );
        assert_eq!(
            memory.index_path(MemoryType::ToolUsage),
            expected_dir.join("tool_usage.json")
        );
    }
}
