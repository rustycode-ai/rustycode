use crate::indexing::symbols::{CodeSymbol, FileOutline};

pub fn render_search_index(outline: &FileOutline) -> Vec<String> {
    let mut results = Vec::new();
    for symbol in &outline.symbols {
        flatten_symbol_paths(symbol, &outline.path.to_string_lossy(), &mut results);
    }
    results
}

fn flatten_symbol_paths(symbol: &CodeSymbol, parent_path: &str, results: &mut Vec<String>) {
    let current_path = format!("{}::{}", parent_path, symbol.name);
    results.push(current_path.clone());
    for child in &symbol.children {
        flatten_symbol_paths(child, &current_path, results);
    }
}
