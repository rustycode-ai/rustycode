use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use lsp_types::Position;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspReferencesParams {
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    pub language: Option<String>,
    pub line: u32,
    pub character: u32,
}

rustycode_tools_api::define_tool! {
    pub struct LspReferencesTool;

    name: "lsp_references",
    description: "Find ALL references (usages) of a symbol across the codebase. PREFER THIS OVER GREP for finding usages — it understands scope, imports, and renames. Use when: you need to refactor and want to know all call sites, you want to understand how a function/type is used, you're checking impact of a change. Requires: file_path, line, character.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug, ToolTag::Explore],

    execute(params: LspReferencesParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let line = params.line;
        let character = params.character;
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

        let references = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client
                    .references(uri.clone(), Position::new(line, character))
                    .await
            })
        })?;

        Ok(ToolOutput::text(serde_json::to_string_pretty(&references)?).with_metadata(ctx, || json!({ "references": references })))
    }
}
