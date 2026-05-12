use tree_sitter::Node;

use super::{extract_doc_comment, node_text};
use crate::indexing::repo_map::{SymbolInfo, SymbolKind};

pub(super) fn extract_rust_symbols_ts(
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
            _ => {}
        }
    }
}

fn extract_rust_impl(impl_node: &Node, source: &str, symbols: &mut Vec<SymbolInfo>) {
    let type_name = impl_node
        .child_by_field_name("type")
        .and_then(|n| node_text(&n, source));

    let impl_sig = node_text(impl_node, source)
        .map(|s| {
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
