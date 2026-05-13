//! In-memory semantic search index and data structures.

use anyhow::{Context, Result};
use rustycode_vector_memory::fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::searcher::cosine_similarity;

/// A chunk of code to be indexed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub language: String,
    /// Optional symbol name (function, class, etc.) if detected
    pub symbol_name: Option<String>,
    /// Optional symbol type (function, class, method, etc.)
    pub symbol_type: Option<String>,
}

/// Search result with relevance score
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub chunk: CodeChunk,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub total_chunks: usize,
    pub total_files: usize,
    pub embedding_model: String,
    pub embedding_dimension: usize,
    pub created_at: String,
    pub updated_at: String,
}

/// In-memory semantic search index using embeddings
pub struct SemanticIndex {
    chunks: Vec<CodeChunk>,
    embeddings: Vec<Vec<f32>>,
    embedder: Mutex<TextEmbedding>,
    /// Map from file path to chunk indices for incremental updates
    file_to_chunks: HashMap<PathBuf, Vec<usize>>,
    /// Index metadata
    metadata: IndexMetadata,
}

impl Default for SemanticIndex {
    fn default() -> Self {
        Self::new().expect("Failed to create default SemanticIndex")
    }
}

impl SemanticIndex {
    pub fn new() -> Result<Self> {
        let embedder = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(false),
        )
        .context("Failed to initialize BGE-Small embedding model")?;

        Ok(Self {
            chunks: Vec::new(),
            embeddings: Vec::new(),
            embedder: Mutex::new(embedder),
            file_to_chunks: HashMap::new(),
            metadata: IndexMetadata {
                total_chunks: 0,
                total_files: 0,
                embedding_model: "BGE-Small-EN-v1.5".to_string(),
                embedding_dimension: 384,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        })
    }

    /// Index a code chunk with embedding
    pub fn add_chunk(&mut self, chunk: CodeChunk) -> Result<()> {
        let embedding = self.compute_embedding(&chunk.content).with_context(|| {
            format!(
                "Failed to compute embedding for chunk in {:?}",
                chunk.file_path
            )
        })?;

        // Track file -> chunks mapping for incremental updates
        let chunk_idx = self.chunks.len();
        self.file_to_chunks
            .entry(chunk.file_path.clone())
            .or_default()
            .push(chunk_idx);

        self.chunks.push(chunk);
        self.embeddings.push(embedding);
        self.metadata.total_chunks = self.chunks.len();
        self.metadata.total_files = self.file_to_chunks.len();
        self.metadata.updated_at = chrono::Utc::now().to_rfc3339();

        Ok(())
    }

    /// Remove chunks for a file (for incremental updates)
    #[allow(dead_code)] // Kept for future use
    pub fn remove_file(&mut self, file_path: &Path) -> Result<usize> {
        let chunk_indices = match self.file_to_chunks.remove(file_path) {
            Some(indices) => indices,
            None => return Ok(0),
        };
        let count = chunk_indices.len();

        // Collect indices to remove, sorted descending so removal doesn't shift later indices
        let mut to_remove = chunk_indices;
        to_remove.sort_unstable_by(|a, b| b.cmp(a));

        for idx in &to_remove {
            if *idx < self.chunks.len() {
                self.chunks.remove(*idx);
                self.embeddings.remove(*idx);
            }
        }

        // Rebuild file_to_chunks since all indices after removed items shifted
        self.rebuild_file_to_chunks();
        self.metadata.total_chunks = self.chunks.len();
        self.metadata.total_files = self.file_to_chunks.len();
        self.metadata.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(count)
    }

    /// Search for semantically similar code
    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchResult>> {
        let mut embedder = self
            .embedder
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let query_embedding = embedder
            .embed(vec![query.to_string()], None)
            .context("Failed to compute query embedding")?
            .into_iter()
            .next()
            .unwrap_or_default();
        if query_embedding.is_empty() {
            tracing::warn!(
                "semantic_search: query embedding returned empty vector for query: {:?}",
                &query[..query.len().min(100)]
            );
        }
        drop(embedder); // Release lock before processing

        let mut results: Vec<SearchResult> = self
            .chunks
            .iter()
            .zip(self.embeddings.iter())
            .filter(|(_, emb)| !emb.is_empty())
            .map(|(chunk, embedding)| SearchResult {
                chunk: chunk.clone(),
                score: cosine_similarity(&query_embedding, embedding),
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Filter out low scores and truncate
        results.retain(|r| r.score > 0.5); // Minimum relevance threshold
        results.truncate(top_k);

        Ok(results)
    }

    /// Get number of indexed chunks
    pub fn len(&self) -> usize {
        self.chunks.iter().filter(|c| !c.content.is_empty()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get metadata
    pub fn metadata(&self) -> &IndexMetadata {
        &self.metadata
    }

    /// Clear the index
    #[allow(dead_code)] // Kept for future use
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.embeddings.clear();
        self.file_to_chunks.clear();
        self.metadata.total_chunks = 0;
        self.metadata.total_files = 0;
        self.metadata.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// Rebuild file_to_chunks mapping from current chunks vector.
    /// Called after bulk removal operations that invalidate stored indices.
    fn rebuild_file_to_chunks(&mut self) {
        self.file_to_chunks.clear();
        for (idx, chunk) in self.chunks.iter().enumerate() {
            self.file_to_chunks
                .entry(chunk.file_path.clone())
                .or_default()
                .push(idx);
        }
    }

    /// Compute embedding for text
    fn compute_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let mut embedder = self
            .embedder
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let embeddings = embedder
            .embed(vec![text.to_string()], None)
            .context("Failed to compute embedding")?;
        drop(embedder); // Release lock
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }
}
