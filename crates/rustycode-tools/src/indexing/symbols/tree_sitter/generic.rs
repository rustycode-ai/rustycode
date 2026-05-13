use std::path::Path;
use tree_sitter::Node;
use std::collections::HashMap;

use super::node_text;
use rustycode_protocol::code_symbol::{CodeSymbol, FileOutline, SymbolKind, Visibility, SymbolRange};

/// Extract Go symbols from a tree-sitter parse tree.
pub fn extract_go_symbols_ts(
    root: &Node,
    source: &str,
    symbols: &mut Vec<CodeSymbol>,
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

fn go_fn_symbol(node: &Node, source: &str, kind: SymbolKind) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;
    Some(CodeSymbol {
        name: name_text,
        kind,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        range: SymbolRange {
            start_line: node.start_position().row + 1,
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        },
        signature: node_text(node, source).map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        }).unwrap_or_default(),
        doc_comment: extract_go_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    })
}

fn go_method_symbol(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;
    Some(CodeSymbol {
        name: name_text,
        kind: SymbolKind::Method,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        range: SymbolRange {
            start_line: node.start_position().row + 1,
            start_col: node.start_position().column,
            end_line: node.end_position().row + 1,
            end_col: node.end_position().column,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        },
        signature: node_text(node, source).map(|s| {
            s.lines()
                .next()
                .unwrap_or("")
                .trim_end_matches('{')
                .trim()
                .to_string()
        }).unwrap_or_default(),
        doc_comment: extract_go_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    })
}

fn extract_go_type_declaration(node: &Node, source: &str, symbols: &mut Vec<CodeSymbol>) {
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
                    symbols.push(CodeSymbol {
                        name: name_text,
                        kind,
                        line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        range: SymbolRange {
                            start_line: node.start_position().row + 1,
                            start_col: node.start_position().column,
                            end_line: node.end_position().row + 1,
                            end_col: node.end_position().column,
                            start_byte: node.start_byte(),
                            end_byte: node.end_byte(),
                        },
                        signature: node_text(node, source).map(|s| {
                            s.lines()
                                .next()
                                .unwrap_or("")
                                .trim_end_matches('{')
                                .trim()
                                .to_string()
                        }).unwrap_or_default(),
                        doc_comment: extract_go_doc_comment(node, source),
                        visibility: Visibility::Public,
                        children: Vec::new(),
                        metadata: HashMap::new(),
                    });
                }
            }
        }
    }
}

fn extract_go_consts(node: &Node, source: &str, symbols: &mut Vec<CodeSymbol>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "const_spec" {
            let name = child.child_by_field_name("name");
            if let Some(name_node) = name {
                if let Some(name_text) = node_text(&name_node, source) {
                    symbols.push(CodeSymbol {
                        name: name_text,
                        kind: SymbolKind::Constant,
                        line: child.start_position().row + 1,
                        end_line: child.end_position().row + 1,
                        range: SymbolRange {
                            start_line: child.start_position().row + 1,
                            start_col: child.start_position().column,
                            end_line: child.end_position().row + 1,
                            end_col: child.end_position().column,
                            start_byte: child.start_byte(),
                            end_byte: child.end_byte(),
                        },
                        signature: node_text(&child, source).map(|s| {
                            s.lines()
                                .next()
                                .unwrap_or("")
                                .trim_end_matches(';')
                                .trim()
                                .to_string()
                        }).unwrap_or_default(),
                        doc_comment: None,
                        visibility: Visibility::Public,
                        children: Vec::new(),
                        metadata: HashMap::new(),
                    });
                }
            }
        }
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
            Some(format!("{}...", &cleaned[..cleaned.char_indices().nth(197).map(|(i,_)| i).unwrap_or(cleaned.len())]))
        }
    } else {
        None
    }
}

/// Parse a file using regex fallback for languages without tree-sitter grammars.
pub fn parse_with_regex(path: &Path, content: &str) -> FileOutline {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    match ext {
        "java" | "kt" | "scala" => extract_java_regex(content, &mut symbols),
        "c" | "cpp" | "h" | "hpp" => extract_c_regex(content, &mut symbols),
        "rb" => extract_ruby_regex(content, &mut symbols),
        _ => extract_generic_regex(content, &mut symbols),
    }
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
    FileOutline {
        path: path.to_path_buf(),
        language: "regex".to_string(),
        symbols,
        imports,
    }
}

fn regex_symbol(
    name: String,
    kind: SymbolKind,
    line: usize,
    signature: String,
) -> CodeSymbol {
    CodeSymbol {
        name,
        kind,
        line,
        end_line: line, // Fallback for regex
        range: SymbolRange {
            start_line: line,
            start_col: 0,
            end_line: line,
            end_col: 0,
            start_byte: 0,
            end_byte: 0,
        },
        signature,
        doc_comment: None,
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    }
}

fn extract_java_regex(content: &str, symbols: &mut Vec<CodeSymbol>) {
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
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
                        symbols.push(regex_symbol(
                            name.to_string(),
                            *kind,
                            i + 1,
                            trimmed.trim_end_matches('{').trim().to_string(),
                        ));
                    }
                }
            }
        }
    }
}

fn extract_c_regex(content: &str, symbols: &mut Vec<CodeSymbol>) {
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
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
                    symbols.push(regex_symbol(
                        name.to_string(),
                        *kind,
                        i + 1,
                        trimmed.trim_end_matches('{').trim().to_string(),
                    ));
                }
            }
        }
    }
}

fn extract_ruby_regex(content: &str, symbols: &mut Vec<CodeSymbol>) {
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
                symbols.push(regex_symbol(
                    name.to_string(),
                    SymbolKind::Function,
                    i + 1,
                    trimmed.to_string(),
                ));
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
                symbols.push(regex_symbol(
                    name.to_string(),
                    kind,
                    i + 1,
                    trimmed.to_string(),
                ));
            }
        }
    }
}

fn extract_generic_regex(content: &str, symbols: &mut Vec<CodeSymbol>) {
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
                    symbols.push(regex_symbol(
                        name,
                        SymbolKind::Function,
                        i + 1,
                        trimmed.to_string(),
                    ));
                }
            }
        }
    }
}
