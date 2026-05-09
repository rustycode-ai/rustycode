use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspExtractSymbolParams {
    #[serde(default)]
    relative_path: String,
    pub name_path: String,
    pub target_file: String,
    #[serde(default)]
    pub language: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct LspExtractSymbolTool;

    name: "lsp_extract_symbol",
    description: "Extract a symbol definition to a new file or module",
    permission: ToolPermission::Write,
    tags: [ToolTag::Refactor],
    defer_loading: true,

    execute(params: LspExtractSymbolParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let name_path_str = &params.name_path;
        let target_file_str = &params.target_file;
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

        // Extract the symbol body
        let body_start =
            crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.start)?;
        let body_end =
            crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.end)?;
        let symbol_body = text[body_start..body_end].to_string();

        // Resolve target file path
        let target_file = ctx.cwd.join(target_file_str);
        ensure_path_within_workspace(ctx, &target_file)?;

        // Create parent directories if needed
        if let Some(parent) = target_file.parent() {
            std::fs::create_dir_all(parent).context("failed to create target directory")?;
        }

        // Write the extracted symbol to target file
        let target_content = if target_file.exists() {
            // Append to existing file
            let mut existing = read_file_blocking(&target_file)?;
            existing.push('\n');
            existing.push('\n');
            existing.push_str(&symbol_body);
            existing
        } else {
            // Create new file with module declaration if needed
            symbol_body.clone()
        };

        safe_write_file(&target_file, target_content.as_bytes())
            .context("failed to write target file")?;

        // Remove from original file
        let new_original =
            crate::providers::symbol::replace_range(&text, &target_symbol.range, "")?;
        safe_write_file(&file_path, new_original.as_bytes())
            .context("failed to update original file")?;

        let import_stmt = format!(
            "mod {}; use {}::*;",
            target_file
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy(),
            name_path_str.split('/').next().unwrap_or("_")
        );

        Ok(ToolOutput::text(serde_json::to_string_pretty(&json!({
                "extracted": name_path_str,
                "from": file_path.to_string_lossy(),
                "to": target_file.to_string_lossy(),
                "import_statement": import_stmt,
                "symbol_size_bytes": symbol_body.len()
            }))?).with_metadata(ctx, || json!({
                "extracted": name_path_str,
                "target_file": target_file.to_string_lossy(),
                "import": import_stmt
            })))
    }
}
