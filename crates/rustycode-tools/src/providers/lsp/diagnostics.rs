use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspDiagnosticsParams {
    #[serde(default = "default_servers")]
    pub servers: Vec<String>,
}

fn default_servers() -> Vec<String> {
    vec![
        "rust-analyzer".to_string(),
        "typescript-language-server".to_string(),
        "pyright-langserver".to_string(),
    ]
}

rustycode_tools_api::define_tool! {
    pub struct LspDiagnosticsTool;

    name: "lsp_diagnostics",
    description: "Check which language servers are available and their status. Use this to verify code intelligence capabilities before using other LSP tools.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug, ToolTag::Explore, ToolTag::Implement],

    execute(params: LspDiagnosticsParams, ctx) {
        let servers = params.servers;
        let statuses = rustycode_lsp::discover(&servers);
        Ok(ToolOutput::text(serde_json::to_string_pretty(&statuses)?).with_metadata(ctx, || json!(statuses)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

    #[test]
    fn test_diagnostics_tool_name_and_description() {
        let tool = LspDiagnosticsTool;
        assert_eq!(tool.name(), "lsp_diagnostics");
        assert_eq!(
            tool.description(),
            "Check which language servers are available and their status. Use this to verify code intelligence capabilities before using other LSP tools."
        );
    }

    #[test]
    fn test_diagnostics_tool_permission() {
        let tool = LspDiagnosticsTool;
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_diagnostics_default_servers() {
        let tool = LspDiagnosticsTool;
        let (ctx, _temp) = create_test_context();

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(
            output.text.contains("rust-analyzer")
                || output.text.contains("typescript-language-server")
                || output.text.contains("pyright-langserver")
        );
        assert!(output.structured.is_some());
    }
}
