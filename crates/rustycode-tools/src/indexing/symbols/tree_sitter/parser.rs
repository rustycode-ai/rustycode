use std::path::Path;
use tree_sitter::Parser;

use crate::indexing::symbols::languages::Lang;
use crate::indexing::symbols::tree_sitter::query_extractor::QueryExtractor;
pub use rustycode_protocol::code_symbol::FileOutline;

pub fn parse_with_treesitter(
    parser: &mut Parser,
    lang: Lang,
    path: &Path,
    source: &str,
) -> FileOutline {
    let (language, query_source): (tree_sitter::Language, &str) = match lang {
        Lang::Rust => (
            tree_sitter_rust::LANGUAGE.into(),
            include_str!("../queries/rust.scm"),
        ),
        Lang::Python => (
            tree_sitter_python::LANGUAGE.into(),
            include_str!("../queries/python.scm"),
        ),
        Lang::JavaScript => (
            tree_sitter_javascript::LANGUAGE.into(),
            include_str!("../queries/javascript.scm"),
        ),
        Lang::TypeScript => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            include_str!("../queries/typescript.scm"),
        ),
        Lang::Go => (
            tree_sitter_go::LANGUAGE.into(),
            include_str!("../queries/go.scm"),
        ),
        Lang::Java => (
            tree_sitter_java::LANGUAGE.into(),
            include_str!("../queries/java.scm"),
        ),
        Lang::Cpp => (
            tree_sitter_cpp::LANGUAGE.into(),
            include_str!("../queries/cpp.scm"),
        ),
        Lang::Scala => (
            tree_sitter_scala::LANGUAGE.into(),
            include_str!("../queries/scala.scm"),
        ),
    };

    let extractor = QueryExtractor::new(lang, query_source);
    if let Err(e) = parser.set_language(&language) {
        tracing::warn!(path = %path.display(), language = ?lang, error = %e, "failed to set tree-sitter language");
        return FileOutline {
            path: path.to_path_buf(),
            language: format!("{:?}", lang),
            symbols: Vec::new(),
            imports: Vec::new(),
        };
    }

    let tree = parser.parse(source, None).expect("failed to parse");
    let (symbols, imports) = extractor.extract(&tree.root_node(), source);

    FileOutline {
        path: path.to_path_buf(),
        language: format!("{:?}", lang),
        symbols,
        imports,
    }
}
