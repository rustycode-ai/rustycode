use super::crawler::extract_symbols;
use super::*;
use std::path::PathBuf;
use storage::extract_words;

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
fn test_extract_symbols() {
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
    let symbols = extract_symbols(&file, content);

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
