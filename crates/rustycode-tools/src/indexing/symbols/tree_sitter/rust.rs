use tree_sitter::Node;
use super::{extract_doc_comment, node_text};
use rustycode_protocol::code_symbol::{CodeSymbol, SymbolKind, Visibility};
use std::collections::HashMap;

pub fn extract_rust_symbols_ts(
    root: &Node,
    source: &str,
    symbols: &mut Vec<CodeSymbol>,
    _imports: &mut Vec<String>,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if let Some(sym) = extract_rust_node(&child, source) {
            symbols.push(sym);
        }
    }
}

fn extract_rust_node(node: &Node, source: &str) -> Option<CodeSymbol> {
    match node.kind() {
        "function_item" | "async_function_item" => rust_fn_symbol(node, source),
        "struct_item" => rust_named_symbol(node, source, SymbolKind::Struct),
        "enum_item" => rust_named_symbol(node, source, SymbolKind::Enum),
        "trait_item" => rust_named_symbol(node, source, SymbolKind::Trait),
        "const_item" => rust_named_symbol(node, source, SymbolKind::Constant),
        "impl_item" => extract_rust_impl(node, source),
        "mod_item" => rust_named_symbol(node, source, SymbolKind::Module),
        "macro_definition" => rust_macro_symbol(node, source),
        _ => None,
    }
}

fn extract_rust_node_as_member(node: &Node, source: &str) -> Option<CodeSymbol> {
    match node.kind() {
        "function_item" | "async_function_item" => {
            let mut sym = rust_fn_symbol(node, source)?;
            sym.kind = SymbolKind::Method;
            Some(sym)
        }
        _ => extract_rust_node(node, source),
    }
}

fn extract_rust_impl(impl_node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = impl_node
        .child_by_field_name("type")
        .and_then(|n| node_text(&n, source))?;

    let mut sym = CodeSymbol {
        name,
        kind: SymbolKind::Impl,
        line: impl_node.start_position().row + 1,
        end_line: impl_node.end_position().row + 1,
        signature: node_text(impl_node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: extract_doc_comment(impl_node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    };

    let mut cursor = impl_node.walk();
    if let Some(body) = impl_node.children(&mut cursor).find(|n| n.kind() == "declaration_list") {
        let mut body_cursor = body.walk();
        for child in body.children(&mut body_cursor) {
            if let Some(child_sym) = extract_rust_node_as_member(&child, source) {
                sym.children.push(child_sym);
            }
        }
    }
    Some(sym)
}

fn rust_fn_symbol(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let mut sym = CodeSymbol {
        name: name_text,
        kind: SymbolKind::Function,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: extract_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    };

    // Recursively find nested symbols in function body
    let mut cursor = node.walk();
    if let Some(body) = node.children(&mut cursor).find(|n| n.kind() == "block") {
        let mut body_cursor = body.walk();
        for child in body.children(&mut body_cursor) {
            if let Some(child_sym) = extract_rust_node(&child, source) {
                sym.children.push(child_sym);
            }
        }
    }

    Some(sym)
}

fn rust_named_symbol(node: &Node, source: &str, kind: SymbolKind) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    Some(CodeSymbol {
        name: name_text,
        kind,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: extract_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    })
}

fn rust_macro_symbol(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    Some(CodeSymbol {
        name: name_text,
        kind: SymbolKind::Macro,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: extract_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    })
}
