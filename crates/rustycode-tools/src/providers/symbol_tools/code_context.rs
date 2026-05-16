use crate::providers::code_index_cache::build_code_index;
use anyhow::Context;
use rustycode_tools_api::{define_tool, ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn rel_path(path: &std::path::Path, cwd: &std::path::Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CodeContextParams {
    pub file: String,
    pub symbol: String,
    pub lines_around: Option<usize>,
}

define_tool! {
    pub struct CodeContextTool;

    name: "code_context",
    namespace: "symbol",
    description: "Get a symbol's full signature, doc comment, and implementation.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],
    defer_loading: true,

    execute(params: CodeContextParams, ctx) {
        let index = build_code_index(ctx)?;
        let lines_around = params.lines_around.unwrap_or(3);

        let target_file = if std::path::Path::new(&params.file).is_absolute() {
            PathBuf::from(&params.file)
        } else {
            ctx.cwd.join(&params.file)
        };
        let target_file = std::fs::canonicalize(&target_file)
            .unwrap_or_else(|_| target_file.clone());

        let all_matches: Vec<_> = index.find_symbols(&params.symbol).into_iter().collect();

        let sym = all_matches.iter()
            .find(|s| s.file_path == target_file)
            .or_else(|| all_matches.first());

        let sym = match sym {
            Some(s) => s,
            None => return Ok(ToolOutput::text(format!(
                "Symbol `{}` not found in {}",
                params.symbol, params.file
            ))),
        };

        let file_path = &sym.file_path;
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        let lines: Vec<&str> = content.lines().collect();

        let start_line = sym.line.saturating_sub(lines_around + 1);
        let end_line = (sym.line + lines_around).min(lines.len());

        let mut header = format!(
            "{} ({}) — {}:{}-{}\n\n",
            sym.name,
            sym.kind,
            rel_path(file_path, &ctx.cwd),
            start_line + 1,
            end_line
        );

        for (i, line) in lines[start_line..end_line].iter().enumerate() {
            let line_num = start_line + i + 1;
            header.push_str(&format!("{:>4}: {}\n", line_num, line));
        }

        if let Some(ref doc) = sym.doc_comment {
            header.push_str(&format!("\nDoc: {}\n", doc.trim()));
        }

        if let Some(ref parent) = sym.parent {
            header.push_str(&format!("\nParent: {}\n", parent));
        } else {
            header.push_str("\nParent: None\n");
        }

        if let Some(ref sig) = sym.signature {
            if !sig.is_empty() {
                header.push_str(&format!("\nSignature: {}\n", sig));
            }
        }

        Ok(ToolOutput::text(header))
    }
}
