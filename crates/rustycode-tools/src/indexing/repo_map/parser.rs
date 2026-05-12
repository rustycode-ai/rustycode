use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

use super::languages::Lang;
use super::{FileSummary, SymbolInfo, SymbolKind};
use super::{CHARS_PER_TOKEN, INDEXED_EXTENSIONS, REGEX_EXTENSIONS};

pub fn parse_with_treesitter(
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

pub fn parse_with_regex(path: &Path, content: &str) -> FileSummary {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    match ext {
        "java" | "kt" | "scala" => extract_java_regex(path, content, &mut symbols),
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
    FileSummary {
        path: path.to_path_buf(),
        symbols,
        imports,
    }
}

fn extract_java_regex(_path: &Path, content: &str, symbols: &mut Vec<SymbolInfo>) {
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

pub fn format_map(
    file_summaries: &HashMap<PathBuf, FileSummary>,
    token_budget: usize,
) -> (String, usize) {
    let mut output = String::new();
    let budget_chars = token_budget * CHARS_PER_TOKEN;
    let mut entries: Vec<(&PathBuf, &FileSummary)> = file_summaries.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (rel_path, summary) in &entries {
        let file_block = format_file_entry(rel_path, summary);
        if output.len().saturating_add(file_block.len()) > budget_chars {
            if output.is_empty() {
                let trunc_len = budget_chars.saturating_sub(20);
                if trunc_len > 0 {
                    output.push_str(truncate_str_safe(&file_block, trunc_len));
                    output.push_str("\n... (truncated)\n");
                }
            } else {
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

pub fn node_text(node: &Node, source: &str) -> Option<String> {
    let start = node.start_byte();
    let end = node.end_byte();
    if start <= end && end <= source.len() {
        source.get(start..end).map(ToString::to_string)
    } else {
        None
    }
}

fn truncate_str_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
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
    let prev = node.prev_named_sibling()?;
    if prev.kind() == "block_comment" || prev.kind() == "line_comment" {
        let text = node_text(&prev, source)?;
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

pub fn collect_source_files(project_root: &Path) -> Result<Vec<PathBuf>> {
    let ext_set: std::collections::HashSet<&str> = INDEXED_EXTENSIONS
        .iter()
        .chain(REGEX_EXTENSIONS.iter())
        .cloned()
        .collect();
    let mut files = Vec::new();
    let mut builder = ignore::WalkBuilder::new(project_root);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .add_custom_ignore_filename(".rustycodeignore");
    for result in builder.build() {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if ext_set.contains(ext) {
            files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

pub fn file_modified_time(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}
