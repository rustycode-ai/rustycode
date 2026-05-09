use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use lsp_types::Position;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspDefinitionParams {
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    pub language: Option<String>,
    pub line: u32,
    pub character: u32,
}

rustycode_tools_api::define_tool! {
    pub struct LspDefinitionTool;

    name: "lsp_definition",
    description: "Jump to the definition of a function, variable, type, or import at a specific position. PREFER THIS OVER GREP for navigation — gives the exact definition location. Use when: you see a symbol used in code and want to find where it's defined, you need to trace an import to its source. Requires: file_path, line, character.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug, ToolTag::Explore],
    defer_loading: true,

    execute(params: LspDefinitionParams, ctx) {
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

        let definition = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let language_str = language_str.clone();
            let text = text.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client
                    .goto_definition(uri.clone(), Position::new(line, character))
                    .await
            })
        })?;

        Ok(ToolOutput::with_structured(
            serde_json::to_string_pretty(&definition)?,
            json!({ "definition": definition }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

    #[test]
    fn test_definition_tool_name_and_description() {
        let tool = LspDefinitionTool;
        assert_eq!(tool.name(), "lsp_definition");
    }

    #[test]
    fn test_definition_missing_parameters() {
        let tool = LspDefinitionTool;
        let (ctx, _temp) = create_test_context();

        let result = tool.execute(json!({ "line": 0, "character": 0 }), &ctx);
        assert!(result.is_err());
    }
}
