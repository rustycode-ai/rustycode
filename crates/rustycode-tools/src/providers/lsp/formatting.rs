use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use lsp_types::Position;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspFormattingParams {
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub character: Option<u32>,
    #[serde(default)]
    pub end_line: Option<u32>,
    #[serde(default)]
    pub end_character: Option<u32>,
}

rustycode_tools_api::define_tool! {
    pub struct LspFormattingTool;

    name: "LspFormatting",
    namespace: "lsp",
    description: "Format a document using the language server's formatter. Use this to:
- Format entire files according to language standards
- Apply consistent code style
- Fix indentation and spacing

Requires: file_path
Optional: range (line, character, end_line, end_character) for range formatting
Returns: Text edits to apply for formatting",
    permission: ToolPermission::Read,
    tags: [ToolTag::Refactor],

    execute(params: LspFormattingParams, ctx) {
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

        let text_edits = if let (Some(line), Some(character), Some(end_line), Some(end_character)) = (
            params.line,
            params.character,
            params.end_line,
            params.end_character,
        ) {
            // Range formatting
            let range = lsp_types::Range {
                start: Position {
                    line,
                    character,
                },
                end: Position {
                    line: end_line,
                    character: end_character,
                },
            };

            with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
                let uri = uri.clone();
                let language_str = language_str.clone();
                let text = text.clone();
                run_async_result(async {
                    client
                        .open_document(uri.clone(), &language_str, 1, &text)
                        .await?;
                    client.document_range_formatting(uri, range).await
                })
            })?
        } else {
            // Full document formatting
            with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
                let uri = uri.clone();
                let language_str = language_str.clone();
                let text = text.clone();
                run_async_result(async {
                    client
                        .open_document(uri.clone(), &language_str, 1, &text)
                        .await?;
                    client.document_formatting(uri).await
                })
            })?
        };

        // Format the output
        let edits_summary: Vec<Value> = text_edits
            .iter()
            .map(|edit| {
                json!({
                    "range": edit.range,
                    "new_text": edit.new_text.chars().take(50).collect::<String>() + if edit.new_text.len() > 50 { "..." } else { "" }
                })
            })
            .collect();

        Ok(ToolOutput::text(serde_json::to_string_pretty(&text_edits)?).with_metadata(ctx, || json!({
                "formatting_edits": text_edits,
                "summary": {
                    "edit_count": text_edits.len()
                },
                "preview": edits_summary
            })))
    }
}
