//! Tree-sitter based repo map for LLM context efficiency.
//!
//! Produces a token-budgeted structural summary of the codebase:
//! function signatures, struct/class definitions, impl blocks, trait definitions,
//! imports, and type aliases.
//!
//! # Architecture
//!
//! - Walk project tree respecting `.gitignore`
//! - Parse each source file with the appropriate tree-sitter grammar
//! - Extract symbols via tree-sitter queries
//! - Format output as a concise tree for LLM injection
//! - Fallback to regex parsing for unsupported languages
//!
//! # Example
//!
//! ```rust,no_run
//! use rustycode_tools::RepoMap;
//! use std::path::Path;
//!
//! let map = RepoMap::build(Path::new("."), 4000).expect("failed to build repo map");
//! println!("{}", map.to_map_string());
//! ```

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Language, Node, Parser};

// ── Public Types ─────────────────────────────────────────────────────────────

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

/// Source language detected from file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
}

impl Lang {
    fn from_ext(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" => Some(Self::Python),
            "js" | "jsx" => Some(Self::JavaScript),
            "ts" | "tsx" => Some(Self::TypeScript),
            "go" => Some(Self::Go),
            _ => None,
        }
    }

    fn language(self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
        }
    }
}

// ── Constants ────────────────────────────────────────────────────────────────

/// Approximate characters per token (used for budget estimation).
const CHARS_PER_TOKEN: usize = 4;

/// Default token budget when none is specified.
pub const DEFAULT_TOKEN_BUDGET: usize = 4000;

/// File extensions that are always indexed.
const INDEXED_EXTENSIONS: &[&str] = &["rs", "py", "js", "jsx", "ts", "tsx", "go"];

/// Directories to skip during traversal.
const SKIP_DIRS: &[&str] = &[
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
    ".idea",
    ".vscode",
];

// ── Implementation ───────────────────────────────────────────────────────────

impl RepoMap {
    /// Build a repo map from the project root with a token budget.
    ///
    /// The token budget controls how much detail to include. Files are prioritized:
    /// 1. Recently modified files first
    /// 2. Then alphabetical order
    ///
    /// When budget is exceeded, less important symbols are trimmed.
    pub fn build(project_root: &Path, token_budget: usize) -> Result<Self> {
        let mut file_summaries = HashMap::new();
        let mut files = collect_source_files(project_root)?;

        // Sort: recently modified first, then alphabetical
        files.sort_by(|a, b| {
            let a_time = file_modified_time(a);
            let b_time = file_modified_time(b);
            b_time.cmp(&a_time).then_with(|| a.cmp(b))
        });

        let mut parser = Parser::new();

        for file_path in &files {
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            let summary = match Lang::from_ext(ext) {
                Some(lang) => parse_with_treesitter(&mut parser, lang, file_path, &content),
                None => parse_with_regex(file_path, &content),
            };

            if !summary.symbols.is_empty() || !summary.imports.is_empty() {
                let rel_path = file_path
                    .strip_prefix(project_root)
                    .unwrap_or(file_path)
                    .to_path_buf();
                file_summaries.insert(rel_path, summary);
            }
        }

        // Build formatted map within token budget
        let (map, total_tokens) = format_map(&file_summaries, token_budget);

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

// ── Tree-sitter Parsing ──────────────────────────────────────────────────────

fn parse_with_treesitter(
    parser: &mut Parser,
    lang: Lang,
    path: &Path,
    content: &str,
) -> FileSummary {
    let language = lang.language();
    parser.set_language(&language).ok();

    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => {
            return FileSummary {
                path: path.to_path_buf(),
                symbols: Vec::new(),
                imports: Vec::new(),
            }
        }
    };

    let root = tree.root_node();
    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    match lang {
        Lang::Rust => extract_rust_symbols_ts(&root, content, &mut symbols, &mut imports),
        Lang::Python => extract_python_symbols_ts(&root, content, &mut symbols, &mut imports),
        Lang::JavaScript | Lang::TypeScript => {
            extract_js_ts_symbols_ts(&root, content, &mut symbols, &mut imports)
        }
        Lang::Go => extract_go_symbols_ts(&root, content, &mut symbols, &mut imports),
    }

    FileSummary {
        path: path.to_path_buf(),
        symbols,
        imports,
    }
}

// ── Rust Symbol Extraction (Tree-sitter) ─────────────────────────────────────

fn extract_rust_symbols_ts(
    root: &Node,
    source: &str,
    symbols: &mut Vec<SymbolInfo>,
    imports: &mut Vec<String>,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_item" | "async_function_item" => {
                if let Some(sym) = rust_fn_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            "struct_item" => {
                if let Some(sym) = rust_named_symbol(&child, source, SymbolKind::Struct) {
                    symbols.push(sym);
                }
            }
            "enum_item" => {
                if let Some(sym) = rust_named_symbol(&child, source, SymbolKind::Enum) {
                    symbols.push(sym);
                }
            }
            "trait_item" => {
                if let Some(sym) = rust_named_symbol(&child, source, SymbolKind::Trait) {
                    symbols.push(sym);
                }
            }
            "type_item" => {
                if let Some(sym) = rust_type_alias_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            "const_item" => {
                if let Some(sym) = rust_const_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            "impl_item" => {
                extract_rust_impl(&child, source, symbols);
            }
            "use_declaration" => {
                if let Some(imp) = node_text(&child, source) {
                    imports.push(imp);
                }
            }
            "mod_item" => {
                if let Some(sym) = rust_named_symbol(&child, source, SymbolKind::Module) {
                    symbols.push(sym);
                }
            }
            // Recurse into expression statements that might contain inner items
            _ => {}
        }
    }
}

fn extract_rust_impl(impl_node: &Node, source: &str, symbols: &mut Vec<SymbolInfo>) {
    // Get the type name from impl block
    let type_name = impl_node
        .child_by_field_name("type")
        .and_then(|n| node_text(&n, source));

    // Extract the impl header as the signature
    let impl_sig = node_text(impl_node, source)
        .map(|s| {
            // Take just the first line (the impl declaration, not the body)
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    if let Some(name) = type_name {
        symbols.push(SymbolInfo {
            name,
            kind: SymbolKind::Impl,
            signature: impl_sig,
            line: impl_node.start_position().row + 1,
            docs: extract_doc_comment(impl_node, source),
        });
    }

    // Extract methods from the impl body - recurse into all child nodes
    // tree-sitter-rust may wrap function items in intermediate nodes
    extract_rust_impl_children(impl_node, source, symbols);
}

fn extract_rust_impl_children(node: &Node, source: &str, symbols: &mut Vec<SymbolInfo>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_item" | "async_function_item" => {
                if let Some(sym) = rust_fn_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            _ => {
                // Recurse into body/block nodes to find nested function items
                extract_rust_impl_children(&child, source, symbols);
            }
        }
    }
}

fn rust_fn_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::Function,
        signature: sig,
        line: node.start_position().row + 1,
        docs: extract_doc_comment(node, source),
    })
}

fn rust_named_symbol(node: &Node, source: &str, kind: SymbolKind) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind,
        signature: sig,
        line: node.start_position().row + 1,
        docs: extract_doc_comment(node, source),
    })
}

fn rust_type_alias_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches(';')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::TypeAlias,
        signature: sig,
        line: node.start_position().row + 1,
        docs: extract_doc_comment(node, source),
    })
}

fn rust_const_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches(';')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::Constant,
        signature: sig,
        line: node.start_position().row + 1,
        docs: extract_doc_comment(node, source),
    })
}

// ── Python Symbol Extraction (Tree-sitter) ───────────────────────────────────

fn extract_python_symbols_ts(
    root: &Node,
    source: &str,
    symbols: &mut Vec<SymbolInfo>,
    imports: &mut Vec<String>,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(sym) = python_fn_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            "class_definition" => {
                extract_python_class(&child, source, symbols);
            }
            "import_statement" | "import_from_statement" => {
                if let Some(imp) = node_text(&child, source) {
                    imports.push(imp);
                }
            }
            "decorated_definition" => {
                // Look for the actual definition inside the decorator
                let mut dec_cursor = child.walk();
                for dec_child in child.children(&mut dec_cursor) {
                    match dec_child.kind() {
                        "function_definition" => {
                            if let Some(sym) = python_fn_symbol(&dec_child, source) {
                                symbols.push(sym);
                            }
                        }
                        "class_definition" => {
                            extract_python_class(&dec_child, source, symbols);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn extract_python_class(node: &Node, source: &str, symbols: &mut Vec<SymbolInfo>) {
    if let Some(name) = node.child_by_field_name("name") {
        if let Some(name_text) = node_text(&name, source) {
            let sig = node_text(node, source)
                .map(|s| {
                    s.lines()
                        .next()
                        .unwrap_or("")
                        .trim_end_matches(':')
                        .trim()
                        .to_string()
                })
                .unwrap_or_default();

            symbols.push(SymbolInfo {
                name: name_text,
                kind: SymbolKind::Class,
                signature: sig,
                line: node.start_position().row + 1,
                docs: extract_python_docstring(node, source),
            });
        }
    }

    // Extract methods from the class body
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "block" {
            let mut block_cursor = child.walk();
            for block_child in child.children(&mut block_cursor) {
                match block_child.kind() {
                    "function_definition" => {
                        if let Some(sym) = python_fn_symbol(&block_child, source) {
                            symbols.push(sym);
                        }
                    }
                    "decorated_definition" => {
                        let mut dec_cursor = block_child.walk();
                        for dec_child in block_child.children(&mut dec_cursor) {
                            if dec_child.kind() == "function_definition" {
                                if let Some(sym) = python_fn_symbol(&dec_child, source) {
                                    symbols.push(sym);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn python_fn_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches(':')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::Function,
        signature: sig,
        line: node.start_position().row + 1,
        docs: extract_python_docstring(node, source),
    })
}

fn extract_python_docstring(node: &Node, source: &str) -> Option<String> {
    // Look for the body block and check if the first statement is an expression_string (docstring)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "block" {
            let mut block_cursor = child.walk();
            let first_statement = child.children(&mut block_cursor).next()?;
            if first_statement.kind() == "expression_statement" {
                let first_child = first_statement.child(0)?;
                if first_child.kind() == "string" {
                    return node_text(&first_child, source)
                        .map(|s| s.trim_matches('"').trim_matches('\'').to_string());
                }
            }
        }
    }
    None
}

// ── JavaScript/TypeScript Symbol Extraction (Tree-sitter) ────────────────────

fn extract_js_ts_symbols_ts(
    root: &Node,
    source: &str,
    symbols: &mut Vec<SymbolInfo>,
    imports: &mut Vec<String>,
) {
    let mut cursor = root.walk();
    extract_js_ts_recursive(root, source, symbols, imports, &mut cursor);
}

fn extract_js_ts_recursive<'a>(
    node: &Node<'a>,
    source: &str,
    symbols: &mut Vec<SymbolInfo>,
    imports: &mut Vec<String>,
    cursor: &mut tree_sitter::TreeCursor<'a>,
) {
    for child in node.children(cursor) {
        match child.kind() {
            "function_declaration" | "generator_function_declaration" => {
                if let Some(sym) = js_fn_symbol(&child, source, SymbolKind::Function) {
                    symbols.push(sym);
                }
            }
            "class_declaration" | "class" => {
                if let Some(sym) = js_class_symbol(&child, source) {
                    symbols.push(sym);
                }
                // Extract methods from class body
                extract_js_class_methods(&child, source, symbols);
            }
            "lexical_declaration" | "variable_declaration" => {
                extract_js_variable_declarations(&child, source, symbols);
            }
            "import_statement" | "import_declaration" => {
                if let Some(imp) = node_text(&child, source) {
                    imports.push(imp);
                }
            }
            "export_statement" => {
                let mut exp_cursor = child.walk();
                for exp_child in child.children(&mut exp_cursor) {
                    match exp_child.kind() {
                        "function_declaration" => {
                            if let Some(sym) =
                                js_fn_symbol(&exp_child, source, SymbolKind::Function)
                            {
                                symbols.push(sym);
                            }
                        }
                        "class_declaration" | "class" => {
                            if let Some(sym) = js_class_symbol(&exp_child, source) {
                                symbols.push(sym);
                            }
                            extract_js_class_methods(&exp_child, source, symbols);
                        }
                        "lexical_declaration" => {
                            extract_js_variable_declarations(&exp_child, source, symbols);
                        }
                        _ => {}
                    }
                }
            }
            // Method definitions inside classes (JS/TS)
            "method_definition" | "public_field_definition" => {
                if let Some(sym) = js_method_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            // Interface declarations (TypeScript)
            "interface_declaration" => {
                if let Some(sym) = ts_interface_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            // Type alias declarations (TypeScript)
            "type_alias_declaration" => {
                if let Some(sym) = ts_type_alias_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            // Enum declarations (TypeScript)
            "enum_declaration" => {
                if let Some(sym) = ts_enum_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            _ => {
                // Recurse into nested structures
                let mut inner_cursor = child.walk();
                extract_js_ts_recursive(&child, source, symbols, imports, &mut inner_cursor);
            }
        }
    }
}

fn js_fn_symbol(node: &Node, source: &str, kind: SymbolKind) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind,
        signature: sig,
        line: node.start_position().row + 1,
        docs: None,
    })
}

fn js_class_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::Class,
        signature: sig,
        line: node.start_position().row + 1,
        docs: None,
    })
}

fn js_method_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::Method,
        signature: sig,
        line: node.start_position().row + 1,
        docs: None,
    })
}

fn extract_js_class_methods(node: &Node, source: &str, symbols: &mut Vec<SymbolInfo>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_body" || child.kind() == "declaration" {
            let mut body_cursor = child.walk();
            for body_child in child.children(&mut body_cursor) {
                match body_child.kind() {
                    "method_definition" | "public_field_definition" => {
                        if let Some(sym) = js_method_symbol(&body_child, source) {
                            symbols.push(sym);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn extract_js_variable_declarations(node: &Node, source: &str, symbols: &mut Vec<SymbolInfo>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            let name = child.child_by_field_name("name");
            if let Some(name_node) = name {
                if let Some(name_text) = node_text(&name_node, source) {
                    // Check if it's an arrow function
                    let value = child.child_by_field_name("value");
                    let is_arrow = value
                        .map(|v| v.kind() == "arrow_function" || v.kind() == "function_expression")
                        .unwrap_or(false);

                    if is_arrow {
                        let sig = node_text(&child, source)
                            .map(|s| {
                                s.lines()
                                    .next()
                                    .unwrap_or("")
                                    .trim_end_matches('{')
                                    .trim()
                                    .to_string()
                            })
                            .unwrap_or_default();

                        symbols.push(SymbolInfo {
                            name: name_text,
                            kind: SymbolKind::Function,
                            signature: sig,
                            line: child.start_position().row + 1,
                            docs: None,
                        });
                    }
                }
            }
        }
    }
}

fn ts_interface_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::Interface,
        signature: sig,
        line: node.start_position().row + 1,
        docs: None,
    })
}

fn ts_type_alias_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches(';')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::TypeAlias,
        signature: sig,
        line: node.start_position().row + 1,
        docs: None,
    })
}

fn ts_enum_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::Enum,
        signature: sig,
        line: node.start_position().row + 1,
        docs: None,
    })
}

// ── Go Symbol Extraction (Tree-sitter) ───────────────────────────────────────

fn extract_go_symbols_ts(
    root: &Node,
    source: &str,
    symbols: &mut Vec<SymbolInfo>,
    imports: &mut Vec<String>,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(sym) = go_fn_symbol(&child, source, SymbolKind::Function) {
                    symbols.push(sym);
                }
            }
            "method_declaration" => {
                if let Some(sym) = go_method_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            "type_declaration" => {
                extract_go_type_declaration(&child, source, symbols);
            }
            "import_declaration" => {
                if let Some(imp) = node_text(&child, source) {
                    imports.push(imp);
                }
            }
            "const_declaration" => {
                extract_go_consts(&child, source, symbols);
            }
            _ => {}
        }
    }
}

fn go_fn_symbol(node: &Node, source: &str, kind: SymbolKind) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind,
        signature: sig,
        line: node.start_position().row + 1,
        docs: extract_go_doc_comment(node, source),
    })
}

fn go_method_symbol(node: &Node, source: &str) -> Option<SymbolInfo> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let sig = node_text(node, source)
        .map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        })
        .unwrap_or_default();

    Some(SymbolInfo {
        name: name_text,
        kind: SymbolKind::Method,
        signature: sig,
        line: node.start_position().row + 1,
        docs: extract_go_doc_comment(node, source),
    })
}

fn extract_go_type_declaration(node: &Node, source: &str, symbols: &mut Vec<SymbolInfo>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_spec" {
            let name = child.child_by_field_name("name");
            let kind_type = child.child_by_field_name("type");

            if let (Some(name_node), Some(type_node)) = (name, kind_type) {
                if let Some(name_text) = node_text(&name_node, source) {
                    let kind = match type_node.kind() {
                        "struct_type" => SymbolKind::Struct,
                        "interface_type" => SymbolKind::Interface,
                        _ => SymbolKind::TypeAlias,
                    };

                    let sig = node_text(node, source)
                        .map(|s| {
                            s.lines()
                                .next()
                                .unwrap_or("")
                                .trim_end_matches('{')
                                .trim()
                                .to_string()
                        })
                        .unwrap_or_default();

                    symbols.push(SymbolInfo {
                        name: name_text,
                        kind,
                        signature: sig,
                        line: node.start_position().row + 1,
                        docs: extract_go_doc_comment(node, source),
                    });
                }
            }
        }
    }
}

fn extract_go_consts(node: &Node, source: &str, symbols: &mut Vec<SymbolInfo>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "const_spec" {
            let name = child.child_by_field_name("name");
            if let Some(name_node) = name {
                if let Some(name_text) = node_text(&name_node, source) {
                    let sig = node_text(&child, source)
                        .map(|s| {
                            s.lines()
                                .next()
                                .unwrap_or("")
                                .trim_end_matches(';')
                                .trim()
                                .to_string()
                        })
                        .unwrap_or_default();

                    symbols.push(SymbolInfo {
                        name: name_text,
                        kind: SymbolKind::Constant,
                        signature: sig,
                        line: child.start_position().row + 1,
                        docs: None,
                    });
                }
            }
        }
    }
}

// ── Regex Fallback Parsing ───────────────────────────────────────────────────

/// Fallback regex-based symbol extraction for unsupported languages.
/// Reuses patterns from `code_index.rs`.
fn parse_with_regex(path: &Path, content: &str) -> FileSummary {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    match ext {
        "java" | "kt" | "scala" => {
            extract_java_regex(path, content, &mut symbols);
        }
        "c" | "cpp" | "h" | "hpp" => {
            extract_c_regex(content, &mut symbols);
        }
        "rb" => {
            extract_ruby_regex(content, &mut symbols);
        }
        _ => {
            extract_generic_regex(content, &mut symbols);
        }
    }

    // Extract imports for known patterns
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ")
            || trimmed.starts_with("from ")
            || trimmed.starts_with("#include")
            || trimmed.starts_with("require ")
        {
            imports.push(trimmed.to_string());
        }
    }

    FileSummary {
        path: path.to_path_buf(),
        symbols,
        imports,
    }
}

fn extract_java_regex(_path: &Path, content: &str, symbols: &mut Vec<SymbolInfo>) {
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Class/interface/enum
        for (keyword, kind) in &[
            ("class ", SymbolKind::Class),
            ("interface ", SymbolKind::Interface),
            ("enum ", SymbolKind::Enum),
        ] {
            if let Some(pos) = trimmed.find(keyword) {
                if let Some(after) = trimmed.get(pos + keyword.len()..) {
                    let name = after
                        .split(|c: char| c.is_whitespace() || c == '{' || c == '<')
                        .next()
                        .unwrap_or("");
                    if !name.is_empty() {
                        symbols.push(SymbolInfo {
                            name: name.to_string(),
                            kind: *kind,
                            signature: trimmed.trim_end_matches('{').trim().to_string(),
                            line: i + 1,
                            docs: None,
                        });
                    }
                }
            }
        }
    }
}

fn extract_c_regex(content: &str, symbols: &mut Vec<SymbolInfo>) {
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Struct/enum/class definitions
        for (keyword, kind) in &[
            ("struct ", SymbolKind::Struct),
            ("enum ", SymbolKind::Enum),
            ("class ", SymbolKind::Class),
        ] {
            if trimmed.starts_with(keyword) {
                if let Some(name) = trimmed
                    .strip_prefix(keyword)
                    .map(|s| {
                        s.split(|c: char| c.is_whitespace() || c == '{' || c == ':')
                            .next()
                            .unwrap_or("")
                    })
                    .filter(|s| !s.is_empty())
                {
                    symbols.push(SymbolInfo {
                        name: name.to_string(),
                        kind: *kind,
                        signature: trimmed.trim_end_matches('{').trim().to_string(),
                        line: i + 1,
                        docs: None,
                    });
                }
            }
        }
    }
}

fn extract_ruby_regex(content: &str, symbols: &mut Vec<SymbolInfo>) {
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("def ") {
            let name = trimmed
                .strip_prefix("def ")
                .unwrap_or("")
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                symbols.push(SymbolInfo {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    signature: trimmed.trim().to_string(),
                    line: i + 1,
                    docs: None,
                });
            }
        }

        if trimmed.starts_with("class ") || trimmed.starts_with("module ") {
            let keyword = if trimmed.starts_with("class ") {
                "class "
            } else {
                "module "
            };
            let name = trimmed
                .strip_prefix(keyword)
                .unwrap_or("")
                .split(|c: char| c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                let kind = if keyword == "class " {
                    SymbolKind::Class
                } else {
                    SymbolKind::Module
                };
                symbols.push(SymbolInfo {
                    name: name.to_string(),
                    kind,
                    signature: trimmed.trim().to_string(),
                    line: i + 1,
                    docs: None,
                });
            }
        }
    }
}

fn extract_generic_regex(content: &str, symbols: &mut Vec<SymbolInfo>) {
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        for pattern in &["function ", "def ", "func ", "sub ", "proc "] {
            if lower.starts_with(pattern) {
                let name = trimmed
                    .split(|c: char| c == '(' || c == '{' || c == ':' || c.is_whitespace())
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    symbols.push(SymbolInfo {
                        name,
                        kind: SymbolKind::Function,
                        signature: trimmed.trim().to_string(),
                        line: i + 1,
                        docs: None,
                    });
                }
            }
        }
    }
}

// ── Formatting ───────────────────────────────────────────────────────────────

fn format_map(
    file_summaries: &HashMap<PathBuf, FileSummary>,
    token_budget: usize,
) -> (String, usize) {
    let mut output = String::new();
    let budget_chars = token_budget * CHARS_PER_TOKEN;

    // Collect and sort files by directory structure
    let mut entries: Vec<(&PathBuf, &FileSummary)> = file_summaries.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    for (rel_path, summary) in &entries {
        let file_block = format_file_entry(rel_path, summary);

        // Check if adding this file would exceed budget
        if output.len().saturating_add(file_block.len()) > budget_chars {
            if output.is_empty() {
                // Even the first file exceeds budget — add a truncated version
                let trunc_len = budget_chars.saturating_sub(20); // leave room for truncation marker
                if trunc_len > 0 {
                    output.push_str(truncate_str_safe(&file_block, trunc_len));
                    output.push_str("\n... (truncated)\n");
                }
            } else {
                // Try to add a truncated version of this file
                let remaining = budget_chars.saturating_sub(output.len()).saturating_sub(20);
                if remaining > 30 {
                    output.push_str(truncate_str_safe(&file_block, remaining));
                    output.push_str("\n... (truncated)\n");
                }
            }
            break;
        }

        output.push_str(&file_block);
    }

    let total_tokens = output.len() / CHARS_PER_TOKEN;
    (output, total_tokens)
}

fn format_file_entry(rel_path: &Path, summary: &FileSummary) -> String {
    let mut out = String::new();
    let path_str = rel_path.to_string_lossy();

    out.push_str(&format!("{path_str}:\n"));

    for symbol in &summary.symbols {
        let sig = if symbol.signature.is_empty() {
            symbol.name.clone()
        } else {
            // Keep the signature concise: truncate at first newline and limit length
            let sig_line = symbol.signature.lines().next().unwrap_or(&symbol.name);
            if sig_line.len() > 120 {
                let truncated = truncate_str_safe(sig_line, 117);
                format!("{truncated}...")
            } else {
                sig_line.to_string()
            }
        };
        out.push_str(&format!("  {} {}\n", symbol.kind, sig));
    }

    out
}

// ── Helper Functions ─────────────────────────────────────────────────────────

fn node_text(node: &Node, source: &str) -> Option<String> {
    let start = node.start_byte();
    let end = node.end_byte();
    if start <= end && end <= source.len() {
        source.get(start..end).map(ToString::to_string)
    } else {
        None
    }
}

/// Truncate a string to approximately `max_bytes` bytes, respecting UTF-8 char boundaries.
fn truncate_str_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the largest byte index <= max_bytes that is a valid char boundary
    if s.is_char_boundary(max_bytes) {
        &s[..max_bytes]
    } else {
        let mut boundary = max_bytes;
        while boundary > 0 && !s.is_char_boundary(boundary) {
            boundary -= 1;
        }
        &s[..boundary]
    }
}

fn extract_doc_comment(node: &Node, source: &str) -> Option<String> {
    // Look for the previous sibling that is a doc comment block
    let prev = node.prev_named_sibling()?;
    if prev.kind() == "block_comment" || prev.kind() == "line_comment" {
        let text = node_text(&prev, source)?;
        // Clean up comment markers
        let cleaned = text
            .trim_start_matches("///")
            .trim_start_matches("//")
            .trim_start_matches("/*")
            .trim_end_matches("*/")
            .trim();
        if cleaned.len() < 200 {
            Some(cleaned.to_string())
        } else {
            Some(format!("{}...", truncate_str_safe(cleaned, 197)))
        }
    } else {
        None
    }
}

fn extract_go_doc_comment(node: &Node, source: &str) -> Option<String> {
    let prev = node.prev_named_sibling()?;
    if prev.kind() == "comment" {
        let text = node_text(&prev, source)?;
        let cleaned = text.trim_start_matches("//").trim();
        if cleaned.len() < 200 {
            Some(cleaned.to_string())
        } else {
            Some(format!("{}...", truncate_str_safe(cleaned, 197)))
        }
    } else {
        None
    }
}

fn collect_source_files(project_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let skip_set: std::collections::HashSet<&str> = SKIP_DIRS.iter().cloned().collect();
    let ext_set: std::collections::HashSet<&str> = INDEXED_EXTENSIONS.iter().cloned().collect();

    walk_project(project_root, &ext_set, &skip_set, &mut files);

    // Also include files with regex fallback support
    let regex_exts: std::collections::HashSet<&str> =
        ["java", "kt", "scala", "c", "cpp", "h", "hpp", "rb"]
            .iter()
            .cloned()
            .collect();
    walk_project(project_root, &regex_exts, &skip_set, &mut files);

    Ok(files)
}

fn walk_project(
    dir: &Path,
    ext_set: &std::collections::HashSet<&str>,
    skip_set: &std::collections::HashSet<&str>,
    files: &mut Vec<PathBuf>,
) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !skip_set.contains(name) && !name.starts_with('.') {
                        walk_project(&path, ext_set, skip_set, files);
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

fn file_modified_time(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}

// ── Tests ────────────────────────────────────────────────────────────────────

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
    /// Creates a new user.
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
        assert!(names.contains(&"HasName"), "Should find HasName trait");
        assert!(names.contains(&"new"), "Should find new method");
        assert!(names.contains(&"greet"), "Should find greet method");
        assert!(names.contains(&"UserId"), "Should find UserId type alias");
        assert!(names.contains(&"submodule"), "Should find submodule");

        // Check symbol kinds
        let user_struct = summary.symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user_struct.kind, SymbolKind::Struct);

        let status_enum = summary.symbols.iter().find(|s| s.name == "Status").unwrap();
        assert_eq!(status_enum.kind, SymbolKind::Enum);

        let new_fn = summary.symbols.iter().find(|s| s.name == "new").unwrap();
        assert_eq!(new_fn.kind, SymbolKind::Function);
        assert!(new_fn.signature.contains("pub fn new"));

        let max_age = summary
            .symbols
            .iter()
            .find(|s| s.name == "MAX_AGE")
            .unwrap();
        assert_eq!(max_age.kind, SymbolKind::Constant);
    }

    #[test]
    fn test_parse_python_file() {
        let dir = setup_test_env();
        let py_file = dir.path().join("app.py");
        std::fs::write(
            &py_file,
            r#""Module docstring."

import os
from typing import List

class Application:
    """Main application class."""

    def __init__(self, name: str):
        self.name = name

    def run(self) -> None:
        """Run the application."""
        print(f"Running {self.name}")

    @staticmethod
    def create() -> "Application":
        return Application("default")

def helper(x: int) -> int:
    return x * 2
"#,
        )
        .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let summary = map
            .for_file(Path::new("app.py"))
            .expect("missing file summary");

        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(
            names.contains(&"Application"),
            "Should find Application class, got: {:?}",
            names
        );
        assert!(names.contains(&"__init__"), "Should find __init__ method");
        assert!(names.contains(&"run"), "Should find run method");
        assert!(names.contains(&"create"), "Should find create method");
        assert!(names.contains(&"helper"), "Should find helper function");

        // Check imports
        assert!(!summary.imports.is_empty(), "Should find imports");
    }

    #[test]
    fn test_parse_javascript_file() {
        let dir = setup_test_env();
        let js_file = dir.path().join("index.js");
        std::fs::write(
            &js_file,
            r#"
import { Router } from 'express';

function handleRequest(req, res) {
    res.send('Hello');
}

const greet = (name) => {
    return `Hello, ${name}`;
};

class Server {
    constructor(port) {
        this.port = port;
    }

    start() {
        console.log(`Listening on ${this.port}`);
    }
}
"#,
        )
        .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let summary = map
            .for_file(Path::new("index.js"))
            .expect("missing file summary");

        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(
            names.contains(&"handleRequest"),
            "Should find handleRequest, got: {:?}",
            names
        );
        assert!(names.contains(&"greet"), "Should find greet arrow fn");
        assert!(names.contains(&"Server"), "Should find Server class");
        assert!(
            names.contains(&"constructor") || names.contains(&"start"),
            "Should find class methods"
        );
    }

    #[test]
    fn test_parse_go_file() {
        let dir = setup_test_env();
        let go_file = dir.path().join("main.go");
        std::fs::write(
            &go_file,
            r#"package main

import "fmt"

// User represents a system user.
type User struct {
    Name string
    Age  int
}

// Service interface for user operations.
type Service interface {
    GetUser(id int) (*User, error)
}

func NewUser(name string, age int) *User {
    return &User{Name: name, Age: age}
}

func (u *User) Greet() string {
    return fmt.Sprintf("Hello, %s!", u.Name)
}

const MaxAge = 150
"#,
        )
        .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let summary = map
            .for_file(Path::new("main.go"))
            .expect("missing file summary");

        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(
            names.contains(&"User"),
            "Should find User struct, got: {:?}",
            names
        );
        assert!(names.contains(&"Service"), "Should find Service interface");
        assert!(names.contains(&"NewUser"), "Should find NewUser function");
        assert!(names.contains(&"Greet"), "Should find Greet method");
        assert!(names.contains(&"MaxAge"), "Should find MaxAge const");

        let user = summary.symbols.iter().find(|s| s.name == "User").unwrap();
        assert_eq!(user.kind, SymbolKind::Struct);

        let service = summary
            .symbols
            .iter()
            .find(|s| s.name == "Service")
            .unwrap();
        assert_eq!(service.kind, SymbolKind::Interface);
    }

    #[test]
    fn test_parse_typescript_file() {
        let dir = setup_test_env();
        let ts_file = dir.path().join("types.ts");
        std::fs::write(
            &ts_file,
            r#"
import { Request, Response } from 'express';

interface User {
    id: number;
    name: string;
}

type UserId = number;

enum Role {
    Admin = 'admin',
    User = 'user',
}

function getUser(id: number): User | null {
    return null;
}

class UserService {
    async fetchUser(id: number): Promise<User> {
        return {} as User;
    }
}
"#,
        )
        .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let summary = map
            .for_file(Path::new("types.ts"))
            .expect("missing file summary");

        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();

        assert!(
            names.contains(&"User"),
            "Should find User interface, got: {:?}",
            names
        );
        assert!(names.contains(&"UserId"), "Should find UserId type alias");
        assert!(names.contains(&"Role"), "Should find Role enum");
        assert!(names.contains(&"getUser"), "Should find getUser function");
        assert!(
            names.contains(&"UserService"),
            "Should find UserService class"
        );
    }

    #[test]
    fn test_token_budget_enforcement() {
        let dir = setup_test_env();

        // Create many files with many symbols to exceed budget
        for i in 0..20 {
            let file = dir.path().join(format!("mod_{:02}.rs", i));
            let mut content = String::new();
            for j in 0..20 {
                content.push_str(&format!(
                    "pub fn function_{}_{}(x: i32, y: i32) -> i32 {{ x + y }}\n",
                    i, j
                ));
            }
            std::fs::write(&file, content).expect("failed to write test file");
        }

        // Build with a small budget
        let small_budget = 100;
        let map = RepoMap::build(dir.path(), small_budget).expect("failed to build repo map");
        let map_str = map.to_map_string();

        // The map should be constrained. With 100 token budget = 400 chars budget.
        // With truncation overhead (~20 chars), max is ~420 chars, allow generous overhead
        let budget_chars = small_budget * CHARS_PER_TOKEN;
        let max_chars = budget_chars + 50; // truncation marker overhead
        assert!(
            map_str.len() <= max_chars,
            "Map length ({}) should be within {} chars (budget {} tokens = {} chars + overhead)",
            map_str.len(),
            max_chars,
            small_budget,
            budget_chars
        );

        // Verify that the map is much smaller than the full content would be
        // 20 files * 20 functions * ~50 chars each = ~20000 chars
        assert!(
            map_str.len() < 5000,
            "Map should be truncated, got {} chars",
            map_str.len()
        );
    }

    #[test]
    fn test_fallback_regex_for_unsupported_extensions() {
        let dir = setup_test_env();

        // Java file (not tree-sitter parsed, falls back to regex)
        let java_file = dir.path().join("App.java");
        std::fs::write(
            &java_file,
            r#"public class App {
    public static void main(String[] args) {
        System.out.println("Hello");
    }
}
"#,
        )
        .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let summary = map
            .for_file(Path::new("App.java"))
            .expect("missing file summary");

        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"App"),
            "Should find App class via regex fallback, got: {:?}",
            names
        );
    }

    #[test]
    fn test_truncate_str_safe_keeps_utf8_valid() {
        let s = "é".repeat(20);
        let truncated = truncate_str_safe(&s, 7);
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
        assert!(truncated.len() <= 7);
    }

    #[test]
    fn test_formatted_output() {
        let dir = setup_test_env();
        let rust_file = dir.path().join("lib.rs");
        std::fs::write(
            &rust_file,
            r#"pub struct Config {
    name: String,
}

impl Config {
    pub fn new() -> Self { Self { name: String::new() } }
}

pub fn load_config() -> Result<Config> { Ok(Config::new()) }
"#,
        )
        .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let output = map.to_map_string();

        // Verify the format contains expected elements
        assert!(
            output.contains("lib.rs:"),
            "Output should contain file path"
        );
        assert!(
            output.contains("struct"),
            "Output should contain struct kind"
        );
        assert!(output.contains("fn"), "Output should contain fn kind");
        assert!(
            output.contains("Config"),
            "Output should contain Config name"
        );
        assert!(
            output.contains("load_config"),
            "Output should contain load_config name"
        );
    }

    #[test]
    fn test_empty_directory() {
        let dir = setup_test_env();
        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");

        assert_eq!(map.file_count(), 0);
        assert_eq!(map.symbol_count(), 0);
        assert!(map.to_map_string().is_empty());
    }

    #[test]
    fn test_estimated_tokens() {
        let dir = setup_test_env();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "fn main() {}\nstruct Foo {}\n").expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let tokens = map.estimated_tokens();

        // Should be a positive number based on output length / 4
        assert!(
            tokens > 0,
            "Token estimate should be positive, got {}",
            tokens
        );
        // Rough sanity: output should be a few dozen tokens at most
        assert!(
            tokens < 1000,
            "Token estimate should be small for this input, got {}",
            tokens
        );
    }

    #[test]
    fn test_gitignore_dirs_skipped() {
        let dir = setup_test_env();

        // Create a file in target/ which should be skipped
        let target_dir = dir.path().join("target");
        std::fs::create_dir_all(&target_dir).expect("failed to create dir");
        std::fs::write(target_dir.join("build.rs"), "fn main() {}\n")
            .expect("failed to write test file");

        // Create a file in node_modules/ which should be skipped
        let nm_dir = dir.path().join("node_modules");
        std::fs::create_dir_all(&nm_dir).expect("failed to create dir");
        std::fs::write(nm_dir.join("index.js"), "function hello() {}\n")
            .expect("failed to write test file");

        // Create a normal file that should be included
        std::fs::write(dir.path().join("src.rs"), "fn main() {}\n")
            .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");

        assert!(
            map.for_file(Path::new("src.rs")).is_some(),
            "Normal files should be included"
        );
        assert!(
            map.for_file(Path::new("target/build.rs")).is_none(),
            "target/ should be skipped"
        );
        assert!(
            map.for_file(Path::new("node_modules/index.js")).is_none(),
            "node_modules/ should be skipped"
        );
    }

    #[test]
    fn test_ruby_regex_fallback() {
        let dir = setup_test_env();
        let rb_file = dir.path().join("app.rb");
        std::fs::write(
            &rb_file,
            r#"class ApplicationController
  def index
    render :index
  end
end

module Helpers
  def format_name(name)
    name.titleize
  end
end
"#,
        )
        .expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let summary = map
            .for_file(Path::new("app.rb"))
            .expect("missing file summary");

        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"ApplicationController"),
            "Should find class, got: {:?}",
            names
        );
        assert!(names.contains(&"index"), "Should find method");
        assert!(names.contains(&"Helpers"), "Should find module");
        assert!(names.contains(&"format_name"), "Should find helper method");
    }

    #[test]
    fn test_parse_own_crate_source() {
        // Parse code_index.rs (a known file in this crate) as a real-world test
        let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let code_index_path = crate_root
            .join("src")
            .join("indexing")
            .join("code_index.rs");
        let content =
            std::fs::read_to_string(&code_index_path).expect("failed to read code_index.rs");

        let dir = setup_test_env();
        let test_file = dir.path().join("code_index.rs");
        std::fs::write(&test_file, &content).expect("failed to write test file");

        let map = RepoMap::build(dir.path(), 10000).expect("failed to build repo map");
        let summary = map
            .for_file(Path::new("code_index.rs"))
            .expect("missing file summary");

        let names: Vec<&str> = summary.symbols.iter().map(|s| s.name.as_str()).collect();

        // Should find at least the main types from code_index.rs
        assert!(
            names.contains(&"CodeIndex"),
            "Should find CodeIndex struct, got: {:?}",
            names
        );
        assert!(
            names.contains(&"Symbol"),
            "Should find Symbol struct, got: {:?}",
            names
        );
    }
}
