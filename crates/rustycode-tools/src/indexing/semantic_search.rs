//! Semantic Code Search with Embeddings
//!
//! This module provides semantic search over codebases using BGE-Small embeddings
//! via fastembed. It complements grep (keyword search) and LSP (symbol lookup) by
//! enabling intent-based queries like "find auth validation logic".

use anyhow::{Context, Result};
use rustycode_vector_memory::fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub total_chunks: usize,
    pub total_files: usize,
    pub embedding_model: String,
    pub embedding_dimension: usize,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for SemanticIndex {
    fn default() -> Self {
        Self::new().expect("Failed to create default SemanticIndex")
    }
}

impl SemanticIndex {
    /// Create a new semantic index
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
        let embedding = self.compute_embedding(&chunk.content)?;

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
        if let Some(chunk_indices) = self.file_to_chunks.remove(file_path) {
            let count = chunk_indices.len();
            // Mark chunks as removed by clearing content
            for idx in chunk_indices {
                if idx < self.chunks.len() {
                    self.chunks[idx].content.clear();
                    self.embeddings[idx].clear();
                }
            }
            self.metadata.updated_at = chrono::Utc::now().to_rfc3339();
            Ok(count)
        } else {
            Ok(0)
        }
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

/// Code indexer that walks directories and creates chunks
pub struct CodeIndexer {
    /// File extensions to index
    pub extensions: Vec<String>,
    /// Maximum chunk size in lines
    pub max_chunk_lines: usize,
    /// Directories to skip
    pub skip_dirs: Vec<String>,
    /// Minimum file size to index (bytes)
    pub min_file_size: usize,
    /// Maximum file size to index (bytes)
    pub max_file_size: usize,
}

impl Default for CodeIndexer {
    fn default() -> Self {
        Self {
            extensions: vec![
                "rs".into(),
                "py".into(),
                "js".into(),
                "ts".into(),
                "jsx".into(),
                "tsx".into(),
                "go".into(),
                "java".into(),
                "rb".into(),
                "c".into(),
                "cpp".into(),
                "h".into(),
                "hpp".into(),
                "cs".into(),
                "swift".into(),
                "kt".into(),
                "scala".into(),
                "sh".into(),
                "bash".into(),
                "zsh".into(),
                "fish".into(),
                "toml".into(),
                "yaml".into(),
                "yml".into(),
                "json".into(),
                "md".into(),
            ],
            max_chunk_lines: 100,
            skip_dirs: vec![
                "target".into(),
                "node_modules".into(),
                ".git".into(),
                "vendor".into(),
                "build".into(),
                "dist".into(),
                "out".into(),
                ".next".into(),
                ".venv".into(),
                "venv".into(),
                "__pycache__".into(),
                ".pytest_cache".into(),
                "cache".into(),
                ".cache".into(),
                ".rustycode".into(),
            ],
            min_file_size: 10,      // Skip files < 10 bytes
            max_file_size: 500_000, // Skip files > 500KB
        }
    }
}

impl CodeIndexer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index a directory and return a semantic index
    pub fn index_directory(&self, dir: &Path) -> Result<SemanticIndex> {
        let mut index = SemanticIndex::new()?;
        let chunks = self.walk_directory(dir)?;

        for chunk in chunks {
            let _ = index.add_chunk(chunk);
        }

        Ok(index)
    }

    /// Walk a directory and collect code chunks
    fn walk_directory(&self, dir: &Path) -> Result<Vec<CodeChunk>> {
        let mut chunks = Vec::new();
        self.walk_recursive(dir, &mut chunks)?;
        Ok(chunks)
    }

    fn walk_recursive(&self, dir: &Path, chunks: &mut Vec<CodeChunk>) -> Result<()> {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if self.skip_dirs.contains(&name.to_string()) {
                            continue;
                        }
                    }
                    self.walk_recursive(&path, chunks)?;
                } else if path.is_file() && self.should_index(&path) {
                    if let Ok(file_chunks) = self.chunk_file(&path) {
                        chunks.extend(file_chunks);
                    }
                }
            }
        }
        Ok(())
    }

    fn should_index(&self, path: &Path) -> bool {
        // Check file size
        if let Ok(meta) = fs::metadata(path) {
            let size = meta.len() as usize;
            if size < self.min_file_size || size > self.max_file_size {
                return false;
            }
        }

        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| self.extensions.contains(&ext.to_string()))
            .unwrap_or(false)
    }

    /// Chunk a file into semantic units
    fn chunk_file(&self, path: &Path) -> Result<Vec<CodeChunk>> {
        let content = fs::read_to_string(path)?;
        let language = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("txt")
            .to_string();

        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();

        // Try to detect semantic boundaries (functions, classes, etc.)
        let semantic_chunks = self.detect_semantic_boundaries(&lines, &language);

        if semantic_chunks.is_empty() {
            // Fall back to line-based chunking
            for (chunk_idx, chunk_lines) in lines.chunks(self.max_chunk_lines).enumerate() {
                let start = chunk_idx * self.max_chunk_lines;
                let end = start + chunk_lines.len();

                let content: String = chunk_lines.join("\n");
                if content.trim().is_empty() || content.lines().count() < 3 {
                    continue;
                }

                chunks.push(CodeChunk {
                    file_path: path.to_path_buf(),
                    start_line: start + 1,
                    end_line: end,
                    content,
                    language: language.clone(),
                    symbol_name: None,
                    symbol_type: None,
                });
            }
        } else {
            // Use semantic chunks
            chunks.extend(semantic_chunks.into_iter().map(
                |(start, end, symbol_name, symbol_type)| {
                    let start = start.min(lines.len());
                    let end = end.max(start).min(lines.len());
                    let content = lines[start..end].join("\n");
                    CodeChunk {
                        file_path: path.to_path_buf(),
                        start_line: start + 1,
                        end_line: end,
                        content,
                        language: language.clone(),
                        symbol_name,
                        symbol_type,
                    }
                },
            ));
        }

        Ok(chunks)
    }

    /// Detect semantic boundaries in code (functions, classes, etc.)
    fn detect_semantic_boundaries(
        &self,
        lines: &[&str],
        language: &str,
    ) -> Vec<(usize, usize, Option<String>, Option<String>)> {
        let mut chunks = Vec::new();
        let mut current_start = 0;

        for (i, line) in lines.iter().enumerate() {
            // Simple heuristic: detect function/class definitions
            if let Some((_symbol_name, _symbol_type)) = self.detect_symbol(line, language) {
                // Start a new chunk if we have accumulated enough lines
                if i > current_start + 5 {
                    chunks.push((current_start, i, None, None));
                    current_start = i;
                }
                // Continue to capture the full symbol
            }
        }

        // Add final chunk
        if current_start < lines.len() {
            chunks.push((current_start, lines.len(), None, None));
        }

        chunks
    }

    /// Detect symbol name and type from a line
    fn detect_symbol(
        &self,
        line: &str,
        language: &str,
    ) -> Option<(Option<String>, Option<String>)> {
        match language {
            "rs" => {
                if let Some((name, sym_type)) = extract_rust_symbol(line) {
                    return Some((Some(name), Some(sym_type)));
                }
            }
            "py" => {
                if let Some((name, sym_type)) = extract_python_symbol(line) {
                    return Some((Some(name), Some(sym_type)));
                }
            }
            "java" => {
                if let Some((name, sym_type)) = extract_java_symbol(line) {
                    return Some((Some(name), Some(sym_type)));
                }
            }
            "go" => {
                if let Some((name, sym_type)) = extract_go_symbol(line) {
                    return Some((Some(name), Some(sym_type)));
                }
            }
            "js" | "jsx" | "ts" | "tsx" => {
                if let Some((name, sym_type)) = extract_javascript_symbol(line) {
                    return Some((Some(name), Some(sym_type)));
                }
            }
            _ => {}
        }
        None
    }
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a > 0.0 && norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}

/// Extract Rust symbol name from a line
fn extract_rust_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Function definition: look for "fn name(" pattern
    if let Some(fn_pos) = trimmed.find("fn ") {
        let after_fn = trimmed.get(fn_pos + 3..)?;
        // Get the identifier before '('
        if let Some(paren_pos) = after_fn.find('(') {
            let name = after_fn.get(..paren_pos)?.trim();
            if !name.is_empty() && !name.contains(' ') {
                return Some((name.to_string(), "function".to_string()));
            }
        }
    }

    // Struct/impl/enum
    for keyword in &[
        "pub struct",
        "pub enum",
        "pub impl",
        "struct",
        "enum",
        "impl",
    ] {
        if let Some(rest) = trimmed.strip_prefix(keyword) {
            let rest = rest.trim();
            let name = rest.split_whitespace().next()?.split('<').next()?;
            return Some((
                name.to_string(),
                keyword
                    .split_whitespace()
                    .last()
                    .unwrap_or("type")
                    .to_string(),
            ));
        }
    }

    None
}

/// Extract Python symbol name from a line
fn extract_python_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Function definition
    if let Some(rest) = trimmed.strip_prefix("def ") {
        let name = rest.split('(').next()?.trim();
        return Some((name.to_string(), "function".to_string()));
    }

    // Class definition
    if let Some(rest) = trimmed.strip_prefix("class ") {
        let name = rest.split('(').next()?.split(':').next()?.trim();
        return Some((name.to_string(), "class".to_string()));
    }

    None
}

/// Extract Java symbol name from a line
fn extract_java_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Class definition - check early since it's a clear pattern
    if trimmed.contains("class ") {
        if let Some(class_pos) = trimmed.find("class ") {
            let after_class = trimmed.get(class_pos + 6..)?;
            let name = after_class
                .split_whitespace()
                .next()?
                .split('{')
                .next()?
                .trim();
            if !name.is_empty() && !name.contains('(') {
                return Some((name.to_string(), "class".to_string()));
            }
        }
    }

    // Interface definition
    if trimmed.contains("interface ") {
        if let Some(iface_pos) = trimmed.find("interface ") {
            let after_iface = trimmed.get(iface_pos + 10..)?;
            let name = after_iface
                .split_whitespace()
                .next()?
                .split('{')
                .next()?
                .trim();
            if !name.is_empty() {
                return Some((name.to_string(), "interface".to_string()));
            }
        }
    }

    // Method definition: look for method_name( pattern with type-like prefix
    // Patterns: "public void foo(", "private String bar(", "int baz(", "void test("
    // Find the opening parenthesis
    let paren_pos = trimmed.find('(')?;
    let before_paren = trimmed.get(..paren_pos)?.trim();

    // Split by whitespace and get the last part (method name)
    let parts: Vec<&str> = before_paren.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let method_name = parts.last()?;

    // Skip if it looks like a control flow statement
    if *method_name == "if"
        || *method_name == "while"
        || *method_name == "for"
        || *method_name == "switch"
        || *method_name == "catch"
    {
        return None;
    }

    // Skip if it looks like a constructor call (new ClassName)
    if before_paren.contains("new ") {
        return None;
    }

    // Validate it looks like a method name (camelCase, starts with lowercase or uppercase for constructors)
    if !method_name.chars().next()?.is_alphabetic() {
        return None;
    }

    // Determine if it's a constructor (starts with uppercase) or method
    let sym_type = if method_name
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        "constructor"
    } else {
        "method"
    };

    Some((method_name.to_string(), sym_type.to_string()))
}

/// Extract Go symbol name from a line
fn extract_go_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Function: func name( or func (receiver) name(
    if let Some(after_func) = trimmed.strip_prefix("func ") {
        // Check for method with receiver: func (r Receiver) MethodName(
        if after_func.trim_start().starts_with('(') {
            // Method with receiver
            if let Some(paren_end) = after_func.find(')') {
                let after_receiver = after_func.get(paren_end + 1..)?.trim();
                if let Some(paren_pos) = after_receiver.find('(') {
                    let name = after_receiver.get(..paren_pos)?.trim();
                    if !name.is_empty() {
                        return Some((name.to_string(), "method".to_string()));
                    }
                }
            }
        } else {
            // Regular function
            if let Some(paren_pos) = after_func.find('(') {
                let name = after_func.get(..paren_pos)?.trim();
                if !name.is_empty() {
                    return Some((name.to_string(), "function".to_string()));
                }
            }
        }
    }

    // Type definitions
    if let Some(after_type) = trimmed.strip_prefix("type ") {
        let after_type = after_type.trim();
        let parts: Vec<&str> = after_type.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        let name = parts[0];
        let sym_type = if parts.len() > 1 {
            match parts[1] {
                "struct" => "struct",
                "interface" => "interface",
                _ => "type",
            }
        } else {
            "type"
        };
        return Some((name.to_string(), sym_type.to_string()));
    }

    None
}

/// Extract JavaScript/TypeScript symbol name from a line
fn extract_javascript_symbol(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Function declaration: function name(
    if let Some(after_func) = trimmed.strip_prefix("function ") {
        let name = after_func.split('(').next()?.trim();
        if !name.is_empty() {
            return Some((name.to_string(), "function".to_string()));
        }
    }

    // Async function: async function name(
    if let Some(after_async) = trimmed.strip_prefix("async function ") {
        let name = after_async.split('(').next()?.trim();
        if !name.is_empty() {
            return Some((name.to_string(), "async_function".to_string()));
        }
    }

    // Arrow function: const/let/var name = (...) => or name = async () =>
    if trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ") {
        let eq_pos = trimmed.find('=')?;
        let before_eq = trimmed.get(..eq_pos)?.trim();
        // Remove type annotation for TypeScript: const name: Type = ...
        let name_part = before_eq.split(':').next()?;
        // Remove const/let/var keyword
        let name = name_part.split_whitespace().last()?.trim();

        let after_eq = trimmed.get(eq_pos + 1..)?.trim();
        if after_eq.starts_with("async") || after_eq.contains("=>") || after_eq.starts_with('(') {
            let sym_type = if after_eq.starts_with("async") {
                "async_function"
            } else {
                "arrow_function"
            };
            return Some((name.to_string(), sym_type.to_string()));
        }
    }

    // Class definition: class Name { or class Name extends ...
    if let Some(after_class) = trimmed.strip_prefix("class ") {
        let name = after_class
            .split_whitespace()
            .next()?
            .split('{')
            .next()?
            .trim();
        if !name.is_empty() {
            return Some((name.to_string(), "class".to_string()));
        }
    }

    // TypeScript interface: interface Name {
    if let Some(after_iface) = trimmed.strip_prefix("interface ") {
        let name = after_iface
            .split_whitespace()
            .next()?
            .split('{')
            .next()?
            .trim();
        if !name.is_empty() {
            return Some((name.to_string(), "interface".to_string()));
        }
    }

    None
}

/// Quick search: index a directory and return top results
#[allow(dead_code)] // Kept for future use
pub fn quick_search(dir: &Path, query: &str, top_k: usize) -> Result<Vec<SearchResult>> {
    let indexer = CodeIndexer::new();
    let index = indexer.index_directory(dir)?;
    index.search(query, top_k)
}

/// Search strategy to use for a given query
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Kept for future use
#[non_exhaustive]
pub enum SearchStrategy {
    /// Use LSP for exact symbol lookups
    Lsp,
    /// Use grep for exact text patterns
    Grep,
    /// Use glob for filename patterns
    Glob,
    /// Use semantic search for intent-based queries
    Semantic,
    /// Try grep first, fallback to semantic if no results
    GrepThenSemantic,
}

/// Analyze query intent and recommend search strategy
#[allow(dead_code)] // Kept for future use
pub fn route_query(query: &str) -> SearchStrategy {
    let q = query.trim().to_lowercase();

    // Exact symbol reference with backticks → LSP
    if query.contains('`') {
        return SearchStrategy::Lsp;
    }

    // Namespace::symbol or module::function → LSP
    if query.contains("::") {
        return SearchStrategy::Lsp;
    }

    // File extension pattern → Glob (check before dot pattern)
    if query.ends_with(".rs")
        || query.ends_with(".py")
        || query.ends_with(".go")
        || query.ends_with(".js")
        || query.ends_with(".ts")
        || query.ends_with(".java")
        || query.ends_with(".tsx")
        || query.ends_with(".jsx")
    {
        return SearchStrategy::Glob;
    }

    // Glob pattern → Glob (but not just a trailing ? for questions)
    if query.contains('*') || (query.contains('?') && !query.trim().ends_with('?')) {
        return SearchStrategy::Glob;
    }

    // Method call pattern: single word with dot (e.g., "user.name") → LSP
    // Exclude paths with slashes
    if query.contains('.') && query.split_whitespace().count() <= 2 && !query.contains('/') {
        return SearchStrategy::Lsp;
    }

    // Regex-like patterns → Grep
    if query.contains(r"\d")
        || query.contains(r"\s")
        || query.contains(r"\w")
        || query.contains("^[")
        || query.contains("$")
        || query.contains("[0-9]")
    {
        return SearchStrategy::Grep;
    }

    // Error message or exact string → Grep
    if query.starts_with('"') && query.ends_with('"') {
        return SearchStrategy::Grep;
    }

    // Quoted string anywhere in query → Grep
    if query.contains('"') {
        return SearchStrategy::Grep;
    }

    // Intent-based keywords → Semantic
    let semantic_triggers = [
        "how",
        "where",
        "what",
        "which",
        "find",
        "show",
        "explain",
        "logic",
        "implementation",
        "pattern",
        "handle",
        "validate",
        "authenticate",
        "authorize",
        "process",
        "workflow",
    ];

    for trigger in &semantic_triggers {
        if q.contains(trigger) {
            return SearchStrategy::Semantic;
        }
    }

    // Question format → Semantic
    if q.starts_with("how")
        || q.starts_with("where")
        || q.starts_with("what")
        || q.starts_with("why")
        || q.ends_with('?')
    {
        return SearchStrategy::Semantic;
    }

    // Short queries (1-2 words) without special chars → GrepThenSemantic
    if query.split_whitespace().count() <= 2
        && !query.contains(|c: char| !c.is_alphanumeric() && !c.is_whitespace())
    {
        return SearchStrategy::GrepThenSemantic;
    }

    // Default: semantic for longer natural language queries
    if query.split_whitespace().count() >= 3 {
        return SearchStrategy::Semantic;
    }

    // Fallback to grep
    SearchStrategy::Grep
}

use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use serde_json::Value;

/// Tool for semantic code search using embeddings
pub struct SemanticSearchTool {
    index: parking_lot::Mutex<Option<SemanticIndex>>,
    metadata: parking_lot::Mutex<Option<IndexMetadata>>,
    project_root: PathBuf,
}

impl SemanticSearchTool {
    pub fn new(project_root: &Path) -> Self {
        Self {
            index: parking_lot::Mutex::new(None),
            metadata: parking_lot::Mutex::new(None),
            project_root: project_root.to_path_buf(),
        }
    }

    /// Get or build the index, returning a cloned copy of search results
    fn search_index(&self, query: &str, top_k: usize) -> Result<Vec<SearchResult>> {
        // Check if index exists and search
        {
            let guard = self.index.lock();
            if let Some(ref index) = *guard {
                if !index.is_empty() {
                    return index.search(query, top_k);
                }
            }
        }

        // Build index
        let indexer = CodeIndexer::new();
        let new_index = indexer.index_directory(&self.project_root)?;
        let metadata = new_index.metadata().clone();

        // Replace index and search
        let mut index_guard = self.index.lock();
        *index_guard = Some(new_index);

        let mut meta_guard = self.metadata.lock();
        *meta_guard = Some(metadata);

        index_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Failed to build index"))?
            .search(query, top_k)
    }

    /// Force rebuild the index
    pub fn rebuild_index(&self) -> Result<IndexMetadata> {
        let indexer = CodeIndexer::new();
        let new_index = indexer.index_directory(&self.project_root)?;
        let metadata = new_index.metadata().clone();

        let mut index_guard = self.index.lock();
        *index_guard = Some(new_index);

        let mut meta_guard = self.metadata.lock();
        *meta_guard = Some(metadata.clone());

        Ok(metadata)
    }
}

impl Tool for SemanticSearchTool {
    fn name(&self) -> &str {
        "semantic_search"
    }

    fn description(&self) -> &str {
        r#"Search code by **intent/meaning** using AI embeddings (not keyword matching).

## When to use:
- **Conceptual queries**: "find auth validation logic", "how do we handle rate limiting?"
- **Unknown symbol names**: "where is JWT token validated?" (don't know function name)
- **Pattern discovery**: "show me error handling patterns", "how are database connections managed?"
- **Cross-file searches**: "find all logging configuration", "where are API routes defined?"

## When NOT to use:
- **Exact symbol lookup**: Use `lsp_definition` for "where is `validate_jwt` defined?"
- **Exact text patterns**: Use `grep` for specific strings, regex patterns
- **File names**: Use `glob` for "*.rs", "src/**/*.ts"

## Examples:
- ✅ "find user authentication middleware"
- ✅ "how do we validate JWT tokens?"
- ✅ "show me database connection pooling logic"
- ❌ "where is `UserService`?" → use `lsp_definition`
- ❌ "grep for 'Unauthorized'" → use `grep`

## Parameters:
- `query` (required): Natural language description of what to find
- `top_k` (optional, default 5): Max results to return (1-20)
- `compact` (optional, default false): Use compact format (~80% token savings)
- `minimal` (optional, default false): Use ultra-compact format (~95% token savings)

## Token Savings:
| Format | Tokens (5 results) | Use case |
|--------|-------------------|----------|
| **Full** | ~500-800 | Detailed analysis, first-time exploration |
| **Compact** | ~50-80 | Broad searches, iterative discovery |
| **Minimal** | ~20-30 | Scanning many results, reference lookup |

**Tip**: Use `compact: true` for broad queries. Auto-enabled for short queries or top_k > 10."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language description of code to find. Examples:\n- 'find user authentication middleware'\n- 'how do we validate JWT tokens?'\n- 'show me database connection pooling logic'\n- 'where is rate limiting implemented?'\n\nGood queries describe INTENT, not exact symbols."
                },
                "top_k": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5, range: 1-20)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 20
                },
                "compact": {
                    "type": "boolean",
                    "description": "Use compact output format to reduce token usage (default: false). Compact format: 'file:line (score) symbol | preview'. Auto-enabled for broad queries or top_k > 10.",
                    "default": false
                },
                "minimal": {
                    "type": "boolean",
                    "description": "Use ultra-compact format: just file references without previews (default: false). Maximum token savings (~95%).",
                    "default": false
                }
            },
            "required": ["query"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("'query' parameter is required and must be a string"))?;

        let top_k = params["top_k"].as_u64().unwrap_or(5) as usize;
        let compact_requested = params["compact"].as_bool().unwrap_or(false);
        let minimal_requested = params["minimal"].as_bool().unwrap_or(false);

        // Auto-compact for broad queries or large result sets
        let use_compact =
            compact_requested || minimal_requested || Self::should_auto_compact(query, top_k);

        // Search
        let results = self.search_index(query, top_k)?;

        if results.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No results found for query: '{}'\n\nTip: Try rephrasing your query or using grep for exact text patterns.",
                query
            )));
        }

        // Format results based on format flags
        let output = if minimal_requested {
            self.format_minimal(&results)
        } else if use_compact {
            self.format_compact(query, &results)
        } else {
            self.format_full(query, &results)
        };

        // Add token estimation comment for compact/minimal formats
        let token_estimate = self.estimate_tokens(&output);
        let output_with_meta = if use_compact || minimal_requested {
            format!("{} [~{} tokens]\n", output, token_estimate)
        } else {
            output
        };

        Ok(ToolOutput::text(output_with_meta))
    }
}

impl SemanticSearchTool {
    /// Determine if auto-compact should be used based on query characteristics
    fn should_auto_compact(query: &str, top_k: usize) -> bool {
        // Auto-compact for large result sets
        if top_k > 10 {
            return true;
        }

        // Auto-compact for broad queries (few specific keywords)
        let query_lower = query.to_lowercase();
        let broad_keywords = [
            "all",
            "everything",
            "any",
            "every",
            "broad",
            "overview",
            "summary",
        ];

        if broad_keywords.iter().any(|kw| query_lower.contains(kw)) {
            return true;
        }

        // Auto-compact for very short queries (likely to return many results)
        if query.split_whitespace().count() <= 2 && query.len() < 15 {
            return true;
        }

        false
    }

    /// Format results in compact single-line format
    ///
    /// Format: `file:line (score) symbol | preview...`
    ///
    /// Example:
    /// ```
    /// src/auth/mod.rs:45 (0.87) validate_jwt | fn validates JWT token and returns user...
    /// src/auth/middleware.rs:12 (0.72) AuthMiddleware | struct holds authentication state...
    /// ```
    fn format_compact(&self, query: &str, results: &[SearchResult]) -> String {
        let mut output = String::new();

        // Header
        output.push_str(&format!("Semantic results for '{}':\n", query));

        // Results - single line each
        for result in results {
            let rel_path = result
                .chunk
                .file_path
                .strip_prefix(&self.project_root)
                .unwrap_or(&result.chunk.file_path)
                .display();

            // file:line (score) symbol | preview
            output.push_str(&format!(
                "  {}:{} ({:.2})",
                rel_path, result.chunk.start_line, result.score
            ));

            if let Some(ref symbol) = result.chunk.symbol_name {
                output.push_str(&format!(" `{}`", symbol));
            }

            // Preview: first 80 chars of content on single line
            let preview: String = result
                .chunk
                .content
                .lines()
                .take(1)
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .chars()
                .take(80)
                .collect();

            if result.chunk.content.chars().count() > 80 {
                output.push_str(&format!(" | {}...\n", preview));
            } else {
                output.push_str(&format!(" | {}\n", preview));
            }
        }

        // Footer with index stats
        let metadata = self.metadata.lock();
        if let Some(ref m) = *metadata {
            output.push_str(&format!(
                "\n[Indexed {} chunks from {} files]",
                m.total_chunks, m.total_files
            ));
        }

        output
    }

    /// Format results in full detailed format
    fn format_full(&self, query: &str, results: &[SearchResult]) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "**Semantic Search Results for: \"{}\"**\n\n",
            query
        ));

        for result in results.iter() {
            let rel_path = result
                .chunk
                .file_path
                .strip_prefix(&self.project_root)
                .unwrap_or(&result.chunk.file_path)
                .display();

            output.push_str(&format!(
                "📄 **{}:{}-{}** (score: {:.2})\n",
                rel_path, result.chunk.start_line, result.chunk.end_line, result.score
            ));

            if let Some(ref symbol) = result.chunk.symbol_name {
                output.push_str(&format!(
                    "   *Symbol:* `{}` ({})\n",
                    symbol,
                    result.chunk.symbol_type.as_deref().unwrap_or("unknown")
                ));
            }

            output.push_str(&format!("```{}\n", result.chunk.language));

            // Show first 10 lines of content
            let preview_lines: Vec<&str> = result.chunk.content.lines().take(10).collect();
            for line in preview_lines {
                output.push_str(line);
                output.push('\n');
            }
            if result.chunk.content.lines().count() > 10 {
                output.push_str("...\n");
            }
            output.push_str("```\n\n");
        }

        output.push_str(&format!(
            "⚡ Indexed {} chunks from {} files",
            self.metadata
                .lock()
                .as_ref()
                .map(|m| m.total_chunks)
                .unwrap_or(0),
            self.metadata
                .lock()
                .as_ref()
                .map(|m| m.total_files)
                .unwrap_or(0)
        ));

        output
    }

    /// Format results in ultra-compact format: just file references
    ///
    /// Format: `file:line (score) [symbol]`
    ///
    /// Example:
    /// ```
    /// src/auth/mod.rs:45 (0.87) [validate_jwt]
    /// src/auth/middleware.rs:12 (0.72) [AuthMiddleware]
    /// ```
    fn format_minimal(&self, results: &[SearchResult]) -> String {
        let mut output = String::new();

        for result in results {
            let rel_path = result
                .chunk
                .file_path
                .strip_prefix(&self.project_root)
                .unwrap_or(&result.chunk.file_path)
                .display();

            output.push_str(&format!(
                "{}:{} ({:.2})",
                rel_path, result.chunk.start_line, result.score
            ));

            if let Some(ref symbol) = result.chunk.symbol_name {
                output.push_str(&format!(" [{}]", symbol));
            }
            output.push('\n');
        }

        output
    }

    /// Estimate token count for output (rough: 1 token ≈ 4 chars for English/code)
    fn estimate_tokens(&self, output: &str) -> usize {
        output.len() / 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_add_and_search() {
        let mut index = SemanticIndex::new().unwrap();

        index
            .add_chunk(CodeChunk {
                file_path: PathBuf::from("src/main.rs"),
                start_line: 1,
                end_line: 10,
                content:
                    "fn authenticate_user(token: &str) -> Result<User> { /* JWT validation */ }"
                        .into(),
                language: "rs".into(),
                symbol_name: Some("authenticate_user".to_string()),
                symbol_type: Some("function".to_string()),
            })
            .unwrap();

        index
            .add_chunk(CodeChunk {
                file_path: PathBuf::from("src/lib.rs"),
                start_line: 1,
                end_line: 10,
                content: "pub struct Config { database_url: String }".into(),
                language: "rs".into(),
                symbol_name: Some("Config".to_string()),
                symbol_type: Some("struct".to_string()),
            })
            .unwrap();

        let results = index.search("user authentication token", 5).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].chunk.content.contains("authenticate_user"));
    }

    #[test]
    fn test_extract_rust_symbol() {
        assert_eq!(
            extract_rust_symbol("pub fn authenticate_user(token: &str)"),
            Some(("authenticate_user".to_string(), "function".to_string()))
        );
        assert_eq!(
            extract_rust_symbol("fn helper()"),
            Some(("helper".to_string(), "function".to_string()))
        );
        assert_eq!(
            extract_rust_symbol("pub struct UserConfig"),
            Some(("UserConfig".to_string(), "struct".to_string()))
        );
        assert_eq!(
            extract_rust_symbol("impl UserService"),
            Some(("UserService".to_string(), "impl".to_string()))
        );
    }

    #[test]
    fn test_extract_python_symbol() {
        assert_eq!(
            extract_python_symbol("def authenticate_user(token):"),
            Some(("authenticate_user".to_string(), "function".to_string()))
        );
        assert_eq!(
            extract_python_symbol("class UserController:"),
            Some(("UserController".to_string(), "class".to_string()))
        );
    }

    #[test]
    fn test_extract_java_symbol() {
        assert_eq!(
            extract_java_symbol("public void authenticateUser(String token) {"),
            Some(("authenticateUser".to_string(), "method".to_string()))
        );
        assert_eq!(
            extract_java_symbol("private String validateToken() {"),
            Some(("validateToken".to_string(), "method".to_string()))
        );
        assert_eq!(
            extract_java_symbol("public class UserService {"),
            Some(("UserService".to_string(), "class".to_string()))
        );
        assert_eq!(
            extract_java_symbol("interface Repository {"),
            Some(("Repository".to_string(), "interface".to_string()))
        );
    }

    #[test]
    fn test_extract_go_symbol() {
        assert_eq!(
            extract_go_symbol("func AuthenticateUser(token string) error {"),
            Some(("AuthenticateUser".to_string(), "function".to_string()))
        );
        assert_eq!(
            extract_go_symbol(
                "func (s *Server) HandleRequest(w http.ResponseWriter, r *http.Request) {"
            ),
            Some(("HandleRequest".to_string(), "method".to_string()))
        );
        assert_eq!(
            extract_go_symbol("type User struct {"),
            Some(("User".to_string(), "struct".to_string()))
        );
        assert_eq!(
            extract_go_symbol("type Service interface {"),
            Some(("Service".to_string(), "interface".to_string()))
        );
    }

    #[test]
    fn test_extract_javascript_symbol() {
        assert_eq!(
            extract_javascript_symbol("function authenticateUser(token) {"),
            Some(("authenticateUser".to_string(), "function".to_string()))
        );
        assert_eq!(
            extract_javascript_symbol("async function fetchData() {"),
            Some(("fetchData".to_string(), "async_function".to_string()))
        );
        assert_eq!(
            extract_javascript_symbol("const handleClick = () => {"),
            Some(("handleClick".to_string(), "arrow_function".to_string()))
        );
        assert_eq!(
            extract_javascript_symbol("class UserService {"),
            Some(("UserService".to_string(), "class".to_string()))
        );
        assert_eq!(
            extract_javascript_symbol("interface User {"),
            Some(("User".to_string(), "interface".to_string()))
        );
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);

        let c = vec![1.0, 0.0, 0.0];
        let d = vec![0.0, 1.0, 0.0];
        let sim_orthogonal = cosine_similarity(&c, &d);
        assert!((sim_orthogonal - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_should_auto_compact() {
        // Large top_k should trigger auto-compact
        assert!(SemanticSearchTool::should_auto_compact("auth", 15));
        assert!(!SemanticSearchTool::should_auto_compact(
            "authentication middleware",
            5
        ));

        // Broad keywords should trigger auto-compact
        assert!(SemanticSearchTool::should_auto_compact(
            "all authentication code",
            5
        ));
        assert!(SemanticSearchTool::should_auto_compact(
            "overview of security",
            5
        ));
        assert!(SemanticSearchTool::should_auto_compact(
            "summary of logging",
            5
        ));

        // Short queries should trigger auto-compact
        assert!(SemanticSearchTool::should_auto_compact("auth", 5)); // short + 1 word
        assert!(SemanticSearchTool::should_auto_compact("jwt", 5)); // short + 1 word
        assert!(!SemanticSearchTool::should_auto_compact(
            "jwt validation logic",
            5
        )); // 3 words

        // Specific queries should not trigger
        assert!(!SemanticSearchTool::should_auto_compact(
            "where is validate_jwt defined",
            5
        ));
        assert!(!SemanticSearchTool::should_auto_compact(
            "how does authentication middleware work",
            5
        ));
    }

    #[test]
    fn test_format_minimal() {
        let mock_results = vec![SearchResult {
            chunk: CodeChunk {
                file_path: PathBuf::from("src/auth/mod.rs"),
                start_line: 45,
                end_line: 55,
                content: "fn validate_jwt(token: &str) -> Result<User> { }".into(),
                language: "rs".into(),
                symbol_name: Some("validate_jwt".to_string()),
                symbol_type: Some("function".to_string()),
            },
            score: 0.87,
        }];

        use std::env;
        let current_dir = env::current_dir().unwrap_or(PathBuf::from("."));
        let tool = SemanticSearchTool::new(&current_dir);

        let minimal = tool.format_minimal(&mock_results);
        let compact = tool.format_compact("jwt", &mock_results);
        let full = tool.format_full("jwt", &mock_results);

        // Verify minimal format
        assert!(minimal.contains("src/auth/mod.rs:45"));
        assert!(minimal.contains("[validate_jwt]"));
        assert!(!minimal.contains("```"));
        assert!(!minimal.contains("|"));

        // Verify token savings: minimal < compact < full
        let minimal_chars = minimal.len();
        let compact_chars = compact.len();
        let full_chars = full.len();

        assert!(
            minimal_chars < compact_chars,
            "minimal={} vs compact={}",
            minimal_chars,
            compact_chars
        );
        assert!(
            compact_chars < full_chars,
            "compact={} vs full={}",
            compact_chars,
            full_chars
        );
    }

    #[test]
    fn test_estimate_tokens() {
        use std::env;
        let current_dir = env::current_dir().unwrap_or(PathBuf::from("."));
        let tool = SemanticSearchTool::new(&current_dir);

        let test_str = "hello world this is a test";
        let estimated = tool.estimate_tokens(test_str);

        // Should be roughly len/4
        assert_eq!(estimated, test_str.len() / 4);
    }

    #[test]
    fn test_compact_format_basic() {
        // Test compact format with mock results
        use std::env;
        let current_dir = env::current_dir().unwrap_or(PathBuf::from("."));
        let tool = SemanticSearchTool::new(&current_dir);

        let mock_results = vec![
            SearchResult {
                chunk: CodeChunk {
                    file_path: PathBuf::from("src/auth/mod.rs"),
                    start_line: 45,
                    end_line: 55,
                    content: "fn validate_jwt(token: &str) -> Result<User> { /* validates JWT token and returns user */ }".into(),
                    language: "rs".into(),
                    symbol_name: Some("validate_jwt".to_string()),
                    symbol_type: Some("function".to_string()),
                },
                score: 0.87,
            }
        ];

        let compact_output = tool.format_compact("jwt validation", &mock_results);
        let full_output = tool.format_full("jwt validation", &mock_results);

        // Verify compact format characteristics
        assert!(compact_output.contains("src/auth/mod.rs:45"));
        assert!(compact_output.contains("validate_jwt"));
        assert!(compact_output.contains("(0.87)"));
        assert!(!compact_output.contains("```"));

        // Verify full format characteristics
        assert!(full_output.contains("```"));
        assert!(full_output.contains("📄"));

        // Token estimation: compact should use significantly fewer tokens
        let compact_chars = compact_output.len();
        let full_chars = full_output.len();
        assert!(
            compact_chars < full_chars,
            "Compact should use fewer chars: compact={} vs full={}",
            compact_chars,
            full_chars
        );

        // Compact should be at least 50% smaller
        assert!(
            compact_chars < full_chars / 2,
            "Compact should be at least 50% smaller"
        );
    }

    #[test]
    fn test_route_query() {
        // LSP routes
        assert_eq!(route_query("`validate_jwt`"), SearchStrategy::Lsp);
        assert_eq!(route_query("auth::middleware"), SearchStrategy::Lsp);
        assert_eq!(route_query("user.name"), SearchStrategy::Lsp);

        // Glob routes
        assert_eq!(route_query("*.rs"), SearchStrategy::Glob);
        assert_eq!(route_query("src/main.rs"), SearchStrategy::Glob);
        assert_eq!(route_query("src/**/*.ts"), SearchStrategy::Glob);

        // Grep routes
        assert_eq!(route_query(r"\d{3}"), SearchStrategy::Grep);
        assert_eq!(route_query("^error"), SearchStrategy::Grep);
        assert_eq!(route_query("\"Unauthorized\""), SearchStrategy::Grep);

        // Semantic routes
        assert_eq!(
            route_query("how do we validate JWT tokens?"),
            SearchStrategy::Semantic
        );
        assert_eq!(
            route_query("find auth validation logic"),
            SearchStrategy::Semantic
        );
        assert_eq!(
            route_query("where is user authentication handled"),
            SearchStrategy::Semantic
        );
        assert_eq!(
            route_query("explain the rate limiting implementation"),
            SearchStrategy::Semantic
        );

        // GrepThenSemantic for short queries
        assert_eq!(route_query("auth"), SearchStrategy::GrepThenSemantic);
        assert_eq!(route_query("jwt"), SearchStrategy::GrepThenSemantic);
        assert_eq!(
            route_query("authentication"),
            SearchStrategy::GrepThenSemantic
        );
    }
}
