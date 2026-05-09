use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use lsp_types::Position;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspCodeActionsParams {
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    pub language: Option<String>,
    pub line: u32,
    pub character: u32,
    #[serde(default)]
    pub end_line: Option<u32>,
    #[serde(default)]
    pub end_character: Option<u32>,
}

rustycode_tools_api::define_tool! {
    pub struct LspCodeActionsTool;

    name: "lsp_code_actions",
    description: "Get available code actions and refactorings for a range. Use this to:
- Find quick fixes for errors and warnings
- Discover available refactorings
- Get code improvements suggested by the language server

Requires: file_path, line, character
Optional: end_line, end_character (for range, defaults to position)
Returns: List of code actions with titles and kinds",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug, ToolTag::Explore],
    defer_loading: true,

    execute(params: LspCodeActionsParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = &params.language {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let line = params.line;
        let character = params.character;
        let end_line = params.end_line;
        let end_character = params.end_character;

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let range = lsp_types::Range {
            start: Position { line, character },
            end: Position {
                line: end_line.unwrap_or(line),
                character: end_character.unwrap_or(character),
            },
        };

        let code_actions = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.code_actions(uri, range).await
            })
        })?;

        // Format the output
        let actions_summary: Vec<Value> = code_actions
            .iter()
            .map(|action| {
                json!({
                    "title": action.title,
                    "kind": action.kind,
                    "is_preferred": action.is_preferred,
                    "disabled": action.disabled,
                })
            })
            .collect();

        Ok(ToolOutput::text(serde_json::to_string_pretty(&code_actions)?).with_metadata(ctx, || json!({ "code_actions": actions_summary })))
    }
}
