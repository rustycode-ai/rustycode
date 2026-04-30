//! Anthropic Memory Tool (`memory_20250818`) implementation.
//!
//! Enables cross-conversation persistence with just-in-time context retrieval.
//! Claude uses this tool to store and retrieve memories in a `/memories/` directory.
//!
//! See: <https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool>

use std::path::{Path, PathBuf};
use tokio::fs;

/// Tool type identifier for the Anthropic memory tool.
pub const MEMORY_TOOL_TYPE: &str = "memory_20250818";
/// Tool name sent to Anthropic API.
pub const MEMORY_TOOL_NAME: &str = "memory";
/// Base directory for memory files.
pub const MEMORIES_DIR: &str = "memories";

/// Returns the Anthropic memory tool definition to include in the tools array.
pub fn memory_tool_definition() -> serde_json::Value {
    serde_json::json!({
        "type": MEMORY_TOOL_TYPE,
        "name": MEMORY_TOOL_NAME
    })
}

/// Memory tool command types that Claude can invoke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryCommand {
    View { path: String },
    Create { path: String, content: String },
    StrReplace { path: String, old_str: String, new_str: String },
    Insert { path: String, insert_after: String, content: String },
    Delete { path: String },
    Rename { path: String, new_path: String },
}

/// Result of executing a memory tool command.
#[derive(Debug)]
pub struct MemoryResult {
    pub content: String,
    pub is_error: bool,
}

impl MemoryResult {
    fn ok(content: impl Into<String>) -> Self {
        Self { content: content.into(), is_error: false }
    }

    fn err(content: impl Into<String>) -> Self {
        Self { content: content.into(), is_error: true }
    }
}

/// Validates that a path is within the memories directory and doesn't escape it.
fn validate_path(base: &Path, path: &str) -> Result<PathBuf, String> {
    // Reject absolute paths and traversal components
    if path.starts_with('/') || path.starts_with("..") || path.contains("\\") {
        return Err(format!("path traversal blocked: {}", path));
    }
    let full = base.join(path);

    // Canonicalize what exists, check the normalized path stays within base
    let canonical_base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let check_path = if full.exists() {
        full.canonicalize().unwrap_or_else(|_| full.clone())
    } else {
        // For non-existent paths, canonicalize the parent and join the filename
        match full.parent() {
            Some(parent) if parent.exists() => {
                parent.canonicalize().unwrap_or(parent.to_path_buf())
            }
            Some(parent) => {
                // Walk up to find an existing ancestor
                let mut p = parent.to_path_buf();
                while !p.exists() && p.starts_with(&canonical_base) {
                    if !p.pop() {
                        break;
                    }
                }
                p.canonicalize().unwrap_or(p)
            }
            None => canonical_base.clone(),
        }
    };

    if !check_path.starts_with(&canonical_base) {
        return Err(format!("path traversal blocked: {}", path));
    }
    Ok(full)
}

/// Parses a memory tool call from Claude into a command.
pub fn parse_memory_command(input: &serde_json::Value) -> Result<MemoryCommand, String> {
    let command = input["command"].as_str().unwrap_or("");
    match command {
        "view" => {
            let path = input["path"].as_str().unwrap_or("").to_string();
            if path.is_empty() {
                return Err("view requires 'path'".to_string());
            }
            Ok(MemoryCommand::View { path })
        }
        "create" => {
            let path = input["path"].as_str().unwrap_or("").to_string();
            let content = input["content"].as_str().unwrap_or("").to_string();
            if path.is_empty() {
                return Err("create requires 'path'".to_string());
            }
            Ok(MemoryCommand::Create { path, content })
        }
        "str_replace" => {
            let path = input["path"].as_str().unwrap_or("").to_string();
            let old_str = input["old_str"].as_str().unwrap_or("").to_string();
            let new_str = input["new_str"].as_str().unwrap_or("").to_string();
            if path.is_empty() {
                return Err("str_replace requires 'path'".to_string());
            }
            Ok(MemoryCommand::StrReplace { path, old_str, new_str })
        }
        "insert" => {
            let path = input["path"].as_str().unwrap_or("").to_string();
            let insert_after = input["insert_after"].as_str().unwrap_or("").to_string();
            let content = input["content"].as_str().unwrap_or("").to_string();
            if path.is_empty() {
                return Err("insert requires 'path'".to_string());
            }
            Ok(MemoryCommand::Insert { path, insert_after, content })
        }
        "delete" => {
            let path = input["path"].as_str().unwrap_or("").to_string();
            if path.is_empty() {
                return Err("delete requires 'path'".to_string());
            }
            Ok(MemoryCommand::Delete { path })
        }
        "rename" => {
            let path = input["path"].as_str().unwrap_or("").to_string();
            let new_path = input["new_path"].as_str().unwrap_or("").to_string();
            if path.is_empty() || new_path.is_empty() {
                return Err("rename requires 'path' and 'new_path'".to_string());
            }
            Ok(MemoryCommand::Rename { path, new_path })
        }
        _ => Err(format!("unknown memory command: {}", command)),
    }
}

/// Executes a memory tool command against the workspace's memories directory.
pub async fn execute_memory_command(
    workspace_root: &Path,
    command: &MemoryCommand,
) -> MemoryResult {
    let memories_dir = workspace_root.join(MEMORIES_DIR);

    // Ensure memories directory exists
    if let Err(e) = fs::create_dir_all(&memories_dir).await {
        return MemoryResult::err(format!("failed to create memories dir: {e}"));
    }

    match command {
        MemoryCommand::View { path } => {
            let full = match validate_path(&memories_dir, path) {
                Ok(p) => p,
                Err(e) => return MemoryResult::err(e),
            };
            match fs::read_to_string(&full).await {
                Ok(content) => MemoryResult::ok(content),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    MemoryResult::err(format!("file not found: {}", path))
                }
                Err(e) => MemoryResult::err(format!("read error: {e}")),
            }
        }
        MemoryCommand::Create { path, content } => {
            let full = match validate_path(&memories_dir, path) {
                Ok(p) => p,
                Err(e) => return MemoryResult::err(e),
            };
            if let Some(parent) = full.parent() {
                if let Err(e) = fs::create_dir_all(parent).await {
                    return MemoryResult::err(format!("failed to create parent dir: {e}"));
                }
            }
            match fs::write(&full, content).await {
                Ok(()) => MemoryResult::ok(format!("created {}", path)),
                Err(e) => MemoryResult::err(format!("write error: {e}")),
            }
        }
        MemoryCommand::StrReplace { path, old_str, new_str } => {
            let full = match validate_path(&memories_dir, path) {
                Ok(p) => p,
                Err(e) => return MemoryResult::err(e),
            };
            let content = match fs::read_to_string(&full).await {
                Ok(c) => c,
                Err(e) => return MemoryResult::err(format!("read error: {e}")),
            };
            if old_str.is_empty() {
                return MemoryResult::err("old_str must not be empty");
            }
            let count = content.matches(old_str.as_str()).count();
            if count == 0 {
                return MemoryResult::err(format!(
                    "old_str not found in {}",
                    path
                ));
            }
            if count > 1 {
                return MemoryResult::err(format!(
                    "old_str found {} times in {} (must be unique)",
                    count, path
                ));
            }
            let new_content = content.replacen(old_str, new_str, 1);
            match fs::write(&full, &new_content).await {
                Ok(()) => MemoryResult::ok(format!("replaced in {}", path)),
                Err(e) => MemoryResult::err(format!("write error: {e}")),
            }
        }
        MemoryCommand::Insert { path, insert_after, content } => {
            let full = match validate_path(&memories_dir, path) {
                Ok(p) => p,
                Err(e) => return MemoryResult::err(e),
            };
            let file_content = match fs::read_to_string(&full).await {
                Ok(c) => c,
                Err(e) => return MemoryResult::err(format!("read error: {e}")),
            };
            let new_content = if insert_after.is_empty() {
                format!("{}{}", file_content, content)
            } else {
                match file_content.find(insert_after.as_str()) {
                    Some(pos) => {
                        let end = pos + insert_after.len();
                        format!(
                            "{}{}{}",
                            &file_content[..end],
                            content,
                            &file_content[end..]
                        )
                    }
                    None => return MemoryResult::err("insert_after not found"),
                }
            };
            match fs::write(&full, &new_content).await {
                Ok(()) => MemoryResult::ok(format!("inserted into {}", path)),
                Err(e) => MemoryResult::err(format!("write error: {e}")),
            }
        }
        MemoryCommand::Delete { path } => {
            let full = match validate_path(&memories_dir, path) {
                Ok(p) => p,
                Err(e) => return MemoryResult::err(e),
            };
            match fs::remove_file(&full).await {
                Ok(()) => MemoryResult::ok(format!("deleted {}", path)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    MemoryResult::err(format!("file not found: {}", path))
                }
                Err(e) => MemoryResult::err(format!("delete error: {e}")),
            }
        }
        MemoryCommand::Rename { path, new_path } => {
            let full_from = match validate_path(&memories_dir, path) {
                Ok(p) => p,
                Err(e) => return MemoryResult::err(e),
            };
            let full_to = match validate_path(&memories_dir, new_path) {
                Ok(p) => p,
                Err(e) => return MemoryResult::err(e),
            };
            if let Some(parent) = full_to.parent() {
                if let Err(e) = fs::create_dir_all(parent).await {
                    return MemoryResult::err(format!("failed to create parent dir: {e}"));
                }
            }
            match fs::rename(&full_from, &full_to).await {
                Ok(()) => MemoryResult::ok(format!("renamed {} to {}", path, new_path)),
                Err(e) => MemoryResult::err(format!("rename error: {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn workspace() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let _ = std::fs::create_dir_all(root.join(MEMORIES_DIR));
        (tmp, root)
    }

    #[test]
    fn test_memory_tool_definition() {
        let def = memory_tool_definition();
        assert_eq!(def["type"], MEMORY_TOOL_TYPE);
        assert_eq!(def["name"], MEMORY_TOOL_NAME);
    }

    #[test]
    fn test_validate_path_blocks_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join(MEMORIES_DIR);
        std::fs::create_dir_all(&base).unwrap();

        // Normal path is fine
        assert!(validate_path(&base, "notes.md").is_ok());
        // Traversal is blocked
        assert!(validate_path(&base, "../../etc/passwd").is_err());
        assert!(validate_path(&base, "../../../tmp/evil").is_err());
    }

    #[test]
    fn test_parse_view_command() {
        let input = serde_json::json!({"command": "view", "path": "notes.md"});
        let cmd = parse_memory_command(&input).unwrap();
        assert_eq!(cmd, MemoryCommand::View { path: "notes.md".into() });
    }

    #[test]
    fn test_parse_create_command() {
        let input = serde_json::json!({"command": "create", "path": "notes.md", "content": "hello"});
        let cmd = parse_memory_command(&input).unwrap();
        assert_eq!(
            cmd,
            MemoryCommand::Create { path: "notes.md".into(), content: "hello".into() }
        );
    }

    #[test]
    fn test_parse_unknown_command() {
        let input = serde_json::json!({"command": "fly"});
        assert!(parse_memory_command(&input).is_err());
    }

    #[tokio::test]
    async fn test_create_and_view() {
        let (_tmp, root) = workspace();
        let cmd = MemoryCommand::Create {
            path: "notes.md".into(),
            content: "my notes".into(),
        };
        let result = execute_memory_command(&root, &cmd).await;
        assert!(!result.is_error, "{}", result.content);

        let view = MemoryCommand::View { path: "notes.md".into() };
        let result = execute_memory_command(&root, &view).await;
        assert!(!result.is_error);
        assert_eq!(result.content, "my notes");
    }

    #[tokio::test]
    async fn test_str_replace() {
        let (_tmp, root) = workspace();
        std::fs::write(root.join(MEMORIES_DIR).join("f.md"), "hello world").unwrap();

        let cmd = MemoryCommand::StrReplace {
            path: "f.md".into(),
            old_str: "world".into(),
            new_str: "rust".into(),
        };
        let result = execute_memory_command(&root, &cmd).await;
        assert!(!result.is_error, "{}", result.content);

        let content = std::fs::read_to_string(root.join(MEMORIES_DIR).join("f.md")).unwrap();
        assert_eq!(content, "hello rust");
    }

    #[tokio::test]
    async fn test_delete() {
        let (_tmp, root) = workspace();
        std::fs::write(root.join(MEMORIES_DIR).join("del.md"), "bye").unwrap();

        let cmd = MemoryCommand::Delete { path: "del.md".into() };
        let result = execute_memory_command(&root, &cmd).await;
        assert!(!result.is_error);
        assert!(!root.join(MEMORIES_DIR).join("del.md").exists());
    }

    #[tokio::test]
    async fn test_rename() {
        let (_tmp, root) = workspace();
        std::fs::write(root.join(MEMORIES_DIR).join("old.md"), "data").unwrap();

        let cmd = MemoryCommand::Rename { path: "old.md".into(), new_path: "new.md".into() };
        let result = execute_memory_command(&root, &cmd).await;
        assert!(!result.is_error);
        assert!(!root.join(MEMORIES_DIR).join("old.md").exists());
        assert_eq!(
            std::fs::read_to_string(root.join(MEMORIES_DIR).join("new.md")).unwrap(),
            "data"
        );
    }

    #[tokio::test]
    async fn test_insert_after() {
        let (_tmp, root) = workspace();
        std::fs::write(root.join(MEMORIES_DIR).join("f.md"), "line1\nline2\n").unwrap();

        let cmd = MemoryCommand::Insert {
            path: "f.md".into(),
            insert_after: "line1\n".into(),
            content: "inserted\n".into(),
        };
        let result = execute_memory_command(&root, &cmd).await;
        assert!(!result.is_error, "{}", result.content);

        let content = std::fs::read_to_string(root.join(MEMORIES_DIR).join("f.md")).unwrap();
        assert_eq!(content, "line1\ninserted\nline2\n");
    }

    #[tokio::test]
    async fn test_path_traversal_blocked() {
        let (_tmp, root) = workspace();
        let cmd = MemoryCommand::View { path: "../../etc/passwd".into() };
        let result = execute_memory_command(&root, &cmd).await;
        assert!(result.is_error);
        assert!(result.content.contains("traversal"));
    }

    #[tokio::test]
    async fn test_str_replace_not_found() {
        let (_tmp, root) = workspace();
        std::fs::write(root.join(MEMORIES_DIR).join("f.md"), "hello").unwrap();

        let cmd = MemoryCommand::StrReplace {
            path: "f.md".into(),
            old_str: "missing".into(),
            new_str: "x".into(),
        };
        let result = execute_memory_command(&root, &cmd).await;
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_str_replace_ambiguous() {
        let (_tmp, root) = workspace();
        std::fs::write(root.join(MEMORIES_DIR).join("f.md"), "abc abc").unwrap();

        let cmd = MemoryCommand::StrReplace {
            path: "f.md".into(),
            old_str: "abc".into(),
            new_str: "x".into(),
        };
        let result = execute_memory_command(&root, &cmd).await;
        assert!(result.is_error);
        assert!(result.content.contains("2 times"));
    }
}
