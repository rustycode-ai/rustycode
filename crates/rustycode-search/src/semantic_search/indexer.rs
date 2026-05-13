//! Code indexing functionality
//!
//! Directory walking, file scanning, and chunk creation for semantic search.

use anyhow::Result;
use std::fs;
use std::path::Path;

use super::chunker::{
    extract_go_symbol, extract_java_symbol, extract_javascript_symbol, extract_python_symbol,
    extract_rust_symbol,
};
use super::store::{CodeChunk, SemanticIndex};

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
                "Bash".into(),
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
            index.add_chunk(chunk).with_context(|| {
                format!("failed to index chunk in {}", chunk.file_path.display())
            })?;
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
