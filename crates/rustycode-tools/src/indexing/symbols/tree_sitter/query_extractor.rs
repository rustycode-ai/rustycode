use crate::indexing::symbols::languages::Lang;
use crate::indexing::symbols::tree_sitter::node_text;
use rustycode_protocol::code_symbol::{CodeSymbol, SymbolKind, SymbolRange, Visibility};
use std::collections::HashMap;
use tree_sitter::{Language, Node, Query, QueryCursor, StreamingIterator};

pub struct QueryExtractor {
    pub language: Language,
    pub query: Query,
}

impl QueryExtractor {
    pub fn new(lang: Lang, query_source: &str) -> Self {
        let language: Language = match lang {
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
        };
        let query = match Query::new(&language, query_source) {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(?lang, error = %e, "Failed to parse tree-sitter query, using empty query");
                // Return a query that matches nothing — safest fallback
                Query::new(&language, "").unwrap_or_else(|_| {
                    // If even empty string fails, use a minimal always-false predicate
                    panic!("tree-sitter query engine broken for {:?}", lang)
                })
            }
        };
        Self { language, query }
    }

    pub fn extract(&self, root: &Node, source: &str) -> (Vec<CodeSymbol>, Vec<String>) {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, *root, source.as_bytes());

        let mut flat_symbols = Vec::new();
        let mut imports = Vec::new();

        while let Some(m) = matches.next() {
            let mut name = String::new();
            let mut kind = SymbolKind::Function;
            let mut doc = None;
            let mut symbol_node: Option<Node> = None;

            for capture in m.captures {
                let capture_name = self.query.capture_names()[capture.index as usize];
                let node = capture.node;

                match capture_name {
                    "symbol.name" => {
                        name = node_text(&node, source).unwrap_or_default();
                    }
                    "symbol.kind.function" => {
                        kind = SymbolKind::Function;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.method" => {
                        kind = SymbolKind::Method;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.struct" => {
                        kind = SymbolKind::Struct;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.class" => {
                        kind = SymbolKind::Class;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.interface" => {
                        kind = SymbolKind::Interface;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.enum" => {
                        kind = SymbolKind::Enum;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.trait" => {
                        kind = SymbolKind::Trait;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.impl" => {
                        kind = SymbolKind::Impl;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.macro" => {
                        kind = SymbolKind::Macro;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.constant" => {
                        kind = SymbolKind::Constant;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.type" => {
                        kind = SymbolKind::TypeAlias;
                        symbol_node = Some(node);
                    }
                    "symbol.kind.module" => {
                        kind = SymbolKind::Module;
                        symbol_node = Some(node);
                    }
                    "symbol.doc" => {
                        doc = node_text(&node, source);
                    }
                    "import" => {
                        if let Some(imp) = node_text(&node, source) {
                            imports.push(imp);
                        }
                    }
                    _ => {}
                }
            }

            if let Some(node) = symbol_node {
                let range = SymbolRange {
                    start_line: node.start_position().row + 1,
                    start_col: node.start_position().column,
                    end_line: node.end_position().row + 1,
                    end_col: node.end_position().column,
                    start_byte: node.start_byte(),
                    end_byte: node.end_byte(),
                };
                let sym = CodeSymbol {
                    name,
                    kind,
                    line: range.start_line,
                    end_line: range.end_line,
                    range,
                    signature: node_text(&node, source)
                        .map(|s| {
                            s.lines()
                                .next()
                                .unwrap_or("")
                                .trim_end_matches('{')
                                .trim()
                                .to_string()
                        })
                        .unwrap_or_default(),
                    doc_comment: doc,
                    visibility: Visibility::Public,
                    children: Vec::new(),
                    metadata: HashMap::new(),
                };
                flat_symbols.push(sym);
            }
        }

        // Sort by start line, then by end line (longer ranges first to ensure they are parents)
        flat_symbols.sort_by(|a, b| {
            a.line
                .cmp(&b.line)
                .then_with(|| b.end_line.cmp(&a.end_line))
        });

        let mut root_symbols = Vec::new();
        let mut stack: Vec<CodeSymbol> = Vec::new();

        for sym in flat_symbols {
            while let Some(top) = stack.last() {
                if sym.line >= top.line && sym.end_line <= top.end_line {
                    // Current symbol is a child of the top of the stack
                    break;
                } else {
                    // Top of stack is not a parent, pop it and move it to its parent or root
                    let finished = stack.pop().unwrap();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(finished);
                    } else {
                        root_symbols.push(finished);
                    }
                }
            }
            stack.push(sym);
        }

        // Clean up remaining stack
        while let Some(finished) = stack.pop() {
            if let Some(parent) = stack.last_mut() {
                parent.children.push(finished);
            } else {
                root_symbols.push(finished);
            }
        }

        (root_symbols, imports)
    }
}
