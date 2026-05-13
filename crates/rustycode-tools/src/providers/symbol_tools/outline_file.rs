use crate::indexing::symbols::extract_file;
use crate::indexing::symbols::renderers::file_outline::{render_symbol_to_buffer, OutlineDepth};
use anyhow::Context;
use rustycode_protocol::code_symbol::CodeSymbol;
use rustycode_tools_api::{define_tool, ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutlineFileParams {
    /// Relative or absolute path to the file to outline.
    pub file: String,
    /// Optional symbol path to drill into (e.g. "User", "User::new", "impl Authenticator").
    /// When provided, only that symbol and its children are shown.
    pub symbol: Option<String>,
    /// Level of detail to return.
    #[serde(default = "default_detail")]
    pub detail: String, // "condensed", "signatures", "detailed"
}

fn default_detail() -> String {
    "detailed".to_string()
}

define_tool! {
    pub struct OutlineFileTool;

    name: "outline_file",
    namespace: "symbol",
    description: "Get a hierarchical outline of a file's code structure (classes, methods, functions). \
                  Use this to understand a file's API and layout without reading the entire implementation. \
                  Adjust 'detail' to 'signatures' for a bird's eye view or 'detailed' for deep analysis.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],
    defer_loading: true,

    execute(params: OutlineFileParams, ctx) {
        let file_path = if std::path::Path::new(&params.file).is_absolute() {
            PathBuf::from(&params.file)
        } else {
            ctx.cwd.join(&params.file)
        };

        let content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;

        let outline = extract_file(&file_path, &content);
        let rel = file_path.strip_prefix(&ctx.cwd).unwrap_or(&file_path)
            .to_string_lossy()
            .replace('\\', "/");

        let total_symbols = count_all(&outline.symbols);
        let lines = content.lines().count();

        let mut output = format!(
            "{} ({} lines, {} symbols)\n{}\n\n",
            rel,
            lines,
            total_symbols,
            "─".repeat(rel.len().min(60))
        );

        let depth = match params.detail.to_lowercase().as_str() {
            "condensed" => OutlineDepth::Condensed,
            "signatures" => OutlineDepth::Signatures,
            _ => OutlineDepth::Detailed,
        };

        if let Some(ref sym_path) = params.symbol {
            // Drill into a specific symbol
            if let Some(sym) = find_symbol(&outline.symbols, sym_path) {
                render_symbol_to_buffer(sym, 0, depth, &mut output);
            } else {
                output.push_str(&format!("Symbol `{}` not found in {}.\n", sym_path, rel));
                output.push_str("\nAvailable top-level symbols:\n");
                for s in &outline.symbols {
                    output.push_str(&format!("  {} {} :{}\n", s.kind, s.name, s.line));
                }
            }
        } else {
            if outline.symbols.is_empty() {
                output.push_str("No symbols found.\n");
            } else {
                for sym in &outline.symbols {
                    render_symbol_to_buffer(sym, 0, depth, &mut output);
                }
            }

            if !outline.imports.is_empty() {
                let shown = outline.imports.len().min(5);
                output.push_str(&format!("\nImports: {} total", outline.imports.len()));
                if shown < outline.imports.len() {
                    output.push_str(&format!(" (showing first {})", shown));
                }
                output.push('\n');
                for imp in &outline.imports[..shown] {
                    output.push_str(&format!("  {}\n", imp));
                }
            }
        }

        Ok(ToolOutput::text(output))
    }
}

fn count_all(symbols: &[CodeSymbol]) -> usize {
    symbols.iter().map(|s| 1 + count_all(&s.children)).sum()
}

fn find_symbol<'a>(symbols: &'a [CodeSymbol], path: &str) -> Option<&'a CodeSymbol> {
    let (head, rest) = match path.split_once("::") {
        Some((h, r)) => (h, Some(r)),
        None => (path, None),
    };

    for sym in symbols {
        if sym.name.eq_ignore_ascii_case(head)
            || format!("{} {}", sym.kind, sym.name).eq_ignore_ascii_case(path)
        {
            return match rest {
                Some(tail) => find_symbol(&sym.children, tail),
                None => Some(sym),
            };
        }
    }
    None
}
