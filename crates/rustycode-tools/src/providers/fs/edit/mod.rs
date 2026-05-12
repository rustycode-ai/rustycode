//! Edit file tool - inline editing capabilities with flexible matching.

mod diff_output;
mod matching;

#[cfg(test)]
mod tests;

use crate::line_endings::generate_diff;
use crate::security::{create_file_symlink_safe, open_file_symlink_safe, validate_write_path};
use crate::{ToolOutput, ToolPermission, ToolTag};
use diff_output::{format_edit_output, suggest_similar_files};
use matching::{
    try_exact_match, try_normalized_match, try_quote_normalized_match, try_trimmed_match,
};
use rustycode_tools_api::tool_error::{ToolError, ToolErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Maximum size for edit operations to prevent memory issues
const MAX_EDIT_SIZE: usize = 1024 * 1024; // 1 MB

/// Maximum lines to show in "not found" error context
const CONTEXT_LINES_ON_FAILURE: usize = 10;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct EditFileParams {
    /// The absolute path to the file to modify
    #[serde(alias = "file_path", alias = "path")]
    pub file_path: PathBuf,
    /// The text to replace
    #[serde(alias = "old_text", alias = "old_string")]
    pub old_string: String,
    /// The text to replace it with (must be different from old_string)
    #[serde(alias = "new_text", alias = "new_string")]
    pub new_string: String,
    /// Replace all occurrences of old_string (default false)
    #[serde(default)]
    pub replace_all: bool,
}

// Edit file tool
rustycode_tools_api::define_tool! {
    pub struct EditFile;

    name: "Edit",
    description: "Performs exact string replacements in files.\n\nUsage:\n- You must use your Read tool at least once in the conversation before editing. This tool will error if you attempt an edit without reading the file.\n- When editing text from Read tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: line number + tab. Everything after that is the actual file content to match. Never include any part of the line number prefix in the old_string or new_string.\n- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.\n- Only use emojis if the user explicitly requests it. Avoid adding emojis to files unless asked.\n- The edit will FAIL if old_string is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use replace_all to change every instance of old_string.\n- Use replace_all for replacing and renaming strings across the file. This parameter is useful if you want to rename a variable for instance.",
    permission: ToolPermission::Write,
    tags: [ToolTag::Implement],

    execute(params: EditFileParams, ctx) {

        // Validate path and check size limits
        let path_str = params
            .file_path
            .to_str()
            .ok_or_else(|| ToolError::new(ToolErrorCode::InvalidInput, "Invalid path: contains non-UTF-8 characters"))?;

        // Validate the path is within workspace and safe
        let validated_path = validate_write_path(
            path_str,
            &ctx.cwd,
            params.new_string.len(),
            !ctx.allow_outside_workspace,
        )
        .map_err(|e| -> anyhow::Error {
            let mut msg = format!("{e}");
            if msg.contains("not found") || msg.contains("No such file") {
                let suggestions = suggest_similar_files(&params.file_path, &ctx.cwd);
                if !suggestions.is_empty() {
                    msg.push_str(&format!(
                        "\n\nDid you mean one of these files?\n{}",
                        suggestions
                            .iter()
                            .map(|s| format!("  - {s}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ));
                }
            }
            anyhow::Error::from(ToolError::new(ToolErrorCode::InvalidInput, msg))
        })?;

        // Read the current file content using symlink-safe operation
        let mut file = open_file_symlink_safe(&validated_path).map_err(|e| -> anyhow::Error {
            let msg = format!("{e}");
            if msg.contains("not found") || msg.contains("No such file") {
                let suggestions = suggest_similar_files(&params.file_path, &ctx.cwd);
                let hint = crate::file_suggest::format_suggestions(&suggestions);
                if !hint.is_empty() {
                    return ToolError::path_not_found(format!("{}{hint}", params.file_path.display())).into();
                }
            }
            ToolError::io("Failed to open file", e).into()
        })?;

        // Defense-in-depth staleness check (also in validate_input)
        if let Some(state) = &ctx.file_read_state {
            let canonical = validated_path.canonicalize().ok();
            let current_mtime = canonical
                .as_ref()
                .and_then(|p| fs::metadata(p).ok())
                .and_then(|m| m.modified().ok());
            let check_path = canonical.as_ref().unwrap_or(&validated_path);
            if let Err(reason) = state.check_stale(check_path, current_mtime) {
                return Err(anyhow::anyhow!("{reason}"));
            }
        }

        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content).map_err(|e| -> anyhow::Error {
            if e.kind() == std::io::ErrorKind::InvalidData {
                ToolError::new(ToolErrorCode::InvalidInput, "Binary or non-UTF-8 file detected; Edit only supports text files").into()
            } else {
                ToolError::io("Failed to read file", e).into()
            }
        })?;

        // Check file size for edit operations
        if content.len() > MAX_EDIT_SIZE {
            return Ok(ToolOutput::text(format!(
                "File is too large for inline editing ({} bytes). Use other tools for large files.",
                content.len()
            )));
        }

        // Reject empty old_text — it would match everywhere and produce nonsensical results
        if params.old_string.is_empty() {
            return Err(ToolError::invalid_parameters("edit", "old_string cannot be empty. Provide the text to search for and replace.").into());
        }

        // Try matching strategies in order: exact → line-ending-normalized → quote-normalized → trimmed
        let new_content = if let Some((start, end)) = try_exact_match(&content, &params.old_string) {
            // Strategy 1: Exact match
            if params.replace_all {
                content.replace(&params.old_string, &params.new_string)
            } else {
                // Check for non-unique match — replacing only one of many is error-prone
                let match_count = content.matches(&params.old_string).count();
                if match_count > 1 {
                    return Ok(ToolOutput::text(format!(
                        "Found {match_count} matches of the old_string in the file, but replace_all is not set. \
                         To replace all occurrences, set replace_all to true. \
                         To replace only one occurrence, provide more surrounding context to uniquely identify the instance.\n\n\
                         Searched for:\n{}",
                        params.old_string.lines().take(CONTEXT_LINES_ON_FAILURE).collect::<Vec<_>>().join("\n")
                    )));
                }
                let mut result =
                    String::with_capacity(content.len() - (end - start) + params.new_string.len());
                result.push_str(&content[..start]);
                result.push_str(&params.new_string);
                result.push_str(&content[end..]);
                result
            }
        } else if let Some(replacement) =
            try_normalized_match(&content, &params.old_string, &params.new_string)
        {
            // Strategy 2: Line-ending-normalized match
            replacement
        } else if let Some(replacement) =
            try_quote_normalized_match(&content, &params.old_string, &params.new_string)
        {
            // Strategy 3: Quote-normalized match (curly → straight quotes)
            replacement
        } else if let Some(replacement) =
            try_trimmed_match(&content, &params.old_string, &params.new_string)
        {
            // Strategy 4: Trimmed match
            replacement
        } else {
            // All strategies failed — provide helpful context
            let file_preview: String = content
                .lines()
                .take(CONTEXT_LINES_ON_FAILURE)
                .enumerate()
                .map(|(i, l)| format!("{:4}: {}", i + 1, l))
                .collect::<Vec<_>>()
                .join("\n");
            let old_preview: String = params
                .old_string
                .lines()
                .take(CONTEXT_LINES_ON_FAILURE)
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(ToolOutput::text(format!(
                "Old text not found in file. No changes made.\n\n\
                 File content (first {CONTEXT_LINES_ON_FAILURE} lines):\n{file_preview}\n\n\
                 Searched for:\n{old_preview}"
            )));
        };

        // Verify replacement didn't dramatically increase file size
        if new_content.len() > MAX_EDIT_SIZE * 2 {
            return Err(anyhow::anyhow!(
                "Edit would increase file size beyond safe limit"
            ));
        }

        // Write the new content atomically: temp file → sync → rename
        use std::io::Write;
        let file_name = validated_path
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or("edit");
        let tmp_name = format!(".{file_name}.rustycode-tmp");
        let tmp_path = validated_path.with_file_name(tmp_name);

        // Clean up stale temp file if it exists
        let _ = fs::remove_file(&tmp_path);

        let mut file = create_file_symlink_safe(&tmp_path)
            .map_err(|e| anyhow::anyhow!("Failed to create temp file: {e}"))?;
        file.write_all(new_content.as_bytes()).map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            anyhow::anyhow!("Failed to write file: {e}")
        })?;
        file.sync_all().map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            anyhow::anyhow!("Failed to sync file: {e}")
        })?;
        drop(file);

        fs::rename(&tmp_path, &validated_path).map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            anyhow::anyhow!("Failed to rename temp file: {e}")
        })?;

        // Generate diff output
        let path_display = params.file_path.display().to_string();
        let diff = generate_diff(&content, &new_content, &path_display, 30);
        let formatter_diff = crate::workspace::formatter::format_file(&validated_path, &ctx.cwd);

        let output = format_edit_output(&path_display, &diff, formatter_diff);

        if let Some(state) = &ctx.file_read_state {
            state.invalidate(&validated_path);
        }

        Ok(ToolOutput::text(output))
    }
}
