use crate::security::{open_file_symlink_safe, validate_read_path, validate_regex_pattern};
use crate::truncation::{format_with_line_numbers, truncate_lines, READ_MAX_LINES};
use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Context};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const BLOCKED_DEVICE_PATHS: &[&str] = &[
    "/dev/",
    "/proc/",
    "/sys/",
    "/run/systemd/",
    "/dev/fd/",
    "/dev/stdin",
    "/dev/stdout",
    "/dev/stderr",
];

fn is_blocked_device_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    BLOCKED_DEVICE_PATHS
        .iter()
        .any(|blocked| path_str.starts_with(blocked))
}

fn detect_binary(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_lowercase().as_str(),
                // Images
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp" | "svg" |
                "tiff" | "psd" | "ai" | "eps" |
                // Audio
                "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" | "wma" |
                // Video
                "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm" |
                // Archives
                "zip" | "tar" | "gz" | "bz2" | "rar" | "7z" | "xz" | "zst" |
                // Executables (blocked)
                "exe" | "dll" | "so" | "dylib" | "app" | "bin" |
                // Documents
                "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" |
                // Fonts
                "ttf" | "otf" | "woff" | "woff2" | "eot" |
                // Database (blocked)
                "db" | "sqlite" | "mdb" |
                // Other binaries
                "class" | "jar" | "war" | "obj" | "o" | "a" | "lib"
            )
        })
        .unwrap_or(false)
}

fn is_binary_content(data: &[u8]) -> bool {
    let check_len = data.len().min(8192);
    data[..check_len].contains(&0)
}

/// Detect programming language from file extension
fn detect_language(path: &Path) -> Option<&'static str> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| match ext.to_lowercase().as_str() {
            "rs" => "rust",
            "go" => "go",
            "py" => "python",
            "js" | "mjs" | "cjs" => "javascript",
            "ts" | "mts" | "cts" => "typescript",
            "java" => "java",
            "kt" | "kts" => "kotlin",
            "cpp" | "cc" | "cxx" | "h" | "hpp" => "cpp",
            "c" => "c",
            "cs" => "csharp",
            "php" => "php",
            "rb" => "ruby",
            "swift" => "swift",
            "sh" => "shell",
            "yaml" | "yml" => "yaml",
            "json" => "json",
            "toml" => "toml",
            "md" => "markdown",
            _ => "text",
        })
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PatternMatch {
    line: usize,
    text: String,
    matched: String,
}

/// Count comment lines in code
pub fn count_comment_lines(lines: &[&str], language: Option<&str>) -> usize {
    match language {
        Some("rust" | "go" | "c" | "cpp" | "java" | "kotlin" | "csharp") => lines
            .iter()
            .filter(|l| {
                let trimmed = l.trim_start();
                trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*")
            })
            .count(),
        Some("python" | "ruby" | "shell" | "perl") => lines
            .iter()
            .filter(|l| l.trim_start().starts_with('#'))
            .count(),
        Some("yaml" | "toml") => lines
            .iter()
            .filter(|l| l.trim_start().starts_with('#'))
            .count(),
        Some("json") => 0,
        Some("markdown" | "md") => lines
            .iter()
            .filter(|l| l.trim_start().starts_with("<!--"))
            .count(),
        _ => 0,
    }
}

fn get_last_modified(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            let duration_since_epoch = t
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            format!("{}", duration_since_epoch.as_secs())
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Calculate code complexity estimate
pub fn estimate_complexity(line_count: usize, comment_lines: usize) -> String {
    let code_ratio = if line_count > 0 {
        (line_count - comment_lines) as f64 / line_count as f64
    } else {
        0.0
    };

    if line_count < 50 {
        "simple".to_string()
    } else if line_count < 200 {
        if code_ratio > 0.7 { "medium" } else { "simple" }.to_string()
    } else if line_count < 500 {
        if code_ratio > 0.6 { "high" } else { "medium" }.to_string()
    } else {
        "very_high".to_string()
    }
}

fn compute_hash_prefix(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    hash[..4]
        .iter()
        .fold(String::with_capacity(8), |mut acc, byte| {
            use std::fmt::Write;
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

fn record_file_read(ctx: &crate::ToolContext, path: &Path, hash_prefix: &str, is_partial: bool) {
    if let Some(state) = &ctx.file_read_state {
        let mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
        if let Some(mtime) = mtime {
            state.record_read(
                path.to_path_buf(),
                mtime,
                hash_prefix.to_string(),
                is_partial,
            );
        }
    }
}

fn truncate_bytes_to_boundary(bytes: &[u8], max_bytes: usize) -> &[u8] {
    if bytes.len() <= max_bytes {
        return bytes;
    }
    let mut end = max_bytes;
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    &bytes[..end]
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
pub struct ReadFileParams {
    /// The absolute path to the file to read
    #[serde(alias = "path")]
    pub file_path: std::path::PathBuf,
    /// The line number to start reading from. Only provide if the file is too large to read at once
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    /// The number of lines to read. Only provide if the file is too large to read at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Page range for PDF files (e.g., "1-5", "3", "10-20"). Only applicable to PDF files. Maximum 20 pages per request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,

    // Hidden from schema — kept for backward compatibility and RustyCode-specific features
    /// First line to return, 1-indexed inclusive
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub start_line: Option<usize>,
    /// Last line to return, 1-indexed inclusive
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub end_line: Option<usize>,
    /// Regex pattern to filter matching lines
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub pattern: Option<String>,
    /// Case-insensitive pattern matching
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub case_insensitive: Option<bool>,
    /// Maximum number of pattern matches to return
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub max_matches: Option<usize>,
    /// Lines to show before/after each pattern match
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub context_lines: Option<usize>,
    /// Return file statistics instead of content
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub stats: Option<bool>,
    /// Read binary files as base64 instead of blocking them
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub binary: Option<bool>,
}

rustycode_tools_api::define_tool! {
    pub struct ReadFileTool;

    name: "Read",
    description: "Reads a file from the local filesystem. You can access any file directly by using this tool.\nAssume this tool is able to read all files on the machine. If the User provides a path to a file assume that path is valid. It is okay to read a file that does not exist; an error will be returned.\nUsage:\n- The file_path parameter must be an absolute path, not a relative path\n- By default, it reads up to 2000 lines starting from the beginning of the file\n- You can optionally specify a line offset and limit (especially handy for large files), but it's recommended to read the whole file by not providing these parameters\n- Results are returned using cat -n format, with line numbers starting at 1\n- This tool allows Claude Code to read images (eg PNG, JPG, etc). When reading an image file the contents are presented visually as Claude Code is a multimodal LLM.\n- This tool can read PDF files (.pdf). For large PDFs (more than 10 pages), you MUST provide the pages parameter to read specific page ranges (e.g., pages: \"1-5\"). Maximum 20 pages per request.\n- This tool can read Jupyter notebooks (.ipynb files) and returns all cells with their outputs, combining code, text, and visualizations.\n- This tool can only read files, not directories. To list files in a directory, use the registered shell tool.\n- You will regularly be asked to read screenshots. If the user provides a path to a screenshot, ALWAYS use this tool to view the file at the path. This tool will work with any temporary file paths.\n- If you read a file that exists but has empty contents you will receive a system reminder warning in place of file contents.\n- Do NOT re-read a file you just edited to verify — Edit/Write would have errored if the change failed, and the harness tracks file state for you.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore, ToolTag::Implement, ToolTag::Debug, ToolTag::Refactor, ToolTag::Ops],

    execute(params: ReadFileParams, ctx) {
        // Block device/system paths (was validate_input)
        if is_blocked_device_path(&params.file_path) {
            return Err(anyhow!(
                "Reading from device/system paths is blocked: {}",
                params.file_path.display()
            ));
        }

        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, "Read")?;
        }
        crate::check_permission(ToolPermission::Read, ctx)?;

        let path_str = params.file_path.to_str()
            .ok_or_else(|| anyhow!("path contains invalid UTF-8"))?;
        let path = validate_read_path(path_str, &ctx.cwd, !ctx.allow_outside_workspace)?;
        crate::check_sandbox_path(&path, ctx)?;

        if super::is_blocked_extension(&path) {
            return Ok(ToolOutput::text(format!(
                "[Blocked] File extension is not allowed for security reasons: {}",
                path.extension().unwrap_or_default().to_string_lossy()
            )));
        }
        if super::is_blocked_filename(&path) {
            return Ok(ToolOutput::text(format!(
                "[Blocked] File is not allowed for security reasons: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )));
        }

        let allow_binary = params.binary.unwrap_or(false);

        // Notebook files: parse as text regardless of binary setting
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("ipynb") {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("read notebook {}", path.display()))?;
                let parsed = crate::notebook::parse_notebook(&content)?;
                let total_lines = parsed.lines().count();
                let hash_prefix = compute_hash_prefix(content.as_bytes());
                record_file_read(ctx, &path, &hash_prefix, true);
                let path_display = path.display().to_string();
                return Ok(ToolOutput::text(parsed.clone()).with_metadata(ctx, || json!({
                        "path": path_display,
                        "type": "notebook",
                        "bytes": parsed.len(),
                        "total_lines": total_lines,
                        "binary": false
                    })));
            }
        }

        // Known binary by extension
        if detect_binary(&path) && !allow_binary {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("unknown");
            let suggestion = match ext.to_lowercase().as_str() {
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" => {
                    "Use an image viewer or tool to extract metadata (e.g., `file` command)"
                }
                "pdf" => "Use a PDF viewer or PDF text extraction tool (e.g., `pdftotext`)",
                "zip" | "tar" | "gz" | "bz2" | "rar" | "7z" => {
                    "Extract the archive first or use archive inspection tools"
                }
                "exe" | "dll" | "so" | "dylib" => {
                    "Use binary analysis tools (e.g., `strings`, `objdump`, `nm`)"
                }
                _ => "Use a specialized tool for this file type",
            };
            return Ok(ToolOutput::text(format!(
                    "[Binary file detected: {} (type: .{})]\n\nRecovery: {}",
                    path.display(),
                    ext,
                    suggestion
                )).with_metadata(ctx, || json!({
                    "path": path.display().to_string(),
                    "extension": ext,
                    "binary": true,
                    "error": "Binary file - use appropriate tool to view this file type",
                    "recovery_hint": suggestion
                })));
        }

        if allow_binary {
            let mut f = open_file_symlink_safe(&path)?;
            let mut bytes = Vec::new();
            use std::io::Read;
            f.read_to_end(&mut bytes)?;

            if crate::image_detect::image_type_from_extension(&path).is_some() {
                match crate::image::process_image(&bytes, crate::image::DEFAULT_MAX_TOKENS) {
                    Ok(processed) => {
                        return Ok(ToolOutput::text(processed.base64_data.clone()).with_metadata(ctx, || json!({
                                "path": path.display().to_string(),
                                "type": "image",
                                "media_type": processed.media_type,
                                "base64": processed.base64_data,
                                "original_dimensions": [processed.original_dimensions.0, processed.original_dimensions.1],
                                "output_dimensions": [processed.output_dimensions.0, processed.output_dimensions.1],
                                "original_size": processed.original_size,
                                "output_size": processed.output_size,
                                "compression_level": format!("{:?}", processed.compression_level),
                            })));
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "image processing failed, falling back to raw base64");
                    }
                }
            }

            let total_bytes = bytes.len();
            let preview = truncate_bytes_to_boundary(&bytes, super::super::web::content::WEB_FETCH_MAX_CHARS);
            let encoded = STANDARD.encode(preview);
            return Ok(ToolOutput::text(encoded).with_metadata(ctx, || json!({
                    "path": path.display().to_string(),
                    "binary": true,
                    "encoding": "base64",
                    "bytes": total_bytes,
                    "shown_bytes": preview.len(),
                    "content_truncated": preview.len() < total_bytes,
                })));
        }

        let mut file = open_file_symlink_safe(&path).map_err(|e| {
            let msg = format!("{e}");
            if msg.contains("not found") || msg.contains("No such file") {
                let target = std::path::Path::new(path_str);
                let suggestions = crate::file_suggest::suggest_similar_files(target, &ctx.cwd, 3);
                let hint = crate::file_suggest::format_suggestions(&suggestions);
                if !hint.is_empty() {
                    return anyhow::anyhow!("File not found: {}{hint}", path.display());
                }
            }
            anyhow::anyhow!("Failed to open file: {e}")
        })?;
        use std::io::Read;

        let mut probe = [0u8; 8192];
        let probe_len = file.read(&mut probe)?;
        if is_binary_content(&probe[..probe_len]) {
            return Ok(ToolOutput {
                text: "File appears to be binary. Use bash with `xxd` or `hexdump` for binary inspection.".to_string(),
                structured: None,
                new_cwd: None,
            });
        }

        let mut content = String::from_utf8_lossy(&probe[..probe_len]).into_owned();
        file.read_to_string(&mut content)?;

        let total_bytes = content.len();
        let (content, _line_ending) = crate::line_endings::normalize_and_detect(&content);
        let total_lines = content.lines().count();
        let path_display = path.display().to_string();

        // Stats mode
        if params.stats.unwrap_or(false) {
            let lines: Vec<&str> = content.lines().collect();
            let blank_lines = lines.iter().filter(|l| l.is_empty()).count();
            let language = detect_language(&path);
            let comment_lines = count_comment_lines(&lines, language);
            let code_lines = total_lines.saturating_sub(blank_lines + comment_lines);
            let stats = json!({
                "path": path_display,
                "size_bytes": total_bytes,
                "total_lines": total_lines,
                "blank_lines": blank_lines,
                "code_lines": code_lines,
                "comment_lines": comment_lines,
                "language": language,
                "encoding": "utf-8",
                "last_modified": get_last_modified(&path),
                "complexity": estimate_complexity(total_lines, comment_lines)
            });
            return Ok(ToolOutput::text(serde_json::to_string_pretty(&stats)?).with_metadata(ctx, || stats.clone()));
        }

        // Pattern matching mode
        if let Some(pattern_str) = params.pattern.as_deref() {
            validate_regex_pattern(pattern_str)
                .map_err(|e| anyhow!("Invalid regex pattern: {e}"))?;

            let case_insensitive = params.case_insensitive.unwrap_or(false);
            let max_matches = params.max_matches.unwrap_or(100);
            let context_lines_count = params.context_lines;

            let regex = if case_insensitive {
                Regex::new(&format!("(?i){pattern_str}"))
            } else {
                Regex::new(pattern_str)
            }?;

            let lines: Vec<&str> = content.lines().collect();

            if let Some(context) = context_lines_count {
                let lines_vec: Vec<&str> = content.lines().collect();
                let mut matches = Vec::new();
                for (i, line) in lines_vec.iter().enumerate() {
                    if regex.is_match(line) && matches.len() < max_matches {
                        let start_idx = i.saturating_sub(context);
                        let end_idx = (i + context + 1).min(lines_vec.len());
                        let ctx_lines: Vec<String> = lines_vec[start_idx..end_idx]
                            .iter()
                            .enumerate()
                            .map(|(j, l)| format!("{}: {}", start_idx + j + 1, l))
                            .collect();
                        matches.push(json!({
                            "line": i + 1,
                            "match": regex.find(line).map(|m| m.as_str()).unwrap_or(""),
                            "text": *line,
                            "context": ctx_lines
                        }));
                    }
                }
                return Ok(ToolOutput::text(format!(
                        "Found {} match(es) for pattern: {}",
                        matches.len(),
                        pattern_str
                    )).with_metadata(ctx, || json!({
                        "pattern": pattern_str,
                        "case_insensitive": case_insensitive,
                        "total_matches": matches.len(),
                        "matches": matches
                    })));
            }

            let matches: Vec<PatternMatch> = lines
                .iter()
                .enumerate()
                .filter_map(|(i, line)| {
                    regex.find(line).map(|m| PatternMatch {
                        line: i + 1,
                        text: line.to_string(),
                        matched: m.as_str().to_string(),
                    })
                })
                .take(max_matches)
                .collect();
            return Ok(ToolOutput::text(matches
                    .iter()
                    .map(|m| format!("Line {}: {}", m.line, m.text))
                    .collect::<Vec<_>>()
                    .join("\n")).with_metadata(ctx, || json!({
                    "pattern": pattern_str,
                    "case_insensitive": case_insensitive,
                    "total_matches": matches.len(),
                    "matches": matches
                })));
        }

        // Pagination mode (offset/limit)
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit;
        if offset > 0 || limit.is_some() {
            let lines: Vec<&str> = content.lines().collect();
            let max_limit = limit.unwrap_or(READ_MAX_LINES);
            let paginated_lines: Vec<&str> =
                lines.iter().skip(offset).take(max_limit).copied().collect();
            let text = format_with_line_numbers(&paginated_lines, offset + 1);
            let text_bytes = text.len();
            let shown_lines = paginated_lines.len();
            let hash_prefix = compute_hash_prefix(content.as_bytes());
            record_file_read(ctx, &path, &hash_prefix, true);
            return Ok(ToolOutput::text(text).with_metadata(ctx, || json!({
                    "path": path_display,
                    "bytes": text_bytes,
                    "total_lines": total_lines,
                    "shown_lines": shown_lines,
                    "offset": offset,
                    "limit": limit,
                    "binary": false
                })));
        }

        // Line range mode
        let start = params.start_line;
        let end = params.end_line;
        if start.is_some() || end.is_some() {
            let lines: Vec<&str> = content.lines().collect();
            let s = start.unwrap_or(1).saturating_sub(1);
            let e = end.unwrap_or(total_lines).min(total_lines);
            let s = s.min(total_lines);
            let e = e.max(s).min(total_lines);
            let range_lines: Vec<&str> = lines[s..e].to_vec();
            let text = format_with_line_numbers(&range_lines, s + 1);
            let hash_prefix = compute_hash_prefix(content.as_bytes());
            record_file_read(ctx, &path, &hash_prefix, true);
            return Ok(ToolOutput::text(text).with_metadata(ctx, || json!({
                    "path": path.display().to_string(),
                    "bytes": content.len(),
                    "total_lines": total_lines,
                    "shown_lines": e - s,
                    "binary": false,
                })));
        }

        // Full file with smart truncation
        let truncated = truncate_lines(&content, READ_MAX_LINES, &path_display, total_lines);
        let output_text = truncated.as_str().to_string();
        let mut metadata = truncated.into_metadata();
        metadata["path"] = json!(path_display);
        metadata["total_bytes"] = json!(total_bytes);

        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = hasher
            .finalize()
            .iter()
            .fold(String::with_capacity(64), |mut acc, byte| {
                use std::fmt::Write;
                let _ = write!(acc, "{byte:02x}");
                acc
            });
        metadata["content_hash"] = json!(content_hash);
        metadata["shown_bytes"] = json!(output_text.len());
        metadata["binary"] = json!(false);

        let hash_prefix = content_hash[..8].to_string();
        record_file_read(ctx, &path, &hash_prefix, false);

        if let Some(language) = detect_language(&path) {
            metadata["language"] = json!(language);
        }

        Ok(ToolOutput::text(output_text).with_metadata(ctx, || metadata.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};
    use std::os::unix::fs::symlink as symlink_file;
    use tempfile::tempdir;

    #[test]
    fn read_file_blocks_outside_workspace_absolute_path() {
        let workspace = tempdir().expect("workspace tempdir");
        let outside = tempdir().expect("outside tempdir");
        let outside_file = outside.path().join("outside.txt");
        fs::write(&outside_file, "nope").expect("write outside file");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(
            serde_json::json!({ "path": outside_file.display().to_string() }),
            &ctx,
        );
        match res {
            Ok(_) => panic!("Expected error for outside workspace path, but got Ok"),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("outside workspace")
                        || msg.contains("blocked")
                        || msg.contains("not within workspace"),
                    "Unexpected error message: {}",
                    msg
                );
            }
        }
    }

    #[test]
    fn read_file_normal_file_works() {
        let workspace = tempdir().expect("workspace tempdir");
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").expect("write test file");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(serde_json::json!({ "path": "test.txt" }), &ctx);
        assert!(res.is_ok());
        let output = res.unwrap();
        assert_eq!(output.text, "1\thello world");
    }

    #[test]
    fn read_file_blocks_symlink_to_file_inside_workspace() {
        let workspace = tempdir().expect("workspace tempdir");
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").expect("write test file");

        let symlink_path = workspace.path().join("symlink.txt");
        symlink_file(&test_file, &symlink_path).expect("create symlink");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(serde_json::json!({ "path": "symlink.txt" }), &ctx);
        match res {
            Ok(_) => panic!("Expected error for symlink path, but got Ok"),
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("symbolic link"), "Unexpected error: {}", msg);
            }
        }
    }

    #[test]
    fn read_file_blocks_symlink_to_directory_inside_workspace() {
        let workspace = tempdir().expect("workspace tempdir");
        let test_dir = workspace.path().join("testdir");
        fs::create_dir(&test_dir).expect("create test dir");
        let test_file = test_dir.join("test.txt");
        fs::write(&test_file, "hello world").expect("write test file");

        let symlink_path = workspace.path().join("symlinkdir");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&test_dir, &symlink_path).expect("create dir symlink");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());

        #[cfg(unix)]
        {
            let res = tool.execute(serde_json::json!({ "path": "symlinkdir/test.txt" }), &ctx);
            match res {
                Ok(_) => panic!("Expected error for symlink path, but got Ok"),
                Err(e) => {
                    let msg = e.to_string();
                    assert!(msg.contains("symbolic link"), "Unexpected error: {}", msg);
                }
            }
        }

        #[cfg(not(unix))]
        let _ = test_dir;
    }

    #[test]
    fn read_file_blocks_symlink_to_outside_workspace() {
        let workspace = tempdir().expect("workspace tempdir");
        let outside = tempdir().expect("outside tempdir");
        let outside_file = outside.path().join("outside.txt");
        fs::write(&outside_file, "secret data").expect("write outside file");

        let symlink_path = workspace.path().join("symlink.txt");
        symlink_file(&outside_file, &symlink_path).expect("create symlink");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(serde_json::json!({ "path": "symlink.txt" }), &ctx);
        match res {
            Ok(_) => panic!("Expected error for symlink path, but got Ok"),
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("symbolic link"), "Unexpected error: {}", msg);
            }
        }
    }

    #[test]
    fn read_file_blocks_parent_directory_traversal() {
        let workspace = tempdir().expect("workspace tempdir");
        let outside = tempdir().expect("outside tempdir");
        let outside_file = outside.path().join("outside.txt");
        fs::write(&outside_file, "secret").expect("write outside file");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(
            serde_json::json!({ "path": format!("../../../{}", outside_file.display()) }),
            &ctx,
        );
        assert!(res.is_err());
    }

    #[test]
    fn read_file_safe_when_end_line_precedes_start_line() {
        let workspace = tempdir().expect("workspace tempdir");
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "line1\nline2\nline3").expect("write test file");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(
            serde_json::json!({
                "path": "test.txt",
                "start_line": 3,
                "end_line": 1
            }),
            &ctx,
        );

        assert!(res.is_ok());
        let output = res.unwrap();
        assert!(output.text.is_empty() || output.text.contains("[Showing lines"));
        assert!(!output.text.contains("line1"));
        assert!(!output.text.contains("line2"));
        assert!(!output.text.contains("line3"));
    }

    #[test]
    fn read_file_binary_returns_base64_when_requested() {
        let workspace = tempdir().expect("workspace tempdir");
        let test_file = workspace.path().join("image.png");
        fs::write(&test_file, [0x89u8, 0x50, 0x4e, 0x47, 0x00, 0x01]).expect("write binary file");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(
            serde_json::json!({ "path": "image.png", "binary": true }),
            &ctx,
        );
        assert!(res.is_ok());
        let output = res.unwrap();
        assert!(!output.text.is_empty());
        let structured = output.structured.expect("structured output");
        assert!(structured["binary"].as_bool().unwrap_or(false));
        assert_eq!(structured["encoding"], "base64");
    }

    // ── detect_language tests ─────────────

    #[test]
    fn test_detect_language_rust() {
        assert_eq!(detect_language(Path::new("main.rs")), Some("rust"));
    }

    #[test]
    fn test_detect_language_python() {
        assert_eq!(detect_language(Path::new("app.py")), Some("python"));
    }

    #[test]
    fn test_detect_language_javascript() {
        assert_eq!(detect_language(Path::new("index.js")), Some("javascript"));
        assert_eq!(detect_language(Path::new("app.mjs")), Some("javascript"));
        assert_eq!(detect_language(Path::new("app.cjs")), Some("javascript"));
    }

    #[test]
    fn test_detect_language_typescript() {
        assert_eq!(detect_language(Path::new("app.ts")), Some("typescript"));
        assert_eq!(detect_language(Path::new("app.mts")), Some("typescript"));
    }

    #[test]
    fn test_detect_language_go() {
        assert_eq!(detect_language(Path::new("main.go")), Some("go"));
    }

    #[test]
    fn test_detect_language_case_insensitive() {
        assert_eq!(detect_language(Path::new("MAIN.RS")), Some("rust"));
        assert_eq!(detect_language(Path::new("App.PY")), Some("python"));
    }

    #[test]
    fn test_detect_language_unknown_extension_returns_text() {
        assert_eq!(detect_language(Path::new("config.xyz")), Some("text"));
        assert_eq!(detect_language(Path::new("data.dat")), Some("text"));
    }

    #[test]
    fn test_detect_language_no_extension_returns_none() {
        assert_eq!(detect_language(Path::new("Dockerfile")), None);
        assert_eq!(detect_language(Path::new("Makefile")), None);
    }

    // ── detect_binary tests ─────────────

    #[test]
    fn test_detect_binary_images() {
        assert!(detect_binary(Path::new("photo.png")));
        assert!(detect_binary(Path::new("photo.jpg")));
        assert!(detect_binary(Path::new("photo.jpeg")));
        assert!(detect_binary(Path::new("photo.gif")));
        assert!(detect_binary(Path::new("photo.webp")));
        assert!(detect_binary(Path::new("photo.svg")));
    }

    #[test]
    fn test_detect_binary_archives() {
        assert!(detect_binary(Path::new("archive.zip")));
        assert!(detect_binary(Path::new("archive.tar")));
        assert!(detect_binary(Path::new("archive.gz")));
        assert!(detect_binary(Path::new("archive.7z")));
    }

    #[test]
    fn test_detect_binary_executables() {
        assert!(detect_binary(Path::new("app.exe")));
        assert!(detect_binary(Path::new("lib.so")));
        assert!(detect_binary(Path::new("lib.dylib")));
    }

    #[test]
    fn test_detect_binary_not_text() {
        assert!(!detect_binary(Path::new("main.rs")));
        assert!(!detect_binary(Path::new("app.py")));
        assert!(!detect_binary(Path::new("README.md")));
        assert!(!detect_binary(Path::new("config.toml")));
    }

    #[test]
    fn test_detect_binary_no_extension() {
        assert!(!detect_binary(Path::new("Makefile")));
    }

    // ── count_comment_lines tests ─────────────

    #[test]
    fn test_count_comment_lines_rust() {
        let lines = vec!["// comment", "fn main() {", "/* block */", "* mid", "code"];
        assert_eq!(count_comment_lines(&lines, Some("rust")), 3);
    }

    #[test]
    fn test_count_comment_lines_python() {
        let lines = vec!["# comment", "def foo():", "# another", "    pass"];
        assert_eq!(count_comment_lines(&lines, Some("python")), 2);
    }

    #[test]
    fn test_count_comment_lines_json() {
        let lines = vec!["{", "\"key\": \"value\"", "}"];
        assert_eq!(count_comment_lines(&lines, Some("json")), 0);
    }

    #[test]
    fn test_count_comment_lines_markdown() {
        let lines = vec!["<!-- html comment -->", "regular text", "<!-- another -->"];
        assert_eq!(count_comment_lines(&lines, Some("markdown")), 2);
    }

    #[test]
    fn test_count_comment_lines_unknown_language() {
        let lines = vec!["line one", "line two"];
        assert_eq!(count_comment_lines(&lines, None), 0);
        assert_eq!(count_comment_lines(&lines, Some("unknown")), 0);
    }

    #[test]
    fn test_count_comment_lines_yaml() {
        let lines = vec!["# config", "key: value", "# another comment"];
        assert_eq!(count_comment_lines(&lines, Some("yaml")), 2);
    }

    // ── estimate_complexity tests ─────────────

    #[test]
    fn test_estimate_complexity_simple() {
        assert_eq!(estimate_complexity(10, 2), "simple");
        assert_eq!(estimate_complexity(49, 0), "simple");
    }

    #[test]
    fn test_estimate_complexity_medium() {
        assert_eq!(estimate_complexity(100, 20), "medium");
        assert_eq!(estimate_complexity(100, 50), "simple");
    }

    #[test]
    fn test_estimate_complexity_high() {
        assert_eq!(estimate_complexity(300, 50), "high");
    }

    #[test]
    fn test_estimate_complexity_very_high() {
        assert_eq!(estimate_complexity(600, 10), "very_high");
        assert_eq!(estimate_complexity(1000, 0), "very_high");
    }

    #[test]
    fn test_estimate_complexity_zero_lines() {
        assert_eq!(estimate_complexity(0, 0), "simple");
    }

    // ── get_last_modified tests ─────────────

    #[test]
    fn test_get_last_modified_existing_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "hello").unwrap();
        let result = get_last_modified(&file);
        assert_ne!(result, "unknown");
        assert!(
            result.parse::<u64>().is_ok(),
            "expected timestamp, got: {result}"
        );
    }

    #[test]
    fn test_get_last_modified_missing_file() {
        let result = get_last_modified(Path::new("/nonexistent/file.txt"));
        assert_eq!(result, "unknown");
    }

    // ── is_binary_content tests ─────────────

    #[test]
    fn test_binary_detection_with_null_bytes() {
        assert!(is_binary_content(b"hello\x00world"));
    }

    #[test]
    fn test_binary_detection_pure_text() {
        assert!(!is_binary_content(b"Hello, world! This is plain text."));
    }

    #[test]
    fn test_binary_detection_empty() {
        assert!(!is_binary_content(b""));
    }

    #[test]
    fn test_binary_detection_only_first_8kb_checked() {
        let mut data = vec![b'x'; 9000];
        data[8200] = 0;
        assert!(!is_binary_content(&data));
    }

    #[test]
    fn test_binary_detection_null_at_boundary() {
        let mut data = vec![b'a'; 8192];
        data[8191] = 0;
        assert!(is_binary_content(&data));
    }

    #[test]
    fn test_binary_detection_utf8_content() {
        let utf8 = "Hello 世界 🌍 Ñoño".as_bytes();
        assert!(!is_binary_content(utf8));
    }

    // ── truncate_bytes_to_boundary tests ─────────────

    #[test]
    fn test_truncate_bytes_within_bounds() {
        let bytes = b"hello";
        assert_eq!(truncate_bytes_to_boundary(bytes, 100), bytes);
    }

    #[test]
    fn test_truncate_bytes_exact() {
        let bytes = b"hello";
        assert_eq!(truncate_bytes_to_boundary(bytes, 5), bytes);
    }

    #[test]
    fn test_truncate_bytes_empty() {
        assert_eq!(truncate_bytes_to_boundary(b"", 10), b"");
    }
}
