use crate::file_formatter;
use crate::security::{
    create_file_symlink_safe, open_file_symlink_safe, validate_list_path, validate_read_path,
    validate_regex_pattern, validate_url, validate_write_path, BLOCKED_EXTENSIONS,
};

const USER_AGENT: &str = concat!("RustyCode/", env!("CARGO_PKG_VERSION"));
use crate::truncation::{truncate_items, truncate_lines, LIST_MAX_ITEMS, READ_MAX_LINES};
use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Maximum number of characters returned by `WebFetchTool` content
const WEB_FETCH_MAX_CHARS: usize = 50_000;

/// Detect if a file is likely binary based on extension
///
/// Includes security-sensitive file types that should be blocked
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

/// Check if bytes contain null bytes (strong indicator of binary content)
fn is_binary_content(data: &[u8]) -> bool {
    // Check first 8KB for null bytes — standard heuristic
    let check_len = data.len().min(8192);
    data[..check_len].contains(&0)
}

/// Check if a file extension is blocked for security reasons
fn is_blocked_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            // BLOCKED_EXTENSIONS entries include the leading dot (e.g., ".env", ".exe")
            let dotted = format!(".{}", ext.to_lowercase());
            BLOCKED_EXTENSIONS.contains(&dotted.as_str())
        })
        .unwrap_or(false)
}

/// Check if a filename is blocked for security reasons
fn is_blocked_filename(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            use crate::security::validation::BLOCKED_FILENAMES;
            BLOCKED_FILENAMES.contains(&name)
        })
        .unwrap_or(false)
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

pub struct ReadFileTool;
pub struct WriteFileTool;
pub struct ListDirTool;

/// Result of a pattern match in a file
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            .filter(|l| l.trim_start().starts_with("#"))
            .count(),
        Some("yaml" | "toml") => lines
            .iter()
            .filter(|l| l.trim_start().starts_with("#"))
            .count(),
        Some("json") => 0, // JSON doesn't support comments
        Some("markdown" | "md") => {
            // Markdown doesn't really have "comments" but we can count HTML comments
            lines
                .iter()
                .filter(|l| l.trim_start().starts_with("<!--"))
                .count()
        }
        _ => 0,
    }
}

/// Get file last modified time
fn get_last_modified(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            let duration_since_epoch = t
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            let timestamp = duration_since_epoch.as_secs();
            format!("{timestamp}")
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

impl Tool for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Read the complete contents of a text file. CRLF line endings are normalized to LF for consistent processing. Supports optional line range (offset/limit). Returns file content with language detection."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path relative to current workspace (alias: file_path)"
                },
                "file_path": {
                    "type": "string",
                    "description": "Alias for path"
                },
                "start_line": {
                    "type": "integer",
                    "description": "First line to return (1-indexed, inclusive)"
                },
                "end_line": {
                    "type": "integer",
                    "description": "Last line to return (1-indexed, inclusive)"
                },
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to filter matching lines"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Use case-insensitive regex matching",
                    "default": false
                },
                "max_matches": {
                    "type": "integer",
                    "description": "Maximum number of pattern matches to return",
                    "default": 100
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of lines to show before/after pattern matches"
                },
                "offset": {
                    "type": "integer",
                    "description": "Skip N lines before reading (for pagination)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to return"
                },
                "stats": {
                    "type": "boolean",
                    "description": "Return file statistics instead of content"
                },
                "binary": {
                    "type": "boolean",
                    "description": "Read binary files as base64 instead of blocking them"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        // Check permissions
        crate::check_permission(self.permission(), ctx)?;

        let path_str = resolve_path_str(&params)?;

        // Validate path using security module
        let path = validate_read_path(path_str, &ctx.cwd)?;

        // Validate against sandbox rules
        // If interactive mode is enabled, this will prompt the user
        // if the path is not in the allowed list
        crate::check_sandbox_path(&path, ctx)?;

        // Check for blocked extensions
        if is_blocked_extension(&path) {
            return Ok(ToolOutput::text(format!(
                "[Blocked] File extension is not allowed for security reasons: {}",
                path.extension().unwrap_or_default().to_string_lossy()
            )));
        }

        // Check for blocked filenames (credentials, SSH keys, etc.)
        if is_blocked_filename(&path) {
            return Ok(ToolOutput::text(format!(
                "[Blocked] File is not allowed for security reasons: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )));
        }

        let allow_binary = params
            .get("binary")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Check for binary file before reading
        if detect_binary(&path) && !allow_binary {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown");

            let suggestion = match ext.to_lowercase().as_str() {
                // Image files
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" => {
                    "Use an image viewer or tool to extract metadata (e.g., `file` command)"
                }
                // PDF files
                "pdf" => "Use a PDF viewer or PDF text extraction tool (e.g., `pdftotext`)",
                // Archives
                "zip" | "tar" | "gz" | "bz2" | "rar" | "7z" => {
                    "Extract the archive first or use archive inspection tools"
                }
                // Executables
                "exe" | "dll" | "so" | "dylib" => {
                    "Use binary analysis tools (e.g., `strings`, `objdump`, `nm`)"
                }
                _ => "Use a specialized tool for this file type",
            };

            return Ok(ToolOutput::with_structured(
                format!(
                    "[Binary file detected: {} (type: .{})]\n\nRecovery: {}",
                    path.display(),
                    ext,
                    suggestion
                ),
                json!({
                    "path": path.display().to_string(),
                    "extension": ext,
                    "binary": true,
                    "error": "Binary file - use appropriate tool to view this file type",
                    "recovery_hint": suggestion
                }),
            ));
        }

        if allow_binary {
            let mut f = open_file_symlink_safe(&path)?;
            let mut bytes = Vec::new();
            use std::io::Read;
            f.read_to_end(&mut bytes)?;
            let total_bytes = bytes.len();
            let preview = truncate_bytes_to_boundary(&bytes, WEB_FETCH_MAX_CHARS);
            let encoded = STANDARD.encode(preview);

            return Ok(ToolOutput::with_structured(
                encoded,
                json!({
                    "path": path.display().to_string(),
                    "binary": true,
                    "encoding": "base64",
                    "bytes": total_bytes,
                    "shown_bytes": preview.len(),
                    "content_truncated": preview.len() < total_bytes,
                }),
            ));
        }

        // Use symlink-safe file open to prevent TOCTOU attacks
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

        // Check for binary content by reading first 8KB
        let mut probe = [0u8; 8192];
        let probe_len = file.read(&mut probe)?;
        if is_binary_content(&probe[..probe_len]) {
            return Ok(ToolOutput {
                text: "File appears to be binary. Use bash with `xxd` or `hexdump` for binary inspection.".to_string(),
                structured: None,
            });
        }

        // Read the rest of the file
        let mut content = String::from_utf8_lossy(&probe[..probe_len]).into_owned();
        file.read_to_string(&mut content)?;

        // Record original byte count before normalization
        let total_bytes = content.len();

        // Normalize CRLF to LF for consistent processing and LLM context
        let (content, _line_ending) = crate::line_endings::normalize_and_detect(&content);

        let total_lines = content.lines().count();
        let path_display = path.display().to_string();

        // Check if user requested file statistics (highest priority mode)
        if params
            .get("stats")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
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

            return Ok(ToolOutput::with_structured(
                serde_json::to_string_pretty(&stats)?,
                stats,
            ));
        }

        // Check if user requested pattern matching
        if let Some(pattern_str) = params.get("pattern").and_then(|p| p.as_str()) {
            // SECURITY: Validate regex pattern to prevent ReDoS attacks
            validate_regex_pattern(pattern_str)
                .map_err(|e| anyhow!("Invalid regex pattern: {e}"))?;

            let case_insensitive = params
                .get("case_insensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let max_matches = params
                .get("max_matches")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(100);

            let context_lines = params
                .get("context_lines")
                .and_then(Value::as_u64)
                .map(|n| n as usize);

            // Build regex with case-insensitive flag if needed
            let regex = if case_insensitive {
                Regex::new(&format!("(?i){pattern_str}"))
            } else {
                Regex::new(pattern_str)
            }?;

            let lines: Vec<&str> = content.lines().collect();

            if let Some(context) = context_lines {
                // Pattern matching with context lines
                let mut matches = Vec::new();
                let lines_vec: Vec<&str> = content.lines().collect();

                for (i, line) in lines_vec.iter().enumerate() {
                    if regex.is_match(line) && matches.len() < max_matches {
                        let start_idx = i.saturating_sub(context);
                        let end_idx = (i + context + 1).min(lines_vec.len());

                        let context_lines: Vec<String> = lines_vec[start_idx..end_idx]
                            .iter()
                            .enumerate()
                            .map(|(j, l)| format!("{}: {}", start_idx + j + 1, l))
                            .collect();

                        matches.push(json!({
                            "line": i + 1,
                            "match": regex.find(line).map(|m| m.as_str()).unwrap_or(""),
                            "text": *line,
                            "context": context_lines
                        }));
                    }
                }

                return Ok(ToolOutput::with_structured(
                    format!(
                        "Found {} match(es) for pattern: {}",
                        matches.len(),
                        pattern_str
                    ),
                    json!({
                        "pattern": pattern_str,
                        "case_insensitive": case_insensitive,
                        "total_matches": matches.len(),
                        "matches": matches
                    }),
                ));
            } else {
                // Simple pattern matching without context
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

                return Ok(ToolOutput::with_structured(
                    matches
                        .iter()
                        .map(|m| format!("Line {}: {}", m.line, m.text))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    json!({
                        "pattern": pattern_str,
                        "case_insensitive": case_insensitive,
                        "total_matches": matches.len(),
                        "matches": matches
                    }),
                ));
            }
        }

        // Check for pagination (offset/limit)
        let offset = params
            .get("offset")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(0);

        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize);

        if offset > 0 || limit.is_some() {
            let lines: Vec<&str> = content.lines().collect();
            let max_limit = limit.unwrap_or(READ_MAX_LINES);

            let paginated_lines: Vec<&str> =
                lines.into_iter().skip(offset).take(max_limit).collect();

            let text = paginated_lines.join("\n");
            let text_bytes = text.len();
            let shown_lines = paginated_lines.len();

            return Ok(ToolOutput::with_structured(
                text,
                json!({
                    "path": path_display,
                    "bytes": text_bytes,
                    "total_lines": total_lines,
                    "shown_lines": shown_lines,
                    "offset": offset,
                    "limit": limit,
                    "binary": false
                }),
            ));
        }

        // Check if user requested specific line range
        let start = params
            .get("start_line")
            .and_then(Value::as_u64)
            .map(|n| n as usize);
        let end = params
            .get("end_line")
            .and_then(Value::as_u64)
            .map(|n| n as usize);

        // If user specified line range, honor it exactly without truncation
        if start.is_some() || end.is_some() {
            let lines: Vec<&str> = content.lines().collect();
            let s = start.unwrap_or(1).saturating_sub(1);
            let e = end.unwrap_or(total_lines).min(total_lines);
            let s = s.min(total_lines);
            let e = e.max(s).min(total_lines);
            let text = lines[s..e].join("\n");

            return Ok(ToolOutput::with_structured(
                text,
                json!({
                    "path": path.display().to_string(),
                    "bytes": content.len(),
                    "total_lines": total_lines,
                    "shown_lines": e - s,
                    "binary": false,
                }),
            ));
        }

        // Otherwise, apply smart truncation
        let path_display = path.display().to_string();
        let truncated = truncate_lines(&content, READ_MAX_LINES, &path_display, total_lines);

        // Extract output text before consuming truncated
        let output_text = truncated.as_str().to_string();

        // Enhance metadata
        let mut metadata = truncated.into_metadata();
        metadata["path"] = json!(path_display);
        metadata["total_bytes"] = json!(total_bytes);

        // Add content hash for caching and change detection
        // Using SHA-256 instead of MD5 for better security properties
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash =
            hasher
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

        // Add language detection
        if let Some(language) = detect_language(&path) {
            metadata["language"] = json!(language);
        }

        Ok(ToolOutput::with_structured(output_text, metadata))
    }
}

impl Tool for WriteFileTool {
    fn name(&self) -> &'static str {
        "write_file"
    }

    fn description(&self) -> &'static str {
        "Write UTF-8 text to a file. Creates parent directories if needed. Set append=true to add content to the end of an existing file (useful for writing large files in multiple chunks). Returns a diff showing what changed vs the previous file content."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path relative to workspace root (alias: file_path). Parent directories are created automatically."
                },
                "file_path": {
                    "type": "string",
                    "description": "Alias for path"
                },
                "content": {
                    "type": "string",
                    "description": "UTF-8 text content to write. Completely replaces any existing file content unless append=true."
                },
                "content_base64": {
                    "type": "string",
                    "description": "Base64-encoded binary content to write. Use this for binary files."
                },
                "append": {
                    "type": "boolean",
                    "description": "If true, append content to the end of the existing file instead of overwriting. Use for writing large files in multiple chunks."
                }
            },
            "oneOf": [
                { "required": ["content"] },
                { "required": ["content_base64"] }
            ]
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }

        crate::check_permission(self.permission(), ctx)?;

        let path_str = resolve_path_str(&params)?;
        let text_content = optional_string(&params, "content");
        let binary_content = optional_string(&params, "content_base64");
        let append = params
            .get("append")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if text_content.is_some() && binary_content.is_some() {
            return Err(anyhow!(
                "use either `content` or `content_base64`, not both"
            ));
        }
        if append && binary_content.is_some() {
            return Err(anyhow!("append mode is not supported for binary content"));
        }
        let binary_bytes = if let Some(encoded) = binary_content {
            Some(
                STANDARD
                    .decode(encoded)
                    .map_err(|e| anyhow!("invalid base64 content: {e}"))?,
            )
        } else {
            None
        };
        let content = text_content.unwrap_or("");

        let write_size = binary_bytes
            .as_ref()
            .map(|b: &Vec<u8>| b.len())
            .unwrap_or(content.len());
        let path = validate_write_path(path_str, &ctx.cwd, write_size)?;

        crate::check_sandbox_path(&path, ctx)?;

        if is_blocked_extension(&path) {
            return Err(anyhow::anyhow!(
                "File extension is blocked for writing: {}",
                path.extension().unwrap_or_default().to_string_lossy()
            ));
        }

        if is_blocked_filename(&path) {
            return Err(anyhow::anyhow!(
                "File is blocked for writing: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let path_display = path.display().to_string();

        if append {
            return Self::execute_append(&path, content, write_size, path_display, ctx);
        }

        // === Non-append (overwrite) mode below ===
        let old_content = if binary_bytes.is_some() {
            Vec::new()
        } else if let Ok(mut f) = open_file_symlink_safe(&path) {
            use std::io::Read;
            let mut buf = String::new();
            let bytes_read = f.read_to_string(&mut buf).unwrap_or_else(|e| {
                tracing::debug!("Failed to read existing file for diff: {}", e);
                0
            });
            if bytes_read == 0 && !buf.is_empty() {
                tracing::debug!(
                    "Read returned 0 bytes but buffer is non-empty for {}",
                    path.display()
                );
            }
            buf.into_bytes()
        } else {
            Vec::new()
        };

        let mut file = create_file_symlink_safe(&path)?;
        use std::io::Write;
        if let Some(bytes) = binary_bytes.as_ref() {
            file.write_all(bytes)?;
        } else {
            file.write_all(content.as_bytes())?;
        }
        file.sync_all()?;

        let bytes = write_size;
        let lines = content.lines().count();

        let diff = if binary_bytes.is_some() {
            format!("Wrote binary file ({bytes} bytes)")
        } else if old_content.is_empty() {
            format!("Created new file ({bytes} bytes, {lines} lines)")
        } else {
            let old_text = String::from_utf8_lossy(&old_content);
            crate::line_endings::generate_diff(&old_text, content, &path_display, 50)
        };

        let mut output_text =
            format!("wrote {path_display} ({bytes} bytes, {lines} lines)\n{diff}");

        if binary_bytes.is_none() {
            if let Some(formatter_diff) = file_formatter::format_file(&path, &ctx.cwd) {
                output_text.push_str(&formatter_diff);
            }
        }

        Ok(ToolOutput::with_structured(
            output_text,
            json!({
                "path": path_display,
                "bytes": bytes,
                "lines": lines
            }),
        ))
    }
}

impl WriteFileTool {
    fn execute_append(
        path: &Path,
        content: &str,
        appended_bytes: usize,
        path_display: String,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        use std::io::Write;

        let existed = path.exists();
        let old_size = if existed {
            fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .with_context(|| format!("Failed to open {path_display} for append"))?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;

        let new_metadata = fs::metadata(path)?;
        let total_bytes = new_metadata.len();
        let total_lines = fs::read_to_string(path)
            .map(|s| s.lines().count())
            .unwrap_or(0);

        let summary = if existed {
            format!(
                "appended {appended_bytes} bytes to {path_display} (total: {total_bytes} bytes, {total_lines} lines)"
            )
        } else {
            format!(
                "created and wrote {appended_bytes} bytes to {path_display} ({total_lines} lines)"
            )
        };

        let mut output_text = summary;

        if let Some(formatter_diff) = file_formatter::format_file(path, &ctx.cwd) {
            output_text.push_str(&formatter_diff);
        }

        Ok(ToolOutput::with_structured(
            output_text,
            json!({
                "path": path_display,
                "appended_bytes": appended_bytes,
                "total_bytes": total_bytes,
                "total_lines": total_lines,
                "old_size": old_size,
            }),
        ))
    }
}

impl Tool for ListDirTool {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn description(&self) -> &'static str {
        "List all files and directories in a specified path. Use this to explore the codebase structure, find files in a directory, or see what's in a folder. Supports recursive listing and filtering by file type or extension."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path relative to current workspace (alias: file_path)"
                },
                "file_path": {
                    "type": "string",
                    "description": "Alias for path"
                },
                "recursive": { "type": "boolean", "description": "List directories recursively" },
                "max_depth": { "type": "integer", "description": "Maximum depth for recursive listing" },
                "filter": {
                    "type": "string",
                    "description": "Filter entries by type (file/dir/all) or extension (e.g., '.rs', '.md')"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }
        let path_str = optional_string(&params, "path").unwrap_or(".");

        // Validate path using security module
        let path = validate_list_path(path_str, &ctx.cwd)?;

        let path_display = path.display().to_string();
        let recursive = params
            .get("recursive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let max_depth = params.get("max_depth").and_then(Value::as_u64).unwrap_or(3) as usize;
        let filter = params.get("filter").and_then(|v| v.as_str());

        let mut entries = Vec::new();

        if recursive {
            // Use WalkDir for recursive listing
            for entry in WalkDir::new(&path)
                .max_depth(max_depth)
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                let file_type = entry.file_type();
                let kind = if file_type.is_dir() {
                    "dir"
                } else if file_type.is_file() {
                    "file"
                } else {
                    "other"
                };

                // Apply filter if specified
                if let Some(filter_str) = filter {
                    // Filter by type (file/dir/all)
                    if matches!(filter_str.to_lowercase().as_str(), "file" | "dir" | "all") {
                        if filter_str != "all" && kind != filter_str {
                            continue;
                        }
                    }
                    // Filter by extension
                    else if filter_str.starts_with('.')
                        && !entry.path().to_string_lossy().ends_with(filter_str)
                    {
                        continue;
                    }
                }

                let relative_path = entry
                    .path()
                    .strip_prefix(&path)
                    .unwrap_or(entry.path())
                    .display()
                    .to_string();
                entries.push(format!("{relative_path}: {kind}"));
            }
        } else {
            // Non-recursive: only direct children
            let mut dir_entries = fs::read_dir(&path)?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let file_type = entry.file_type().ok();
                    let kind = match file_type {
                        Some(ft) if ft.is_dir() => "dir",
                        Some(ft) if ft.is_file() => "file",
                        _ => "other",
                    };

                    // Apply filter if specified
                    if let Some(filter_str) = filter {
                        // Filter by type
                        if matches!(filter_str.to_lowercase().as_str(), "file" | "dir" | "all") {
                            if filter_str != "all" && kind != filter_str {
                                return None;
                            }
                        }
                        // Filter by extension
                        else if filter_str.starts_with('.')
                            && !entry.file_name().to_string_lossy().ends_with(filter_str)
                        {
                            return None;
                        }
                    }

                    Some(format!("{}: {}", entry.file_name().to_string_lossy(), kind))
                })
                .collect::<Vec<_>>();
            entries.append(&mut dir_entries);
        }

        let total_count = entries.len();
        entries.sort();

        // Apply truncation
        let truncated = truncate_items(entries, LIST_MAX_ITEMS, &path_display);

        // Group by type for dense display
        let output_text = format!(
            "**{}** ({} items{})\n\n{}",
            path_display,
            total_count,
            if recursive {
                format!(", recursive (depth={max_depth})")
            } else {
                String::new()
            },
            truncated.as_str()
        );

        // Build metadata
        let mut metadata = truncated.into_metadata();
        metadata["path"] = json!(path_display);
        metadata["total_items"] = json!(total_count);
        metadata["recursive"] = json!(recursive);
        if recursive {
            metadata["max_depth"] = json!(max_depth);
        }
        if let Some(filter_str) = filter {
            metadata["filter"] = json!(filter_str);
        }

        Ok(ToolOutput::with_structured(output_text, metadata))
    }
}

/// Check if content appears to be HTML
fn is_html_content(content: &str) -> bool {
    let trimmed = content.trim().to_lowercase();
    trimmed.starts_with("<!doctype")
        || trimmed.starts_with("<html")
        || (trimmed.starts_with("<") && trimmed.contains("xmlns="))
}

/// Convert HTML to markdown using a proper HTML parser.
/// This uses the `html2md` crate for more robust handling of real‑world HTML.
fn html_to_simple_markdown(html: &str) -> String {
    let markdown = html2md::parse_html(html);
    markdown.trim().to_string()
}

fn truncate_to_char_boundary(content: &str, max_chars: usize) -> &str {
    if content.len() <= max_chars {
        return content;
    }

    let mut end = max_chars;
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    &content[..end]
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

pub struct WebFetchTool;

impl Tool for WebFetchTool {
    fn name(&self) -> &'static str {
        "web_fetch"
    }

    fn description(&self) -> &'static str {
        "Fetch and read content from a web page or PDF. Use this to read documentation, blog posts, GitHub files, or online articles."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from (e.g., 'https://docs.anthropic.com', 'https://github.com/user/repo/blob/main/README.md')"
                },
                "convert_markdown": {
                    "type": "boolean",
                    "description": "Convert HTML to simplified markdown format"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Role-based gating
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, self.name())?;
        }
        let url = required_string(&params, "url")?;

        // Validate URL for security
        validate_url(url)?;

        let convert_markdown = params
            .get("convert_markdown")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Track execution time
        let start_time = std::time::Instant::now();

        // Use blocking reqwest for simplicity in tool context
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()?;

        let response = client.get(url).send()?;
        let time_to_first_byte = start_time.elapsed();

        let status_code = response.status().as_u16();

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP error {}: {}",
                response.status(),
                response
                    .text()
                    .unwrap_or_else(|_| "unable to read error".to_string())
            ));
        }

        // Extract response headers for metadata
        let headers_map = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_string(), v.to_string()))
            })
            .collect::<std::collections::HashMap<String, String>>();

        let content = response.text()?;
        let total_time = start_time.elapsed();

        // Convert HTML to simplified markdown if requested
        let (content, converted) = if convert_markdown && is_html_content(&content) {
            (html_to_simple_markdown(&content), true)
        } else {
            (content, false)
        };

        // Truncate content if too large (limit to ~50k chars to avoid overwhelming context)
        let (content, truncated) = if content.len() > WEB_FETCH_MAX_CHARS {
            (
                truncate_to_char_boundary(&content, WEB_FETCH_MAX_CHARS),
                true,
            )
        } else {
            (&content[..], false)
        };

        let output = if truncated {
            format!("{content}\n\n[Content truncated at {WEB_FETCH_MAX_CHARS} characters]")
        } else {
            content.to_string()
        };

        // Build enhanced metadata with headers and timing
        let mut metadata = json!({
            "url": url,
            "chars": content.len(),
            "truncated": truncated,
            "converted": converted,
            "status_code": status_code,
            "time_to_first_byte_ms": time_to_first_byte.as_millis(),
            "total_time_ms": total_time.as_millis(),
        });

        // Add headers to metadata
        if !headers_map.is_empty() {
            metadata["headers"] = json!(headers_map);
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

/// Resolve a path-like parameter, accepting both `path` and `file_path` keys.
fn resolve_path_str(value: &Value) -> Result<&str> {
    value
        .get("path")
        .or_else(|| value.get("file_path"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string parameter `path` or `file_path`"))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let res = tool.execute(json!({ "path": outside_file.display().to_string() }), &ctx);
        match res {
            Ok(_) => panic!("Expected error for outside workspace path, but got Ok"),
            Err(e) => {
                let msg = e.to_string();
                // The error message indicates the path is not within workspace
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
        let res = tool.execute(json!({ "path": "test.txt" }), &ctx);
        assert!(res.is_ok());
        let output = res.unwrap();
        assert_eq!(output.text, "hello world");
    }

    #[test]
    fn read_file_blocks_symlink_to_file_inside_workspace() {
        let workspace = tempdir().expect("workspace tempdir");
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "hello world").expect("write test file");

        // Create a symlink inside the workspace
        let symlink_path = workspace.path().join("symlink.txt");
        symlink_file(&test_file, &symlink_path).expect("create symlink");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(json!({ "path": "symlink.txt" }), &ctx);
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

        // Create a symlink to the directory
        let symlink_path = workspace.path().join("symlinkdir");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&test_dir, &symlink_path).expect("create dir symlink");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());

        #[cfg(unix)]
        {
            let res = tool.execute(json!({ "path": "symlinkdir/test.txt" }), &ctx);
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

        // Create a symlink inside workspace pointing outside
        let symlink_path = workspace.path().join("symlink.txt");
        symlink_file(&outside_file, &symlink_path).expect("create symlink");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(json!({ "path": "symlink.txt" }), &ctx);
        match res {
            Ok(_) => panic!("Expected error for symlink path, but got Ok"),
            Err(e) => {
                let msg = e.to_string();
                // Should be blocked because it's a symlink, regardless of target
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

        // Try to access file via parent traversal
        let res = tool.execute(
            json!({ "path": format!("../../../{}", outside_file.display()) }),
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
            json!({
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
        fs::write(&test_file, [0x89, 0x50, 0x4e, 0x47, 0x00, 0x01]).expect("write binary file");

        let tool = ReadFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(json!({ "path": "image.png", "binary": true }), &ctx);
        assert!(res.is_ok());
        let output = res.unwrap();
        assert!(!output.text.is_empty());
        let structured = output.structured.expect("structured output");
        assert!(structured["binary"].as_bool().unwrap_or(false));
        assert_eq!(structured["encoding"], "base64");
    }

    #[test]
    fn write_file_supports_base64_binary() {
        let workspace = tempdir().expect("workspace tempdir");
        let tool = WriteFileTool;
        let ctx = ToolContext::new(workspace.path());

        let bytes = [0x89, 0x50, 0x4e, 0x47, 0x00, 0x01];
        let encoded = STANDARD.encode(bytes);
        let res = tool.execute(
            json!({
                "path": "out.dat",
                "content_base64": encoded
            }),
            &ctx,
        );
        assert!(res.is_ok(), "write failed: {:?}", res.err());
        let written = fs::read(workspace.path().join("out.dat")).expect("read written binary");
        assert_eq!(written, bytes);
    }

    #[test]
    fn truncate_to_char_boundary_keeps_utf8_valid() {
        let content = "é".repeat(10) + "abc";
        let truncated = truncate_to_char_boundary(&content, 3);
        assert!(truncated.is_char_boundary(truncated.len()));
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    #[test]
    fn write_file_blocks_symlink() {
        let workspace = tempdir().expect("workspace tempdir");
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "original").expect("write test file");

        // Create a symlink inside the workspace
        let symlink_path = workspace.path().join("symlink.txt");
        symlink_file(&test_file, &symlink_path).expect("create symlink");

        let tool = WriteFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(
            json!({ "path": "symlink.txt", "content": "modified" }),
            &ctx,
        );
        match res {
            Ok(_) => panic!("Expected error for symlink path, but got Ok"),
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("symbolic link"), "Unexpected error: {}", msg);
            }
        }
    }

    #[test]
    fn list_dir_blocks_symlink() {
        let workspace = tempdir().expect("workspace tempdir");
        let outside = tempdir().expect("outside tempdir");

        // Create a symlink inside workspace pointing to outside directory
        let symlink_path = workspace.path().join("symlinkdir");

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &symlink_path).expect("create dir symlink");

        let tool = ListDirTool;
        let ctx = ToolContext::new(workspace.path());

        #[cfg(unix)]
        {
            let res = tool.execute(json!({ "path": "symlinkdir" }), &ctx);
            match res {
                Ok(_) => panic!("Expected error for symlink path, but got Ok"),
                Err(e) => {
                    let msg = e.to_string();
                    assert!(msg.contains("symbolic link"), "Unexpected error: {}", msg);
                }
            }
        }
    }

    #[test]
    fn write_file_normal_path_works() {
        let workspace = tempdir().expect("workspace tempdir");

        let tool = WriteFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(
            json!({ "path": "newfile.txt", "content": "test content" }),
            &ctx,
        );
        assert!(res.is_ok());
    }

    // ============================================================================
    // WebFetchTool Tests
    // ============================================================================

    #[test]
    fn test_web_fetch_tool_metadata() {
        let tool = WebFetchTool;
        assert_eq!(tool.name(), "web_fetch");
        assert_eq!(
            tool.description(),
            "Fetch and read content from a web page or PDF. Use this to read documentation, blog posts, GitHub files, or online articles."
        );
        assert_eq!(tool.permission(), ToolPermission::Read);
    }

    #[test]
    fn test_web_fetch_parameters_schema() {
        let tool = WebFetchTool;
        let schema = tool.parameters_schema();

        assert_eq!(schema["type"], "object");
        assert!(schema["required"].is_array());
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "url");

        // Check url property
        assert_eq!(schema["properties"]["url"]["type"], "string");
        assert!(schema["properties"]["url"]["description"].is_string());

        // Check convert_markdown property (optional)
        assert_eq!(schema["properties"]["convert_markdown"]["type"], "boolean");
        assert!(schema["properties"]["convert_markdown"]["description"].is_string());
    }

    #[test]
    fn test_web_fetch_missing_required_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("url"));
    }

    #[test]
    fn test_web_fetch_blocks_file_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "file:///etc/passwd" }), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed"));
    }

    #[test]
    fn test_web_fetch_blocks_missing_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "example.com" }), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("scheme"));
    }

    #[test]
    fn test_web_fetch_blocks_ftp_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "ftp://example.com/file.txt" }), &ctx);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("only http:// and https://"));
    }

    #[test]
    fn test_web_fetch_allows_http_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        // This will fail due to network/mock, but URL validation should pass
        let result = tool.execute(json!({ "url": "http://example.com" }), &ctx);
        // Should get past URL validation but fail on HTTP request
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_allows_https_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        // This will fail due to network/mock, but URL validation should pass
        let result = tool.execute(json!({ "url": "https://example.com" }), &ctx);
        // Should get past URL validation but fail on HTTP request
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_convert_markdown_default() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        // Test that convert_markdown defaults to false when not provided
        let result = tool.execute(json!({ "url": "https://example.com" }), &ctx);

        // Will fail on network, but parameter parsing should work
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("missing"));
            assert!(!err_msg.contains("parameter"));
        }
    }

    #[test]
    fn test_web_fetch_convert_markdown_explicit_false() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "url": "https://example.com",
                "convert_markdown": false
            }),
            &ctx,
        );

        // Will fail on network, but parameter parsing should work
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("missing"));
            assert!(!err_msg.contains("parameter"));
        }
    }

    #[test]
    fn test_web_fetch_convert_markdown_explicit_true() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({
                "url": "https://example.com",
                "convert_markdown": true
            }),
            &ctx,
        );

        // Will fail on network, but parameter parsing should work
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("missing"));
            assert!(!err_msg.contains("parameter"));
        }
    }

    #[test]
    fn test_web_fetch_url_case_insensitive() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        // Test uppercase HTTP (should be allowed)
        let result = tool.execute(json!({ "url": "HTTP://EXAMPLE.COM" }), &ctx);

        // URL validation should be case-insensitive
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("only http:// and https://"));
        }
    }

    #[test]
    fn test_web_fetch_empty_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "" }), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_web_fetch_url_with_fragment() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "https://example.com#section" }), &ctx);

        // URL with fragment should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_url_with_query_params() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            json!({ "url": "https://example.com?query=value&other=123" }),
            &ctx,
        );

        // URL with query params should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_url_with_port() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "https://example.com:8443/path" }), &ctx);

        // URL with port should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_url_with_ipv4() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "https://192.168.1.1/path" }), &ctx);

        // IPv4 URL should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_url_with_localhost() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({ "url": "http://localhost:3000/api" }), &ctx);

        // localhost URL should pass validation
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(!err_msg.contains("scheme"));
            assert!(!err_msg.contains("not allowed"));
        }
    }

    #[test]
    fn test_web_fetch_invalid_url_type() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");

        // Pass number instead of string
        let result = tool.execute(json!({ "url": 12345 }), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_html_content_doctype() {
        assert!(is_html_content(
            "<!DOCTYPE html><html><body>Test</body></html>"
        ));
    }

    #[test]
    fn test_is_html_content_html_tag() {
        assert!(is_html_content(
            "<html><head><title>Test</title></head><body>Content</body></html>"
        ));
    }

    #[test]
    fn test_is_html_content_xmlns() {
        assert!(is_html_content(
            "<div xmlns='http://www.w3.org/1999/xhtml'>Content</div>"
        ));
    }

    #[test]
    fn test_is_html_content_false_plain_text() {
        assert!(!is_html_content("Just plain text"));
    }

    #[test]
    fn test_is_html_content_false_json() {
        assert!(!is_html_content("{\"key\": \"value\"}"));
    }

    #[test]
    fn test_is_html_content_case_insensitive() {
        assert!(is_html_content(
            "<!DOCTYPE HTML>\n<HTML><BODY>Test</BODY></HTML>"
        ));
    }

    #[test]
    fn test_is_html_content_with_whitespace() {
        assert!(is_html_content(
            "  \n  <!DOCTYPE html>\n  <html>Test</html>  \n"
        ));
    }

    #[test]
    fn test_html_to_simple_markdown_basic() {
        let html = "<html><body><h1>Title</h1><p>Paragraph</p></body></html>";
        let markdown = html_to_simple_markdown(html);
        assert!(!markdown.is_empty());
        assert!(markdown.contains("Title") || markdown.contains("Paragraph"));
    }

    #[test]
    fn test_html_to_simple_markdown_trims_whitespace() {
        let html = "  \n  <html><body>Content</body></html>  \n  ";
        let markdown = html_to_simple_markdown(html);
        assert_eq!(markdown, markdown.trim());
    }

    #[test]
    fn test_html_to_simple_markdown_handles_empty() {
        let html = "";
        let markdown = html_to_simple_markdown(html);
        assert!(markdown.is_empty());
    }

    #[test]
    fn test_web_fetch_max_chars_constant() {
        // Verify the constant is set to expected value
        assert_eq!(WEB_FETCH_MAX_CHARS, 50_000);
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
        // Extensions not in the match table map to "text"
        assert_eq!(detect_language(Path::new("config.xyz")), Some("text"));
        assert_eq!(detect_language(Path::new("data.dat")), Some("text"));
    }

    #[test]
    fn test_detect_language_no_extension_returns_none() {
        // No extension at all → None
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

    // ── is_blocked_extension tests ─────────────

    #[test]
    fn test_is_blocked_extension_env() {
        // ".env" as a filename has no extension (hidden file)
        assert!(!is_blocked_extension(Path::new(".env")));
        // "local.env" has extension "env" → matches ".env" in BLOCKED_EXTENSIONS
        assert!(is_blocked_extension(Path::new("local.env")));
    }

    #[test]
    fn test_is_blocked_extension_secrets() {
        assert!(is_blocked_extension(Path::new("id_rsa.key")));
        assert!(is_blocked_extension(Path::new("cert.pem")));
        assert!(is_blocked_extension(Path::new("app.exe")));
        assert!(is_blocked_extension(Path::new("lib.so")));
        assert!(is_blocked_extension(Path::new("lib.dylib")));
    }

    #[test]
    fn test_is_blocked_extension_not_text() {
        assert!(!is_blocked_extension(Path::new("main.rs")));
        assert!(!is_blocked_extension(Path::new("app.py")));
        assert!(!is_blocked_extension(Path::new("README.md")));
    }

    #[test]
    fn test_is_blocked_filename_credentials() {
        assert!(is_blocked_filename(Path::new("credentials.json")));
        assert!(is_blocked_filename(Path::new(".credentials.json")));
        assert!(is_blocked_filename(Path::new("id_rsa")));
        assert!(is_blocked_filename(Path::new("id_ed25519")));
        assert!(is_blocked_filename(Path::new("terraform.tfstate")));
    }

    #[test]
    fn test_is_blocked_filename_not_blocked() {
        assert!(!is_blocked_filename(Path::new("package.json")));
        assert!(!is_blocked_filename(Path::new("Cargo.toml")));
        assert!(!is_blocked_filename(Path::new("main.rs")));
        assert!(!is_blocked_filename(Path::new("README.md")));
    }

    #[test]
    fn test_is_blocked_filename_nested_path() {
        // is_blocked_filename only checks the final component
        assert!(is_blocked_filename(Path::new("/home/user/.ssh/id_rsa")));
        assert!(!is_blocked_filename(Path::new("/home/user/src/main.rs")));
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
        // 100 lines, 20 comments → 80% code ratio, < 200 lines → "medium"
        assert_eq!(estimate_complexity(100, 20), "medium");
        // 100 lines, 50 comments → 50% code ratio, < 200 lines → "simple"
        assert_eq!(estimate_complexity(100, 50), "simple");
    }

    #[test]
    fn test_estimate_complexity_high() {
        // 300 lines, < 40% comments → high
        assert_eq!(estimate_complexity(300, 50), "high"); // 250 code lines, 83% ratio
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

    // ── truncate_to_char_boundary tests ─────────────

    #[test]
    fn test_truncate_to_char_boundary_ascii() {
        let content = "hello world";
        assert_eq!(truncate_to_char_boundary(content, 5), "hello");
    }

    #[test]
    fn test_truncate_to_char_boundary_within_bounds() {
        let content = "hello";
        assert_eq!(truncate_to_char_boundary(content, 100), "hello");
    }

    #[test]
    fn test_truncate_to_char_boundary_empty() {
        assert_eq!(truncate_to_char_boundary("", 10), "");
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

    // ── required_string / optional_string tests ─────────────

    #[test]
    fn test_required_string_present() {
        let v = json!({"name": "alice"});
        assert_eq!(required_string(&v, "name").unwrap(), "alice");
    }

    #[test]
    fn test_required_string_missing() {
        let v = json!({"age": 30});
        assert!(required_string(&v, "name").is_err());
    }

    #[test]
    fn test_required_string_wrong_type() {
        let v = json!({"name": 42});
        assert!(required_string(&v, "name").is_err());
    }

    #[test]
    fn test_optional_string_present() {
        let v = json!({"name": "alice"});
        assert_eq!(optional_string(&v, "name"), Some("alice"));
    }

    #[test]
    fn test_optional_string_missing() {
        let v = json!({"age": 30});
        assert_eq!(optional_string(&v, "name"), None);
    }

    #[test]
    fn test_resolve_path_str_prefers_path() {
        let v = json!({"path": "/a/b", "file_path": "/x/y"});
        assert_eq!(resolve_path_str(&v).unwrap(), "/a/b");
    }

    #[test]
    fn test_resolve_path_str_falls_back_to_file_path() {
        let v = json!({"file_path": "/x/y"});
        assert_eq!(resolve_path_str(&v).unwrap(), "/x/y");
    }

    #[test]
    fn test_resolve_path_str_missing_both() {
        let v = json!({"command": "ls"});
        assert!(resolve_path_str(&v).is_err());
    }

    #[test]
    fn test_optional_string_wrong_type() {
        let v = json!({"name": 42});
        assert_eq!(optional_string(&v, "name"), None);
    }

    // ── get_last_modified tests ─────────────

    #[test]
    fn test_get_last_modified_existing_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "hello").unwrap();
        let result = get_last_modified(&file);
        assert_ne!(result, "unknown");
        // Should be a numeric timestamp
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
        // Null byte after 8KB boundary should not trigger detection
        data[8200] = 0;
        assert!(!is_binary_content(&data));
    }

    #[test]
    fn test_binary_detection_null_at_boundary() {
        let mut data = vec![b'a'; 8192];
        // Null byte at position 8191 (within 8KB check) should trigger
        data[8191] = 0;
        assert!(is_binary_content(&data));
    }

    #[test]
    fn test_binary_detection_utf8_content() {
        let utf8 = "Hello 世界 🌍 Ñoño".as_bytes();
        assert!(!is_binary_content(utf8));
    }
}
