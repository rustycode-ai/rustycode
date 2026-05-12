use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use lsp_types::{CompletionContext, CompletionTriggerKind, Position};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspCompletionParams {
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    pub language: Option<String>,
    pub line: u32,
    pub character: u32,
    #[serde(default)]
    pub trigger_character: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct LspCompletionTool;

    name: "LspCompletion",
    namespace: "lsp",
    description: "Get code completions (suggestions) at a specific position. Use this to see what functions, variables, or keywords are available while typing. Requires file_path, line, and character position.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug, ToolTag::Implement],

    execute(params: LspCompletionParams, ctx) {
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

        let trigger_character = params.trigger_character.clone();
        let context = trigger_character.clone().map(|ch| CompletionContext {
            trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(ch),
        });

        let completion = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client
                    .completion(uri.clone(), Position::new(line, character), context)
                    .await
            })
        })?;

        Ok(ToolOutput::text(serde_json::to_string_pretty(&completion)?).with_metadata(ctx, || json!({ "completion": completion })))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

    #[test]
    fn test_completion_tool_name_and_description() {
        let tool = LspCompletionTool;
        assert_eq!(tool.name(), "LspCompletion");
    }

    #[test]
    fn test_completion_missing_required_parameters() {
        let tool = LspCompletionTool;
        let (ctx, _temp) = create_test_context();

        let result = tool.execute(json!({ "line": 0, "character": 0 }), &ctx);
        assert!(result.is_err());
    }
}
