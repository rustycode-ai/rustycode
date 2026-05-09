use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspRenameSymbolParams {
    #[serde(default)]
    relative_path: String,
    pub name_path: String,
    pub new_name: String,
    #[serde(default)]
    pub language: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct LspRenameSymbolTool;

    name: "lsp_rename_symbol",
    description: "Rename a symbol across the codebase",
    permission: ToolPermission::Write,
    tags: [ToolTag::Refactor],
    defer_loading: true,

    execute(params: LspRenameSymbolParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let name_path_str = &params.name_path;
        let new_name = &params.new_name;
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

        // Use the selection_range.start position (identifier location) for rename
        let rename_pos = target_symbol.selection_range.start;

        // Get rename edits from LSP
        let workspace_edits = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            run_async_result(async {
                client
                    .rename(uri.clone(), rename_pos, new_name.to_string())
                    .await
            })
        })?;

        // Apply edits to files
        let mut affected_files = Vec::new();
        if let Some(changes) = workspace_edits.changes {
            for (file_uri, edits) in changes {
                let file_path_from_uri = PathBuf::from(file_uri.path().to_string());

                let mut file_text = safe_read_file_to_string(&file_path_from_uri)
                    .context("failed to read file for rename")?;

                // Apply edits in reverse order to preserve positions
                for edit in edits.iter().rev() {
                    file_text = crate::providers::symbol::replace_range(
                        &file_text,
                        &edit.range,
                        &edit.new_text,
                    )?;
                }

                safe_write_file(&file_path_from_uri, file_text.as_bytes())
                    .context("failed to write renamed file")?;
                affected_files.push(file_path_from_uri.to_string_lossy().to_string());
            }
        }

        affected_files.sort();

        Ok(ToolOutput::text(serde_json::to_string_pretty(&json!({
                "renamed": name_path_str,
                "new_name": new_name,
                "files_modified": affected_files.len(),
                "files": affected_files
            }))?).with_metadata(ctx, || json!({
                "renamed": name_path_str,
                "new_name": new_name,
                "files_modified": affected_files.len(),
                "files": affected_files
            })))
    }
}
