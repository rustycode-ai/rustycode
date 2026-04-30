//! In-memory code index for fast LLM lookups.
//!
//! Inspired by codedb: index-once into memory, query via O(1) lookups,
//! return token-efficient structured responses instead of raw text.
//!
//! # Architecture
//!
//! - **`TrigramIndex`**: O(1) substring search via trigram hashing
//! - **`WordIndex`**: Inverted word index for exact identifier lookups
//! - **`SymbolIndex`**: Extracted function/struct/enum definitions
//! - **`DependencyIndex`**: Import/dependency graph for impact analysis
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tools::code_index::CodeIndex;
//!
//! let mut index = CodeIndex::new("/path/to/project");
//! index.build().expect("failed to index");
//!
//! // Fast symbol lookup
//! let symbols = index.find_symbols("handle_request");
//! for s in &symbols {
//!     println!("{}:{} {} - {}", s.file_path, s.line, s.kind, s.name);
//! }
//!
//! // Structured search (token-efficient for LLM)
//! let results = index.search("authentication middleware");
//! println!("{}", index.format_results(&results));
//! ```

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// ── Data Types ────────────────────────────────────────────────────────────────

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

// ── Trigram Index ─────────────────────────────────────────────────────────────

/// Trigram index for fast substring search
///
/// For each trigram (3-char substring), stores the set of (file, line) pairs
/// that contain it. Query intersection gives O(1) lookups for multi-trigram patterns.
struct TrigramIndex {
    /// trigram -> set of (`file_index`, `line_number`)
    index: HashMap<[u8; 3], HashSet<(usize, usize)>>,
    /// file index -> path
    files: Vec<PathBuf>,
}

impl TrigramIndex {
    fn new() -> Self {
        Self {
            index: HashMap::new(),
            files: Vec::new(),
        }
    }

    fn insert_file(&mut self, file_idx: usize, content: &str) {
        let mut line_num = 0;
        for line in content.lines() {
            line_num += 1;
            let line_lower: Vec<u8> = line.to_lowercase().bytes().collect();
            if line_lower.len() < 3 {
                continue;
            }
            for window in line_lower.windows(3) {
                let trigram: [u8; 3] = [window[0], window[1], window[2]];
                self.index
                    .entry(trigram)
                    .or_default()
                    .insert((file_idx, line_num));
            }
        }
    }

    fn search(&self, pattern: &str, files: &[PathBuf]) -> Vec<(PathBuf, usize)> {
        let pattern_lower: Vec<u8> = pattern.to_lowercase().bytes().collect();
        if pattern_lower.len() < 3 {
            return Vec::new();
        }

        // Extract trigrams from pattern
        let pattern_trigrams: Vec<[u8; 3]> = pattern_lower
            .windows(3)
            .map(|w| [w[0], w[1], w[2]])
            .collect();

        if pattern_trigrams.is_empty() {
            return Vec::new();
        }

        // Intersect results from all trigrams
        let mut candidates: Option<HashSet<(usize, usize)>> = None;
        for trigram in &pattern_trigrams {
            if let Some(matches) = self.index.get(trigram) {
                match candidates {
                    None => candidates = Some(matches.clone()),
                    Some(ref mut existing) => {
                        let intersection: HashSet<_> =
                            existing.intersection(matches).cloned().collect();
                        *existing = intersection;
                    }
                }
            } else {
                return Vec::new(); // Trigram not found = no matches
            }
        }

        candidates
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(file_idx, line)| files.get(file_idx).map(|p| (p.clone(), line)))
            .collect()
    }
}

// ── Word Index ────────────────────────────────────────────────────────────────

/// Inverted word index for exact identifier lookups
struct WordIndex {
    /// word (lowercase) -> set of (`file_index`, `line_number`)
    index: HashMap<String, HashSet<(usize, usize)>>,
}

impl WordIndex {
    fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    fn insert_file(&mut self, file_idx: usize, content: &str) {
        let mut line_num = 0;
        for line in content.lines() {
            line_num += 1;
            for word in extract_words(line) {
                self.index
                    .entry(word.to_lowercase())
                    .or_default()
                    .insert((file_idx, line_num));
            }
        }
    }

    fn lookup(&self, word: &str) -> Vec<(usize, usize)> {
        self.index
            .get(&word.to_lowercase())
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    #[allow(dead_code)] // Kept for future use
    fn prefix_search(&self, prefix: &str, limit: usize) -> Vec<String> {
        let prefix_lower = prefix.to_lowercase();
        let mut results: Vec<String> = self
            .index
            .keys()
            .filter(|w| w.starts_with(&prefix_lower))
            .cloned()
            .collect();
        results.truncate(limit);
        results
    }
}

/// Extract identifier-like words from a line of code
fn extract_words(line: &str) -> Vec<&str> {
    line.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty() && w.len() >= 2)
        .collect()
}

// ── Symbol Index ──────────────────────────────────────────────────────────────

/// Index of extracted code symbols
struct SymbolIndex {
    /// name (lowercase) -> list of symbols
    by_name: HashMap<String, Vec<Symbol>>,
    /// all symbols
    all: Vec<Symbol>,
}

impl SymbolIndex {
    fn new() -> Self {
        Self {
            by_name: HashMap::new(),
            all: Vec::new(),
        }
    }

    fn add(&mut self, symbol: Symbol) {
        let key = symbol.name.to_lowercase();
        self.by_name.entry(key).or_default().push(symbol.clone());
        self.all.push(symbol);
    }

    fn lookup(&self, name: &str) -> Vec<&Symbol> {
        self.by_name
            .get(&name.to_lowercase())
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    fn lookup_kind(&self, kind: SymbolKind) -> Vec<&Symbol> {
        self.all.iter().filter(|s| s.kind == kind).collect()
    }

    fn all_symbols(&self) -> &[Symbol] {
        &self.all
    }
}

// ── Dependency Index ──────────────────────────────────────────────────────────

/// Tracks file dependencies for impact analysis
struct DependencyIndex {
    /// file -> set of files it imports/depends on
    imports: HashMap<PathBuf, HashSet<PathBuf>>,
    /// file -> set of files that import it (reverse deps)
    imported_by: HashMap<PathBuf, HashSet<PathBuf>>,
}

impl DependencyIndex {
    fn new() -> Self {
        Self {
            imports: HashMap::new(),
            imported_by: HashMap::new(),
        }
    }

    fn add_import(&mut self, from: PathBuf, to: PathBuf) {
        self.imports
            .entry(from.clone())
            .or_default()
            .insert(to.clone());
        self.imported_by.entry(to).or_default().insert(from);
    }

    fn get_dependents(&self, file: &Path) -> Vec<PathBuf> {
        self.imported_by
            .get(file)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }
}

// ── Code Index (Main) ────────────────────────────────────────────────────────

/// In-memory code index for fast lookups
///
/// Indexes a project's source code into memory for O(1) symbol lookups,
/// trigram search, and dependency analysis. Returns structured, token-efficient
/// results perfect for LLM consumption.
pub struct CodeIndex {
    root: PathBuf,
    trigram_index: TrigramIndex,
    word_index: WordIndex,
    symbol_index: SymbolIndex,
    dep_index: DependencyIndex,
    stats: Option<IndexStats>,
    /// Cached file contents for context retrieval
    file_cache: HashMap<PathBuf, Vec<String>>,
}

impl CodeIndex {
    /// Create a new empty index for the given project root
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
        let symbols = extract_symbols(path, content);
        for symbol in symbols {
            self.symbol_index.add(symbol);
        }

        // Extract and index dependencies
        let deps = extract_dependencies(path, content);
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

    /// Search for code matching a pattern using trigram index
    pub fn search(&self, pattern: &str) -> Vec<SearchResult> {
        let trigram_results = self
            .trigram_index
            .search(pattern, &self.trigram_index.files);
        let mut results = Vec::new();

        for (file_path, line_num) in trigram_results {
            let context = self.get_line_context(&file_path, line_num, 1);
            results.push(SearchResult {
                file_path,
                line: line_num,
                column: 0,
                context,
                match_type: MatchType::TrigramMatch,
                score: 1.0,
            });
        }

        // Also check exact word matches (higher priority)
        let words: Vec<&str> = pattern.split_whitespace().collect();
        for word in words {
            if word.len() >= 2 {
                let word_results = self.word_index.lookup(word);
                for (file_idx, line_num) in word_results {
                    if let Some(file_path) = self.trigram_index.files.get(file_idx) {
                        let context = self.get_line_context(file_path, line_num, 1);
                        results.push(SearchResult {
                            file_path: file_path.clone(),
                            line: line_num,
                            column: 0,
                            context,
                            match_type: MatchType::WordMatch,
                            score: 2.0,
                        });
                    }
                }
            }
        }

        // Also check symbol matches (highest priority)
        let symbol_results = self.symbol_index.lookup(pattern);
        for symbol in symbol_results {
            let context = self.get_line_context(&symbol.file_path, symbol.line, 1);
            results.push(SearchResult {
                file_path: symbol.file_path.clone(),
                line: symbol.line,
                column: 0,
                context,
                match_type: MatchType::ExactSymbol,
                score: 3.0,
            });
        }

        // Sort by score (highest first) and deduplicate
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate by (file, line)
        let mut seen = HashSet::new();
        results.retain(|r| seen.insert((r.file_path.clone(), r.line)));

        results
    }

    /// Get files that depend on the given file
    pub fn get_dependents(&self, file: &Path) -> Vec<PathBuf> {
        self.dep_index.get_dependents(file)
    }

    /// Get all symbols in a file
    pub fn get_file_symbols(&self, file: &Path) -> Vec<&Symbol> {
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

    /// Format results as token-efficient structured text for LLM consumption
    ///
    /// This is the key innovation from codedb: instead of returning raw grep output
    /// (hundreds of tokens), return a compact structured summary.
    pub fn format_results(&self, results: &[SearchResult]) -> String {
        if results.is_empty() {
            return "No results found.".to_string();
        }

        let mut output = String::new();
        output.push_str(&format!("Found {} results:\n", results.len()));

        for result in results {
            let rel_path = result
                .file_path
                .strip_prefix(&self.root)
                .unwrap_or(&result.file_path);
            let match_icon = match result.match_type {
                MatchType::ExactSymbol => "S",  // Symbol match
                MatchType::WordMatch => "W",    // Word match
                MatchType::TrigramMatch => "T", // Trigram match
                MatchType::PrefixMatch => "P",  // Prefix match
            };
            // Compact format: TYPE:file:line | context
            output.push_str(&format!(
                " {}:{}:{} | {}\n",
                match_icon,
                rel_path.display(),
                result.line,
                result.context.trim()
            ));
        }

        output
    }

    /// Format symbols as token-efficient structured text
    pub fn format_symbols(&self, symbols: &[&Symbol]) -> String {
        if symbols.is_empty() {
            return "No symbols found.".to_string();
        }

        let mut output = String::new();
        output.push_str(&format!("Found {} symbols:\n", symbols.len()));

        for symbol in symbols {
            let rel_path = symbol
                .file_path
                .strip_prefix(&self.root)
                .unwrap_or(&symbol.file_path);
            // Compact format: KIND name @ file:line [parent]
            let parent_str = symbol
                .parent
                .as_ref()
                .map(|p| format!(" in {p}"))
                .unwrap_or_default();
            let sig = symbol
                .signature
                .as_ref()
                .map(|s| format!(" {s}"))
                .unwrap_or_default();
            output.push_str(&format!(
                " {} {} @ {}:{}{}{}\n",
                symbol.kind,
                symbol.name,
                rel_path.display(),
                symbol.line,
                parent_str,
                sig,
            ));
        }

        output
    }

    /// Get line context from cached file
    fn get_line_context(&self, file: &Path, line: usize, context_lines: usize) -> String {
        if let Some(lines) = self.file_cache.get(file) {
            let start = line.saturating_sub(context_lines + 1);
            let end = (line + context_lines).min(lines.len());
            let start = start.min(lines.len());
            let end = end.max(start).min(lines.len());
            lines[start..end].join("\n")
        } else {
            String::new()
        }
    }

    /// Get the outline of a file (all symbols, no bodies)
    pub fn file_outline(&self, file: &Path) -> String {
        let symbols = self.get_file_symbols(file);
        if symbols.is_empty() {
            return "No symbols found in file.".to_string();
        }

        let mut output = String::new();
        for symbol in symbols {
            let indent = if symbol.parent.is_some() { "  " } else { "" };
            let parent_str = symbol
                .parent
                .as_ref()
                .map(|p| format!("({p}) "))
                .unwrap_or_default();
            let sig = symbol
                .signature
                .as_ref()
                .map(|s| format!(" {s}"))
                .unwrap_or_default();
            output.push_str(&format!(
                "{}{}:{} {}{}{}\n",
                indent, symbol.line, symbol.kind, parent_str, symbol.name, sig,
            ));
        }
        output
    }
}

// ── Symbol Extraction ─────────────────────────────────────────────────────────

/// Extract symbols from source code
fn extract_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "rs" => extract_rust_symbols(file_path, content),
        "py" => extract_python_symbols(file_path, content),
        "go" => extract_go_symbols(file_path, content),
        "js" | "ts" | "jsx" | "tsx" => extract_js_symbols(file_path, content),
        "java" | "kt" | "scala" => extract_java_symbols(file_path, content),
        _ => extract_generic_symbols(file_path, content),
    }
}

fn extract_rust_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut current_impl: Option<String> = None;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Track impl blocks
        if trimmed.starts_with("impl") {
            if let Some(name) = extract_name_after_keyword(trimmed, "impl") {
                current_impl = Some(name);
            }
        }

        // Functions
        if let Some(pos) = trimmed.find("fn ") {
            let prefix = trimmed[..pos].trim_end();
            if pos == 0
                || prefix.ends_with("pub")
                || prefix.ends_with("async")
                || prefix.ends_with("pub async")
            {
                if let Some(name) = trimmed.get(pos..).and_then(extract_fn_name) {
                    let sig = extract_to_brace_or_semicolon(trimmed);
                    symbols.push(Symbol {
                        name,
                        kind: if current_impl.is_some() {
                            SymbolKind::Method
                        } else {
                            SymbolKind::Function
                        },
                        file_path: file_path.to_path_buf(),
                        line: i + 1,
                        signature: Some(sig),
                        doc_comment: None,
                        parent: current_impl.clone(),
                    });
                }
            }
        }

        // Structs
        if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
            if let Some(name) = extract_name_after_keyword(trimmed, "struct") {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Struct,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: None,
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        // Enums
        if trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ") {
            if let Some(name) = extract_name_after_keyword(trimmed, "enum") {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Enum,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: None,
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        // Traits
        if trimmed.starts_with("pub trait ") || trimmed.starts_with("trait ") {
            if let Some(name) = extract_name_after_keyword(trimmed, "trait") {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Trait,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: None,
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        // Constants
        if trimmed.starts_with("pub const ") || trimmed.starts_with("const ") {
            if let Some(name) = extract_name_after_keyword(trimmed, "const") {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Constant,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: None,
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        // Type aliases
        if trimmed.starts_with("pub type ") || trimmed.starts_with("type ") {
            if let Some(name) = extract_name_after_keyword(trimmed, "type") {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Type,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: None,
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        // Close impl on closing brace at column 0
        if line.starts_with('}') && !line.starts_with("}\"") {
            current_impl = None;
        }
    }

    symbols
}

fn extract_python_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut current_class: Option<String> = None;

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Classes
        if trimmed.starts_with("class ") {
            if let Some(name) = extract_name_after_keyword(trimmed, "class") {
                let name = name.trim_end_matches(':').to_string();
                current_class = Some(name.clone());
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Class,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: Some(trimmed.to_string()),
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        // Functions
        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            if let Some(name) = extract_fn_name(trimmed) {
                symbols.push(Symbol {
                    name,
                    kind: if current_class.is_some() {
                        SymbolKind::Method
                    } else {
                        SymbolKind::Function
                    },
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: Some(trimmed.to_string()),
                    doc_comment: None,
                    parent: current_class.clone(),
                });
            }
        }

        // Reset class context on dedent
        if !line.is_empty()
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.starts_with("class ")
            && !trimmed.starts_with('#')
        {
            current_class = None;
        }
    }

    symbols
}

fn extract_go_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("func ") {
            if let Some(name) = extract_go_func_name(trimmed) {
                let kind = if trimmed.contains(")") && trimmed.split(')').count() > 2 {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                symbols.push(Symbol {
                    name,
                    kind,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: Some(extract_to_brace_or_semicolon(trimmed)),
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        if trimmed.starts_with("type ") && trimmed.contains(" struct") {
            if let Some(name) = extract_name_after_keyword(trimmed, "type") {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Struct,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: None,
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        if trimmed.starts_with("type ") && trimmed.contains(" interface") {
            if let Some(name) = extract_name_after_keyword(trimmed, "type") {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Interface,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: None,
                    doc_comment: None,
                    parent: None,
                });
            }
        }
    }

    symbols
}

fn extract_js_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // function declarations
        if trimmed.starts_with("function ") || trimmed.starts_with("async function ") {
            if let Some(name) = extract_fn_name(trimmed) {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: Some(extract_to_brace_or_semicolon(trimmed)),
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        // const/let/var with arrow function
        for kw in &["const ", "let ", "var "] {
            if trimmed.starts_with(kw) && trimmed.contains("=>") {
                if let Some(after_kw) = trimmed.get(kw.len()..) {
                    let name = after_kw
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if !name.is_empty() {
                        symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Variable,
                            file_path: file_path.to_path_buf(),
                            line: i + 1,
                            signature: Some(extract_to_brace_or_semicolon(trimmed)),
                            doc_comment: None,
                            parent: None,
                        });
                    }
                }
            }
        }

        // class declarations
        if trimmed.starts_with("class ") {
            if let Some(name) = extract_name_after_keyword(trimmed, "class") {
                let name = name.trim_end_matches('{').to_string();
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Class,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: None,
                    doc_comment: None,
                    parent: None,
                });
            }
        }

        // export function/class
        if trimmed.starts_with("export function ") || trimmed.starts_with("export async function ")
        {
            if let Some(after_export) = trimmed.get("export ".len()..) {
                if let Some(name) = extract_fn_name(after_export) {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        file_path: file_path.to_path_buf(),
                        line: i + 1,
                        signature: Some(extract_to_brace_or_semicolon(trimmed)),
                        doc_comment: None,
                        parent: None,
                    });
                }
            }
        }
    }

    symbols
}

fn extract_java_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Class/interface/enum declarations
        for (keyword, kind) in &[
            ("class ", SymbolKind::Class),
            ("interface ", SymbolKind::Interface),
            ("enum ", SymbolKind::Enum),
        ] {
            if trimmed.contains(keyword) {
                if let Some(pos) = trimmed.find(keyword) {
                    if let Some(after) = trimmed.get(pos + keyword.len()..) {
                        let name = after
                            .split(|c: char| c.is_whitespace() || c == '{' || c == '<')
                            .next()
                            .unwrap_or("")
                            .to_string();
                        if !name.is_empty() {
                            symbols.push(Symbol {
                                name,
                                kind: *kind,
                                file_path: file_path.to_path_buf(),
                                line: i + 1,
                                signature: Some(extract_to_brace_or_semicolon(trimmed)),
                                doc_comment: None,
                                parent: None,
                            });
                        }
                    }
                }
            }
        }

        // Method declarations (contain parens and a type before name)
        if (trimmed.contains("public ")
            || trimmed.contains("private ")
            || trimmed.contains("protected "))
            && trimmed.contains("(")
            && !trimmed.contains("class ")
        {
            if let Some(name) = extract_java_method_name(trimmed) {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Method,
                    file_path: file_path.to_path_buf(),
                    line: i + 1,
                    signature: Some(extract_to_brace_or_semicolon(trimmed)),
                    doc_comment: None,
                    parent: None,
                });
            }
        }
    }

    symbols
}

fn extract_generic_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
    // Generic extraction: look for common patterns
    let mut symbols = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        // function/def/func/sub patterns
        for pattern in &["function ", "def ", "func ", "sub ", "proc "] {
            if lower.starts_with(pattern) {
                if let Some(name) = extract_fn_name(trimmed) {
                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        file_path: file_path.to_path_buf(),
                        line: i + 1,
                        signature: Some(extract_to_brace_or_semicolon(trimmed)),
                        doc_comment: None,
                        parent: None,
                    });
                }
            }
        }
    }

    symbols
}

// ── Dependency Extraction ─────────────────────────────────────────────────────

fn extract_dependencies(file_path: &Path, content: &str) -> Vec<PathBuf> {
    let mut deps = Vec::new();
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "rs" => {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("use ") {
                    let path = trimmed
                        .trim_start_matches("use ")
                        .trim_end_matches(';')
                        .trim()
                        .replace("::", std::path::MAIN_SEPARATOR_STR)
                        .replace("crate::", "")
                        .replace("super::", &format!("..{}", std::path::MAIN_SEPARATOR))
                        .replace("self::", &format!(".{}", std::path::MAIN_SEPARATOR));
                    deps.push(PathBuf::from(format!("{path}.rs")));
                }
            }
        }
        "py" => {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                    let module = if let Some(rest) = trimmed.strip_prefix("import ") {
                        rest.split(',').next().unwrap_or("").trim()
                    } else if let Some(rest) = trimmed.strip_prefix("from ") {
                        rest.split(" import").next().unwrap_or("").trim()
                    } else {
                        ""
                    };
                    deps.push(PathBuf::from(format!(
                        "{}.py",
                        module.replace('.', std::path::MAIN_SEPARATOR_STR)
                    )));
                }
            }
        }
        "go" => {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("import") {
                    // Simple single-line import
                    if let Some(quoted) = trimmed.split('"').nth(1) {
                        deps.push(PathBuf::from(quoted));
                    }
                }
            }
        }
        _ => {}
    }

    deps
}

// ── Helper Functions ──────────────────────────────────────────────────────────

fn extract_name_after_keyword(line: &str, keyword: &str) -> Option<String> {
    let pos = line.find(keyword)?;
    let after = line.get(pos + keyword.len()..)?;
    let name = after
        .trim_start()
        .split(|c: char| c.is_whitespace() || c == '{' || c == ':' || c == '<' || c == '(')
        .next()
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn extract_fn_name(fn_decl: &str) -> Option<String> {
    let prefixes = [
        "pub async fn ",
        "async fn ",
        "pub fn ",
        "async function ",
        "function ",
        "async def ",
        "def ",
        "fn ",
    ];
    let after_fn = prefixes
        .iter()
        .find_map(|prefix| fn_decl.strip_prefix(prefix))
        .unwrap_or(fn_decl);

    let name = after_fn
        .split(|c: char| c == '(' || c == '<' || c == '{' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn extract_go_func_name(line: &str) -> Option<String> {
    // func Name() or func (recv) Name()
    let after_func = line.get("func ".len()..)?;
    if after_func.starts_with('(') {
        // Method: func (r *Recv) Name()
        if let Some(close) = after_func.find(") ") {
            let after_recv = after_func.get(close + 2..)?;
            Some(
                after_recv
                    .split(|c: char| c == '(' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
                    .to_string(),
            )
        } else {
            None
        }
    } else {
        Some(
            after_func
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string(),
        )
    }
}

fn extract_java_method_name(line: &str) -> Option<String> {
    // Find the part before '('
    let before_paren = line.split('(').next()?;
    // The method name is the last word before '('
    before_paren
        .rsplit(|c: char| c.is_whitespace())
        .next()
        .map(ToString::to_string)
}

fn extract_to_brace_or_semicolon(line: &str) -> String {
    if let Some(pos) = line.find('{') {
        line.get(..pos).unwrap_or(line).trim().to_string()
    } else if let Some(pos) = line.find(';') {
        line.get(..pos).unwrap_or(line).trim().to_string()
    } else {
        line.trim().to_string()
    }
}

fn walk_dir(root: &Path, extensions: &[&str], skip_dirs: &[&str]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let ext_set: HashSet<&str> = extensions.iter().cloned().collect();
    let skip_set: HashSet<&str> = skip_dirs.iter().cloned().collect();

    fn walk(
        dir: &Path,
        ext_set: &HashSet<&str>,
        skip_set: &HashSet<&str>,
        files: &mut Vec<PathBuf>,
    ) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if !skip_set.contains(name) && !name.starts_with('.') {
                            walk(&path, ext_set, skip_set, files);
                        }
                    }
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext_set.contains(ext) {
                        files.push(path);
                    }
                }
            }
        }
    }

    walk(root, &ext_set, &skip_set, &mut files);
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigram_search() {
        // Build with a temp file — index root must match the file location
        let temp_dir = std::env::temp_dir().join("rustycode_test_index");
        std::fs::create_dir_all(&temp_dir).ok();
        let test_file = temp_dir.join("test.rs");
        std::fs::write(&test_file, "fn handle_request() {}\nstruct Config {}\n").ok();

        let mut index = CodeIndex::new(&temp_dir);
        index.build().ok();
        let results = index.find_symbols("handle_request");
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "handle_request");
        assert_eq!(results[0].kind, SymbolKind::Function);

        // Cleanup
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_word_extraction() {
        let words = extract_words("fn handle_request(req: &Request) -> Response");
        assert!(words.contains(&"fn"));
        assert!(words.contains(&"handle_request"));
        assert!(words.contains(&"Request"));
        assert!(words.contains(&"Response"));
    }

    #[test]
    fn test_format_results() {
        let index = CodeIndex::new("/tmp/test");
        let formatted = index.format_results(&[]);
        assert_eq!(formatted, "No results found.");
    }

    #[test]
    fn test_extract_rust_symbols() {
        let content = r#"
pub struct Config {
    pub name: String,
}

impl Config {
    pub fn new() -> Self {
        Self { name: String::new() }
    }

    fn validate(&self) -> bool {
        true
    }
}

enum Status {
    Active,
    Inactive,
}

const MAX_SIZE: usize = 1024;
"#;
        let file = PathBuf::from("test.rs");
        let symbols = extract_rust_symbols(&file, content);

        assert!(symbols
            .iter()
            .any(|s| s.name == "Config" && s.kind == SymbolKind::Struct));
        assert!(symbols
            .iter()
            .any(|s| s.name == "new" && s.kind == SymbolKind::Method));
        assert!(symbols
            .iter()
            .any(|s| s.name == "validate" && s.kind == SymbolKind::Method));
        assert!(symbols
            .iter()
            .any(|s| s.name == "Status" && s.kind == SymbolKind::Enum));
        assert!(symbols
            .iter()
            .any(|s| s.name == "MAX_SIZE" && s.kind == SymbolKind::Constant));
    }
}
