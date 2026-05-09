use crate::security::{validate_list_path, validate_regex_pattern, MAX_REGEX_MATCHES};
use crate::truncation::{truncate_items, GREP_MAX_MATCHES};
use crate::Checkpoint;
use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

use super::glob::{glob_pattern_match, should_skip};

/// Type alias for the regex cache to reduce type complexity
type RegexCache = Arc<Mutex<lru::LruCache<String, Arc<Regex>>>>;

/// Thread-safe LRU cache for compiled regex patterns
static REGEX_CACHE: std::sync::LazyLock<RegexCache> = std::sync::LazyLock::new(|| {
    Arc::new(Mutex::new(lru::LruCache::new(
        std::num::NonZeroUsize::new(256).unwrap(),
    )))
});

/// Get or compile a regex pattern from cache
pub fn regex(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
    get_regex_with_flags(pattern, false, false)
}

/// Get or compile a case-insensitive regex pattern from cache
pub fn regex_insensitive(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
    get_regex_with_flags(pattern, true, false)
}

/// Get or compile a multiline (dotall) regex pattern from cache
pub fn regex_multiline(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
    get_regex_with_flags(pattern, false, true)
}

fn get_regex_with_flags(
    pattern: &str,
    case_insensitive: bool,
    multiline: bool,
) -> Result<Arc<Regex>, regex::Error> {
    let mut flags = String::new();
    if case_insensitive {
        flags.push('i');
    }
    if multiline {
        flags.push('s');
    }

    let cache_key = if flags.is_empty() {
        pattern.to_string()
    } else {
        format!("(?{flags}){pattern}")
    };

    {
        let mut cache = REGEX_CACHE.lock();
        if let Some(regex) = cache.get(&cache_key) {
            return Ok(Arc::clone(regex));
        }
    }

    let compiled = if flags.is_empty() {
        Regex::new(pattern)?
    } else {
        Regex::new(&format!("(?{flags}){pattern}"))?
    };
    let compiled = Arc::new(compiled);

    let mut cache = REGEX_CACHE.lock();
    cache.put(cache_key, Arc::clone(&compiled));

    Ok(compiled)
}

/// Parameters for the grep tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
pub struct GrepParams {
    /// The regular expression pattern to search for in file contents
    pub pattern: String,
    /// File or directory to search in. Defaults to current working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Glob pattern to filter files (e.g. "*.js", "*.{ts,tsx}") - maps to rg --glob
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
    /// Output mode: "content" shows matching lines (supports -A/-B/-C context, -n line numbers, head_limit),
    /// "files_with_matches" shows file paths, "count" shows match counts. Defaults to "files_with_matches".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_mode: Option<String>,
    /// Number of lines to show before each match (rg -B). Requires output_mode: "content", ignored otherwise.
    #[serde(rename = "-B", skip_serializing_if = "Option::is_none")]
    pub context_before: Option<u64>,
    /// Number of lines to show after each match (rg -A). Requires output_mode: "content", ignored otherwise.
    #[serde(rename = "-A", skip_serializing_if = "Option::is_none")]
    pub context_after: Option<u64>,

    // Hidden from schema — kept for backward compatibility and RustyCode-specific features
    /// Alias for context.
    #[serde(rename = "-C", skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub context_alias_c: Option<u64>,
    /// Number of lines to show before and after each match. Requires output_mode: "content".
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub context: Option<u64>,
    /// Show line numbers in output. Requires output_mode: "content". Defaults to true.
    #[serde(rename = "-n", skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub show_line_numbers: Option<bool>,
    /// Case insensitive search
    #[serde(rename = "-i", skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub case_insensitive: Option<bool>,
    /// File type to search (e.g. "js", "py", "rust", "go"). More efficient than glob for standard file types.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub type_filter: Option<String>,
    /// Limit output to first N lines/entries. Defaults to 250. Pass 0 for unlimited.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub head_limit: Option<u64>,
    /// Skip first N lines/entries before applying head_limit. Defaults to 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub offset: Option<u64>,
    /// Enable multiline mode where . matches newlines. Default: false.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub multiline: Option<bool>,
}

/// Check if a file path matches a glob pattern.
/// Supports simple patterns like "*.rs" and brace expansion like "*.{ts,tsx}".
fn matches_glob(path: &Path, pattern: &str) -> bool {
    if let Some(start) = pattern.find('{') {
        if let Some(end) = pattern[start..].find('}') {
            let prefix = &pattern[..start];
            let suffix = &pattern[start + end + 1..];
            let alternatives = &pattern[start + 1..start + end];
            for alt in alternatives.split(',') {
                let expanded = format!("{prefix}{alt}{suffix}");
                if glob_pattern_match(path.to_string_lossy().as_ref(), &expanded) {
                    return true;
                }
            }
            return false;
        }
    }

    glob_pattern_match(path.to_string_lossy().as_ref(), pattern)
}

/// Map a file type name to its extension(s).
fn matches_type(path: &Path, type_name: &str) -> bool {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let extensions: &[&str] = match type_name.to_lowercase().as_str() {
        "rust" | "rs" => &["rs"],
        "js" | "javascript" => &["js", "mjs", "cjs"],
        "ts" | "typescript" => &["ts", "tsx", "mts", "cts"],
        "python" | "py" => &["py", "pyw", "pyi"],
        "go" => &["go"],
        "java" => &["java"],
        "c" => &["c", "h"],
        "cpp" | "c++" | "cxx" => &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        "ruby" | "rb" => &["rb"],
        "swift" => &["swift"],
        "kotlin" | "kt" => &["kt", "kts"],
        "markdown" | "md" => &["md", "mdx"],
        "html" => &["html", "htm"],
        "css" => &["css", "scss", "sass", "less"],
        "json" => &["json"],
        "yaml" | "yml" => &["yaml", "yml"],
        "toml" => &["toml"],
        "sh" | "shell" | "bash" => &["sh", "bash", "zsh"],
        "sql" => &["sql"],
        _ => &[],
    };

    if extensions.is_empty() {
        return true;
    }

    extensions.iter().any(|e| ext == *e)
}

/// Default head_limit when unspecified (matches Claude Code reference).
const DEFAULT_HEAD_LIMIT: usize = 250;

rustycode_tools_api::define_tool! {
    pub struct GrepTool;

    name: "Grep",
    description: "A powerful search tool built on ripgrep\nUsage:\n- ALWAYS use Grep for search tasks. NEVER invoke grep or rg as a Bash command. The Grep tool has been optimized for correct permissions and access.\n- Supports full regex syntax (e.g., \"log.*Error\", \"function\\s+\\w+\")\n- Filter files with glob parameter (e.g., \"*.js\", \"**/*.tsx\") or type parameter (e.g., \"js\", \"py\", \"rust\")\n- Output modes: \"content\" shows matching lines, \"files_with_matches\" shows only file paths (default), \"count\" shows match counts\n- Use Agent tool for open-ended searches requiring multiple rounds\n- Pattern syntax: Uses ripgrep (not grep) - literal braces need escaping (use `interface\\{\\}` to find `interface{}` in Go code)\n- Multiline matching: By default patterns match within single lines only. For cross-line patterns like `struct \\{[\\s\\S]*?field`, use `multiline: true`",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore, ToolTag::Debug],

    execute(params: GrepParams, ctx) {
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, "Grep")?;
        }

        let pattern = &params.pattern;

        validate_regex_pattern(pattern)?;

        let path_str = params.path.as_deref().unwrap_or(".");
        let root = validate_list_path(path_str, &ctx.cwd, !ctx.allow_outside_workspace)?;

        let case_insensitive = params.case_insensitive.unwrap_or(false);
        let multiline = params.multiline.unwrap_or(false);

        let re = if case_insensitive && multiline {
            get_regex_with_flags(pattern, true, true)
                .map_err(|e| anyhow!("Invalid regex pattern '{pattern}': {e}"))?
        } else if case_insensitive {
            regex_insensitive(pattern)
                .map_err(|e| anyhow!("Invalid regex pattern '{pattern}': {e}"))?
        } else if multiline {
            regex_multiline(pattern)
                .map_err(|e| anyhow!("Invalid regex pattern '{pattern}': {e}"))?
        } else {
            regex(pattern).map_err(|e| anyhow!("Invalid regex pattern '{pattern}': {e}"))?
        };

        // Context: -C/context takes precedence, then individual -B/-A
        let context_from_c = params.context.or(params.context_alias_c).map(|v| v as usize);
        let before_context = context_from_c
            .unwrap_or_else(|| params.context_before.unwrap_or(0) as usize);
        let after_context = context_from_c
            .unwrap_or_else(|| params.context_after.unwrap_or(0) as usize);

        let glob_filter = params.glob.as_deref();
        let type_filter = params.type_filter.as_deref();

        let offset = params.offset.unwrap_or(0) as usize;

        // head_limit: explicit 0 = unlimited, otherwise default to 250
        let head_limit_val = params.head_limit.map(|v| v as usize);
        let head_limit = if head_limit_val == Some(0) {
            None // unlimited
        } else {
            Some(head_limit_val.unwrap_or(DEFAULT_HEAD_LIMIT))
        };

        // Group matches by file for dense display
        let mut file_matches: Vec<(String, Vec<(usize, String)>)> = Vec::new();

        ctx.checkpoint()?;

        let mut file_count = 0;
        for entry in WalkDir::new(&root)
            .max_depth(4)
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

            if let Some(glob_pattern) = glob_filter {
                if !matches_glob(entry.path(), glob_pattern) {
                    continue;
                }
            }

            if let Some(type_name) = type_filter {
                if !matches_type(entry.path(), type_name) {
                    continue;
                }
            }

            let Ok(content) = fs::read_to_string(entry.path()) else {
                continue;
            };

            let lines: Vec<&str> = content.lines().collect();
            let mut matches_in_file = Vec::new();
            let mut match_count = 0;
            let mut total_matches = 0;

            if multiline {
                for mat in re.find_iter(&content) {
                    if total_matches >= MAX_REGEX_MATCHES {
                        break;
                    }
                    total_matches += 1;

                    if let Some(limit) = head_limit {
                        if match_count >= limit {
                            break;
                        }
                        match_count += 1;
                    }

                    let line_num = content[..mat.start()].lines().count().max(1);
                    let matched_text = mat.as_str().lines().next().unwrap_or("");
                    matches_in_file.push((line_num, matched_text.to_string()));
                }
            } else {
                for (index, line) in lines.iter().enumerate() {
                    if total_matches >= MAX_REGEX_MATCHES {
                        break;
                    }
                    if re.is_match(line) {
                        total_matches += 1;

                        if let Some(limit) = head_limit {
                            if match_count >= limit {
                                break;
                            }
                            match_count += 1;
                        }

                        #[allow(clippy::needless_range_loop)]
                        if before_context > 0 || after_context > 0 {
                            let start = index.saturating_sub(before_context);
                            let end = (index + after_context + 1).min(lines.len());

                            for ctx_idx in start..end {
                                let prefix = if ctx_idx == index {
                                    "→"
                                } else if ctx_idx < index {
                                    "◀"
                                } else {
                                    "▶"
                                };
                                matches_in_file.push((
                                    ctx_idx + 1,
                                    format!("{} {}", prefix, lines[ctx_idx].trim()),
                                ));
                            }
                        } else {
                            matches_in_file.push((index + 1, line.trim().to_string()));
                        }
                    }
                }
            }

            if !matches_in_file.is_empty() {
                file_matches.push((entry.path().display().to_string(), matches_in_file));
                ctx.checkpoint()?;
            }
        }

        // Flatten all matches
        let mut all_matches: Vec<String> = file_matches
            .iter()
            .flat_map(|(path, matches)| {
                matches
                    .iter()
                    .map(move |(line, text)| format!("{path}:{line} → {text}"))
            })
            .collect();

        let files_with_matches = file_matches.len();

        // Apply offset (skip first N results)
        if offset > 0 {
            all_matches = all_matches.into_iter().skip(offset).collect();
        }

        // Apply head_limit
        if let Some(limit) = head_limit {
            all_matches.truncate(limit);
        }

        let total_count = all_matches.len();

        // Calculate file-level statistics
        let mut file_stats: Vec<(String, usize)> = file_matches
            .iter()
            .map(|(path, matches)| (path.clone(), matches.len()))
            .collect();
        file_stats.sort_by_key(|a| std::cmp::Reverse(a.1));

        let truncated = truncate_items(all_matches, GREP_MAX_MATCHES, "grep results");

        let mut output = format!(
            "**{total_count} matches in {files_with_matches} file(s)** for \"{pattern}\"\n\n"
        );
        output.push_str(truncated.as_str());

        let mut metadata = truncated.into_metadata();
        metadata["pattern"] = json!(pattern);
        metadata["total_matches"] = json!(total_count);
        metadata["files_with_matches"] = json!(files_with_matches);
        if offset > 0 {
            metadata["offset"] = json!(offset);
        }
        if head_limit.is_some() {
            metadata["head_limit"] = json!(head_limit);
        }

        let top_files: Vec<Value> = file_stats
            .iter()
            .take(10)
            .map(|(path, count)| {
                json!({
                    "path": path,
                    "matches": count
                })
            })
            .collect();
        if !top_files.is_empty() {
            metadata["top_files"] = json!(top_files);
        }

        if before_context > 0 {
            metadata["before_context"] = json!(before_context);
        }
        if after_context > 0 {
            metadata["after_context"] = json!(after_context);
        }

        Ok(ToolOutput::text(output).with_metadata(ctx, || metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use crate::ToolContext;
    use std::fs;
    use tempfile::tempdir;

    // --- get_regex ---

    #[test]
    fn get_regex_valid_pattern() {
        let re = regex(r"\d+").unwrap();
        assert!(re.is_match("123"));
        assert!(!re.is_match("abc"));
    }

    #[test]
    fn get_regex_invalid_pattern() {
        assert!(regex(r"[invalid").is_err());
    }

    #[test]
    fn get_regex_caches_compiled() {
        let re1 = regex(r"test").unwrap();
        let re2 = regex(r"test").unwrap();
        assert!(re1.is_match("test"));
        assert!(re2.is_match("test"));
    }

    // --- Tool metadata ---

    #[test]
    fn grep_tool_name() {
        assert_eq!(GrepTool.name(), "grep");
    }

    #[test]
    fn grep_tool_permission() {
        assert_eq!(GrepTool.permission(), ToolPermission::Read);
    }

    // --- GrepTool schema ---

    #[test]
    fn grep_tool_schema_has_required_fields() {
        let schema = GrepTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "pattern"));
    }

    // --- glob matching ---

    #[test]
    fn glob_match_brace_expansion() {
        assert!(matches_glob(Path::new("app.ts"), "*.{ts,tsx}"));
        assert!(matches_glob(Path::new("app.tsx"), "*.{ts,tsx}"));
        assert!(!matches_glob(Path::new("app.js"), "*.{ts,tsx}"));
    }

    // --- type matching ---

    #[test]
    fn type_match_rust() {
        assert!(matches_type(Path::new("main.rs"), "rust"));
        assert!(!matches_type(Path::new("main.go"), "rust"));
    }

    #[test]
    fn type_match_typescript() {
        assert!(matches_type(Path::new("app.ts"), "ts"));
        assert!(matches_type(Path::new("app.tsx"), "typescript"));
        assert!(!matches_type(Path::new("app.js"), "ts"));
    }

    #[test]
    fn type_match_unknown_passes() {
        assert!(matches_type(Path::new("main.rs"), "unknown_lang"));
    }

    // --- case-insensitive regex ---

    #[test]
    fn case_insensitive_regex() {
        let re = regex_insensitive("hello").unwrap();
        assert!(re.is_match("HELLO"));
        assert!(re.is_match("Hello"));
        assert!(re.is_match("hello"));
    }

    #[test]
    fn case_sensitive_regex_default() {
        let re = regex("hello").unwrap();
        assert!(!re.is_match("HELLO"));
        assert!(re.is_match("hello"));
    }

    // --- multiline regex ---

    #[test]
    fn multiline_regex_matches_across_lines() {
        let workspace = tempdir().unwrap();
        fs::create_dir_all(workspace.path().join("src")).unwrap();
        fs::write(
            workspace.path().join("src/test.rs"),
            "struct Foo {\n    x: i32,\n    y: i32,\n}",
        )
        .unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(workspace.path());

        let result_no_ml = tool
            .execute(
                serde_json::json!({"pattern": r"Foo \{.*?\}", "output_mode": "content"}),
                &ctx,
            )
            .unwrap();
        assert!(
            result_no_ml.text.contains("0 matches"),
            "without multiline, cross-line pattern should not match: {}",
            result_no_ml.text
        );

        let result_ml = tool
            .execute(
                serde_json::json!({"pattern": r"Foo \{.*?\}", "multiline": true, "output_mode": "content"}),
                &ctx,
            )
            .unwrap();
        assert!(
            result_ml.text.contains("1 matches"),
            "with multiline, cross-line pattern should match: {}",
            result_ml.text
        );
    }

    #[test]
    fn grep_offset_and_head_limit() {
        let workspace = tempdir().unwrap();
        fs::write(
            workspace.path().join("test.txt"),
            "line1 match\nline2 match\nline3 match\nline4 match\nline5 match",
        )
        .unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool
            .execute(
                serde_json::json!({"pattern": "match", "output_mode": "content", "head_limit": 2}),
                &ctx,
            )
            .unwrap();
        assert!(
            result.text.contains("2 matches"),
            "expected 2 matches with head_limit: {}",
            result.text
        );

        let result_offset = tool
            .execute(
                serde_json::json!({"pattern": "match", "output_mode": "content", "offset": 3}),
                &ctx,
            )
            .unwrap();
        assert!(
            result_offset.text.contains("2 matches"),
            "expected 2 matches after offset=3: {}",
            result_offset.text
        );
    }
}
