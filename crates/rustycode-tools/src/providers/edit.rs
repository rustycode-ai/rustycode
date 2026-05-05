//! Edit file tool - inline editing capabilities with flexible matching.
//!
//! Supports multiple matching strategies (exact, line-ending-normalized,
//! quote-normalized, trimmed) to handle common LLM output issues like
//! whitespace normalization and curly quotes.
//! Preserves original line endings and shows diff output.

use crate::file_formatter;
use crate::line_endings::{detect_line_ending, generate_diff, normalize_quotes, normalize_to_lf};
use crate::security::{create_file_symlink_safe, open_file_symlink_safe, validate_write_path};
use crate::{Tool, ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Maximum size for edit operations to prevent memory issues
const MAX_EDIT_SIZE: usize = 1024 * 1024; // 1 MB

/// Maximum lines to show in "not found" error context
const CONTEXT_LINES_ON_FAILURE: usize = 10;

/// Find files with similar names in the workspace (for "did you mean?" suggestions).
fn suggest_similar_files(target: &std::path::Path, cwd: &std::path::Path) -> Vec<String> {
    crate::file_suggest::suggest_similar_files(target, cwd, 3)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EditFileInput {
    #[serde(alias = "file_path")]
    pub path: PathBuf,
    #[serde(alias = "old_string")]
    pub old_text: String,
    #[serde(alias = "new_string")]
    pub new_text: String,
    #[serde(default)]
    pub replace_all: bool,
}

/// Try exact string match
pub(crate) fn try_exact_match(content: &str, old_text: &str) -> Option<(usize, usize)> {
    content
        .find(old_text)
        .map(|start| (start, start + old_text.len()))
}

/// Try matching after normalizing line endings (CRLF → LF).
/// Normalizes both content and `old_text` to LF, performs replacement, then
/// restores original line endings.
pub(crate) fn try_normalized_match(
    content: &str,
    old_text: &str,
    new_text: &str,
) -> Option<String> {
    let norm_content = normalize_to_lf(content);
    let norm_old = normalize_to_lf(old_text);
    if norm_content.contains(&norm_old) {
        let norm_new = normalize_to_lf(new_text);
        let result = norm_content.replacen(&norm_old, &norm_new, 1);
        // Restore original line ending style
        let ending = detect_line_ending(content);
        Some(crate::line_endings::apply_line_ending(&result, ending))
    } else {
        None
    }
}

/// Try matching after normalizing curly/smart quotes to straight quotes.
/// Handles the case where LLMs generate `\u{201C}`/`\u{201D}` (curly double)
/// or `\u{2018}`/`\u{2019}` (curly single) instead of straight quotes.
///
/// Quote normalization preserves character count (1 char → 1 char) but changes
/// byte length (3-byte curly quote → 1-byte straight quote), so we must map
/// through character indices rather than using byte offsets directly.
pub(crate) fn try_quote_normalized_match(
    content: &str,
    old_text: &str,
    new_text: &str,
) -> Option<String> {
    let norm_content = normalize_quotes(content);
    let norm_old = normalize_quotes(old_text);
    let match_start_byte = norm_content.find(&norm_old)?;

    // Convert byte offset in normalized content to character offset
    let start_char_idx = norm_content[..match_start_byte].chars().count();
    let match_char_count = norm_old.chars().count();
    let end_char_idx = start_char_idx + match_char_count;

    // Map character offsets to byte offsets in the original content
    let byte_start = content
        .char_indices()
        .nth(start_char_idx)
        .map(|(i, _)| i)
        .unwrap_or(content.len());
    let byte_end = content
        .char_indices()
        .nth(end_char_idx)
        .map(|(i, _)| i)
        .unwrap_or(content.len());

    let mut result =
        String::with_capacity(content.len() - (byte_end - byte_start) + new_text.len());
    result.push_str(&content[..byte_start]);
    result.push_str(new_text);
    result.push_str(&content[byte_end..]);
    Some(result)
}

/// Try matching where each line is trimmed of whitespace.
/// Returns the full file content with the matched window replaced by `new_text`.
pub(crate) fn try_trimmed_match(content: &str, old_text: &str, new_text: &str) -> Option<String> {
    let content_lines: Vec<&str> = content.lines().collect();
    let old_lines: Vec<&str> = old_text.lines().collect();
    if old_lines.is_empty() || old_lines.len() > content_lines.len() {
        return None;
    }
    for (i, window) in content_lines.windows(old_lines.len()).enumerate() {
        if window
            .iter()
            .zip(old_lines.iter())
            .all(|(file_line, old_line)| file_line.trim() == old_line.trim())
        {
            // Found matching window — reconstruct the full file with replacement
            let line_ending = detect_line_ending(content);
            let normalized_new = normalize_to_lf(new_text);
            let new_lines: Vec<&str> = normalized_new.lines().collect();

            let mut result_lines =
                Vec::with_capacity(content_lines.len() - old_lines.len() + new_lines.len());
            // Lines before the match
            result_lines.extend_from_slice(&content_lines[..i]);
            // Replacement lines
            result_lines.extend_from_slice(&new_lines);
            // Lines after the match
            let after = i + old_lines.len();
            result_lines.extend_from_slice(&content_lines[after..]);

            let mut joined = result_lines.join(line_ending.as_str());
            // Preserve trailing newline if original had one
            if content.ends_with('\n') || content.ends_with("\r\n") {
                joined.push_str(line_ending.as_str());
            }
            return Some(joined);
        }
    }
    None
}

pub struct EditFile;

impl Tool for EditFile {
    fn name(&self) -> &'static str {
        "edit_file"
    }

    fn description(&self) -> &'static str {
        "Replace text in a file. Tries exact match first, then line-ending-normalized match (handles CRLF/LF differences), then quote-normalized match (handles curly/smart quotes), then trimmed-whitespace match. Preserves original line endings. Returns a diff of changes."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path relative to workspace root (alias: file_path)"
                },
                "file_path": {
                    "type": "string",
                    "description": "Alias for path"
                },
                "old_text": {
                    "type": "string",
                    "description": "Text to find (alias: old_string). Matching is flexible: tries exact, then line-ending-normalized (CRLF/LF), then quote-normalized (curly/smart quotes), then trimmed-whitespace."
                },
                "old_string": {
                    "type": "string",
                    "description": "Alias for old_text"
                },
                "new_text": {
                    "type": "string",
                    "description": "Replacement text (alias: new_string). Original file line endings (CRLF/LF) are preserved."
                },
                "new_string": {
                    "type": "string",
                    "description": "Alias for new_text"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences of old_text (default false). Only applies to exact match strategy."
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    fn tags(&self) -> &[ToolTag] {
        &[ToolTag::Implement]
    }

    fn validate_input(&self, params: &serde_json::Value, ctx: &ToolContext) -> Result<()> {
        let input: EditFileInput = serde_json::from_value(params.clone())
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {e}"))?;
        let path_str = input
            .path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path: contains non-UTF-8 characters"))?;
        let path = if std::path::Path::new(path_str).is_absolute() {
            std::path::PathBuf::from(path_str)
        } else {
            ctx.cwd.join(path_str)
        };
        if let Some(state) = &ctx.file_read_state {
            let canonical = path.canonicalize().ok();
            let current_mtime = canonical
                .as_ref()
                .and_then(|p| fs::metadata(p).ok())
                .and_then(|m| m.modified().ok());
            let check_path = canonical.as_ref().unwrap_or(&path);
            if let Err(reason) = state.check_stale(check_path, current_mtime) {
                return Err(anyhow::anyhow!("{reason}"));
            }
        }
        Ok(())
    }

    fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }
        let input: EditFileInput = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {e}"))?;

        // Validate path and check size limits
        let path_str = input
            .path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path: contains non-UTF-8 characters"))?;

        // Validate the path is within workspace and safe
        let validated_path = validate_write_path(
            path_str,
            &ctx.cwd,
            input.new_text.len(),
            !ctx.allow_outside_workspace,
        )
        .map_err(|e| {
            let mut msg = format!("{e}");
            if msg.contains("not found") || msg.contains("No such file") {
                let suggestions = suggest_similar_files(&input.path, &ctx.cwd);
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
            anyhow::anyhow!("{msg}")
        })?;

        // Read the current file content using symlink-safe operation
        let mut file = open_file_symlink_safe(&validated_path).map_err(|e| {
            let msg = format!("{e}");
            if msg.contains("not found") || msg.contains("No such file") {
                let suggestions = suggest_similar_files(&input.path, &ctx.cwd);
                let hint = crate::file_suggest::format_suggestions(&suggestions);
                if !hint.is_empty() {
                    return anyhow::anyhow!("File not found: {}{hint}", input.path.display());
                }
            }
            anyhow::anyhow!("Failed to open file: {e}")
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
        file.read_to_string(&mut content).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                anyhow::anyhow!(
                    "Binary or non-UTF-8 file detected; edit_file only supports text files"
                )
            } else {
                anyhow::anyhow!("Failed to read file: {e}")
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
        if input.old_text.is_empty() {
            return Err(anyhow::anyhow!(
                "old_text cannot be empty. Provide the text to search for and replace."
            ));
        }

        // Try matching strategies in order: exact → line-ending-normalized → quote-normalized → trimmed
        let new_content = if let Some((start, end)) = try_exact_match(&content, &input.old_text) {
            // Strategy 1: Exact match
            if input.replace_all {
                content.replace(&input.old_text, &input.new_text)
            } else {
                // Check for non-unique match — replacing only one of many is error-prone
                let match_count = content.matches(&input.old_text).count();
                if match_count > 1 {
                    return Ok(ToolOutput::text(format!(
                        "Found {match_count} matches of the old_text in the file, but replace_all is not set. \
                         To replace all occurrences, set replace_all to true. \
                         To replace only one occurrence, provide more surrounding context to uniquely identify the instance.\n\n\
                         Searched for:\n{}",
                        input.old_text.lines().take(CONTEXT_LINES_ON_FAILURE).collect::<Vec<_>>().join("\n")
                    )));
                }
                let mut result =
                    String::with_capacity(content.len() - (end - start) + input.new_text.len());
                result.push_str(&content[..start]);
                result.push_str(&input.new_text);
                result.push_str(&content[end..]);
                result
            }
        } else if let Some(replacement) =
            try_normalized_match(&content, &input.old_text, &input.new_text)
        {
            // Strategy 2: Line-ending-normalized match
            replacement
        } else if let Some(replacement) =
            try_quote_normalized_match(&content, &input.old_text, &input.new_text)
        {
            // Strategy 3: Quote-normalized match (curly → straight quotes)
            replacement
        } else if let Some(replacement) =
            try_trimmed_match(&content, &input.old_text, &input.new_text)
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
            let old_preview: String = input
                .old_text
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
        let path_display = input.path.display().to_string();
        let diff = generate_diff(&content, &new_content, &path_display, 30);

        let mut output = format!("Edited {path_display}:\n{diff}");

        if let Some(formatter_diff) = file_formatter::format_file(&validated_path, &ctx.cwd) {
            output.push_str(&formatter_diff);
        }

        if let Some(state) = &ctx.file_read_state {
            state.invalidate(&validated_path);
        }

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_edit_file_valid_operation() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello world").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "world",
            "new_text": "rust"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hello rust");
    }

    #[test]
    fn test_edit_file_blocks_path_traversal() {
        let workspace = tempdir().unwrap();
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "../../../etc/passwd",
            "old_text": "root",
            "new_text": "hacked"
        });

        let tool = EditFile;
        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_edit_file_respects_size_limits() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "small").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let huge_content = "x".repeat(20 * 1024 * 1024);

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "small",
            "new_text": huge_content
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("limit"));
    }

    #[test]
    fn edit_file_tool_name() {
        assert_eq!(EditFile.name(), "edit_file");
    }

    #[test]
    fn edit_file_tool_permission() {
        assert_eq!(EditFile.permission(), ToolPermission::Write);
    }

    #[test]
    fn edit_file_schema_has_required_fields() {
        let schema = EditFile.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "path"));
        assert!(required.iter().any(|r| r == "old_text"));
        assert!(required.iter().any(|r| r == "new_text"));
    }

    #[test]
    fn edit_file_input_serde_roundtrip() {
        let input = EditFileInput {
            path: PathBuf::from("src/main.rs"),
            old_text: "fn main".into(),
            new_text: "fn main()".into(),
            replace_all: false,
        };
        let json = serde_json::to_string(&input).unwrap();
        let decoded: EditFileInput = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.path, PathBuf::from("src/main.rs"));
        assert_eq!(decoded.old_text, "fn main");
    }

    #[test]
    fn edit_file_old_text_not_found() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello world").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "does_not_exist",
            "new_text": "replacement"
        });

        let result = tool.execute(params, &ctx).unwrap();
        assert!(result.text.contains("not found"));
    }

    #[test]
    fn edit_file_invalid_params() {
        let workspace = tempdir().unwrap();
        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": 123
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn edit_file_rejects_binary_content() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.bin");
        std::fs::write(&test_file, [0xff, 0xfe, 0xfd]).unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.bin",
            "old_text": "a",
            "new_text": "b"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-UTF-8 file"));
    }

    #[test]
    fn edit_file_shows_diff_output() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello world\nfoo bar\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "world\nfoo",
            "new_text": "rust\nbaz"
        });

        let result = tool.execute(params, &ctx).unwrap();
        assert!(result.text.contains("Changes in test.txt"));
        assert!(result.text.contains("+2 -2"));
        assert!(result.text.contains("+hello rust"));
        assert!(result.text.contains("-hello world"));
    }

    #[test]
    fn edit_file_line_ending_normalized_match() {
        // File has CRLF, search text has LF — should still match
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello\r\nworld\r\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "hello\nworld",
            "new_text": "hello\nrust"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn edit_file_trimmed_match_uses_new_text() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "fn main() {\nprintln!(\"hi\");\n}",
            "new_text": "fn main() {\nprintln!(\"bye\");\n}"
        });

        let result = tool.execute(params, &ctx).unwrap();
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("bye"));
        assert!(!content.contains("hi"));
        assert!(result.text.contains("Edited test.txt"));
    }

    #[test]
    fn edit_file_trimmed_match_preserves_replacement_indentation() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "if true {\n    old_call();\n}\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "if true {\nold_call();\n}",
            "new_text": "if true {\n        new_call();\n    nested();\n}"
        });

        let result = tool.execute(params, &ctx).unwrap();
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("        new_call();"));
        assert!(content.contains("    nested();"));
        assert!(result.text.contains("Edited test.txt"));
    }

    #[test]
    fn edit_file_not_found_shows_context() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "line one\nline two\nline three").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "not here",
            "new_text": "replacement"
        });

        let result = tool.execute(params, &ctx).unwrap();
        assert!(result.text.contains("File content"));
        assert!(result.text.contains("line one"));
        assert!(result.text.contains("Searched for"));
    }

    #[test]
    fn edit_file_rejects_empty_old_text() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello world").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "",
            "new_text": "injected"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn edit_file_trimmed_match_preserves_surrounding_content() {
        // Bug regression test: trimmed match must NOT discard lines around the match
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(
            &test_file,
            "line one\nline two\n    line three\nline four\nline five\n",
        )
        .unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "line two\nline three\nline four",
            "new_text": "replaced two\nreplaced three\nreplaced four"
        });

        let _ = tool.execute(params, &ctx).unwrap();
        let content = std::fs::read_to_string(&test_file).unwrap();
        // Must preserve lines before and after the match
        assert!(
            content.contains("line one"),
            "should preserve line before match"
        );
        assert!(
            content.contains("line five"),
            "should preserve line after match"
        );
        assert!(content.contains("replaced two"));
    }

    #[test]
    fn edit_file_exact_match_multiline() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "aaa\nbbb\nccc\nddd\neee\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "bbb\nccc\nddd",
            "new_text": "BBB\nCCC\nDDD"
        });

        let _ = tool.execute(params, &ctx).unwrap();
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "aaa\nBBB\nCCC\nDDD\neee\n");
    }

    #[test]
    fn edit_file_normalized_match_preserves_crlf() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "alpha\r\nbeta\r\ngamma\r\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "alpha\nbeta",
            "new_text": "ALPHA\nBETA"
        });

        let result = tool.execute(params, &ctx).unwrap();
        assert!(result.text.contains("Edited test.txt"));
        let content = std::fs::read_to_string(&test_file).unwrap();
        // CRLF should be preserved
        assert!(
            content.contains("ALPHA\r\nBETA"),
            "CRLF should be preserved in output"
        );
        assert!(
            content.contains("gamma\r\n"),
            "unmatched line should keep CRLF"
        );
    }

    #[test]
    fn edit_file_exact_match_single_line_no_newline() {
        // File with no trailing newline should remain without one after exact match
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello world").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "world",
            "new_text": "rust"
        });

        let _ = tool.execute(params, &ctx).unwrap();
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hello rust");
        assert!(!content.ends_with('\n'), "should not add trailing newline");
    }

    /// Integration test: read normalizes CRLF→LF, edit handles CRLF files correctly
    #[test]
    fn edit_file_after_read_normalization() {
        // Simulate what happens in production: read_file gives the LLM LF-normalized
        // content, the LLM sends old_text with LF, but the actual file has CRLF.
        // edit_file's normalized match strategy handles this.
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "line one\r\nline two\r\nline three\r\n").unwrap();

        // LLM sees (from read_file): "line one\nline two\nline three"
        // LLM sends old_text with LF, edit_file should handle CRLF
        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "line two",
            "new_text": "LINE TWO"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&test_file).unwrap();
        // CRLF should be preserved in unchanged lines
        assert!(
            content.contains("line one\r\n"),
            "CRLF preserved before edit"
        );
        assert!(content.contains("LINE TWO"), "replacement applied");
        assert!(
            content.contains("line three\r\n"),
            "CRLF preserved after edit"
        );
    }

    /// Integration test: trimmed match works for indented code
    #[test]
    fn edit_file_trimmed_match_for_indented_code() {
        // LLM often normalizes indentation. Trimmed match handles this.
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.rs");
        std::fs::write(
            &test_file,
            "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n",
        )
        .unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        // LLM sends without indentation
        let params = serde_json::json!({
            "path": "test.rs",
            "old_text": "let x = 1;\nprintln!(\"{}\", x);",
            "new_text": "let x = 2;\nprintln!(\"{}\", x);"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("let x = 2;"), "replacement applied");
        assert!(content.contains("fn main()"), "surrounding code preserved");
        assert!(content.contains("}"), "closing brace preserved");
    }

    /// Integration test: non-unique match without replace_all returns warning
    #[test]
    fn edit_file_non_unique_match_returns_warning() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "aaa\nbbb\naaa\nccc\naaa\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "aaa",
            "new_text": "XXX"
        });

        let result = tool.execute(params, &ctx).unwrap();
        // Should warn about multiple matches instead of silently replacing
        assert!(
            result.text.contains("Found 3 matches"),
            "should warn about multiple matches: {}",
            result.text
        );
        // File should be unchanged
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(
            content, "aaa\nbbb\naaa\nccc\naaa\n",
            "file unchanged after non-unique match"
        );
    }

    /// Regression test: trimmed match must preserve trailing newlines
    #[test]
    fn edit_file_trimmed_match_preserves_trailing_newline() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "if true {\n    old();\n}\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "if true {\nold();\n}",
            "new_text": "if true {\nnew();\n}"
        });

        let _ = tool.execute(params, &ctx).unwrap();
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(
            content.ends_with('\n'),
            "trailing newline must be preserved, got: {:?}",
            content
        );
        assert!(content.contains("new();"));
    }

    /// Regression test: trimmed match must preserve CRLF trailing newlines
    #[test]
    fn edit_file_trimmed_match_preserves_crlf_trailing_newline() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "if true {\r\n    old();\r\n}\r\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "if true {\nold();\n}",
            "new_text": "if true {\nnew();\n}"
        });

        let _ = tool.execute(params, &ctx).unwrap();
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(
            content.ends_with("\r\n"),
            "CRLF trailing newline must be preserved, got: {:?}",
            content
        );
        assert!(content.contains("new();"));
    }

    // ── Unit tests for matching functions ──

    #[test]
    fn try_exact_match_finds_substring() {
        let content = "hello world";
        assert_eq!(try_exact_match(content, "world"), Some((6, 11)));
        assert_eq!(try_exact_match(content, "missing"), None);
    }

    #[test]
    fn try_exact_match_empty_old_text() {
        // Empty old_text should match at position 0
        assert_eq!(try_exact_match("content", ""), Some((0, 0)));
    }

    #[test]
    fn try_normalized_match_crlf_to_lf() {
        let content = "line1\r\nline2\r\nline3\r\n";
        let result = try_normalized_match(content, "line1\nline2", "foo\nbar");
        assert!(result.is_some());
        let output = result.unwrap();
        // Should preserve CRLF line endings
        assert!(output.contains("\r\n"));
        assert!(output.contains("foo"));
        assert!(output.contains("bar"));
    }

    #[test]
    fn try_normalized_match_no_match_returns_none() {
        let content = "line1\r\nline2\r\n";
        assert_eq!(
            try_normalized_match(content, "missing", "replacement"),
            None
        );
    }

    #[test]
    fn try_trimmed_match_ignores_whitespace() {
        let content = "  fn main()  \n    println!(\"hi\");  \n  }  \n";
        let result = try_trimmed_match(
            content,
            "fn main()\nprintln!(\"hi\");\n}",
            "fn main()\nprintln!(\"bye\");\n}",
        );
        assert!(result.is_some());
        assert!(result.unwrap().contains("bye"));
    }

    #[test]
    fn try_trimmed_match_no_match_returns_none() {
        let content = "hello\nworld\n";
        assert_eq!(try_trimmed_match(content, "foo\nbar", "baz"), None);
    }

    #[test]
    fn try_trimmed_match_empty_old_returns_none() {
        let content = "hello\n";
        assert_eq!(try_trimmed_match(content, "", "replacement"), None);
    }

    // ── Quote normalization tests ──

    #[test]
    fn try_quote_normalized_match_curly_double_quotes() {
        let content = "let msg = \u{201C}hello world\u{201D};\n";
        let result =
            try_quote_normalized_match(content, "let msg = \"hello world\";", "let msg = \"bye\";");
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(
            output.contains("\"bye\""),
            "should replace content portion, got: {output:?}"
        );
        assert!(
            output.ends_with(";\n"),
            "should preserve suffix after match"
        );
    }

    #[test]
    fn try_quote_normalized_match_curly_single_quotes() {
        let content = "it\u{2019}s working\n";
        let result = try_quote_normalized_match(content, "it's working", "it's broken");
        assert!(result.is_some());
        assert!(result.unwrap().contains("broken"));
    }

    #[test]
    fn try_quote_normalized_match_no_match_returns_none() {
        let content = "hello world\n";
        assert_eq!(
            try_quote_normalized_match(content, "missing", "replacement"),
            None
        );
    }

    #[test]
    fn edit_file_curly_quotes_in_old_text() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        // File has curly quotes
        std::fs::write(&test_file, "msg = \u{201C}hello\u{201D}\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        // LLM sends straight quotes in old_text
        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "msg = \"hello\"",
            "new_text": "msg = \"world\""
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("world"), "replacement should be applied");
    }

    #[test]
    fn edit_file_curly_quotes_in_file_and_old_text() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        // Both file and old_text have curly quotes — exact match should handle it
        std::fs::write(&test_file, "msg = \u{201C}hello\u{201D}\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "msg = \u{201C}hello\u{201D}",
            "new_text": "msg = \u{201C}world\u{201D}"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(
            content.contains("\u{201C}world\u{201D}"),
            "exact match with curly quotes"
        );
    }

    #[test]
    fn edit_file_accepts_catalog_field_names() {
        // The LLM tool schema uses file_path/old_string/new_string
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello world\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "file_path": "test.txt",
            "old_string": "hello",
            "new_string": "goodbye"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("goodbye"));
    }

    #[test]
    fn edit_file_replace_all() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "aaa\nbbb\naaa\nccc\naaa\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "aaa",
            "new_text": "XXX",
            "replace_all": true
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(
            content, "XXX\nbbb\nXXX\nccc\nXXX\n",
            "all occurrences replaced"
        );
    }

    #[test]
    fn edit_file_replace_all_false_warns_on_multiple_matches() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "aaa\nbbb\naaa\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "aaa",
            "new_text": "XXX",
            "replace_all": false
        });

        let result = tool.execute(params, &ctx).unwrap();
        assert!(
            result.text.contains("Found 2 matches"),
            "should warn about multiple matches: {}",
            result.text
        );
        // File should be unchanged
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(
            content, "aaa\nbbb\naaa\n",
            "file unchanged without replace_all"
        );
    }

    #[test]
    fn edit_file_unique_match_replaces_without_replace_all() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "aaa\nbbb\nccc\n").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "old_text": "bbb",
            "new_text": "XXX",
            "replace_all": false
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "aaa\nXXX\nccc\n", "unique match replaced");
    }

    #[test]
    fn edit_file_not_found_suggests_similar() {
        let workspace = tempdir().unwrap();
        // Create a file with a similar name
        std::fs::write(workspace.path().join("main.rs"), "fn main() {}").unwrap();

        let tool = EditFile;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "man.rs",
            "old_text": "fn main",
            "new_text": "fn foo"
        });

        let result = tool.execute(params, &ctx);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("File not found"), "got: {err}");
        // Should suggest main.rs since it has a similar stem
        assert!(err.contains("Did you mean"), "got: {err}");
        assert!(
            err.contains("main.rs"),
            "should suggest main.rs, got: {err}"
        );
    }

    #[test]
    fn suggest_similar_finds_matching_stem() {
        let workspace = tempdir().unwrap();
        std::fs::write(workspace.path().join("config.toml"), "").unwrap();
        std::fs::write(workspace.path().join("config.yaml"), "").unwrap();
        std::fs::write(workspace.path().join("other.txt"), "").unwrap();

        let target = std::path::Path::new("config.json");
        let suggestions = suggest_similar_files(target, workspace.path());
        assert!(!suggestions.is_empty(), "should find similar files");
        assert!(
            suggestions.iter().any(|s| s.contains("config")),
            "should suggest config files, got: {suggestions:?}"
        );
    }

    #[test]
    fn suggest_similar_empty_for_no_matches() {
        let workspace = tempdir().unwrap();
        let target = std::path::Path::new("completely_unique_name.xyz");
        let suggestions = suggest_similar_files(target, workspace.path());
        assert!(suggestions.is_empty(), "should find no matches");
    }
}
