use crate::providers::code_index_cache::build_code_index;
use crate::{ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Context, Result};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Deserialize, JsonSchema)]
pub struct CodeContextParams {
    /// Relative file path
    pub file: String,
    /// Symbol name to locate in the file
    pub symbol: String,
    /// Number of context lines above and below the symbol (default: 5)
    pub lines_around: Option<usize>,
}

rustycode_tools_api::define_tool! {
    pub struct CodeContextTool;

    name: "code_context",
    description: "Get a symbol's full signature, doc comment, and implementation. More focused than read_file.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: CodeContextParams, ctx) {
        let index = build_code_index(ctx)?;
        let full_path = ctx.cwd.join(&params.file);
        let lines_around = params.lines_around.unwrap_or(5);
        
        let symbols = index.file_symbols(&full_path);
        let symbol = symbols.into_iter().find(|s| s.name == params.symbol)
            .ok_or_else(|| anyhow!("Symbol `{}` not found in `{}`", params.symbol, params.file))?;
            
        // In a real implementation, we would need the end_line too.
        // For now, we'll read a reasonable chunk of lines.
        // Wait, CodeSymbol (the tree one) has end_line, but IndexedSymbol (the flat one) does NOT.
        // I should probably update IndexedSymbol to have end_line if we want code_context to be accurate.
        
        let content = std::fs::read_to_string(&full_path)
            .with_context(|| format!("failed to read {}", params.file))?;
            
        let lines: Vec<&str> = content.lines().collect();
        let start_line = symbol.line.saturating_sub(lines_around + 1);
        let end_line = (symbol.end_line + lines_around).min(lines.len());
        
        let mut output = format!("{}:{}  {} {}\n", params.file, symbol.line, symbol.kind, symbol.name);
        if let Some(ref docs) = symbol.doc_comment {
            for line in docs.lines() {
                output.push_str(&format!("/// {}\n", line));
            }
        }
        output.push_str("─────────────────────────────────────────\n");
        for (i, line) in lines[start_line..end_line].iter().enumerate() {
            let line_num = start_line + i + 1;
            let marker = if line_num == symbol.line { ">" } else { " " };
            output.push_str(&format!("{:4}{} | {}\n", line_num, marker, line));
        }
        output.push_str("─────────────────────────────────────────\n");
        
        Ok(ToolOutput::text(output))
    }
}
