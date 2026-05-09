use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspInsertBeforeSymbolParams {
    #[serde(default)]
    relative_path: String,
    pub name_path: String,
    pub body: String,
    #[serde(default)]
    pub language: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct LspInsertBeforeSymbolTool;

    name: "lsp_insert_before_symbol",
    description: "Insert text before a symbol (at the beginning of its range)",
    permission: ToolPermission::Write,
    tags: [ToolTag::Implement],

    execute(params: LspInsertBeforeSymbolParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let name_path_str = &params.name_path;
        let body = &params.body;
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

        let insertion_line = target_symbol.range.start.line;
        let new_text = crate::providers::symbol::insert_at_line(&text, insertion_line, body)?;
        safe_write_file(&file_path, new_text.as_bytes())?;

        Ok(ToolOutput::text(serde_json::to_string_pretty(&json!({
                "inserted_before": name_path_str,
                "file_path": file_path.to_string_lossy(),
                "line": insertion_line
            }))?).with_metadata(ctx, || json!({
                "inserted_before": name_path_str,
                "file_path": file_path.to_string_lossy(),
                "line": insertion_line
            })))
    }
}
