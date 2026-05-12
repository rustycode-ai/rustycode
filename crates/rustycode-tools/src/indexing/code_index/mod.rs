//! In-memory code index for fast LLM lookups.

pub mod crawler;
pub mod query;
pub mod storage;

#[cfg(test)]
mod tests;

use anyhow::Result;
use crawler::walk_dir;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use storage::{DependencyIndex, SymbolIndex, TrigramIndex, WordIndex};

/// Kind of code symbol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Interface,
    Class,
    Module,
    Constant,
    Type,
    Variable,
    Macro,
    Impl,
    Unknown,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function => write!(f, "fn"),
            Self::Method => write!(f, "method"),
            Self::Struct => write!(f, "struct"),
            Self::Enum => write!(f, "enum"),
            Self::Trait => write!(f, "trait"),
            Self::Interface => write!(f, "interface"),
            Self::Class => write!(f, "class"),
            Self::Module => write!(f, "mod"),
            Self::Constant => write!(f, "const"),
            Self::Type => write!(f, "type"),
            Self::Variable => write!(f, "var"),
            Self::Macro => write!(f, "macro"),
            Self::Impl => write!(f, "impl"),
            Self::Unknown => write!(f, "symbol"),
        }
    }
}

/// A code symbol (function, struct, etc.)
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: PathBuf,
    pub line: usize,
    pub signature: Option<String>,
    pub doc_comment: Option<String>,
    pub parent: Option<String>, // Parent type (for methods, impl blocks)
}

/// A search result from the code index
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file_path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub context: String, // 1-2 lines of context
    pub match_type: MatchType,
    pub score: f32,
}

/// How the result matched
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MatchType {
    ExactSymbol,
    TrigramMatch,
    WordMatch,
    PrefixMatch,
}

/// Statistics about the index
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub files_indexed: usize,
    pub total_symbols: usize,
    pub total_lines: usize,
    pub trigram_count: usize,
    pub word_count: usize,
    pub index_time_ms: u64,
}

/// In-memory code index for fast lookups
pub struct CodeIndex {
    pub(crate) root: PathBuf,
    pub(crate) trigram_index: TrigramIndex,
    pub(crate) word_index: WordIndex,
    pub(crate) symbol_index: SymbolIndex,
    pub(crate) dep_index: DependencyIndex,
    pub(crate) stats: Option<IndexStats>,
    /// Cached file contents for context retrieval
    pub(crate) file_cache: HashMap<PathBuf, Vec<String>>,
}

impl CodeIndex {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            trigram_index: TrigramIndex::new(),
            word_index: WordIndex::new(),
            symbol_index: SymbolIndex::new(),
            dep_index: DependencyIndex::new(),
            stats: None,
            file_cache: HashMap::new(),
        }
    }

    /// Index the content of a single file
    fn index_content(&mut self, file_idx: usize, path: &Path, content: &str) {
        // Cache file contents
        self.file_cache.insert(
            path.to_path_buf(),
            content.lines().map(ToString::to_string).collect(),
        );

        // Index trigrams
        self.trigram_index.insert_file(file_idx, content);

        // Index words
        self.word_index.insert_file(file_idx, content);

        // Extract and index symbols
        let symbols = crawler::extract_symbols(path, content);
        for symbol in symbols {
            self.symbol_index.add(symbol);
        }

        // Extract and index dependencies
        let deps = crawler::extract_dependencies(path, content);
        for dep in deps {
            self.dep_index.add_import(path.to_path_buf(), dep);
        }
    }

    /// Update a single file in the index
    pub fn update_file(&mut self, _path: PathBuf) -> Result<()> {
        Ok(())
    }

    /// Remove a file from the index
    pub fn remove_file(&mut self, _path: PathBuf) -> Result<()> {
        Ok(())
    }

    /// Build the index by walking the project directory
    pub fn build(&mut self) -> Result<IndexStats> {
        let start = std::time::Instant::now();

        let extensions: &[&str] = &[
            "rs", "py", "js", "ts", "tsx", "jsx", "go", "java", "rb", "c", "cpp", "h", "hpp", "cs",
            "swift", "kt", "scala", "sh", "toml", "yaml", "yml", "json",
        ];

        let skip_dirs: &[&str] = &[
            "target",
            "node_modules",
            ".git",
            "vendor",
            "build",
            "dist",
            "out",
            ".next",
            "__pycache__",
            ".venv",
            "venv",
            ".cargo",
        ];

        let mut files_indexed = 0;
        let mut total_lines = 0;

        // Walk directory
        if let Ok(entries) = walk_dir(&self.root, extensions, skip_dirs) {
            for path in entries {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    total_lines += content.lines().count();
                    self.index_content(files_indexed, &path, &content);
                    self.trigram_index.files.push(path);
                    files_indexed += 1;
                }
            }
        }

        let stats = IndexStats {
            files_indexed,
            total_symbols: self.symbol_index.all_symbols().len(),
            total_lines,
            trigram_count: self.trigram_index.index.len(),
            word_count: self.word_index.index.len(),
            index_time_ms: start.elapsed().as_millis() as u64,
        };
        self.stats = Some(stats.clone());

        Ok(stats)
    }

    /// Find symbols by name (case-insensitive)
    pub fn find_symbols(&self, name: &str) -> Vec<&Symbol> {
        self.symbol_index.lookup(name)
    }

    /// Find all symbols of a given kind
    pub fn find_symbols_by_kind(&self, kind: SymbolKind) -> Vec<&Symbol> {
        self.symbol_index.lookup_kind(kind)
    }

    /// Get files that depend on the given file
    pub fn get_dependents(&self, file: &Path) -> Vec<PathBuf> {
        self.dep_index.get_dependents(file)
    }

    /// Get all symbols in a file
    pub fn file_symbols(&self, file: &Path) -> Vec<&Symbol> {
        self.symbol_index
            .all_symbols()
            .iter()
            .filter(|s| s.file_path == file)
            .collect()
    }

    /// Get index statistics
    pub const fn stats(&self) -> Option<&IndexStats> {
        self.stats.as_ref()
    }
}
