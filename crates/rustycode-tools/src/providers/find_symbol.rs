use crate::providers::code_index_cache::build_code_index;
use crate::{ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Context, Result};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use rustycode_protocol::code_symbol::SymbolKind;

fn rel_path_string(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Deserialize, JsonSchema)]
pub struct FindSymbolParams {
    /// Name or partial name of the symbol to search for
    pub query: String,
    /// Optional symbol kind to filter results (e.g. Function, Struct, Class)
    pub kind: Option<String>,
    /// Optional glob pattern to restrict search scope (e.g. "src/auth/**")
    pub file_pattern: Option<String>,
    /// Maximum number of results to return (default: 10)
    pub limit: Option<usize>,
}

rustycode_tools_api::define_tool! {
    pub struct FindSymbolTool;

    name: "find_symbol",
    description: "Find functions, types, or methods by name across the project. Faster and more precise than grep for code navigation.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: FindSymbolParams, ctx) {
        let index = build_code_index(ctx)?;
        let limit = params.limit.unwrap_or(10).clamp(1, 50);
        
        let symbols = index.find_symbols(&params.query);
        
        let mut filtered: Vec<_> = symbols.into_iter().filter(|s| {
            if let Some(ref kind_filter) = params.kind {
                if format!("{:?}", s.kind).to_lowercase() != kind_filter.to_lowercase() {
                    return false;
                }
            }
            
            if let Some(ref pattern) = params.file_pattern {
                let rel_path = rel_path_string(&s.file_path, &ctx.cwd);
                if !glob_match::glob_match(pattern, &rel_path) {
                    return false;
                }
            }
            
            true
        }).collect();
        
        filtered.sort_by(|a, b| a.name.len().cmp(&b.name.len())); // Prefer shorter (more exact) matches
        
        let results: Vec<_> = filtered.into_iter().take(limit).map(|s| {
            let rel_path = rel_path_string(&s.file_path, &ctx.cwd);
            let parent_str = s.parent.as_ref().map(|p| format!(" [impl {}]", p)).unwrap_or_default();
            format!("{}:{}  {} {}{}", rel_path, s.line, s.kind, s.name, parent_str)
        }).collect();
        
        if results.is_empty() {
            Ok(ToolOutput::text(format!("No symbols found matching `{}`", params.query)))
        } else {
            let mut output = format!("Found {} symbols matching `{}`:\n", results.len(), params.query);
            for res in results {
                output.push_str(&format!("{}\n", res));
            }
            Ok(ToolOutput::text(output))
        }
    }
}
