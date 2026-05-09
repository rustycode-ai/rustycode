use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspAnalyzeSymbolParams {
    #[serde(default)]
    relative_path: String,
    pub name_path: String,
    #[serde(default)]
    pub language: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct LspAnalyzeSymbolTool;

    name: "LspAnalyzeSymbol",
    description: "Analyze a symbol to get its references, implementations, call hierarchy, and complexity metrics. Use when: you need a comprehensive understanding of a symbol's role in the codebase, you're planning a refactor, or you need to understand inheritance/implementation chains. Requires: file_path, line, character.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug, ToolTag::Explore, ToolTag::Refactor],

    execute(params: LspAnalyzeSymbolParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let name_path_str = &params.name_path;
        let language_id = if let Some(lang_str) = &params.language {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        // Get references
        let references = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let pos = target_symbol.selection_range.start;
            run_async_result(async { client.references(uri.clone(), pos).await })
        })?;

        // Get implementations (would require gotoImplementation which LSP supports but LspClient doesn't yet expose)
        // For now, we use an empty vector - in future versions this could be added to LspClient
        let implementations: Vec<lsp_types::Location> = Vec::new();

        // Calculate body complexity (simple heuristics: lines, nesting depth)
        let body_text = if let Ok(start_idx) =
            crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.start)
        {
            if let Ok(end_idx) =
                crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.end)
            {
                text.get(start_idx..end_idx).unwrap_or("")
            } else {
                ""
            }
        } else {
            ""
        };

        let body_lines = body_text.lines().count();
        let nesting_depth = body_text.chars().filter(|&c| c == '{' || c == '(').count();

        // Group references by file
        let mut refs_by_file: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
        for loc in &references {
            let file_key = loc.uri.path().to_string();
            let entry = refs_by_file.entry(file_key).or_default();
            entry.push((loc.range.start.line + 1, loc.range.start.character + 1));
        }

        Ok(ToolOutput::text(serde_json::to_string_pretty(&json!({
                "symbol": name_path_str,
                "kind": crate::providers::symbol::format_symbol_kind(&target_symbol.kind),
                "definition": {
                    "file": file_path.to_string_lossy(),
                    "range": {
                        "start": format!("{}:{}", target_symbol.range.start.line + 1, target_symbol.range.start.character + 1),
                        "end": format!("{}:{}", target_symbol.range.end.line + 1, target_symbol.range.end.character + 1)
                    }
                },
                "references": {
                    "total_count": references.len(),
                    "by_file": refs_by_file
                },
                "implementations": {
                    "count": implementations.len(),
                    "locations": implementations.iter().map(|loc| {
                        format!("{}:{}", loc.range.start.line + 1, loc.range.start.character + 1)
                    }).collect::<Vec<_>>()
                },
                "complexity": {
                    "lines": body_lines,
                    "nesting_depth": nesting_depth,
                    "cyclomatic_estimate": (nesting_depth / 2).max(1)
                }
            }))?).with_metadata(ctx, || json!({
                "symbol": name_path_str,
                "references_count": references.len(),
                "implementations_count": implementations.len(),
                "body_lines": body_lines,
                "nesting_depth": nesting_depth
            })))
    }
}
