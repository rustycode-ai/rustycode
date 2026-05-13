use crate::indexing::symbols::languages::Lang;
use crate::indexing::symbols::tree_sitter::{parse_with_treesitter, parse_with_regex};
use rustycode_protocol::code_symbol::FileOutline;
use std::path::{Path, PathBuf};
use tree_sitter::Parser;
use walkdir::WalkDir;

pub fn extract_file(path: &Path, content: &str) -> FileOutline {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let lang = Lang::from_ext(ext);

    match lang {
        Some(l) => {
            let mut parser = Parser::new();
            parse_with_treesitter(&mut parser, l, path, content)
        }
        None => parse_with_regex(path, content),
    }
}

pub fn collect_source_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        if matches!(ext, "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "go" | "java" | "c" | "cpp" | "h" | "hpp") {
            files.push(path.to_path_buf());
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::code_symbol::{CodeSymbol, SymbolKind, Visibility};

    fn find_symbol<'a>(
        outline: &'a FileOutline,
        name: &str,
    ) -> Option<&'a CodeSymbol> {
        fn search<'a>(syms: &'a [CodeSymbol], name: &str) -> Option<&'a CodeSymbol> {
            for s in syms {
                if s.name == name {
                    return Some(s);
                }
                if let Some(child) = search(&s.children, name) {
                    return Some(child);
                }
            }
            None
        }
        search(&outline.symbols, name)
    }

    // ---------------------------------------------------------------------------
    // 1. Rust pub(crate) visibility — should extract with correct visibility
    // ---------------------------------------------------------------------------
    #[test]
    fn rust_pub_crate_visibility_is_extracted() {
        let content = "\
pub(crate) fn my_function() -> i32 {
    42
}
";
        let outline = extract_file(Path::new("test.rs"), content);
        let sym = find_symbol(&outline, "my_function")
            .expect("pub(crate) function should not be dropped");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.visibility, Visibility::Public);
    }

    // ---------------------------------------------------------------------------
    // 2. Generic parameters — name should be "foo", not "foo<T>"
    // ---------------------------------------------------------------------------
    #[test]
    fn rust_generic_fn_name_excludes_type_params() {
        let content = "\
fn foo<T>(x: T) -> T {
    x
}
";
        let outline = extract_file(Path::new("test.rs"), content);
        let sym = find_symbol(&outline, "foo")
            .expect("generic function should be extracted");
        assert_eq!(sym.name, "foo");
    }

    // ---------------------------------------------------------------------------
    // 3. Nested functions — inner function should appear as child of outer
    // ---------------------------------------------------------------------------
    #[test]
    fn rust_nested_function_appears_as_child() {
        let content = "\
fn outer() {
    fn inner() {
        let _x = 1;
    }
}
";
        let outline = extract_file(Path::new("test.rs"), content);
        let outer = find_symbol(&outline, "outer")
            .expect("outer function should be extracted");
        assert!(
            !outer.children.is_empty(),
            "inner function should appear as child of outer function"
        );
        assert_eq!(outer.children[0].name, "inner");
        assert_eq!(outer.children[0].kind, SymbolKind::Function);
    }

    // ---------------------------------------------------------------------------
    // 4. Macro definitions — macro_rules! should be SymbolKind::Macro
    // ---------------------------------------------------------------------------
    #[test]
    fn rust_macro_rules_detected_as_macro_kind() {
        let content = r#"\
macro_rules! say_hello {
    () => {
        println!("Hello!");
    };
}
"#;
        let outline = extract_file(Path::new("test.rs"), content);
        let sym = find_symbol(&outline, "say_hello")
            .expect("macro_rules! should be extracted");
        assert_eq!(sym.kind, SymbolKind::Macro);
    }

    // ---------------------------------------------------------------------------
    // 5. Comments inside function bodies — should NOT create symbols
    // ---------------------------------------------------------------------------
    #[test]
    fn rust_comments_inside_fn_body_create_no_symbols() {
        let content = "\
fn my_function() {
    // This is a comment, not a function
    /* This is a block comment */
    let x = 5;
}
";
        let outline = extract_file(Path::new("test.rs"), content);
        assert_eq!(outline.symbols.len(), 1);
        let sym = &outline.symbols[0];
        assert_eq!(sym.name, "my_function");
        assert!(sym.children.is_empty());
    }

    // ---------------------------------------------------------------------------
    // 6. String literals with function-like patterns — should NOT create symbols
    // ---------------------------------------------------------------------------
    #[test]
    fn rust_string_literal_fn_pattern_creates_no_symbols() {
        let content = r#"
fn real_function() {
    let s = "fn fake() {}";
    let t = "struct FakeStruct {}";
    let u = "enum FakeEnum { A, B }";
}
"#;
        let outline = extract_file(Path::new("test.rs"), content);
        assert_eq!(outline.symbols.len(), 1);
        assert_eq!(outline.symbols[0].name, "real_function");
    }

    // ---------------------------------------------------------------------------
    // 7. Python async def — should be extracted as a function symbol
    // ---------------------------------------------------------------------------
    #[test]
    fn python_async_def_is_extracted() {
        let content = "\
async def fetch_data(url):
    pass
";
        let outline = extract_file(Path::new("test.py"), content);
        let sym = find_symbol(&outline, "fetch_data")
            .expect("async function should be extracted");
        assert_eq!(sym.kind, SymbolKind::Function);
    }

    // ---------------------------------------------------------------------------
    // 8. Python nested class — inner class should be child of outer class
    // ---------------------------------------------------------------------------
    #[test]
    fn python_nested_class_is_child_of_outer_class() {
        let content = "\
class Outer:
    class Inner:
        pass
";
        let outline = extract_file(Path::new("test.py"), content);
        let outer = find_symbol(&outline, "Outer")
            .expect("outer class should be extracted");
        assert_eq!(outer.kind, SymbolKind::Class);
        assert!(
            !outer.children.is_empty(),
            "inner class should be child of outer class"
        );
        assert_eq!(outer.children[0].name, "Inner");
        assert_eq!(outer.children[0].kind, SymbolKind::Class);
    }

    // ---------------------------------------------------------------------------
    // 9. JavaScript export default function — should be extracted
    // ---------------------------------------------------------------------------
    #[test]
    fn js_export_default_function_is_extracted() {
        let content = "\
export default function myFunc() {
    return 42;
}
";
        let outline = extract_file(Path::new("test.js"), content);
        let sym = find_symbol(&outline, "myFunc")
            .expect("export default function should be extracted");
        assert_eq!(sym.kind, SymbolKind::Function);
    }

    // ---------------------------------------------------------------------------
    // 10. JavaScript export async function — should be extracted
    // ---------------------------------------------------------------------------
    #[test]
    fn js_export_async_function_is_extracted() {
        let content = "\
export async function fetchData() {
    return await fetch('/api');
}
";
        let outline = extract_file(Path::new("test.js"), content);
        let sym = find_symbol(&outline, "fetchData")
            .expect("export async function should be extracted");
        assert_eq!(sym.kind, SymbolKind::Function);
    }
}
