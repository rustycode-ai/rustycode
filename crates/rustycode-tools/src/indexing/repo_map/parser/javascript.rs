use tree_sitter::Node;

use super::node_text;
use crate::indexing::repo_map::{SymbolInfo, SymbolKind};

pub(super) fn extract_js_ts_symbols_ts(
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
            "method_definition" | "public_field_definition" => {
                if let Some(sym) = js_method_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            "interface_declaration" => {
                if let Some(sym) = ts_interface_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            "type_alias_declaration" => {
                if let Some(sym) = ts_type_alias_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            "enum_declaration" => {
                if let Some(sym) = ts_enum_symbol(&child, source) {
                    symbols.push(sym);
                }
            }
            _ => {
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
