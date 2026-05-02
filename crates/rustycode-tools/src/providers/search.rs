use crate::security::{validate_list_path, validate_regex_pattern, MAX_REGEX_MATCHES};
use crate::truncation::{truncate_items, GREP_MAX_MATCHES, LIST_MAX_ITEMS};
use crate::{Checkpoint, Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

/// Type alias for the regex cache to reduce type complexity
type RegexCache = Arc<Mutex<lru::LruCache<String, Arc<Regex>>>>;

/// Thread-safe LRU cache for compiled regex patterns
/// Reduces regex compilation overhead for repeated patterns
static REGEX_CACHE: std::sync::LazyLock<RegexCache> = std::sync::LazyLock::new(|| {
    Arc::new(Mutex::new(lru::LruCache::new(
        std::num::NonZeroUsize::new(256).unwrap(),
    )))
});

/// Get or compile a regex pattern from cache
/// Made public for benchmarking and external use
pub fn get_regex(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
    get_regex_with_flags(pattern, false, false)
}

/// Get or compile a case-insensitive regex pattern from cache
pub fn get_regex_insensitive(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
    get_regex_with_flags(pattern, true, false)
}

/// Get or compile a multiline (dotall) regex pattern from cache
pub fn get_regex_multiline(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
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

    // Try to get from cache first
    {
        let mut cache = REGEX_CACHE.lock();
        if let Some(regex) = cache.get(&cache_key) {
            return Ok(Arc::clone(regex));
        }
    }

    // Not in cache, compile and insert
    let compiled = if flags.is_empty() {
        Regex::new(pattern)?
    } else {
        Regex::new(&format!("(?{flags}){pattern}"))?
    };
    let compiled = Arc::new(compiled);

    // Insert into cache (will evict LRU if full)
    let mut cache = REGEX_CACHE.lock();
    cache.put(cache_key, Arc::clone(&compiled));

    Ok(compiled)
}

pub struct GrepTool;
pub struct GlobTool;

impl Tool for GrepTool {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn description(&self) -> &'static str {
        "Search for text patterns across all files in the codebase. Use this to find function definitions, variable usages, or any text pattern in code. Supports simple text search (no regex required) and can show context around matches."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern to search for" },
                "path": { "type": "string", "description": "Directory or file path to search within (alias: file_path, include_pattern)" },
                "file_path": { "type": "string", "description": "Alias for path" },
                "include_pattern": { "type": "string", "description": "Alias for path" },
                "before_context": { "type": "integer", "description": "Lines of context before match" },
                "after_context": { "type": "integer", "description": "Lines of context after match" },
                "max_matches_per_file": { "type": "integer", "description": "Limit matches per file" }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        let pattern = required_string(&params, "pattern")?;

        // Validate regex pattern for ReDoS
        validate_regex_pattern(pattern)?;

        let path_str = params
            .get("path")
            .or_else(|| params.get("file_path"))
            .or_else(|| params.get("include_pattern"))
            .and_then(Value::as_str)
            .unwrap_or(".");
        let root = validate_list_path(path_str, &ctx.cwd)?;

        // Case-insensitive flag — support both -i and case_insensitive
        let case_insensitive = params
            .get("-i")
            .or_else(|| params.get("case_insensitive"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Multiline mode — . matches newlines
        let multiline = params
            .get("multiline")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Use cached regex compilation for better performance
        let re = if case_insensitive && multiline {
            get_regex_with_flags(pattern, true, true)
                .map_err(|e| anyhow!("Invalid regex pattern '{pattern}': {e}"))?
        } else if case_insensitive {
            get_regex_insensitive(pattern)
                .map_err(|e| anyhow!("Invalid regex pattern '{pattern}': {e}"))?
        } else if multiline {
            get_regex_multiline(pattern)
                .map_err(|e| anyhow!("Invalid regex pattern '{pattern}': {e}"))?
        } else {
            get_regex(pattern).map_err(|e| anyhow!("Invalid regex pattern '{pattern}': {e}"))?
        };

        // Get context parameters — support both internal and LLM schema field names
        // LLM schema uses -B/-A/context, internal uses before_context/after_context
        let context_from_c = params
            .get("context")
            .or_else(|| params.get("-C"))
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let before_context = context_from_c.unwrap_or_else(|| {
            params
                .get("before_context")
                .or_else(|| params.get("-B"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize
        });
        let after_context = context_from_c.unwrap_or_else(|| {
            params
                .get("after_context")
                .or_else(|| params.get("-A"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize
        });
        let max_matches_per_file = params
            .get("max_matches_per_file")
            .or_else(|| params.get("head_limit"))
            .and_then(Value::as_u64)
            .map(|v| v as usize);

        // Glob filter for file inclusion (e.g., "*.rs", "*.{ts,tsx}")
        let glob_filter = optional_string(&params, "glob");
        // Type filter maps to file extensions (e.g., "rust" → ".rs")
        let type_filter = optional_string(&params, "type");

        // Offset: skip first N results
        let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        // Limit: cap total matches (-1 means unlimited)
        let match_limit = params
            .get("limit")
            .and_then(Value::as_i64)
            .map(|v| if v < 0 { None } else { Some(v as usize) })
            .unwrap_or(None);

        // Group matches by file for dense display
        let mut file_matches: Vec<(String, Vec<(usize, String)>)> = Vec::new();

        // Check for cancellation before starting file walk
        ctx.checkpoint()?;

        let mut file_count = 0;
        for entry in WalkDir::new(&root)
            .max_depth(4)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            file_count += 1;

            // Check for cancellation every 50 files to balance responsiveness with performance
            if file_count % 50 == 0 {
                ctx.checkpoint()?;
            }

            if should_skip(entry.path()) {
                continue;
            }

            // Apply glob filter if specified
            if let Some(glob_pattern) = glob_filter {
                if !matches_glob(entry.path(), glob_pattern) {
                    continue;
                }
            }

            // Apply type filter if specified
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
            let mut total_matches = 0; // Track total across all files

            if multiline {
                // Multiline mode: match against entire file content
                for mat in re.find_iter(&content) {
                    if total_matches >= MAX_REGEX_MATCHES {
                        break;
                    }
                    total_matches += 1;

                    if let Some(limit) = max_matches_per_file {
                        if match_count >= limit {
                            break;
                        }
                        match_count += 1;
                    }

                    // Find the line number of the match start
                    let line_num = content[..mat.start()].lines().count().max(1);
                    let matched_text = mat.as_str().lines().next().unwrap_or("");
                    matches_in_file.push((line_num, matched_text.to_string()));
                }
            } else {
                // Line-by-line matching (default)

                for (index, line) in lines.iter().enumerate() {
                    // Enforce global match limit to prevent DoS
                    if total_matches >= MAX_REGEX_MATCHES {
                        break;
                    }
                    // Use the pre-compiled regex instead of compiling on every iteration
                    if re.is_match(line) {
                        total_matches += 1;

                        // Check per-file limit
                        if let Some(limit) = max_matches_per_file {
                            if match_count >= limit {
                                break;
                            }
                            match_count += 1;
                        }

                        // Include context lines if requested
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
            } // end else (line-by-line matching)
            if !matches_in_file.is_empty() {
                file_matches.push((entry.path().display().to_string(), matches_in_file));

                // Check for cancellation after each file with matches
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

        // Apply match limit
        if let Some(limit) = match_limit {
            all_matches.truncate(limit);
        }

        // Recount after offset/limit
        let total_count = all_matches.len();

        // Calculate file-level statistics
        let mut file_stats: Vec<(String, usize)> = file_matches
            .iter()
            .map(|(path, matches)| (path.clone(), matches.len()))
            .collect();
        file_stats.sort_by_key(|a| std::cmp::Reverse(a.1));

        // Apply truncation
        let truncated = truncate_items(all_matches, GREP_MAX_MATCHES, "grep results");

        // Format output densely
        let mut output = format!(
            "**{total_count} matches in {files_with_matches} file(s)** for \"{pattern}\"\n\n"
        );
        output.push_str(truncated.as_str());

        // Build metadata with file statistics
        let mut metadata = truncated.into_metadata();
        metadata["pattern"] = json!(pattern);
        metadata["total_matches"] = json!(total_count);
        metadata["files_with_matches"] = json!(files_with_matches);
        if offset > 0 {
            metadata["offset"] = json!(offset);
        }
        if match_limit.is_some() {
            metadata["limit"] = json!(match_limit);
        }

        // Add top files by match count (up to 10)
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

        // Add context parameters to metadata
        if before_context > 0 {
            metadata["before_context"] = json!(before_context);
        }
        if after_context > 0 {
            metadata["after_context"] = json!(after_context);
        }
        if let Some(limit) = max_matches_per_file {
            metadata["max_matches_per_file"] = json!(limit);
        }

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

impl Tool for GlobTool {
    fn name(&self) -> &'static str {
        "glob"
    }

    fn description(&self) -> &'static str {
        "Find files whose path contains a glob-like fragment."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., '**/*.rs', 'src/**/*.ts')"
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search in (alias: file_path). Default: workspace root."
                },
                "file_path": {
                    "type": "string",
                    "description": "Alias for path"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        let pattern = required_string(&params, "pattern")?;

        // Resolve search directory — support optional "path" parameter and "file_path" alias
        let search_root = if let Some(custom_path) = params
            .get("path")
            .or_else(|| params.get("file_path"))
            .and_then(Value::as_str)
        {
            let resolved = ctx.cwd.join(custom_path);
            if !resolved.is_dir() {
                anyhow::bail!("path '{custom_path}' is not a directory");
            }
            resolved
        } else {
            ctx.cwd.clone()
        };

        // Check for cancellation before starting file walk
        ctx.checkpoint()?;

        let mut matches = Vec::new();
        let mut file_count = 0;
        for entry in WalkDir::new(&search_root)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            file_count += 1;

            // Check for cancellation every 50 files
            if file_count % 50 == 0 {
                ctx.checkpoint()?;
            }

            if should_skip(entry.path()) {
                continue;
            }

            // Get relative path from search root for pattern matching
            let relative = entry
                .path()
                .strip_prefix(&search_root)
                .unwrap_or(entry.path());
            let rel_str = relative.to_string_lossy();

            if glob_pattern_match(&rel_str, pattern) {
                // Return path relative to workspace root
                let display_path = entry
                    .path()
                    .strip_prefix(&ctx.cwd)
                    .unwrap_or(entry.path())
                    .display()
                    .to_string();
                matches.push(display_path);

                // Check for cancellation after each match
                ctx.checkpoint()?;
            }
        }

        let total_count = matches.len();

        // Calculate file extension statistics (clone path strings to avoid borrow issues)
        let mut extension_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
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

        // Apply truncation
        let truncated = truncate_items(matches, LIST_MAX_ITEMS, "glob results");

        // Format output densely
        let output = format!(
            "**{} matches** for \"{}\"\n\n{}",
            total_count,
            pattern,
            truncated.as_str()
        );

        // Build metadata with extension statistics
        let mut metadata = truncated.into_metadata();
        metadata["pattern"] = json!(pattern);
        metadata["total_matches"] = json!(total_count);
        metadata["files_searched"] = json!(file_count);

        // Add extension breakdown
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

        Ok(ToolOutput::with_structured(output, metadata))
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string parameter `{key}`"))
}

fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value == ".git" || value == "target" || value == "node_modules"
    })
}

/// Check if a file path matches a glob pattern.
/// Supports simple patterns like "*.rs" and brace expansion like "*.{ts,tsx}".
fn matches_glob(path: &Path, pattern: &str) -> bool {
    // Handle brace expansion: *.{ts,tsx} → try each alternative
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

/// Simple glob matching supporting `*`, `?`, and `**` wildcards.
///
/// `**` matches across directory separators (globstar), while `*` matches
/// any single path segment (stops at `/`).
fn glob_pattern_match(text: &str, pattern: &str) -> bool {
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
            // For single *, don't cross / boundaries
            if !star_is_globstar && star_ti > 0 && text_bytes[star_ti - 1] == b'/' {
                // Star can't match past / — move past it
                // but only if we haven't already consumed the /
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

/// Map a file type name to its extension(s).
/// Covers common types supported by ripgrep's --type flag.
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
        // Unknown type — don't filter, let it through
        return true;
    }

    extensions.iter().any(|e| ext == *e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // --- get_regex ---

    #[test]
    fn get_regex_valid_pattern() {
        let re = get_regex(r"\d+").unwrap();
        assert!(re.is_match("123"));
        assert!(!re.is_match("abc"));
    }

    #[test]
    fn get_regex_invalid_pattern() {
        assert!(get_regex(r"[invalid").is_err());
    }

    #[test]
    fn get_regex_caches_compiled() {
        let re1 = get_regex(r"test").unwrap();
        let re2 = get_regex(r"test").unwrap();
        assert!(re1.is_match("test"));
        assert!(re2.is_match("test"));
    }

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
    fn grep_tool_name() {
        assert_eq!(GrepTool.name(), "grep");
    }

    #[test]
    fn grep_tool_permission() {
        assert_eq!(GrepTool.permission(), ToolPermission::Read);
    }

    #[test]
    fn glob_tool_name() {
        assert_eq!(GlobTool.name(), "glob");
    }

    #[test]
    fn glob_tool_permission() {
        assert_eq!(GlobTool.permission(), ToolPermission::Read);
    }

    // --- GrepTool schema ---

    #[test]
    fn grep_tool_schema_has_required_fields() {
        let schema = GrepTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "pattern"));
    }

    // --- GlobTool schema ---

    #[test]
    fn glob_tool_schema_has_required_fields() {
        let schema = GlobTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "pattern"));
        // Verify path property exists (optional)
        assert!(schema["properties"]["path"].is_object());
    }

    // --- required_string / optional_string helpers ---

    #[test]
    fn required_string_present() {
        let v = json!({"name": "test"});
        assert_eq!(required_string(&v, "name").unwrap(), "test");
    }

    #[test]
    fn required_string_missing() {
        let v = json!({});
        assert!(required_string(&v, "name").is_err());
    }

    #[test]
    fn optional_string_present() {
        let v = json!({"name": "test"});
        assert_eq!(optional_string(&v, "name"), Some("test"));
    }

    #[test]
    fn optional_string_missing() {
        let v = json!({});
        assert_eq!(optional_string(&v, "name"), None);
    }

    #[test]
    fn optional_string_wrong_type() {
        let v = json!({"name": 123});
        assert_eq!(optional_string(&v, "name"), None);
    }

    // --- glob matching ---

    #[test]
    fn glob_match_star_extension() {
        assert!(matches_glob(Path::new("src/main.rs"), "*.rs"));
        assert!(!matches_glob(Path::new("src/main.go"), "*.rs"));
    }

    #[test]
    fn glob_match_brace_expansion() {
        assert!(matches_glob(Path::new("app.ts"), "*.{ts,tsx}"));
        assert!(matches_glob(Path::new("app.tsx"), "*.{ts,tsx}"));
        assert!(!matches_glob(Path::new("app.js"), "*.{ts,tsx}"));
    }

    #[test]
    fn glob_match_wildcard() {
        assert!(matches_glob(Path::new("src/main.rs"), "*.rs"));
        assert!(matches_glob(Path::new("lib.rs"), "*.rs"));
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
        // Unknown types should not filter — let everything through
        assert!(matches_type(Path::new("main.rs"), "unknown_lang"));
    }

    // --- case-insensitive regex ---

    #[test]
    fn case_insensitive_regex() {
        let re = get_regex_insensitive("hello").unwrap();
        assert!(re.is_match("HELLO"));
        assert!(re.is_match("Hello"));
        assert!(re.is_match("hello"));
    }

    #[test]
    fn case_sensitive_regex_default() {
        let re = get_regex("hello").unwrap();
        assert!(!re.is_match("HELLO"));
        assert!(re.is_match("hello"));
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

        // Without multiline, pattern won't match across lines
        let result_no_ml = tool
            .execute(
                json!({"pattern": r"Foo \{.*?\}", "output_mode": "content"}),
                &ctx,
            )
            .unwrap();
        assert!(
            result_no_ml.text.contains("0 matches"),
            "without multiline, cross-line pattern should not match: {}",
            result_no_ml.text
        );

        // With multiline, pattern should match across lines
        let result_ml = tool
            .execute(
                json!({"pattern": r"Foo \{.*?\}", "multiline": true, "output_mode": "content"}),
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
    fn grep_offset_and_limit() {
        let workspace = tempdir().unwrap();
        fs::write(
            workspace.path().join("test.txt"),
            "line1 match\nline2 match\nline3 match\nline4 match\nline5 match",
        )
        .unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(workspace.path());

        // With limit=2, should get at most 2 results
        let result = tool
            .execute(
                json!({"pattern": "match", "output_mode": "content", "limit": 2}),
                &ctx,
            )
            .unwrap();
        assert!(
            result.text.contains("2 matches"),
            "expected 2 matches with limit: {}",
            result.text
        );

        // With offset=3, should skip first 3 results
        let result_offset = tool
            .execute(
                json!({"pattern": "match", "output_mode": "content", "offset": 3}),
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
