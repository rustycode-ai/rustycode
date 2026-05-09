use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspDocumentSymbolsParams {
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    pub language: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct LspDocumentSymbolsTool;

    name: "lsp_document_symbols",
    description: "Get the structure of a file (functions, classes, modules, etc.) without reading the entire content. Use this to:
- Understand what's in a file before reading it
- Get an overview of file organization
- Find specific symbols in a file
- Navigate large files efficiently

Requires: file_path
Returns: Hierarchical list of symbols with their types and locations",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug, ToolTag::Explore],
    defer_loading: true,

    execute(params: LspDocumentSymbolsParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = &params.language {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri).await
            })
        })?;

        Ok(ToolOutput::text(serde_json::to_string_pretty(&symbols)?).with_metadata(ctx, || json!({ "symbols": symbols })))
    }
}
