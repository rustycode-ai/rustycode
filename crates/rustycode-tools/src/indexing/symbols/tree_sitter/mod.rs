use tree_sitter::{Node, Parser};
use std::path::Path;

use crate::indexing::symbols::languages::Lang;
pub use rustycode_protocol::code_symbol::{CodeSymbol, FileOutline, SymbolKind};

pub mod generic;
pub mod query_extractor;

use query_extractor::QueryExtractor;

pub use generic::parse_with_regex;

/// Extract text from a tree-sitter node.
pub(crate) fn node_text(node: &Node, source: &str) -> Option<String> {
    node.utf8_text(source.as_bytes()).ok().map(|s| s.to_string())
}

pub fn parse_with_treesitter(
    parser: &mut Parser,
    lang: Lang,
    path: &Path,
    source: &str,
) -> FileOutline {
    let (language, query_source): (tree_sitter::Language, &str) = match lang {
        Lang::Rust => (tree_sitter_rust::LANGUAGE.into(), include_str!("../queries/rust.scm")),
        Lang::JavaScript => (tree_sitter_javascript::LANGUAGE.into(), include_str!("../queries/javascript.scm")),
        Lang::TypeScript => (tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), include_str!("../queries/javascript.scm")),
        Lang::Python => (tree_sitter_python::LANGUAGE.into(), include_str!("../queries/python.scm")),
        Lang::Go => (tree_sitter_go::LANGUAGE.into(), include_str!("../queries/go.scm")),
    };

    let extractor = QueryExtractor::new(lang, query_source);
    parser.set_language(&language).expect("Error loading grammar");
    let tree = parser.parse(source, None).unwrap();
    let (symbols, imports) = extractor.extract(&tree.root_node(), source);

    FileOutline {
        path: path.to_path_buf(),
        language: format!("{:?}", lang),
        symbols,
        imports,
    }
}
