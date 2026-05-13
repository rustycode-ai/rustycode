use crate::indexing::symbols::CodeSymbol;

pub fn render_llm_outline(symbols: &[CodeSymbol]) -> String {
    let mut buffer = String::new();
    for symbol in symbols {
        render_symbol_recursive(symbol, 0, &mut buffer);
    }
    buffer
}

fn render_symbol_recursive(symbol: &CodeSymbol, depth: usize, buffer: &mut String) {
    let indent = "  ".repeat(depth);
    let sig = if symbol.signature.is_empty() {
        ""
    } else {
        &symbol.signature
    };
    let kind = format!("{:?}", symbol.kind);

    buffer.push_str(&format!(
        "{}[{}] {} (lines {}-{}) {}\n",
        indent, kind, symbol.name, symbol.line, symbol.end_line, sig
    ));

    for child in &symbol.children {
        render_symbol_recursive(child, depth + 1, buffer);
    }
}
