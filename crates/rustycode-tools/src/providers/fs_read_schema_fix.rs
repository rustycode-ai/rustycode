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
