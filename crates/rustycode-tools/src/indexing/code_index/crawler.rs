use super::{Symbol, SymbolKind};
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ── Symbol Extraction ─────────────────────────────────────────────────────────

/// Extract symbols from source code
pub(crate) fn extract_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
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

pub(crate) fn extract_rust_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
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

pub(crate) fn extract_python_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
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

pub(crate) fn extract_go_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
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

pub(crate) fn extract_js_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
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

pub(crate) fn extract_java_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
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

pub(crate) fn extract_generic_symbols(file_path: &Path, content: &str) -> Vec<Symbol> {
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
