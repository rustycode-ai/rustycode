use crate::indexing::symbols::{diff_outlines, extract_file, FileOutline};
use rustycode_tools_api::{define_tool, ToolOutput, ToolPermission, ToolTag};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use anyhow::Context;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckSymbolDriftParams {
    /// The path to the file that was modified.
    pub file: String,
    /// The file outline as it was BEFORE the change.
    /// You should have saved this from a previous outline_file call.
    pub previous_outline: FileOutline,
}

define_tool! {
    pub struct CheckSymbolDriftTool;

    name: "check_symbol_drift",
    namespace: "symbol",
    description: "Compare the current file structure against a previous outline to detect structural drift (added/removed/modified public symbols). \
                  ALWAYS run this after an edit to verify you haven't accidentally broken a public API signature.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Debug],
    defer_loading: true,

    execute(params: CheckSymbolDriftParams, ctx) {
        let file_path = if std::path::Path::new(&params.file).is_absolute() {
            PathBuf::from(&params.file)
        } else {
            ctx.cwd.join(&params.file)
        };

        let content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;

        let current_outline = extract_file(&file_path, &content);
        let diff = diff_outlines(&params.previous_outline, &current_outline);

        if diff.added.is_empty() && diff.removed.is_empty() && diff.modified.is_empty() {
            return Ok(ToolOutput::text("No structural drift detected. All symbols match the previous outline."));
        }

        let mut output = format!("Structural drift detected in {}:\n\n", params.file);

        if !diff.removed.is_empty() {
            output.push_str("REMOVED symbols:\n");
            for s in &diff.removed {
                output.push_str(&format!("  - {} {} (was at line {})\n", s.kind, s.name, s.line));
            }
            output.push('\n');
        }

        if !diff.added.is_empty() {
            output.push_str("ADDED symbols:\n");
            for s in &diff.added {
                output.push_str(&format!("  - {} {} (line {})\n", s.kind, s.name, s.line));
            }
            output.push('\n');
        }

        if !diff.modified.is_empty() {
            output.push_str("MODIFIED signatures:\n");
            for s in &diff.modified {
                output.push_str(&format!("  - {} {}:\n", s.kind, s.name));
                output.push_str(&format!("    Old: {}\n", s.old_signature));
                output.push_str(&format!("    New: {}\n", s.new_signature));
            }
        }

        Ok(ToolOutput::text(output).with_metadata(ctx, || serde_json::to_value(&diff).unwrap_or_default()))
    }
}
