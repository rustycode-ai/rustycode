use super::{Symbol, SymbolKind};
use crate::indexing::symbols::extract_file;
use anyhow::Result;
use rustycode_protocol::code_symbol::{CodeSymbol, SymbolKind as NewSymbolKind};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ── Symbol Extraction ─────────────────────────────────────────────────────────

/// Extract symbols from source code
pub(crate) fn extract_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
    let outline = extract_file(file_path, content);
    let mut symbols = Vec::new();

    // Bridge the new hierarchical CodeSymbol tree to the old flat Symbol list
    for symbol in outline.symbols {
        flatten_symbols(symbol, None, &mut symbols, file_path);
    }

    symbols
}

fn flatten_symbols(
    symbol: CodeSymbol,
    parent: Option<String>,
    symbols: &mut Vec<Symbol>,
    file_path: &Path,
) {
    let symbol_name = if let Some(p) = parent.clone() {
        format!("{}::{}", p, symbol.name)
    } else {
        symbol.name.clone()
    };

    symbols.push(Symbol {
        name: symbol.name,
        kind: map_kind(symbol.kind),
        parent: parent.clone(),
        line: symbol.line,
        file_path: file_path.to_path_buf(),
        doc_comment: symbol.doc_comment,
        signature: Some(symbol.signature),
    });

    for child in symbol.children {
        flatten_symbols(child, Some(symbol_name.clone()), symbols, file_path);
    }
}

fn map_kind(kind: NewSymbolKind) -> SymbolKind {
    match kind {
        NewSymbolKind::Function => SymbolKind::Function,
        NewSymbolKind::Method => SymbolKind::Method,
        NewSymbolKind::Struct => SymbolKind::Struct,
        NewSymbolKind::Enum => SymbolKind::Enum,
        NewSymbolKind::Trait => SymbolKind::Trait,
        NewSymbolKind::Class => SymbolKind::Class,
        NewSymbolKind::Module => SymbolKind::Module,
        NewSymbolKind::Constant => SymbolKind::Constant,
        NewSymbolKind::TypeAlias => SymbolKind::Type,
        NewSymbolKind::Variable => SymbolKind::Variable,
        NewSymbolKind::Macro => SymbolKind::Macro,
        NewSymbolKind::Impl => SymbolKind::Impl,
        NewSymbolKind::Interface => SymbolKind::Interface,
    }
}

// ── Dependency Extraction ─────────────────────────────────────────────────────

pub(crate) fn extract_dependencies(_file_path: &Path, content: &str) -> Vec<PathBuf> {
    let mut deps = Vec::new();
    // Simplified: check for use/import keywords
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
        } else if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            let module = if let Some(rest) = trimmed.strip_prefix("import ") {
                rest.split(',').next().unwrap_or("").trim()
            } else if let Some(rest) = trimmed.strip_prefix("from ") {
                rest.split(" import").next().unwrap_or("").trim()
            } else {
                ""
            };
            if !module.is_empty() {
                deps.push(PathBuf::from(format!(
                    "{}.py",
                    module.replace('.', std::path::MAIN_SEPARATOR_STR)
                )));
            }
        } else if trimmed.starts_with("import") {
            if let Some(quoted) = trimmed.split('"').nth(1) {
                deps.push(PathBuf::from(quoted));
            }
        }
    }

    deps
}

// ── Helper Functions ──────────────────────────────────────────────────────────

pub(crate) fn walk_dir(
    root: &Path,
    extensions: &[&str],
    skip_dirs: &[&str],
) -> Result<Vec<PathBuf>> {
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
