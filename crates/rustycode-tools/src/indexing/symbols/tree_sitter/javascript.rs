use tree_sitter::Node;
use super::{extract_doc_comment, node_text};
use rustycode_protocol::code_symbol::{CodeSymbol, SymbolKind, Visibility};
use std::collections::HashMap;

pub fn extract_js_ts_symbols_ts(
    root: &Node,
    source: &str,
    symbols: &mut Vec<CodeSymbol>,
    _imports: &mut Vec<String>,
) {
    let mut cursor = root.walk();
    extract_js_ts_recursive(root, source, symbols, &mut cursor);
}

fn extract_js_ts_recursive<'a>(
    node: &Node<'a>,
    source: &str,
    symbols: &mut Vec<CodeSymbol>,
    cursor: &mut tree_sitter::TreeCursor<'a>,
) {
    for child in node.children(cursor) {
        if let Some(sym) = extract_js_ts_node(&child, source) {
            symbols.push(sym);
        }
    }
}

fn extract_js_ts_node(node: &Node, source: &str) -> Option<CodeSymbol> {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => js_fn_symbol(node, source, SymbolKind::Function),
        "class_declaration" | "class" => extract_js_class(node, source),
        "method_definition" | "public_field_definition" => js_method_symbol(node, source),
        "interface_declaration" => ts_interface_symbol(node, source),
        "type_alias_declaration" => ts_type_alias_symbol(node, source),
        "enum_declaration" => ts_enum_symbol(node, source),
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                 if let Some(sym) = extract_js_ts_node(&child, source) {
                     return Some(sym);
                 }
            }
            None
        }
    }
}

fn js_fn_symbol(node: &Node, source: &str, kind: SymbolKind) -> Option<CodeSymbol> {
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

fn extract_js_class(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;
    
    let mut sym = CodeSymbol {
        name: name_text,
        kind: SymbolKind::Class,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: extract_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    };

    let mut cursor = node.walk();
    if let Some(body) = node.children(&mut cursor).find(|n| n.kind() == "class_body") {
        let mut body_cursor = body.walk();
        for child in body.children(&mut body_cursor) {
            if let Some(child_sym) = extract_js_ts_node(&child, source) {
                sym.children.push(child_sym);
            }
        }
    }
    Some(sym)
}

fn js_method_symbol(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;
    
    Some(CodeSymbol {
        name: name_text,
        kind: SymbolKind::Method,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: extract_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    })
}

fn ts_interface_symbol(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;
    
    Some(CodeSymbol {
        name: name_text,
        kind: SymbolKind::Interface,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: extract_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    })
}

fn ts_type_alias_symbol(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;
    
    Some(CodeSymbol {
        name: name_text,
        kind: SymbolKind::TypeAlias,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: extract_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    })
}

fn ts_enum_symbol(node: &Node, source: &str) -> Option<CodeSymbol> {
    let name = node.child_by_field_name("name")?;
    let name_text = node_text(&name, source)?;
    
    Some(CodeSymbol {
        name: name_text,
        kind: SymbolKind::Enum,
        line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: node_text(node, source).map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default(),
        doc_comment: extract_doc_comment(node, source),
        visibility: Visibility::Public,
        children: Vec::new(),
        metadata: HashMap::new(),
    })
}
