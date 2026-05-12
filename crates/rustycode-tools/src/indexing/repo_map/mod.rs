use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::Parser;

pub mod languages;
pub mod parser;

pub use languages::Lang;

/// A structural map of the codebase, token-budgeted for LLM consumption.
pub struct RepoMap {
    map: String,
    file_summaries: HashMap<PathBuf, FileSummary>,
    total_tokens: usize,
}

/// Structural summary of a single source file.
#[derive(Debug, Clone)]
pub struct FileSummary {
    pub path: PathBuf,
    pub symbols: Vec<SymbolInfo>,
    pub imports: Vec<String>,
}

/// A single symbol extracted from source code.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    pub line: usize,
    pub docs: Option<String>,
}

/// The kind of a code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Enum,
    Trait,
    Interface,
    Module,
    Constant,
    TypeAlias,
    Impl,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function => write!(f, "fn"),
            Self::Method => write!(f, "method"),
            Self::Struct => write!(f, "struct"),
            Self::Class => write!(f, "class"),
            Self::Enum => write!(f, "enum"),
            Self::Trait => write!(f, "trait"),
            Self::Interface => write!(f, "interface"),
            Self::Module => write!(f, "mod"),
            Self::Constant => write!(f, "const"),
            Self::TypeAlias => write!(f, "type"),
            Self::Impl => write!(f, "impl"),
        }
    }
}

/// Approximate characters per token (used for budget estimation).
pub(crate) const CHARS_PER_TOKEN: usize = 4;

/// Default token budget when none is specified.
pub const DEFAULT_TOKEN_BUDGET: usize = 4000;

/// File extensions that are always indexed.
pub(crate) const INDEXED_EXTENSIONS: &[&str] = &["rs", "py", "js", "jsx", "ts", "tsx", "go"];

/// Extensions also indexed via regex fallback (no tree-sitter grammar).
pub(crate) const REGEX_EXTENSIONS: &[&str] = &["java", "kt", "scala", "c", "cpp", "h", "hpp", "rb"];

impl RepoMap {
    /// Build a repo map from the project root with a token budget.
    pub fn build(project_root: &Path, token_budget: usize) -> Result<Self> {
        let mut file_summaries = HashMap::new();
        let mut files = parser::collect_source_files(project_root)?;

        files.sort_by(|a, b| {
            let a_time = parser::file_modified_time(a);
            let b_time = parser::file_modified_time(b);
            b_time.cmp(&a_time).then_with(|| a.cmp(b))
        });

        let mut ts_parser = Parser::new();

        for file_path in &files {
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            let summary = match Lang::from_ext(ext) {
                Some(lang) => {
                    parser::parse_with_treesitter(&mut ts_parser, lang, file_path, &content)
                }
                None => parser::parse_with_regex(file_path, &content),
            };

            if !summary.symbols.is_empty() || !summary.imports.is_empty() {
                let rel_path = file_path
                    .strip_prefix(project_root)
                    .unwrap_or(file_path)
                    .to_path_buf();
                file_summaries.insert(rel_path, summary);
            }
        }

        let (map, total_tokens) = parser::format_map(&file_summaries, token_budget);

        Ok(Self {
            map,
            file_summaries,
            total_tokens,
        })
    }

    /// Get the formatted map string for LLM context injection.
    pub fn to_map_string(&self) -> &str {
        &self.map
    }

    /// Get summary for a specific file (by relative path).
    pub fn for_file(&self, path: &Path) -> Option<&FileSummary> {
        self.file_summaries.get(path)
    }

    /// Estimate token count (rough: 1 token = 4 chars).
    pub const fn estimated_tokens(&self) -> usize {
        self.total_tokens
    }

    /// Number of files in the map.
    pub fn file_count(&self) -> usize {
        self.file_summaries.len()
    }

    /// Total number of symbols across all files.
    pub fn symbol_count(&self) -> usize {
        self.file_summaries.values().map(|s| s.symbols.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_env() -> tempfile::TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn test_parse_rust_file() {
        let dir = setup_test_env();
        let rust_file = dir.path().join("example.rs");
        std::fs::write(
            &rust_file,
            r#"/// A user in the system.
pub struct User {
    name: String,
    age: u32,
}

impl User {
    pub fn new(name: &str, age: u32) -> Self {
        Self { name: name.to_string(), age }
    }

    fn greet(&self) -> String {
        format!("Hello, {}!", self.name)
    }
}

#[non_exhaustive]
pub enum Status {
    Active,
    Inactive,
}

const MAX_AGE: u32 = 150;

type UserId = u64;

pub trait HasName {
    fn name(&self) -> &str;
}

mod submodule;
"#,
        )
        .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let summary = map
            .for_file(Path::new("example.rs"))
            .expect("missing file summary");

        // Check that we found the expected symbols
        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(
            names.contains(&"User"),
            "Should find User struct, got: {:?}",
            names
        );
        assert!(names.contains(&"Status"), "Should find Status enum");
        assert!(names.contains(&"MAX_AGE"), "Should find MAX_AGE const");
    }

    #[test]
    fn test_budget_truncation() {
        let dir = setup_test_env();
        for i in 0..10 {
            let file = dir.path().join(format!("file_{}.rs", i));
            std::fs::write(&file, "pub fn my_function() {}\n").expect("failed to write test file");
        }

        // Build with a very small budget (e.g. 50 tokens = ~200 chars)
        let map = RepoMap::build(dir.path(), 50).expect("failed to build repo map");
        let _map_str = map.to_map_string();

        // Budget truncation only affects the map string, not file_summaries.
        // Verify the map string is non-empty and tokens are bounded.
        assert!(!map.to_map_string().is_empty());
        assert!(map.estimated_tokens() <= 60); // give some buffer
    }

    #[test]
    fn test_repo_map_counts() {
        let dir = setup_test_env();
        let f1 = dir.path().join("f1.rs");
        std::fs::write(&f1, "pub fn a() {}").expect("fail");
        let f2 = dir.path().join("f2.py");
        std::fs::write(&f2, "def b(): pass").expect("fail");

        let map = RepoMap::build(dir.path(), 4000).expect("fail");
        assert_eq!(map.file_count(), 2);
        assert_eq!(map.symbol_count(), 2);
    }
}
