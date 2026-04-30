//! Search and replace tool
use crate::file_formatter;
use crate::security::{
    create_file_symlink_safe, open_file_symlink_safe, validate_read_path, validate_regex_pattern,
    validate_write_path,
};
use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum number of replacements to prevent `DoS`
const MAX_REPLACEMENTS: usize = 10000;

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchReplaceInput {
    #[serde(alias = "file_path")]
    pub path: PathBuf,
    pub pattern: String,
    pub replacement: String,
    pub regex: Option<bool>,
}

pub struct SearchReplace;

impl Tool for SearchReplace {
    fn name(&self) -> &'static str {
        "search_replace"
    }

    fn description(&self) -> &'static str {
        "Search and replace text in a file (supports regex)"
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
                    "description": "File path relative to workspace root"
                },
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (literal or regex)"
                },
                "replacement": {
                    "type": "string",
                    "description": "Replacement text"
                },
                "regex": {
                    "type": "boolean",
                    "description": "Use regex for pattern matching (default: false)"
                }
            },
            "required": ["path", "pattern", "replacement"]
        })
    }

    fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let input: SearchReplaceInput = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {e}"))?;

        // Validate path is within workspace
        let path_str = input
            .path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path: contains non-UTF-8 characters"))?;

        let validated_path = validate_read_path(path_str, &ctx.cwd)?;

        // For regex mode, validate the pattern
        let use_regex = input.regex.unwrap_or(false);
        if use_regex {
            validate_regex_pattern(&input.pattern)?;
        }

        // Read file content using symlink-safe operation
        let mut file = open_file_symlink_safe(&validated_path)
            .map_err(|e| anyhow::anyhow!("Failed to open file: {e}"))?;
        let mut content = String::new();
        use std::io::Read;
        file.read_to_string(&mut content).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                anyhow::anyhow!(
                    "Binary or non-UTF-8 file detected; search_replace only supports text files"
                )
            } else {
                anyhow::anyhow!("Failed to read file: {e}")
            }
        })?;

        // Perform replacement
        let (new_content, replacements_made) = if use_regex {
            let re =
                Regex::new(&input.pattern).map_err(|e| anyhow::anyhow!("Invalid regex: {e}"))?;

            // Count replacements first to prevent DoS
            let count = re.find_iter(&content).count();
            if count > MAX_REPLACEMENTS {
                return Err(anyhow::anyhow!(
                    "Pattern matches {count} times, exceeding maximum of {MAX_REPLACEMENTS} replacements"
                ));
            }

            let replaced = re.replace_all(&content, &input.replacement).to_string();
            (replaced, count)
        } else {
            let count = content.matches(&input.pattern).count();
            if count > MAX_REPLACEMENTS {
                return Err(anyhow::anyhow!(
                    "Pattern matches {count} times, exceeding maximum of {MAX_REPLACEMENTS} replacements"
                ));
            }
            (content.replace(&input.pattern, &input.replacement), count)
        };

        // Validate write path (content size)
        validate_write_path(path_str, &ctx.cwd, new_content.len())?;

        // Write the modified content using symlink-safe operation
        let mut file = create_file_symlink_safe(&validated_path)
            .map_err(|e| anyhow::anyhow!("Failed to create file: {e}"))?;
        use std::io::Write;
        file.write_all(new_content.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to write file: {e}"))?;
        file.sync_all()
            .map_err(|e| anyhow::anyhow!("Failed to sync file: {e}"))?;

        let mut output = format!(
            "Successfully performed {} replacement(s) in {}",
            replacements_made,
            input.path.display()
        );

        if let Some(formatter_diff) = file_formatter::format_file(&validated_path, &ctx.cwd) {
            output.push_str(&formatter_diff);
        }

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_search_replace_literal() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "hello world, hello universe").unwrap();

        let tool = SearchReplace;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "pattern": "hello",
            "replacement": "hi",
            "regex": false
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hi world, hi universe");
    }

    #[test]
    fn test_search_replace_regex() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "foo123 bar456 baz789").unwrap();

        let tool = SearchReplace;
        let ctx = ToolContext::new(workspace.path());

        // Note: In JSON, \b is interpreted as backspace. To get the regex word
        // boundary \b, we need to use \\b which becomes \b after JSON parsing.
        let params = serde_json::json!({
            "path": "test.txt",
            "pattern": r"\d+",  // Simple pattern without word boundaries for JSON safety
            "replacement": "NUMBER",
            "regex": true
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "fooNUMBER barNUMBER bazNUMBER");
    }

    #[test]
    fn test_search_replace_blocks_dangerous_regex() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");
        std::fs::write(&test_file, "test content").unwrap();

        let tool = SearchReplace;
        let ctx = ToolContext::new(workspace.path());

        // Nested quantifiers - ReDoS risk
        let params = serde_json::json!({
            "path": "test.txt",
            "pattern": r"(.*).*",
            "replacement": "x",
            "regex": true
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("nested quantifiers"));
    }

    #[test]
    fn test_search_replace_blocks_path_traversal() {
        let workspace = tempdir().unwrap();
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "../../../etc/passwd",
            "pattern": "root",
            "replacement": "hacked"
        });

        let tool = SearchReplace;
        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_search_replace_limits_replacements() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.txt");

        // Create a file with many occurrences
        let content = "a\n".repeat(MAX_REPLACEMENTS + 100);
        std::fs::write(&test_file, &content).unwrap();

        let tool = SearchReplace;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.txt",
            "pattern": "a",
            "replacement": "b",
            "regex": false
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("exceeding maximum"));
    }

    #[test]
    fn test_search_replace_rejects_binary_content() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("test.bin");
        std::fs::write(&test_file, [0xff, 0xfe, 0xfd]).unwrap();

        let tool = SearchReplace;
        let ctx = ToolContext::new(workspace.path());

        let params = serde_json::json!({
            "path": "test.bin",
            "pattern": "a",
            "replacement": "b",
            "regex": false
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-UTF-8 file"));
    }
}
