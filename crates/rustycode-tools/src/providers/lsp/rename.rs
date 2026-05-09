use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use lsp_types::Position;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspRenameParams {
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    pub language: Option<String>,
    pub line: u32,
    pub character: u32,
    pub new_name: String,
}

rustycode_tools_api::define_tool! {
    pub struct LspRenameTool;

    name: "lsp_rename",
    description: "Rename a symbol at a position across all references. Use this to:
- Rename variables, functions, types, and other symbols
- Update all references automatically
- Ensure code remains consistent

Requires: file_path, line, character, new_name
Returns: Workspace edit with all changes to apply",
    permission: ToolPermission::Write,
    tags: [ToolTag::Refactor],
    defer_loading: true,

    execute(params: LspRenameParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let language_id = if let Some(lang_str) = &params.language {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let line = params.line;
        let character = params.character;
        let new_name = &params.new_name;

        let text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read file {}", file_path.display()))?;
        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path for URI: {}", file_path.display()))?;
        let language_str = language_id.language_id_str().to_string();

        let position = Position { line, character };

        let workspace_edit = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            let new_name = new_name.to_string();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.rename(uri, position, new_name).await
            })
        })?;

        // Format the output
        let changes_summary = if let Some(changes) = &workspace_edit.changes {
            changes
                .iter()
                .map(|(uri, edits)| {
                    json!({
                        "file": uri,
                        "edits": edits.len()
                    })
                })
                .collect::<Vec<_>>()
        } else if let Some(document_changes) = &workspace_edit.document_changes {
            match document_changes {
                lsp_types::DocumentChanges::Edits(edits) => edits
                    .iter()
                    .map(|edit| {
                        json!({
                            "file": edit.text_document.uri,
                            "edits": edit.edits.len()
                        })
                    })
                    .collect::<Vec<_>>(),
                lsp_types::DocumentChanges::Operations(ops) => ops
                    .iter()
                    .map(|op| match op {
                        lsp_types::DocumentChangeOperation::Op(op) => match op {
                            lsp_types::ResourceOp::Create(create) => json!({
                                "file": create.uri,
                                "operation": "create"
                            }),
                            lsp_types::ResourceOp::Rename(rename) => json!({
                                "old": rename.old_uri,
                                "new": rename.new_uri,
                                "operation": "rename"
                            }),
                            lsp_types::ResourceOp::Delete(delete) => json!({
                                "file": delete.uri,
                                "operation": "delete"
                            }),
                        },
                        lsp_types::DocumentChangeOperation::Edit(edit) => {
                            json!({
                                "file": edit.text_document.uri,
                                "edits": edit.edits.len()
                            })
                        }
                    })
                    .collect::<Vec<_>>(),
            }
        } else {
            vec![]
        };

        Ok(ToolOutput::text(serde_json::to_string_pretty(&workspace_edit)?).with_metadata(ctx, || json!({
                "workspace_edit": workspace_edit,
                "summary": {
                    "new_name": new_name,
                    "changes": changes_summary
                }
            })))
    }
}
