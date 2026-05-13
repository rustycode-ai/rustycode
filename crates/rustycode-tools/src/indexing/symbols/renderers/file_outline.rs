use crate::indexing::symbols::{CodeSymbol, FileOutline};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineDepth {
    /// Line, Name, and Kind only.
    Condensed,
    /// Line, Name, Kind, and Signature.
    Signatures,
    /// Line, Name, Kind, Signature, and First line of Doc comment.
    Detailed,
}

pub fn render_file_outline(outline: &FileOutline, depth: OutlineDepth) -> String {
    let mut buffer = String::new();
    for symbol in &outline.symbols {
        render_symbol_to_buffer(symbol, 0, depth, &mut buffer);
    }
    buffer
}

pub fn render_symbol_to_buffer(symbol: &CodeSymbol, indent_level: usize, depth: OutlineDepth, buffer: &mut String) {
    let indent = "  ".repeat(indent_level);
    
    match depth {
        OutlineDepth::Condensed => {
            let _ = writeln!(buffer, "{indent}{}:{}  {}", symbol.line, symbol.name, symbol.kind);
        }
        OutlineDepth::Signatures | OutlineDepth::Detailed => {
            // Prefer signature if available, otherwise fallback to "kind name"
            if !symbol.signature.is_empty() {
                let _ = writeln!(buffer, "{indent}{} :{}", symbol.signature.trim(), symbol.line);
            } else {
                let _ = writeln!(buffer, "{indent}{} {} :{}", symbol.kind, symbol.name, symbol.line);
            }
            
            if depth == OutlineDepth::Detailed {
                if let Some(ref doc) = symbol.doc_comment {
                    let first_line = doc.lines()
                        .find(|l| !l.trim().is_empty())
                        .map(|l| l.trim().trim_start_matches("///").trim_start_matches("/**").trim_start_matches("/*").trim_start_matches('*').trim())
                        .unwrap_or("");
                    if !first_line.is_empty() {
                        let truncated = if first_line.len() > 80 { &first_line[..80] } else { first_line };
                        let _ = writeln!(buffer, "{indent}  // {truncated}");
                    }
                }
            }
        }
    }

    for child in &symbol.children {
        render_symbol_to_buffer(child, indent_level + 1, depth, buffer);
    }
}
