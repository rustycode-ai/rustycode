use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspWorkspaceSymbolsParams {
    pub query: String,
    #[serde(default = "default_language")]
    pub language: String,
}

fn default_language() -> String {
    "rust".to_string()
}

rustycode_tools_api::define_tool! {
    pub struct LspWorkspaceSymbolsTool;

    name: "LspWorkspaceSymbols",
    description: "Search for symbols across the entire workspace by name. PREFER THIS OVER GREP for finding function, struct, enum, or trait definitions — it returns exact locations with symbol kinds. Use when: you need to find where a type/function is defined, you know the symbol name but not the file. Requires: query (symbol name), language.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: LspWorkspaceSymbolsParams, ctx) {
        let query = &params.query;
        let language_str = &params.language;

        let language_id = LanguageId::from_path(&PathBuf::from(format!("dummy.{language_str}")));
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = match with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            run_async_result(async { client.workspace_symbols(query).await })
        }) {
            Ok(syms) => syms,
            Err(e) => {
                // Workspace symbols may fail if the language server hasn't finished
                // indexing or doesn't support the request. Return empty results with
                // a helpful message so callers can fall back to grep.
                tracing::debug!("workspace_symbols failed (server may still be indexing): {e:#}");
                return Ok(ToolOutput::text(format!("Workspace symbol search unavailable for '{query}'. The language server may still be indexing. Try lsp_document_symbols on a specific file instead, or use grep.\n")).with_metadata(ctx, || json!({
                        "query": query,
                        "count": 0,
                        "symbols": [],
                        "error": format!("{e:#}")
                    })));
            }
        };

        // Format the output
        let symbol_info: Vec<Value> = symbols
            .iter()
            .map(|sym| {
                json!({
                    "name": sym.name,
                    "kind": format!("{:?}", sym.kind),
                    "file": sym.location.uri.path().to_string(),
                    "line": sym.location.range.start.line,
                    "character": sym.location.range.start.character,
                    "container": sym.container_name.as_deref().unwrap_or("<root>")
                })
            })
            .collect();

        let text_summary = format!("Found {} symbol(s) matching '{}'\n\n", symbols.len(), query);

        let detailed = symbol_info.iter().map(|s| {
            format!(
                "{} ({}): {}:{}",
                s["name"].as_str().unwrap_or("?"),
                s["kind"].as_str().unwrap_or("?"),
                s["file"].as_str().unwrap_or("?"),
                s["line"].as_u64().unwrap_or(0)
            )
        });

        let detailed_text = detailed.collect::<Vec<_>>().join("\n");

        Ok(ToolOutput::text(format!("{text_summary}{detailed_text}")).with_metadata(ctx, || json!({
                "query": query,
                "count": symbols.len(),
                "symbols": symbol_info
            })))
    }
}
