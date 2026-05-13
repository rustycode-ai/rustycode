use tree_sitter::Node;

pub use rustycode_protocol::code_symbol::{CodeSymbol, FileOutline, SymbolKind};

pub mod generic;
pub mod parser;
pub mod query_extractor;

pub use generic::parse_with_regex;
pub use parser::parse_with_treesitter;

/// Extract text from a tree-sitter node.
pub(crate) fn node_text(node: &Node, source: &str) -> Option<String> {
    node.utf8_text(source.as_bytes())
        .ok()
        .map(|s| s.to_string())
}
