use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspFindSymbolParams {
    #[serde(default)]
    relative_path: String,
    pub name_path: String,
    #[serde(default)]
    pub include_body: bool,
    #[serde(default)]
    pub language: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct LspFindSymbolTool;

    name: "lsp_find_symbol",
    description: "Search for symbols (functions, structs, enums, traits, modules) by qualified name path. FASTER and MORE PRECISE than grep for finding definitions — use this instead of grep when you know a symbol name. Returns symbol kind, file path, and location. Examples: 'main', 'Session::new', 'hash_map::Entry'. Requires: query (symbol name or path), language.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore, ToolTag::Debug],

    execute(params: LspFindSymbolParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let name_path_str = &params.name_path;
        let include_body = params.include_body;
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
        let matches = crate::providers::symbol::find_symbols(&symbols, &sym_path);

        let results: Vec<Value> = matches
            .iter()
            .map(|found| {
                let mut obj = json!({
                    "name": &found.symbol.name,
                    "kind": crate::providers::symbol::format_symbol_kind(&found.symbol.kind),
                    "qualified_path": &found.qualified_path,
                    "range": {
                        "start": {
                            "line": found.symbol.range.start.line,
                            "character": found.symbol.range.start.character,
                        },
                        "end": {
                            "line": found.symbol.range.end.line,
                            "character": found.symbol.range.end.character,
                        }
                    },
                    "selection_range": {
                        "start": {
                            "line": found.symbol.selection_range.start.line,
                            "character": found.symbol.selection_range.start.character,
                        },
                        "end": {
                            "line": found.symbol.selection_range.end.line,
                            "character": found.symbol.selection_range.end.character,
                        }
                    }
                });

                if include_body {
                    if let Ok(start_idx) = crate::providers::symbol::position_to_byte_index(
                        &text,
                        found.symbol.range.start,
                    ) {
                        if let Ok(end_idx) = crate::providers::symbol::position_to_byte_index(
                            &text,
                            found.symbol.range.end,
                        ) {
                            if let Some(body) = text.get(start_idx..end_idx) {
                                obj["body"] = Value::String(body.to_string());
                            }
                        }
                    }
                }

                obj
            })
            .collect();

        Ok(ToolOutput::text(serde_json::to_string_pretty(&results)?).with_metadata(ctx, || json!({
                "file_path": file_path.to_string_lossy(),
                "matches_count": results.len(),
                "matches": results
            })))
    }
}
