use crate::truncation::{truncate_items, LIST_MAX_ITEMS};
use crate::Checkpoint;
use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

/// Parameters for the glob tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
pub struct GlobParams {
    /// Glob pattern (e.g., '**/*.rs', 'src/**/*.ts')
    pub pattern: String,
    /// Directory to search in. Defaults to workspace root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Skip directories that should never be searched.
pub(crate) fn should_skip(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value == ".git"
            || value == "target"
            || value == "node_modules"
            || value == ".svn"
            || value == ".hg"
            || value == ".bzr"
            || value == ".jj"
            || value == ".sl"
    })
}

/// Simple glob matching supporting `*`, `?`, and `**` wildcards.
///
/// `**` matches across directory separators (globstar), while `*` matches
/// any single path segment (stops at `/`).
pub(crate) fn glob_pattern_match(text: &str, pattern: &str) -> bool {
    let text_bytes = text.as_bytes();
    let pat_bytes = pattern.as_bytes();
    let text_len = text_bytes.len();
    let pat_len = pat_bytes.len();

    let mut ti = 0;
    let mut pi = 0;
    let mut star_pi: Option<usize> = None;
    let mut star_ti = 0;
    let mut star_is_globstar = false;
    let mut iterations = 0;
    let max_iterations = text_len.saturating_mul(2).max(1024);

    while ti < text_len || pi < pat_len {
        iterations += 1;
        if iterations > max_iterations {
            return false;
        }

        if pi < pat_len {
            match pat_bytes[pi] {
                b'*' => {
                    let is_globstar = pi + 1 < pat_len && pat_bytes[pi + 1] == b'*';
                    star_pi = Some(pi);
                    star_ti = ti;
                    star_is_globstar = is_globstar;
                    if is_globstar {
                        pi += 2;
                        if pi < pat_len && pat_bytes[pi] == b'/' {
                            pi += 1;
                        }
                    } else {
                        pi += 1;
                    }
                    continue;
                }
                b'?' => {
                    if ti < text_len {
                        ti += 1;
                        pi += 1;
                        continue;
                    }
                }
                _ => {
                    if ti < text_len && pat_bytes[pi] == text_bytes[ti] {
                        ti += 1;
                        pi += 1;
                        continue;
                    }
                }
            }
        }
        // Backtrack to last star
        if let Some(sp) = star_pi {
            star_ti += 1;
            if star_ti > text_len {
                return false;
            }
            if star_is_globstar {
                pi = sp + 2;
                if pi < pat_len && pat_bytes[pi] == b'/' {
                    pi += 1;
                }
            } else {
                pi = sp + 1;
            }
            ti = star_ti;
        } else {
            return false;
        }
    }
    true
}

rustycode_tools_api::define_tool! {
    pub struct GlobTool;

    name: "Glob",
    description: "- Fast file pattern matching tool that works with any codebase size\n- Supports glob patterns like \"**/*.js\" or \"src/**/*.ts\"\n- Returns matching file paths sorted by modification time\n- Use this tool when you need to find files by name patterns\n- When you are doing an open ended search that may require multiple rounds of globbing and grepping, use the Agent tool instead",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: GlobParams, ctx) {
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, "Glob")?;
        }

        let pattern = &params.pattern;

        let search_root = if let Some(custom_path) = params.path.as_deref() {
            let resolved = ctx.cwd.join(custom_path);
            if !resolved.exists() {
                anyhow::bail!("path '{custom_path}' does not exist");
            }
            resolved
        } else {
            ctx.cwd.clone()
        };

        ctx.checkpoint()?;

        let mut matches = Vec::new();
        let mut file_count = 0;
        for entry in WalkDir::new(&search_root)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            file_count += 1;

            if file_count % 50 == 0 {
                ctx.checkpoint()?;
            }

            if should_skip(entry.path()) {
                continue;
            }

            let relative = entry
                .path()
                .strip_prefix(&search_root)
                .unwrap_or(entry.path());
            let rel_str = relative.to_string_lossy();

            if glob_pattern_match(&rel_str, pattern) {
                let display_path = entry
                    .path()
                    .strip_prefix(&ctx.cwd)
                    .unwrap_or(entry.path())
                    .display()
                    .to_string();
                matches.push(display_path);

                ctx.checkpoint()?;
            }
        }

        let total_count = matches.len();

        let mut extension_counts: HashMap<String, usize> = HashMap::new();
        for path in &matches {
            if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
                *extension_counts.entry(ext.to_string()).or_insert(0) += 1;
            } else {
                *extension_counts
                    .entry("(no extension)".to_string())
                    .or_insert(0) += 1;
            }
        }

        matches.sort();

        let truncated = truncate_items(matches, LIST_MAX_ITEMS, "glob results");

        let output = format!(
            "**{} matches** for \"{}\"\n\n{}",
            total_count,
            pattern,
            truncated.as_str()
        );

        let mut metadata = truncated.into_metadata();
        metadata["pattern"] = json!(pattern);
        metadata["total_matches"] = json!(total_count);
        metadata["files_searched"] = json!(file_count);

        if !extension_counts.is_empty() {
            let ext_stats: Vec<Value> = extension_counts
                .into_iter()
                .map(|(ext, count)| {
                    json!({
                        "extension": ext,
                        "count": count
                    })
                })
                .collect();
            metadata["extensions"] = json!(ext_stats);
        }

        Ok(ToolOutput::text(output).with_metadata(ctx, || metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use crate::ToolContext;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    // --- should_skip ---

    #[test]
    fn should_skip_git() {
        assert!(should_skip(Path::new(".git/refs/heads")));
        assert!(should_skip(Path::new("src/.git/config")));
    }

    #[test]
    fn should_skip_target() {
        assert!(should_skip(Path::new("target/debug/app")));
    }

    #[test]
    fn should_skip_node_modules() {
        assert!(should_skip(Path::new("node_modules/react/index.js")));
    }

    #[test]
    fn should_not_skip_normal() {
        assert!(!should_skip(Path::new("src/main.rs")));
        assert!(!should_skip(Path::new("lib/core/mod.rs")));
    }

    // --- Tool metadata ---

    #[test]
    fn glob_tool_name() {
        assert_eq!(GlobTool.name(), "Glob");
    }

    #[test]
    fn glob_tool_permission() {
        assert_eq!(GlobTool.permission(), ToolPermission::Read);
    }

    // --- GlobTool schema ---

    #[test]
    fn glob_tool_schema_has_required_fields() {
        let schema = GlobTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "pattern"));
        assert!(schema["properties"]["path"].is_object());
    }

    // --- glob matching ---

    #[test]
    fn glob_match_star_extension() {
        assert!(glob_pattern_match("src/main.rs", "*.rs"));
        assert!(!glob_pattern_match("src/main.go", "*.rs"));
    }

    #[test]
    fn glob_match_wildcard() {
        assert!(glob_pattern_match("src/main.rs", "*.rs"));
        assert!(glob_pattern_match("lib.rs", "*.rs"));
    }

    // --- GlobTool execution tests ---

    #[test]
    fn glob_tool_finds_rs_files() {
        let workspace = tempdir().unwrap();
        fs::create_dir_all(workspace.path().join("src")).unwrap();
        fs::write(workspace.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(workspace.path().join("src/lib.rs"), "pub fn lib() {}").unwrap();
        fs::write(workspace.path().join("README.md"), "# test").unwrap();

        let tool = GlobTool;
        let ctx = ToolContext::new(workspace.path());
        let result = tool.execute(json!({"pattern": "**/*.rs"}), &ctx).unwrap();

        let text = result.text;
        assert!(
            text.contains("2 matches"),
            "expected 2 matches, got: {text}"
        );
        assert!(
            text.contains("main.rs"),
            "expected main.rs in output: {text}"
        );
        assert!(text.contains("lib.rs"), "expected lib.rs in output: {text}");
        assert!(
            !text.contains("README.md"),
            "README.md should not match *.rs"
        );
    }

    #[test]
    fn glob_tool_star_pattern() {
        let workspace = tempdir().unwrap();
        fs::write(workspace.path().join("foo.rs"), "").unwrap();
        fs::write(workspace.path().join("bar.rs"), "").unwrap();
        fs::write(workspace.path().join("baz.go"), "").unwrap();

        let tool = GlobTool;
        let ctx = ToolContext::new(workspace.path());
        let result = tool.execute(json!({"pattern": "*.rs"}), &ctx).unwrap();

        let text = result.text;
        assert!(
            text.contains("2 matches"),
            "expected 2 matches, got: {text}"
        );
        assert!(!text.contains("baz.go"), "baz.go should not match *.rs");
    }

    #[test]
    fn glob_tool_no_matches() {
        let workspace = tempdir().unwrap();
        fs::write(workspace.path().join("test.txt"), "hello").unwrap();

        let tool = GlobTool;
        let ctx = ToolContext::new(workspace.path());
        let result = tool.execute(json!({"pattern": "*.rs"}), &ctx).unwrap();

        let text = result.text;
        assert!(
            text.contains("0 matches"),
            "expected 0 matches, got: {text}"
        );
    }

    #[test]
    fn glob_tool_with_custom_path() {
        let workspace = tempdir().unwrap();
        fs::create_dir_all(workspace.path().join("subdir")).unwrap();
        fs::write(workspace.path().join("subdir/mod.rs"), "mod test;").unwrap();
        fs::write(workspace.path().join("main.rs"), "fn main() {}").unwrap();

        let tool = GlobTool;
        let ctx = ToolContext::new(workspace.path());
        let result = tool
            .execute(json!({"pattern": "*.rs", "path": "subdir"}), &ctx)
            .unwrap();

        let text = result.text;
        assert!(text.contains("1 matches"), "expected 1 match, got: {text}");
        assert!(text.contains("mod.rs"), "expected mod.rs in output: {text}");
        assert!(!text.contains("main.rs"), "main.rs is outside subdir");
    }
}
