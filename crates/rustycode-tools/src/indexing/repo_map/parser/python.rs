use tree_sitter::Node;

use super::node_text;
use crate::indexing::repo_map::{SymbolInfo, SymbolKind};

pub(super) fn extract_python_symbols_ts(
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
