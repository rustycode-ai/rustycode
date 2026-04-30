# Tool Metadata Reference

**Last Updated:** 2025-03-14

This document provides a comprehensive reference for all metadata fields returned by rustycode tools. Metadata is structured JSON data that provides additional context about tool execution results.

## Table of Contents

- [Common Metadata Fields](#common-metadata-fields)
- [File Operation Metadata](#file-operation-metadata)
- [Search Tool Metadata](#search-tool-metadata)
- [Command Execution Metadata](#command-execution-metadata)
- [Web Fetch Metadata](#web-fetch-metadata)
- [Version Control Metadata](#version-control-metadata)
- [LSP Tool Metadata](#lsp-tool-metadata)

## Common Metadata Fields

Fields that may appear across multiple tools:

| Field | Type | Description |
|-------|------|-------------|
| `truncated` | boolean | Whether output was truncated due to size limits |
| `source` | string | Source of the data (file path, directory, etc.) |
| `total_*` | number | Total count (lines, bytes, items, matches) |
| `shown_*` | number | Count actually shown in output |
| `omitted_*` | number | Count omitted due to truncation |

## File Operation Metadata

### ReadFileTool (`read_file`)

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Absolute path to the file |
| `total_bytes` | number | Total file size in bytes |
| `shown_bytes` | number | Bytes included in output |
| `total_lines` | number | Total line count in file |
| `shown_lines` | number | Lines included in output |
| `binary` | boolean | Whether file was detected as binary |
| `content_hash` | string | SHA-256 hash of file content (for caching) |
| `language` | string | Detected programming language (if applicable) |
| `error` | string | Error message if operation failed |
| `extension` | string | File extension (for binary files) |
| `recovery_hint` | string | Suggested recovery action (for binary files) |

**Example:**
```json
{
  "path": "/Users/dev/project/src/main.rs",
  "total_bytes": 4521,
  "shown_bytes": 4521,
  "total_lines": 127,
  "shown_lines": 127,
  "binary": false,
  "content_hash": "a1b2c3d4e5f6...",
  "language": "rust",
  "truncated": false
}
```

**Binary File Response:**
```json
{
  "path": "/Users/dev/project/image.png",
  "extension": "png",
  "binary": true,
  "error": "Binary file - use appropriate tool to view this file type",
  "recovery_hint": "Use an image viewer or tool to extract metadata (e.g., `file` command)"
}
```

### WriteFileTool (`write_file`)

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Absolute path to the file |
| `bytes` | number | Number of bytes written |
| `lines` | number | Number of lines written |

**Example:**
```json
{
  "path": "/Users/dev/project/src/new_file.rs",
  "bytes": 1234,
  "lines": 42
}
```

### ListDirTool (`list_dir`)

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Directory path listed |
| `total_items` | number | Total entries in directory |
| `recursive` | boolean | Whether listing was recursive |
| `max_depth` | number | Maximum depth for recursive listing |
| `filter` | string | Filter applied (if any) |

**Example:**
```json
{
  "path": "/Users/dev/project/src",
  "total_items": 156,
  "recursive": true,
  "max_depth": 3,
  "filter": ".rs",
  "truncated": false
}
```

## Search Tool Metadata

### GrepTool (`grep`)

| Field | Type | Description |
|-------|------|-------------|
| `pattern` | string | Regex pattern searched for |
| `total_matches` | number | Total matches found |
| `files_with_matches` | number | Number of files containing matches |
| `top_files` | array | Up to 10 files with most matches |
| `before_context` | number | Lines of context before each match |
| `after_context` | number | Lines of context after each match |
| `max_matches_per_file` | number | Limit on matches per file (if set) |

**top_files array format:**
```json
[
  {"path": "src/main.rs", "matches": 23},
  {"path": "src/lib.rs", "matches": 15}
]
```

**Full Example:**
```json
{
  "pattern": "async fn",
  "total_matches": 42,
  "files_with_matches": 5,
  "top_files": [
    {"path": "src/runtime.rs", "matches": 18},
    {"path": "src/api.rs", "matches": 12}
  ],
  "truncated": false
}
```

### GlobTool (`glob`)

| Field | Type | Description |
|-------|------|-------------|
| `pattern` | string | Search pattern used |
| `total_matches` | number | Total files matching pattern |
| `extensions` | array | Breakdown by file extension |

**extensions array format:**
```json
[
  {"extension": "rs", "count": 42},
  {"extension": "toml", "count": 5},
  {"extension": "(no extension)", "count": 2}
]
```

**Full Example:**
```json
{
  "pattern": "*.rs",
  "total_matches": 47,
  "extensions": [
    {"extension": "rs", "count": 47}
  ],
  "truncated": false
}
```

## Command Execution Metadata

### BashTool (`bash`)

| Field | Type | Description |
|-------|------|-------------|
| `exit_code` | number | Process exit code (0 = success) |
| `command` | string | The command that was executed |
| `execution_time_ms` | number | Execution time in milliseconds |
| `timeout_secs` | number | Timeout limit in seconds |
| `failed` | boolean | True if exit_code != 0 |
| `total_lines` | number | Total lines of output |
| `total_bytes` | number | Total bytes of output |
| `truncated` | boolean | Whether output was truncated |

**Example:**
```json
{
  "exit_code": 0,
  "command": "cargo test",
  "execution_time_ms": 4231,
  "timeout_secs": 30,
  "failed": false,
  "total_lines": 45,
  "total_bytes": 12345,
  "truncated": false
}
```

**Failed Command Example:**
```json
{
  "exit_code": 1,
  "command": "cargo build",
  "execution_time_ms": 2156,
  "timeout_secs": 30,
  "failed": true,
  "truncated": false
}
```

## Web Fetch Metadata

### WebFetchTool (`web_fetch`)

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | The URL that was fetched |
| `chars` | number | Number of characters returned |
| `truncated` | boolean | Whether content was truncated |
| `converted` | boolean | Whether HTML was converted to markdown |
| `status_code` | number | HTTP status code |
| `time_to_first_byte_ms` | number | Time to first byte in milliseconds |
| `total_time_ms` | number | Total request time in milliseconds |
| `headers` | object | HTTP response headers |

**headers object format:**
```json
{
  "content-type": "text/html; charset=utf-8",
  "content-length": "12345",
  "server": "nginx",
  ...
}
```

**Full Example:**
```json
{
  "url": "https://example.com/docs",
  "chars": 15234,
  "truncated": false,
  "converted": true,
  "status_code": 200,
  "time_to_first_byte_ms": 245,
  "total_time_ms": 892,
  "headers": {
    "content-type": "text/html; charset=utf-8",
    "content-length": "15234",
    "server": "nginx"
  }
}
```

## Version Control Metadata

### GitStatusTool (`git_status`)

| Field | Type | Description |
|-------|------|-------------|
| `branch` | string | Current branch name |
| `ahead` | number | Commits ahead of remote |
| `behind` | number | Commits behind remote |
| `staged` | number | Staged changes |
| `unstaged` | number | Unstaged changes |
| `untracked` | number | Untracked files |

### GitDiffTool (`git_diff`)

| Field | Type | Description |
|-------|------|-------------|
| `files_changed` | number | Number of files changed |
| `additions` | number | Lines added |
| `deletions` | number | Lines deleted |
| `path_spec` | string | Path spec used for diff |

### GitLogTool (`git_log`)

| Field | Type | Description |
|-------|------|-------------|
| `commit_count` | number | Number of commits returned |
| `limit` | number | Limit applied to query |
| `path_spec` | string | Path spec used (if any) |

### GitCommitTool (`git_commit`)

| Field | Type | Description |
|-------|------|-------------|
| `commit_hash` | string | SHA of created commit |
| `branch` | string | Branch committed to |
| `files_changed` | number | Files in commit |
| `message` | string | Commit message |

## LSP Tool Metadata

### LspDiagnosticsTool (`lsp_diagnostics`)

| Field | Type | Description |
|-------|------|-------------|
| `file_count` | number | Files with diagnostics |
| `error_count` | number | Total errors |
| `warning_count` | number | Total warnings |
| `info_count` | number | Total info messages |
| `hint_count` | number | Total hints |

### LspHoverTool (`lsp_hover`)

| Field | Type | Description |
|-------|------|-------------|
| `file` | string | File path |
| `line` | number | Line number |
| `column` | number | Column number |
| `language` | string | Language ID |

### LspDefinitionTool (`lsp_definition`)

| Field | Type | Description |
|-------|------|-------------|
| `file` | string | Target file path |
| `line` | number | Target line number |
| `column` | number | Target column number |
| `definition_kind` | string | Type of definition |

### LspCompletionTool (`lsp_completion`)

| Field | Type | Description |
|-------|------|-------------|
| `file` | string | File path |
| `line` | number | Line number |
| `column` | number | Column number |
| `completion_count` | number | Completions provided |
| `incomplete` | boolean | Whether results are incomplete |

## Truncation Metadata

When output is truncated, additional fields provide context:

| Field | Type | Description |
|-------|------|-------------|
| `omitted_lines` | number | Lines not shown |
| `omitted_bytes` | number | Bytes not shown |
| `omitted_items` | number | Items not shown |
| `omitted_matches` | number | Matches not shown |

## Critical Content Handling

Some content is never truncated (compilation errors, test failures, security issues):

| Field | Type | Description |
|-------|------|-------------|
| `critical` | boolean | Whether content was detected as critical |
| `never_truncated_reason` | string | Reason for not truncating |

## Binary File Detection

The following 70+ file extensions are detected as binary:

**Images:** png, jpg, jpeg, gif, bmp, ico, webp, svg, tiff, psd, ai, eps
**Audio:** mp3, wav, ogg, flac, aac, m4a, wma
**Video:** mp4, avi, mkv, mov, wmv, flv, webm
**Archives:** zip, tar, gz, bz2, rar, 7z, xz, zst
**Executables:** exe, dll, so, dylib, app, bin
**Documents:** pdf, doc, docx, xls, xlsx, ppt, pptx
**Fonts:** ttf, otf, woff, woff2, eot
**Database:** db, sqlite, mdb
**Other:** class, jar, war, obj, o, a, lib

## Security Considerations

1. **Content Hashing:** SHA-256 is used instead of MD5 for better security properties
2. **Path Validation:** All paths are validated to be within workspace bounds
3. **Symlink Detection:** Symbolic links are blocked for security
4. **Binary Detection:** Binary files are blocked with helpful recovery hints

## Performance Metrics

Timing fields help identify slow operations:

- `execution_time_ms` - Command execution duration
- `time_to_first_byte_ms` - Network latency for web requests
- `total_time_ms` - Total request duration

These can be used to:
- Identify performance bottlenecks
- Track tool usage patterns
- Optimize tool selection
- Cache slow operations
