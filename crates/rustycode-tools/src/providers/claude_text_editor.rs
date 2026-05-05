//! Claude Text Editor Tool
//!
//! Implements Claude's native text editor tool specification:
//! <https://platform.claude.com/docs/en/agents-and-tools/tool-use/text-editor-tool>
//!
//! This is a schema-less tool - Anthropic provides the schema to Claude models.
//!
//! Tool type: `text_editor_20250728` for Claude 4, `text_editor_20250124` for Sonnet 3.7
//!
//! Commands:
//! - view: Read file contents or list directory
//! - `str_replace`: Replace exact string match in file
//! - create: Create new file with content
//! - insert: Insert text at specific line number
//! - `undo_edit`: Revert last edit (Sonnet 3.7 only)

use crate::file_formatter;
use crate::providers::edit::{
    try_exact_match, try_normalized_match, try_quote_normalized_match, try_trimmed_match,
};
use crate::security::{
    create_file_exclusive, create_file_symlink_safe, open_file_symlink_safe, validate_read_path,
    validate_write_path,
};
use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum file size for text editor operations (1MB)
const MAX_FILE_SIZE: usize = 1_048_576;

/// Maximum line count for view operations (prevents overwhelming output)
const MAX_VIEW_LINES: usize = 10_000;

/// Number of backups to keep per file
const MAX_BACKUPS: usize = 5;

/// Backup directory name
const BACKUP_DIR: &str = ".text_editor_backups";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command")]
enum TextEditorCommand {
    #[serde(rename = "view")]
    View {
        path: String,
        /// Optional line range for view (1-indexed, inclusive)
        #[serde(flatten)]
        range: Option<ViewRange>,
    },
    #[serde(rename = "str_replace")]
    StrReplace {
        path: String,
        old_str: String,
        new_str: String,
    },
    #[serde(rename = "create")]
    Create { path: String, content: String },
    #[serde(rename = "insert")]
    Insert {
        path: String,
        line: usize,
        content: String,
    },
    #[serde(rename = "undo_edit")]
    UndoEdit { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ViewRange {
    start_line: Option<usize>,
    end_line: Option<usize>,
}

/// Backup manager for undo operations
struct BackupManager {
    backup_dir: PathBuf,
}

impl BackupManager {
    fn new(cwd: &Path) -> Self {
        let backup_dir = cwd.join(BACKUP_DIR);
        // Create backup directory if it doesn't exist
        let _ = fs::create_dir_all(&backup_dir);
        Self { backup_dir }
    }

    /// Create a backup of the file before editing
    fn backup(&self, path: &Path) -> Result<PathBuf> {
        let filename = path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid file path"))?
            .to_string_lossy()
            .to_string();

        // Create timestamped backup
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let backup_name = format!("{filename}.{timestamp}.backup");
        let backup_path = self.backup_dir.join(&backup_name);

        // Copy file to backup location
        fs::copy(path, &backup_path).map_err(|e| anyhow!("Failed to create backup: {e}"))?;

        // Clean up old backups (keep only MAX_BACKUPS most recent)
        self.cleanup_old_backups(&filename);

        Ok(backup_path)
    }

    /// Restore the most recent backup for a file
    fn restore(&self, path: &Path) -> Result<()> {
        let filename = path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid file path"))?
            .to_string_lossy()
            .to_string();

        // Find most recent backup
        let most_recent = self
            .list_backups(&filename)?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No backups found for {filename}"))?;

        // Restore from backup using symlink-safe I/O to prevent TOCTOU
        let mut src = open_file_symlink_safe(&most_recent)
            .map_err(|e| anyhow!("Failed to open backup: {e}"))?;
        let mut content = Vec::new();
        use std::io::Read;
        src.read_to_end(&mut content)
            .map_err(|e| anyhow!("Failed to read backup: {e}"))?;

        let mut dst = create_file_symlink_safe(path)
            .map_err(|e| anyhow!("Failed to open file for restore: {e}"))?;
        use std::io::Write;
        dst.write_all(&content)
            .map_err(|e| anyhow!("Failed to write restored content: {e}"))?;
        dst.sync_all()
            .map_err(|e| anyhow!("Failed to sync restored file: {e}"))?;

        Ok(())
    }

    /// List all backups for a file, sorted by timestamp (newest first)
    fn list_backups(&self, filename: &str) -> Result<Vec<PathBuf>> {
        // Backup format: {filename}.{timestamp}.backup
        // Must match exactly: filename + "." + digits + ".backup"
        let expected_prefix = format!("{filename}.");
        let suffix = ".backup";
        let mut backups: Vec<_> = fs::read_dir(&self.backup_dir)?
            .filter_map(std::result::Result::ok)
            .filter(|entry| {
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();
                if !name.starts_with(&expected_prefix) || !name.ends_with(suffix) {
                    return false;
                }
                // Extract the middle part (should be just the timestamp digits)
                let middle = &name[expected_prefix.len()..name.len() - suffix.len()];
                middle.chars().all(|c| c.is_ascii_digit())
            })
            .map(|entry| entry.path())
            .collect();

        // Sort by modification time (newest first)
        backups.sort_by_key(|path| {
            fs::metadata(path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        });
        backups.reverse();

        Ok(backups)
    }

    /// Clean up old backups, keeping only `MAX_BACKUPS` most recent
    fn cleanup_old_backups(&self, filename: &str) {
        if let Ok(backups) = self.list_backups(filename) {
            for old_backup in backups.iter().skip(MAX_BACKUPS) {
                let _ = fs::remove_file(old_backup);
            }
        }
    }
}

pub struct ClaudeTextEditor;

impl Tool for ClaudeTextEditor {
    fn name(&self) -> &'static str {
        // Use the latest tool type for Claude 4
        "text_editor_20250728"
    }

    fn description(&self) -> &'static str {
        "Claude's native text editor tool - a unified interface for file operations including view, create, edit, and undo"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        // This is a schema-less tool - Anthropic provides the schema to Claude
        // We return an empty object to indicate no schema validation on our side
        serde_json::json!({
            "type": "object",
            "description": "Schema-less tool - Anthropic provides schema to Claude models. See: https://platform.claude.com/docs/en/agents-and-tools/tool-use/text-editor-tool",
            "properties": {},
            "additionalProperties": true
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Parse command from parameters
        let command: TextEditorCommand = serde_json::from_value(params)
            .map_err(|e| anyhow!("Invalid text editor command: {e}"))?;

        // Initialize backup manager
        let backup_manager = BackupManager::new(&ctx.cwd);

        match command {
            TextEditorCommand::View { path, range } => self.execute_view(&path, range, ctx),
            TextEditorCommand::StrReplace {
                path,
                old_str,
                new_str,
            } => self.execute_str_replace(&path, &old_str, &new_str, &backup_manager, ctx),
            TextEditorCommand::Create { path, content } => {
                self.execute_create(&path, &content, ctx)
            }
            TextEditorCommand::Insert {
                path,
                line,
                content,
            } => self.execute_insert(&path, line, &content, &backup_manager, ctx),
            TextEditorCommand::UndoEdit { path } => self.execute_undo(&path, &backup_manager, ctx),
        }
    }
}

impl ClaudeTextEditor {
    /// View file contents or list directory
    fn execute_view(
        &self,
        path_str: &str,
        range: Option<ViewRange>,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        // Validate and resolve path
        let path = validate_read_path(path_str, &ctx.cwd, !ctx.allow_outside_workspace)?;

        // Check if it's a directory or file
        if path.is_dir() {
            // List directory contents
            self.list_directory(&path, ctx)
        } else {
            // Read file contents
            self.view_file(&path, range, ctx)
        }
    }

    /// List directory contents
    fn list_directory(&self, path: &Path, _ctx: &ToolContext) -> Result<ToolOutput> {
        let mut entries = Vec::new();

        for entry in fs::read_dir(path)?
            .filter_map(std::result::Result::ok)
            .take(200)
        // Limit to prevent overwhelming output
        {
            let file_type = entry.file_type()?;
            let name = entry.file_name().to_string_lossy().to_string();

            let kind = if file_type.is_dir() {
                "DIR"
            } else if file_type.is_file() {
                "FILE"
            } else {
                "OTHER"
            };

            entries.push(format!("{name} [{kind}]"));
        }

        entries.sort();

        let output = if entries.is_empty() {
            format!("{} (empty directory)", path.display())
        } else {
            format!("**{}**\n\n{}", path.display(), entries.join("\n"))
        };

        Ok(ToolOutput::with_structured(
            output,
            serde_json::json!({
                "path": path.display().to_string(),
                "type": "directory",
                "entry_count": entries.len(),
            }),
        ))
    }

    /// View file contents
    fn view_file(
        &self,
        path: &Path,
        range: Option<ViewRange>,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        // Check file size
        let metadata = fs::metadata(path)?;
        let file_size = metadata.len() as usize;

        if file_size > MAX_FILE_SIZE {
            return Ok(ToolOutput::text(format!(
                "[Error] File too large for text editor ({file_size} bytes, max {MAX_FILE_SIZE} bytes)"
            )));
        }

        // Use symlink-safe file open to prevent TOCTOU attacks
        let mut file = open_file_symlink_safe(path)?;
        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content)?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // Apply line range if specified
        let (start_idx, end_idx) = if let Some(ViewRange {
            start_line,
            end_line,
        }) = range
        {
            let start = start_line.unwrap_or(1).saturating_sub(1);
            let end = end_line.unwrap_or(total_lines).min(total_lines);
            (start.min(total_lines), end.max(start).min(total_lines))
        } else {
            // Apply truncation if no range specified
            let start = 0;
            let end = total_lines.min(MAX_VIEW_LINES);
            (start, end)
        };

        let selected_lines = &lines[start_idx..end_idx];
        let truncated = total_lines > end_idx - start_idx;

        let output_text = selected_lines.join("\n");

        // Build result with metadata
        let metadata = serde_json::json!({
            "path": path.display().to_string(),
            "type": "file",
            "size": file_size,
            "total_lines": total_lines,
            "shown_lines": selected_lines.len(),
            "truncated": truncated,
            "line_range": {
                "start": start_idx + 1,
                "end": end_idx
            }
        });

        // Add truncation notice if applicable
        let final_output = if truncated {
            format!(
                "{}\n\n[Showing lines {}-{} of {}]",
                output_text,
                start_idx + 1,
                end_idx,
                total_lines
            )
        } else {
            output_text
        };

        Ok(ToolOutput::with_structured(final_output, metadata))
    }

    /// String replace operation
    fn execute_str_replace(
        &self,
        path_str: &str,
        old_str: &str,
        new_str: &str,
        backup_manager: &BackupManager,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        // Validate old_str is not empty
        if old_str.is_empty() {
            return Ok(ToolOutput::text(
                "[Error] old_str cannot be empty for str_replace",
            ));
        }

        // Validate and resolve path
        let path = validate_write_path(
            path_str,
            &ctx.cwd,
            new_str.len(),
            !ctx.allow_outside_workspace,
        )?;

        // Create backup before editing
        backup_manager.backup(&path)?;

        // Use symlink-safe file open to prevent TOCTOU attacks
        let mut file = open_file_symlink_safe(&path)?;
        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content)?;

        // Try matching strategies in order: exact → line-ending-normalized → quote-normalized → trimmed
        let (new_content, count) = if try_exact_match(&content, old_str).is_some() {
            // Exact match — count all occurrences and replace all (preserving existing behavior)
            let count = content.matches(old_str).count();
            let new_content = content.replace(old_str, new_str);
            (new_content, count)
        } else if let Some(replacement) = try_normalized_match(&content, old_str, new_str) {
            (replacement, 1)
        } else if let Some(replacement) = try_quote_normalized_match(&content, old_str, new_str) {
            (replacement, 1)
        } else if let Some(replacement) = try_trimmed_match(&content, old_str, new_str) {
            (replacement, 1)
        } else {
            return Ok(ToolOutput::text(format!(
                "[Error] old_str not found in file (tried exact, line-ending-normalized, quote-normalized, and trimmed matching):\n\"{old_str}\""
            )));
        };

        if new_content.len() > MAX_FILE_SIZE {
            return Ok(ToolOutput::text(format!(
                "[Error] Replacement would exceed {MAX_FILE_SIZE} byte limit for text editor"
            )));
        }

        // Write back using symlink-safe operation
        let mut out_file = create_file_symlink_safe(&path)?;
        use std::io::Write;
        out_file.write_all(new_content.as_bytes())?;
        out_file.sync_all()?;

        let mut output = format!(
            "Successfully replaced {} occurrence(s) in {}",
            count,
            path.display()
        );

        if let Some(formatter_diff) = file_formatter::format_file(&path, &ctx.cwd) {
            output.push_str(&formatter_diff);
        }

        Ok(ToolOutput::with_structured(
            output,
            serde_json::json!({
                "path": path.display().to_string(),
                "replacements": count,
                "old_length": old_str.len(),
                "new_length": new_str.len(),
            }),
        ))
    }

    /// Create new file
    fn execute_create(
        &self,
        path_str: &str,
        content: &str,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        // Validate and resolve path
        let path = validate_write_path(
            path_str,
            &ctx.cwd,
            content.len(),
            !ctx.allow_outside_workspace,
        )?;

        // Create parent directories if needed (atomic - no TOCTOU)
        // fs::create_dir_all is idempotent - handles AlreadyExists gracefully
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create file exclusively - fails atomically if file already exists (no TOCTOU)
        // Uses O_CREAT | O_EXCL for atomic "create only if not exists"
        let mut file = create_file_exclusive(&path)?;
        use std::io::Write;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;

        let line_count = content.lines().count();

        let mut output = format!(
            "Created {} ({} bytes, {} lines)",
            path.display(),
            content.len(),
            line_count
        );

        if let Some(formatter_diff) = file_formatter::format_file(&path, &ctx.cwd) {
            output.push_str(&formatter_diff);
        }

        Ok(ToolOutput::with_structured(
            output,
            serde_json::json!({
                "path": path.display().to_string(),
                "bytes": content.len(),
                "lines": line_count,
            }),
        ))
    }

    /// Insert content at specific line number
    fn execute_insert(
        &self,
        path_str: &str,
        line_num: usize,
        content: &str,
        backup_manager: &BackupManager,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        // Validate line number (1-indexed)
        if line_num == 0 {
            return Ok(ToolOutput::text("[Error] Line numbers start at 1, not 0"));
        }

        // Validate and resolve path
        let path = validate_write_path(
            path_str,
            &ctx.cwd,
            content.len(),
            !ctx.allow_outside_workspace,
        )?;

        // Create backup before editing
        backup_manager.backup(&path)?;

        // Use symlink-safe file open to prevent TOCTOU attacks
        let mut file = open_file_symlink_safe(&path)?;
        let mut old_content = String::new();
        use std::io::Read;
        file.read_to_string(&mut old_content)?;

        // Detect original line ending before splitting
        let line_ending = if old_content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };

        let mut lines: Vec<&str> = old_content.lines().collect();
        let total_lines = lines.len();

        // Validate line number
        if line_num > total_lines + 1 {
            return Ok(ToolOutput::text(format!(
                "[Error] File has {total_lines} lines, cannot insert at line {line_num}"
            )));
        }

        // Insert content at specified line (convert to 0-indexed).
        // Multiline inserts must expand into multiple logical lines rather than
        // being treated as a single line payload.
        let insert_idx = line_num.saturating_sub(1);
        let insert_lines: Vec<&str> = content.lines().collect();
        if insert_lines.is_empty() {
            return Ok(ToolOutput::text(
                "[Error] content cannot be empty for insert",
            ));
        }
        lines.splice(insert_idx..insert_idx, insert_lines.iter().copied());

        // Write back using symlink-safe operation, preserving original line endings
        let mut new_content = lines.join(line_ending);
        // Preserve trailing newline if original had one
        if old_content.ends_with('\n') || old_content.ends_with("\r\n") {
            new_content.push_str(line_ending);
        }
        let mut file = create_file_symlink_safe(&path)?;
        use std::io::Write;
        file.write_all(new_content.as_bytes())?;
        file.sync_all()?;

        let mut output = format!(
            "Inserted {} line(s) at line {} in {}",
            insert_lines.len(),
            line_num,
            path.display()
        );

        if let Some(formatter_diff) = file_formatter::format_file(&path, &ctx.cwd) {
            output.push_str(&formatter_diff);
        }

        Ok(ToolOutput::with_structured(
            output,
            serde_json::json!({
                "path": path.display().to_string(),
                "insert_line": line_num,
                "inserted_lines": insert_lines.len(),
                "total_lines": total_lines + insert_lines.len(),
            }),
        ))
    }

    /// Undo last edit
    fn execute_undo(
        &self,
        path_str: &str,
        backup_manager: &BackupManager,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        // Validate and resolve path
        let path = validate_write_path(path_str, &ctx.cwd, 0, !ctx.allow_outside_workspace)?;

        // Check file exists
        if !path.exists() {
            return Ok(ToolOutput::text(format!(
                "[Error] File not found: {}",
                path.display()
            )));
        }

        // Restore from backup
        backup_manager.restore(&path)?;

        Ok(ToolOutput::with_structured(
            format!("Restored {} from backup", path.display()),
            serde_json::json!({
                "path": path.display().to_string(),
                "action": "undo",
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_view_file() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\nline2\nline3").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "view",
            "path": "test.txt"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("line1"));
        assert!(output.text.contains("line2"));
        assert!(output.text.contains("line3"));
    }

    #[test]
    fn test_view_file_with_range() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\nline2\nline3\nline4\nline5").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "view",
            "path": "test.txt",
            "start_line": 2,
            "end_line": 4
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("line2"));
        assert!(output.text.contains("line3"));
        assert!(output.text.contains("line4"));
        assert!(!output.text.contains("line1"));
        assert!(!output.text.contains("line5"));
    }

    #[test]
    fn test_view_file_with_reverse_range_is_safe() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\nline2\nline3").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "view",
            "path": "test.txt",
            "start_line": 3,
            "end_line": 1
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.is_empty() || output.text.contains("[Showing lines"));
        assert!(!output.text.contains("line1"));
        assert!(!output.text.contains("line2"));
        assert!(!output.text.contains("line3"));
    }

    #[test]
    fn test_view_directory() {
        let workspace = tempdir().unwrap();
        let test_dir = workspace.path().join("testdir");
        fs::create_dir(&test_dir).unwrap();
        fs::write(test_dir.join("file1.txt"), "content1").unwrap();
        fs::write(test_dir.join("file2.txt"), "content2").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "view",
            "path": "testdir"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("file1.txt"));
        assert!(output.text.contains("file2.txt"));
    }

    #[test]
    fn test_str_replace() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "hello",
            "new_str": "hi"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hi world");
    }

    #[test]
    fn test_str_replace_empty_old_str() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "",
            "new_str": "hi"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("old_str cannot be empty"));
    }

    #[test]
    fn test_str_replace_old_str_not_found() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "goodbye",
            "new_str": "hi"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("old_str not found"));
    }

    #[test]
    fn test_str_replace_rejects_too_large_result() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "a").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "a",
            "new_str": "x".repeat(1_100_000)
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("would exceed"));
    }

    #[test]
    fn test_create_file() {
        let workspace = tempdir().unwrap();
        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "create",
            "path": "newfile.txt",
            "content": "hello world"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let test_file = workspace.path().join("newfile.txt");
        assert!(test_file.exists());
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_create_file_already_exists() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "existing content").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "create",
            "path": "test.txt",
            "content": "new content"
        });

        // Create should fail with an error when file already exists
        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));

        // Original content should be unchanged
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "existing content");
    }

    #[test]
    fn test_insert_at_line() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\nline2\nline3").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "insert",
            "path": "test.txt",
            "line": 2,
            "content": "inserted line"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "line1\ninserted line\nline2\nline3");
    }

    #[test]
    fn test_insert_at_beginning() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\nline2").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "insert",
            "path": "test.txt",
            "line": 1,
            "content": "first line"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "first line\nline1\nline2");
    }

    #[test]
    fn test_insert_at_end() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\nline2").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "insert",
            "path": "test.txt",
            "line": 3,
            "content": "last line"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "line1\nline2\nlast line");
    }

    #[test]
    fn test_insert_multiline_content() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\nline2\nline3").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "insert",
            "path": "test.txt",
            "line": 2,
            "content": "inserted-a\ninserted-b"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "line1\ninserted-a\ninserted-b\nline2\nline3");
    }

    #[test]
    fn test_insert_empty_content_rejected() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "insert",
            "path": "test.txt",
            "line": 2,
            "content": ""
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("content cannot be empty"));
    }

    #[test]
    fn test_insert_zero_line() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "insert",
            "path": "test.txt",
            "line": 0,
            "content": "test"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("Line numbers start at 1"));
    }

    #[test]
    fn test_undo_edit() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "original content").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        // First, make an edit
        let edit_params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "original",
            "new_str": "modified"
        });

        let edit_result = tool.execute(edit_params, &ctx);
        assert!(edit_result.is_ok());

        // Verify edit happened
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "modified content");

        // Now undo
        let undo_params = serde_json::json!({
            "command": "undo_edit",
            "path": "test.txt"
        });

        let undo_result = tool.execute(undo_params, &ctx);
        assert!(undo_result.is_ok());

        // Verify undo restored original content
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "original content");
    }

    #[test]
    fn test_insert_preserves_crlf() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\r\nline2\r\nline3\r\n").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "insert",
            "path": "test.txt",
            "line": 2,
            "content": "inserted"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("line1\r\n"), "CRLF before insert");
        assert!(
            content.contains("inserted\r\n"),
            "inserted line should have CRLF"
        );
        assert!(content.contains("line2\r\n"), "CRLF after insert");
        assert!(content.ends_with("\r\n"), "trailing CRLF preserved");
    }

    #[test]
    fn test_insert_preserves_trailing_newline() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\nline2\nline3\n").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "insert",
            "path": "test.txt",
            "line": 2,
            "content": "inserted"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.ends_with('\n'), "trailing newline preserved");
        assert!(content.contains("inserted"));
    }

    #[test]
    fn test_str_replace_rejects_empty_old_str() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "",
            "new_str": "injected"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("Error"));

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_backup_list_only_matches_exact_filename() {
        let workspace = tempdir().unwrap();
        let backup_dir = workspace.path().join(BACKUP_DIR);
        fs::create_dir_all(&backup_dir).unwrap();

        let mgr = BackupManager::new(workspace.path());

        // Create backups for "test.txt" and "test.txt.bak"
        let t1 = backup_dir.join("test.txt.1000.backup");
        let t2 = backup_dir.join("test.txt.2000.backup");
        let t3 = backup_dir.join("test.txt.bak.3000.backup");
        fs::write(&t1, "v1").unwrap();
        fs::write(&t2, "v2").unwrap();
        fs::write(&t3, "bak-v1").unwrap();

        // Listing backups for "test.txt" should NOT include "test.txt.bak" entries
        let backups = mgr.list_backups("test.txt").unwrap();
        assert_eq!(backups.len(), 2, "should only match exact filename prefix");

        let names: Vec<String> = backups
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names
            .iter()
            .all(|n| n.starts_with("test.txt.") && !n.contains("bak")));
    }

    #[test]
    fn test_str_replace_multiple_occurrences_replaces_all() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "foo bar foo baz foo").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "foo",
            "new_str": "qux"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("3 occurrence"));

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "qux bar qux baz qux");
    }

    // --- Flexible matching tests ---

    #[test]
    fn test_str_replace_exact_match_still_works() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "hello",
            "new_str": "hi"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hi world");
    }

    #[test]
    fn test_str_replace_crlf_normalization() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        // File has CRLF line endings
        fs::write(&test_file, "line1\r\nline2\r\nline3").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        // LLM sends LF-only old_str (common mismatch)
        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "line1\nline2",
            "new_str": "replaced1\nreplaced2"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok(), "Should succeed via normalized match");

        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("replaced1"), "Should contain replacement");
        // CRLF should be preserved in the rest of the file
        assert!(
            content.contains("line3"),
            "Should preserve unchanged content"
        );
    }

    #[test]
    fn test_str_replace_curly_quotes() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        // File has curly quotes
        fs::write(&test_file, "msg = \u{201C}hello\u{201D}").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        // LLM sends straight quotes
        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "msg = \"hello\"",
            "new_str": "msg = \"world\""
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok(), "Should succeed via quote-normalized match");

        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("world"), "Should contain replacement");
    }

    #[test]
    fn test_str_replace_trimmed_match() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        // File has trailing whitespace on lines
        fs::write(&test_file, "  def foo():  \n    return 42  \n").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        // LLM sends trimmed version
        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "def foo():\n    return 42",
            "new_str": "def bar():\n    return 99"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok(), "Should succeed via trimmed match");

        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("bar"), "Should contain replacement");
        assert!(content.contains("99"), "Should contain new value");
    }

    #[test]
    fn test_str_replace_all_strategies_fail() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        let tool = ClaudeTextEditor;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "command": "str_replace",
            "path": "test.txt",
            "old_str": "completely different content",
            "new_str": "replacement"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(
            output.text.contains("old_str not found"),
            "Should report not found with all strategies mentioned"
        );
    }
}
