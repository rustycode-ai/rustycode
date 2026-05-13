use rustycode_protocol::code_symbol::{CodeSymbol, FileOutline, SymbolKind};
use std::path::Path;
use super::CHARS_PER_TOKEN;

/// Render a token-budgeted repository map from multiple file outlines.
pub fn render_repo_map(outlines: &[FileOutline], budget: usize, project_root: &Path) -> String {
    let mut output = String::new();
    let char_budget = budget * CHARS_PER_TOKEN;

    for outline in outlines {
        if output.len() >= char_budget {
            break;
        }

        let rel_path = outline.path.strip_prefix(project_root).unwrap_or(&outline.path);
        output.push_str(&format!("{}:\n", rel_path.display()));

        for symbol in &outline.symbols {
            render_symbol_recursive(symbol, 1, &mut output, char_budget);
        }
    }

    output
}

fn render_symbol_recursive(symbol: &CodeSymbol, depth: usize, output: &mut String, budget: usize) {
    if output.len() >= budget {
        return;
    }

    let indent = "  ".repeat(depth);
    let kind_str = match symbol.kind {
        SymbolKind::Function => "fn",
        SymbolKind::Method => "method",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::Trait => "trait",
        SymbolKind::Impl => "impl",
        SymbolKind::Class => "class",
        SymbolKind::Module => "mod",
        SymbolKind::Constant => "const",
        SymbolKind::TypeAlias => "type",
        SymbolKind::Variable => "var",
        SymbolKind::Macro => "macro",
        SymbolKind::Interface => "interface",
    };

    output.push_str(&format!("{}{} {}\n", indent, kind_str, symbol.name));

    for child in &symbol.children {
        render_symbol_recursive(child, depth + 1, output, budget);
    }
}
