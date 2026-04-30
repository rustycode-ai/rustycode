# Tool Quick Reference

**Last Updated:** 2025-03-14

A quick reference for rustycode-tools parameters and metadata.

## File Operations

### read_file

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| path | string | Yes | File path relative to workspace |
| start_line | integer | No | First line to return (1-indexed) |
| end_line | integer | No | Last line to return (1-indexed) |

**Metadata:**
```json
{
  "path": "/absolute/path",
  "total_bytes": 1234,
  "shown_bytes": 1234,
  "total_lines": 42,
  "shown_lines": 42,
  "binary": false,
  "content_hash": "sha256...",
  "language": "rust"
}
```

### write_file

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| path | string | Yes | File path |
| content | string | Yes | File content |

**Metadata:**
```json
{
  "path": "/absolute/path",
  "bytes": 1234,
  "lines": 42
}
```

### list_dir

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| path | string | No | Directory path (default: ".") |
| recursive | boolean | No | List recursively (default: false) |
| max_depth | integer | No | Maximum depth (default: 3) |
| filter | string | No | "file", "dir", "all", or ".ext" |

**Metadata:**
```json
{
  "path": "/absolute/path",
  "total_items": 156,
  "recursive": true,
  "max_depth": 3,
  "filter": ".rs"
}
```

## Search Operations

### grep

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| pattern | string | Yes | Regex pattern |
| path | string | No | Search path (default: ".") |
| before_context | integer | No | Lines before match |
| after_context | integer | No | Lines after match |
| max_matches_per_file | integer | No | Limit per file |

**Metadata:**
```json
{
  "pattern": "async fn",
  "total_matches": 42,
  "files_with_matches": 5,
  "top_files": [
    {"path": "src/file.rs", "matches": 18}
  ]
}
```

### glob

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| pattern | string | Yes | Glob pattern |

**Metadata:**
```json
{
  "pattern": "*.rs",
  "total_matches": 47,
  "extensions": [
    {"extension": "rs", "count": 47}
  ]
}
```

## Command Execution

### bash

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| command | string | Yes | Shell command |
| cwd | string | No | Working directory override |
| timeout_secs | integer | No | Timeout (default: 30) |
| transform | string | No | Output transformation |

**Metadata:**
```json
{
  "exit_code": 0,
  "command": "cargo test",
  "execution_time_ms": 4231,
  "timeout_secs": 30,
  "failed": false
}
```

## Web Operations

### web_fetch

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| url | string | Yes | URL to fetch |
| convert_markdown | boolean | No | Convert HTML to markdown |

**Metadata:**
```json
{
  "url": "https://example.com",
  "chars": 15234,
  "truncated": false,
  "converted": true,
  "status_code": 200,
  "time_to_first_byte_ms": 245,
  "total_time_ms": 892,
  "headers": {
    "content-type": "text/html",
    "content-length": "15234"
  }
}
```

## Version Control

### git_status

**Metadata:**
```json
{
  "branch": "main",
  "ahead": 2,
  "behind": 0,
  "staged": 3,
  "unstaged": 1,
  "untracked": 2
}
```

### git_diff

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| path_spec | string | No | Path spec |
| cached | boolean | No | Show staged changes |
| color_words | boolean | No | Word-level diff |

**Metadata:**
```json
{
  "files_changed": 3,
  "additions": 42,
  "deletions": 15
}
```

### git_log

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| limit | integer | No | Max commits (default: 10) |
| path_spec | string | No | Path to filter |

**Metadata:**
```json
{
  "commit_count": 10,
  "limit": 10
}
```

### git_commit

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| message | string | Yes | Commit message |
| paths | array | No | Files to commit |

**Metadata:**
```json
{
  "commit_hash": "abc123...",
  "branch": "main",
  "files_changed": 3
}
```

## LSP Tools

### lsp_diagnostics

**Metadata:**
```json
{
  "file_count": 5,
  "error_count": 2,
  "warning_count": 8,
  "info_count": 3,
  "hint_count": 1
}
```

### lsp_hover

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| file | string | Yes | File path |
| line | integer | Yes | Line number |
| column | integer | Yes | Column number |

### lsp_definition

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| file | string | Yes | File path |
| line | integer | Yes | Line number |
| column | integer | Yes | Column number |

### lsp_completion

**Parameters:**
| Param | Type | Required | Description |
|-------|------|----------|-------------|
| file | string | Yes | File path |
| line | integer | Yes | Line number |
| column | integer | Yes | Column number |

**Metadata:**
```json
{
  "file": "/path/to/file",
  "line": 42,
  "column": 10,
  "completion_count": 15,
  "incomplete": false
}
```

## Binary File Types

The following extensions are detected as binary:

**Images:** png, jpg, jpeg, gif, bmp, ico, webp, svg, tiff, psd, ai, eps

**Audio:** mp3, wav, ogg, flac, aac, m4a, wma

**Video:** mp4, avi, mkv, mov, wmv, flv, webm

**Archives:** zip, tar, gz, bz2, rar, 7z, xz, zst

**Executables:** exe, dll, so, dylib, app, bin

**Documents:** pdf, doc, docx, xls, xlsx, ppt, pptx

**Fonts:** ttf, otf, woff, woff2, eot

**Database:** db, sqlite, mdb

**Other:** class, jar, war, obj, o, a, lib

## Filter Values (list_dir)

| Value | Description |
|-------|-------------|
| "file" | Show only files |
| "dir" | Show only directories |
| "all" | Show all (default) |
| ".rs" | Show only .rs files |
| ".md" | Show only .md files |
| ".toml" | Show only .toml files |

## Truncation Limits

| Tool | Limit | Field |
|------|-------|-------|
| read_file | 80 lines / 10KB | READ_MAX_LINES, READ_MAX_BYTES |
| bash | 30 lines / 50KB | BASH_MAX_LINES, BASH_MAX_BYTES |
| grep | 15 matches | GREP_MAX_MATCHES |
| list_dir / glob | 30 items | LIST_MAX_ITEMS |
| web_fetch | 50,000 chars | (inline limit) |

Critical content (errors, failures) is never truncated.

## Permission Levels

| Level | Tools |
|-------|-------|
| Read | read_file, list_dir, grep, glob, git_*, lsp_*, web_fetch |
| Write | write_file, git_commit |
| Execute | bash |
| Network | web_fetch |
