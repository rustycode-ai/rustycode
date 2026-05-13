use tree_sitter::Node;
use super::node_text;
use rustycode_protocol::code_symbol::{CodeSymbol, SymbolKind, Visibility};
use std::collections::HashMap;

pub fn extract_python_symbols_ts(
    root: &Node,
    source: &str,
    symbols: &mut Vec<CodeSymbol>,
    _imports: &mut Vec<String>,
) {
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if let Some(sym) = extract_python_node(&child, source) {
            symbols.push(sym);
        }
    }
}

fn extract_python_node(node: &Node, source: &str) -> Option<CodeSymbol> {
    match node.kind() {
        "function_definition" => python_fn_symbol(node, source),
        "class_definition" => extract_python_class(node, source),
        "decorated_definition" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(sym) = extract_python_node(&child, source) {
                    return Some(sym);
                }
            }
            None
        }
        _ => None,
    }
}

fn extract_python_class(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    let mut sym = CodeSymbol {
        name: name_text,
        kind: SymbolKind::Class,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: None,
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    };

    let mut cursor = node.walk();
    if let Some(block) = node.children(&mut cursor).find(|n| n.kind() == "block") {
        let mut block_cursor = block.walk();
        for child in block.children(&mut block_cursor) {
            if let Some(child_sym) = extract_python_node(&child, source) {
                sym.children.push(child_sym);
            }
        }
    }
    Some(sym)
}

fn python_fn_symbol(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;

    Some(CodeSymbol {
        name: name_text,
        kind: SymbolKind::Function,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: None,
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    })
}
