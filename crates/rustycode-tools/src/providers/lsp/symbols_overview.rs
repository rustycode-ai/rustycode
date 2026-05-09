use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspGetSymbolsOverviewParams {
    #[serde(default)]
    relative_path: String,
    #[serde(default = "default_depth")]
    pub depth: u32,
    #[serde(default)]
    pub language: Option<String>,
}

fn default_depth() -> u32 {
    2
}

rustycode_tools_api::define_tool! {
    pub struct LspGetSymbolsOverviewTool;

    name: "LspGetSymbolsOverview",
    description: "Get a compact overview of symbols in a file grouped by kind",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug, ToolTag::Implement],

    execute(params: LspGetSymbolsOverviewParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let depth = params.depth as usize;
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

        let overview = crate::providers::symbol::symbols_overview(&symbols, depth);

        Ok(ToolOutput::text(serde_json::to_string_pretty(&overview)?).with_metadata(ctx, || json!({
                "file_path": file_path.to_string_lossy().to_string(),
                "symbols": overview
            })))
    }
}
