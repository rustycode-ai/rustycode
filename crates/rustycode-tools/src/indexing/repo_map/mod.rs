use crate::indexing::symbols::{collect_source_files, extract_file, renderers, FileOutline};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// A structural map of the codebase, token-budgeted for LLM consumption.
pub struct RepoMap {
    map: String,
    outlines: Vec<FileOutline>,
    total_tokens: usize,
}

/// Default token budget when none is specified.
pub const DEFAULT_TOKEN_BUDGET: usize = 4000;

impl RepoMap {
    /// Build a repo map from the project root with a token budget.
    pub fn build(project_root: &Path, token_budget: usize) -> Result<Self> {
        let files: Vec<PathBuf> = collect_source_files(project_root)?;
        let mut all_outlines = Vec::new();

        for file_path in files {
            let content = match std::fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let outline = extract_file(&file_path, &content);
            if !outline.symbols.is_empty() || !outline.imports.is_empty() {
                all_outlines.push(outline);
            }
        }

        // Sort outlines for deterministic rendering (e.g. by path)
        all_outlines.sort_by(|a, b| a.path.cmp(&b.path));

        let map = renderers::render_repo_map(&all_outlines, token_budget);
        let total_tokens = map.len() / crate::indexing::symbols::renderers::CHARS_PER_TOKEN;

        Ok(Self {
            map,
            outlines: all_outlines,
            total_tokens,
        })
    }

    /// Get the formatted map string for LLM context injection.
    pub fn to_map_string(&self) -> &str {
        &self.map
    }

    /// Estimate token count (rough: 1 token = 4 chars).
    pub const fn estimated_tokens(&self) -> usize {
        self.total_tokens
    }

    /// Number of files in the map.
    pub fn file_count(&self) -> usize {
        self.outlines.len()
    }

    /// Total number of symbols across all files.
    pub fn symbol_count(&self) -> usize {
        self.outlines.iter().map(|o| o.symbols.len()).sum()
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
}
"#,
        )
        .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");

        // Check that we found the expected symbols
        assert!(map.symbol_count() >= 2);
        assert!(map.to_map_string().contains("User"));
        assert!(
            map.to_map_string().contains("Struct") || map.to_map_string().contains("Function"),
            "expected Struct or Function kind in map output"
        );
    }

    #[test]
    fn test_budget_truncation() {
        let dir = setup_test_env();
        for i in 0..10 {
            let file = dir.path().join(format!("file_{}.rs", i));
            std::fs::write(&file, "pub fn my_function() {}\n").expect("failed to write test file");
        }

        // Build with a very small budget (e.g. 5 tokens = ~20 chars)
        let map = RepoMap::build(dir.path(), 5).expect("failed to build repo map");

        assert!(!map.to_map_string().is_empty());
        // The map should contain at most a few files because of the small budget
        assert!(map.estimated_tokens() < 100);
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
