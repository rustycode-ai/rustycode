use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use lsp_types::DiagnosticSeverity;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspFullDiagnosticsParams {
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    pub language: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct LspFullDiagnosticsTool;

    name: "lsp_full_diagnostics",
    description: "Get diagnostics (errors, warnings, hints) for a file WITHOUT running a build. PREFER THIS OVER cargo check for quick feedback on recent edits — faster and shows inline error locations. Use when: you just edited a file and want to check for errors, you need to verify types/signatures match. Requires: file_path.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug, ToolTag::Explore, ToolTag::Implement],
    defer_loading: true,

    execute(params: LspFullDiagnosticsParams, ctx) {
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

        let diagnostics = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.diagnostic(uri).await
            })
        })?;

        // Calculate build status
        let error_count = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .count();
        let warning_count = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
            .count();
        let hint_count = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::HINT))
            .count();

        let status = if error_count > 0 {
            "failed"
        } else if warning_count > 0 {
            "warnings"
        } else {
            "success"
        };

        Ok(ToolOutput::text(serde_json::to_string_pretty(&diagnostics)?).with_metadata(ctx, || json!({
                "diagnostics": diagnostics,
                "build_status": {
                    "status": status,
                    "error_count": error_count,
                    "warning_count": warning_count,
                    "hint_count": hint_count
                }
            })))
    }
}
