use crate::indexing::CodeIndex;
use anyhow::{anyhow, Context, Result};
use rustycode_tools_api::{define_tool, ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

static CODE_INDEX_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<CodeIndex>>>> = OnceLock::new();

fn code_indexes() -> &'static Mutex<HashMap<PathBuf, Arc<CodeIndex>>> {
    CODE_INDEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn workspace_root(ctx: &crate::ToolContext) -> PathBuf {
    std::fs::canonicalize(&ctx.cwd).unwrap_or_else(|_| ctx.cwd.clone())
}

pub(super) fn build_code_index(ctx: &crate::ToolContext) -> Result<Arc<CodeIndex>> {
    let root = workspace_root(ctx);
    let mut guard = code_indexes()
        .lock()
        .map_err(|_| anyhow!("failed to lock symbol index cache"))?;

    if let Some(index) = guard.get(&root) {
        return Ok(Arc::clone(index));
    }

    let mut index = CodeIndex::new(root.clone());
    index
        .build()
        .with_context(|| format!("failed to build code index for {}", root.display()))?;

    let index = Arc::new(index);
    guard.insert(root, Arc::clone(&index));
    Ok(index)
}

fn rel_path(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FindSymbolParams {
    pub query: String,
    /// Optional kind filter: fn, struct, enum, trait, impl, class, mod, const, type, var, macro, interface
    pub kind: Option<String>,
    pub file_pattern: Option<String>,
    pub limit: Option<usize>,
}

define_tool! {
    pub struct FindSymbolTool;

    name: "find_symbol",
    namespace: "symbol",
    description: "Find functions, types, or methods by name across the project.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],
    defer_loading: true,

    execute(params: FindSymbolParams, ctx) {
        let index = build_code_index(ctx)?;
        let limit = params.limit.unwrap_or(20).clamp(1, 50);
        let query_lower = params.query.to_lowercase();

        let mut symbols: Vec<_> = index.find_symbols(&params.query).into_iter().collect();

        if symbols.is_empty() {
            symbols = index.symbol_index.all_symbols()
                .iter()
                .filter(|s| {
                    let name_lower = s.name.to_lowercase();
                    name_lower.contains(&query_lower)
                        || name_lower.starts_with(&query_lower)
                })
                .collect();
        }

        if let Some(ref kind_filter) = params.kind {
            let kind_lower = kind_filter.to_lowercase();
            symbols.retain(|s| format!("{}", s.kind).to_lowercase() == kind_lower);
        }

        if let Some(ref pattern) = params.file_pattern {
            let pat = pattern.replace('\\', "/").to_lowercase();
            symbols.retain(|s| {
                rel_path(&s.file_path, &ctx.cwd).to_lowercase().contains(&pat)
            });
        }

        symbols.sort_by(|a, b| {
            a.name.len().cmp(&b.name.len())
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.line.cmp(&b.line))
        });

        symbols.truncate(limit);

        if symbols.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No symbols found matching `{}`",
                params.query
            )));
        }

        let mut output = format!(
            "Found {} symbols matching `{}`:\n\n",
            symbols.len(),
            params.query
        );

        for sym in &symbols {
            let rel = rel_path(&sym.file_path, &ctx.cwd);
            output.push_str(&format!(
                "  {} {} ({}:{})\n",
                sym.kind, sym.name, rel, sym.line
            ));
            if let Some(ref sig) = sym.signature {
                if !sig.is_empty() {
                    output.push_str(&format!("    {}\n", sig));
                }
            }
        }

        Ok(ToolOutput::text(output))
    }
}
