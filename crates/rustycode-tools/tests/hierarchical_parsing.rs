#![allow(clippy::expect_used, clippy::needless_raw_string_hashes)]
use rustycode_tools::indexing::symbols::extract_file;
use std::path::Path;

#[test]
fn test_rust_nested_hierarchy() {
    let code = r#"
        struct Outer {
            x: i32,
        }
        impl Outer {
            fn method(&self) -> i32 {
                fn inner() {}
                self.x
            }
        }
    "#;
    let outline = extract_file(Path::new("test.rs"), code);

    // Debug output
    println!("{:#?}", outline.symbols);

    // Check struct
    let outer = outline
        .symbols
        .iter()
        .find(|s| s.name == "Outer")
        .expect("Outer struct not found");
    assert_eq!(
        outer.kind,
        rustycode_protocol::code_symbol::SymbolKind::Struct
    );

    // Check impl block
    let impl_block = outline
        .symbols
        .iter()
        .find(|s| s.kind == rustycode_protocol::code_symbol::SymbolKind::Impl)
        .expect("Impl block not found");

    // Check nested method
    let method = impl_block
        .children
        .iter()
        .find(|s| s.name == "method")
        .expect("method not found in impl");
    assert_eq!(
        method.kind,
        rustycode_protocol::code_symbol::SymbolKind::Method
    );

    // Check nested function inside method
    let inner = method
        .children
        .iter()
        .find(|s| s.name == "inner")
        .expect("inner function not found in method");
    assert_eq!(
        inner.kind,
        rustycode_protocol::code_symbol::SymbolKind::Function
    );
}

#[test]
fn test_python_nested_hierarchy() {
    let code = r#"
        class Outer:
            def method(self):
                def inner():
                    pass
    "#;
    let outline = extract_file(Path::new("test.py"), code);

    let outer = outline
        .symbols
        .iter()
        .find(|s| s.name == "Outer")
        .expect("Outer class not found");
    let method = outer
        .children
        .iter()
        .find(|s| s.name == "method")
        .expect("method not found in class");
    let inner = method
        .children
        .iter()
        .find(|s| s.name == "inner")
        .expect("inner function not found in method");

    assert_eq!(
        inner.kind,
        rustycode_protocol::code_symbol::SymbolKind::Function
    );
}
