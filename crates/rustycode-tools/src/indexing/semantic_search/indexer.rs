//! Semantic search indexing logic.
//!
//! Directory walking, file scanning, and chunk creation for semantic search.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::store::{CodeChunk, SemanticIndex};
use crate::indexing::symbols;

/// High-level indexer for semantic search
pub struct CodeIndexer {
    // We could add configuration here (e.g. chunk size, overlap)
}

impl Default for CodeIndexer {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeIndexer {
    pub fn new() -> Self {
        Self {}
    }

    /// Index a directory and return a new SemanticIndex
    pub fn index_directory(&self, dir: &Path) -> Result<SemanticIndex> {
        let mut index = SemanticIndex::new()?;
        let files = symbols::collect_source_files(dir)?;
        for file_path in files {
            if let Err(e) = self.index_file(&mut index, &file_path) {
                tracing::warn!("Failed to index file {:?}: {}", file_path, e);
            }
        }
        Ok(index)
    }

    /// Index a single file into an existing index
    pub fn index_file(&self, index: &mut SemanticIndex, path: &Path) -> Result<()> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read file for indexing: {:?}", path))?;

        let outline = symbols::extract_file(path, &content);

        if outline.symbols.is_empty() {
            let chunk = CodeChunk {
                file_path: path.to_path_buf(),
                start_line: 1,
                end_line: content.lines().count().max(1),
                content: content.clone(),
                language: outline.language.clone(),
                symbol_name: None,
                symbol_type: None,
                parent_context: None,
            };
            index.add_chunk(chunk)?;
        } else {
            for symbol in &outline.symbols {
                self.index_symbol(index, path, &content, symbol, &outline.language, None)?;
            }
        }

        Ok(())
    }

    fn index_symbol(
        &self,
        index: &mut SemanticIndex,
        path: &Path,
        full_content: &str,
        symbol: &rustycode_protocol::code_symbol::CodeSymbol,
        language: &str,
        parent_context: Option<&str>,
    ) -> Result<()> {
        let lines: Vec<&str> = full_content.lines().collect();
        if symbol.line > 0 && symbol.line <= lines.len() {
            let start = symbol.line - 1;
            let end = symbol.end_line.min(lines.len());
            let chunk_content = lines[start..end].join("\n");

            let chunk = CodeChunk {
                file_path: path.to_path_buf(),
                start_line: symbol.line,
                end_line: symbol.end_line,
                content: chunk_content,
                language: language.to_string(),
                symbol_name: Some(symbol.name.clone()),
                symbol_type: Some(symbol.kind.to_string()),
                parent_context: parent_context.map(|s| s.to_string()),
            };

            let file_path_display = path.display().to_string();
            index.add_chunk(chunk).with_context(|| {
                format!(
                    "Failed to add chunk to index for file: {}",
                    file_path_display
                )
            })?;
        }

        for child in &symbol.children {
            self.index_symbol(
                index,
                path,
                full_content,
                child,
                language,
                Some(&symbol.name),
            )?;
        }

        Ok(())
    }
}
