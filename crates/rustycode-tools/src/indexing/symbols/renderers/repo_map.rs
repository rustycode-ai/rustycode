use crate::indexing::symbols::{CodeSymbol, FileOutline};

pub fn render_repo_map(outlines: &[FileOutline], budget: usize) -> String {
    let max_chars = budget * super::CHARS_PER_TOKEN;
    let mut buffer = String::new();

    for outline in outlines {
        let header = format!("{}:\n", outline.path.display());
        if !buffer.is_empty() && buffer.len() + header.len() > max_chars {
            break;
        }
        buffer.push_str(&header);
        for symbol in &outline.symbols {
            render_symbol_to_map(symbol, 1, &mut buffer);
        }
        if buffer.len() > max_chars {
            buffer.truncate(max_chars);
            break;
        }
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
