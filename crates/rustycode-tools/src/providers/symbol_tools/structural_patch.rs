use crate::indexing::symbols::extract_file;
use anyhow::Context;
use rustycode_tools_api::{define_tool, ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StructuralPatchParams {
    /// Path to the file to modify.
    pub path: String,
    /// Name of the symbol to replace.
    pub symbol: String,
    /// New content for the symbol.
    pub content: String,
}

define_tool! {
    pub struct StructuralPatchTool;

    name: "structural_patch",
    namespace: "symbol",
    description: "Replace the entire body of a specific code symbol (function, method, struct, enum, impl) in a file. \
                  This is more robust than line-based editing because it targets the symbol's structural range using byte offsets. \
                  Use when: you need to rewrite an entire function, method, or type definition and know its name.",
    permission: ToolPermission::Write,
    tags: [ToolTag::Implement],
    defer_loading: true,

    execute(params: StructuralPatchParams, ctx) {
        let file_path = if std::path::Path::new(&params.path).is_absolute() {
            PathBuf::from(&params.path)
        } else {
            ctx.cwd.join(&params.path)
        };

        let full_content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read {}", file_path.display()))?;

        let outline = extract_file(&file_path, &full_content);

        let target_symbol = outline.symbols.iter()
            .find_map(|s| s.find_by_name_recursive(&params.symbol))
            .with_context(|| format!("Symbol '{}' not found in {}", params.symbol, params.path))?;

        let range = target_symbol.range;

        let mut new_full_content = full_content;
        new_full_content.replace_range(range.start_byte..range.end_byte, &params.content);

        std::fs::write(&file_path, new_full_content)?;

        Ok(ToolOutput::text(format!(
            "Successfully patched symbol '{}' in {}",
            params.symbol,
            params.path
        )))
    }
}
