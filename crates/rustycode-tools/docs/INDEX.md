# rustycode-tools Documentation

Welcome to the rustycode-tools documentation. This crate provides the tool execution framework for RustyCode.

## Documentation

| Document | Description |
|----------|-------------|
| [QUICKSTART.md](QUICKSTART.md) | Get started in minutes |
| [QUICK_REFERENCE.md](QUICK_REFERENCE.md) | Quick parameter and metadata lookup |
| [API_REFERENCE.md](API_REFERENCE.md) | Complete API reference |
| [EXAMPLES.md](EXAMPLES.md) | Real-world usage examples |
| [INTEGRATION.md](INTEGRATION.md) | LLM provider integration guide |
| [TROUBLESHOOTING.md](TROUBLESHOOTING.md) | Common issues and solutions |
| [TOOL_METADATA.md](TOOL_METADATA.md) | Complete reference for all tool metadata fields |
| [TOOL_ENHANCEMENTS.md](TOOL_ENHANCEMENTS.md) | Overview of recent tool enhancements |
| [LLM_METADATA_SPEC.md](LLM_METADATA_SPEC.md) | LLM metadata format specification |
| [../README.md](../README.md) | Crate overview and quick start |
| [../COMPILE_TIME_TOOLS.md](../COMPILE_TIME_TOOLS.md) | Compile-time tool system guide |

## Quick Links

### Tool Reference
- [File Operations](TOOL_METADATA.md#file-operation-metadata) - read_file, write_file, list_dir
- [Search Tools](TOOL_METADATA.md#search-tool-metadata) - grep, glob
- [Command Execution](TOOL_METADATA.md#command-execution-metadata) - bash
- [Web Fetch](TOOL_METADATA.md#web-fetch-metadata) - web_fetch
- [Version Control](TOOL_METADATA.md#version-control-metadata) - git_* tools
- [LSP Tools](TOOL_METADATA.md#lsp-tool-metadata) - lsp_* tools

### Key Features
- [Binary File Detection](TOOL_ENHANCEMENTS.md#3-binary-file-detection-readfiletool) - 70+ extensions
- [File Type Filtering](TOOL_ENHANCEMENTS.md#1-file-type-filtering-listdirtool) - Filter by type/extension
- [Usage Statistics](TOOL_ENHANCEMENTS.md#8-usage-statistics-usagetracker) - Tool usage tracking
- [Security](TOOL_METADATA.md#security-considerations) - Path validation, SHA-256 hashing

## Getting Started

**New to rustycode-tools?** Start with the [Quick Start Guide](QUICKSTART.md).

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

let executor = ToolExecutor::new(std::path::PathBuf::from("/project"));

let call = ToolCall {
    call_id: "1".to_string(),
    name: "read_file".to_string(),
    arguments: json!({"path": "src/main.rs"}),
};

let result = executor.execute(&call);
println!("{}", result.output);

// Access structured metadata
if let Some(metadata) = result.structured {
    println!("Lines: {}", metadata["total_lines"]);
    println!("Hash: {}", metadata["content_hash"]);
}
```

## Tool Metadata Structure

All tools return structured metadata:

```json
{
  "output": "tool output text",
  "structured": {
    "truncated": false,
    "total_lines": 127,
    "shown_lines": 127,
    "content_hash": "a1b2c3..."
  }
}
```

## Recent Enhancements (2025-03-14)

1. **File Type Filtering** - Filter directory listings by type or extension
2. **Line Count Tracking** - Track lines written by write_file
3. **Binary File Detection** - Detect 70+ binary file types
4. **Execution Time Tracking** - Timing metrics for bash and web_fetch
5. **Enhanced Error Messages** - Recovery hints for binary files
6. **Web Fetch Headers** - Full HTTP response headers
7. **SHA-256 Hashing** - Content hashing with SHA-256
8. **Usage Statistics** - Comprehensive tool usage tracking
9. **Grep Statistics** - File-level match analysis
10. **Bash Timing** - Execution time and failure flags
11. **Glob Statistics** - Extension breakdown

See [TOOL_ENHANCEMENTS.md](TOOL_ENHANCEMENTS.md) for complete details.

## Security Features

- **Path Validation** - All operations confined to workspace
- **Symlink Protection** - Symbolic links blocked
- **Binary Detection** - Binary files blocked with helpful hints
- **Rate Limiting** - Configurable execution rate limits
- **Permission Levels** - Read/Write/Execute/Network tiers

## Performance

- **Compile-Time Tools** - Zero-cost dispatch for hot paths
- **Regex Optimization** - Pre-compiled regex in grep
- **Early Filtering** - Filter during traversal, not after
- **Smart Truncation** - Critical errors never truncated

## Contributing

When adding new tools:

1. Add comprehensive metadata fields
2. Include execution timing where applicable
3. Add security considerations
4. Update this documentation
5. Add tests for metadata

## Support

For questions or issues, please refer to the main RustyCode documentation.
