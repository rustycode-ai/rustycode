use crate::indexing::symbols::extract_file;
use crate::security::sanitize_for_log;
use crate::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use rustycode_tools_api::{define_tool, ToolContext, ToolOutput};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use std::path::PathBuf;

define_tool!(
    StructuralPatchTool,
    "structural_patch",
    "Replace the entire body of a specific code symbol (function, method, class) in a file. \
     This is more robust than line-based editing as it targets the symbol's structural range.",
    StructuralPatchArgs
);

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct StructuralPatchArgs {
    /// Path to the file to modify.
    pub path: String,
    /// Name of the symbol to replace.
    pub symbol: String,
    /// New content for the symbol.
    pub content: String,
}

#[async_trait]
impl Tool for StructuralPatchTool {
    async fn run(&self, ctx: &ToolContext, args: StructuralPatchArgs) -> Result<ToolOutput> {
        let path = ctx.resolve_path(&args.path)?;
        let full_content = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let outline = extract_file(&path, &full_content);
        
        let target_symbol = outline.symbols.iter()
            .find_map(|s| s.find_by_name_recursive(&args.symbol))
            .with_context(|| format!("Symbol '{}' not found in {}", args.symbol, args.path))?;

        let range = target_symbol.range;
        
        // We use byte offsets for maximum precision
        let mut new_full_content = full_content.clone();
        new_full_content.replace_range(range.start_byte..range.end_byte, &args.content);

        tokio::fs::write(&path, new_full_content).await?;

        Ok(ToolOutput::new(format!(
            "Successfully patched symbol '{}' in {}",
            args.symbol,
            args.path
        )))
    }
}
