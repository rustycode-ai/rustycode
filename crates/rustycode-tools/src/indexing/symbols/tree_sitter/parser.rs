use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Parser;

use crate::indexing::symbols::languages::Lang;
use crate::indexing::symbols::tree_sitter::query_extractor::QueryExtractor;
pub use rustycode_protocol::code_symbol::FileOutline;

static RUST_EXTRACTOR: OnceLock<QueryExtractor> = OnceLock::new();
static PYTHON_EXTRACTOR: OnceLock<QueryExtractor> = OnceLock::new();
static JAVASCRIPT_EXTRACTOR: OnceLock<QueryExtractor> = OnceLock::new();
static TYPESCRIPT_EXTRACTOR: OnceLock<QueryExtractor> = OnceLock::new();
static GO_EXTRACTOR: OnceLock<QueryExtractor> = OnceLock::new();
static JAVA_EXTRACTOR: OnceLock<QueryExtractor> = OnceLock::new();
static CPP_EXTRACTOR: OnceLock<QueryExtractor> = OnceLock::new();
static SCALA_EXTRACTOR: OnceLock<QueryExtractor> = OnceLock::new();

fn get_extractor(lang: Lang) -> &'static QueryExtractor {
    match lang {
        Lang::Rust => RUST_EXTRACTOR
            .get_or_init(|| QueryExtractor::new(lang, include_str!("../queries/rust.scm"))),
        Lang::Python => PYTHON_EXTRACTOR
            .get_or_init(|| QueryExtractor::new(lang, include_str!("../queries/python.scm"))),
        Lang::JavaScript => JAVASCRIPT_EXTRACTOR
            .get_or_init(|| QueryExtractor::new(lang, include_str!("../queries/javascript.scm"))),
        Lang::TypeScript => TYPESCRIPT_EXTRACTOR
            .get_or_init(|| QueryExtractor::new(lang, include_str!("../queries/typescript.scm"))),
        Lang::Go => GO_EXTRACTOR
            .get_or_init(|| QueryExtractor::new(lang, include_str!("../queries/go.scm"))),
        Lang::Java => JAVA_EXTRACTOR
            .get_or_init(|| QueryExtractor::new(lang, include_str!("../queries/java.scm"))),
        Lang::Cpp => CPP_EXTRACTOR
            .get_or_init(|| QueryExtractor::new(lang, include_str!("../queries/cpp.scm"))),
        Lang::Scala => SCALA_EXTRACTOR
            .get_or_init(|| QueryExtractor::new(lang, include_str!("../queries/scala.scm"))),
    }
}

pub fn parse_with_treesitter(
    parser: &mut Parser,
    lang: Lang,
    path: &Path,
    source: &str,
) -> FileOutline {
    let extractor = get_extractor(lang);
    if let Err(e) = parser.set_language(&extractor.language) {
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
