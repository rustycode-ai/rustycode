use crate::indexing::CodeIndex;
use anyhow::{anyhow, Context, Result};
use rustycode_protocol::code_symbol::CodeSymbol;
use rustycode_tools_api::{define_tool, ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
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

        let mut symbols: Vec<_> = index.symbol_index.all_symbols().iter().filter(|s| {
            let matches_query = s.name.contains(&params.query);
            let matches_kind = params.kind.as_ref().is_none_or(|k| format!("{:?}", s.kind).to_lowercase() == k.to_lowercase());
            matches_query && matches_kind
        }).cloned().collect();

        symbols.truncate(limit);

        if symbols.is_empty() {
            return Ok(ToolOutput::text(format!("No symbols found matching `{}`", params.query)));
        }

        let mut output = format!("Found {} matches:\n", symbols.len());
        let code_symbols: Vec<CodeSymbol> = symbols.iter().map(|s| CodeSymbol {
            name: s.name.clone(),
            kind: rustycode_protocol::code_symbol::SymbolKind::Function,
            line: s.line,
            end_line: s.line,
            range: rustycode_protocol::code_symbol::SymbolRange {
                start_line: 0, start_col: 0, end_line: 0, end_col: 0, start_byte: 0, end_byte: 0
            },
            signature: String::new(),
            doc_comment: s.doc_comment.clone(),
            visibility: rustycode_protocol::code_symbol::Visibility::Private,
            children: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }).collect();

        // Use the new hierarchical renderer
        output.push_str(&crate::indexing::symbols::renderers::render_llm_outline(&code_symbols));

        Ok(ToolOutput::text(output))
    }
}
