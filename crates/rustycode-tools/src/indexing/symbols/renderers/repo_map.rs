use super::SymbolDisplay;
use crate::indexing::symbols::renderers::CHARS_PER_TOKEN;
use crate::indexing::symbols::{CodeSymbol, FileOutline};

pub fn render_repo_map(outline: &FileOutline, _budget: usize) -> String {
    let mut buffer = String::new();
    buffer.push_str(&format!("{}:\n", outline.path.display()));
    for symbol in &outline.symbols {
        render_symbol_to_map(symbol, 1, &mut buffer);
    }
    buffer
}

fn render_symbol_to_map(symbol: &CodeSymbol, depth: usize, buffer: &mut String) {
    let indent = "  ".repeat(depth);
    buffer.push_str(&format!(
        "{indent}{}: {}\n",
        symbol.kind.to_variant_name(),
        symbol.name
    ));
    for child in &symbol.children {
        render_symbol_to_map(child, depth + 1, buffer);
    }
}
