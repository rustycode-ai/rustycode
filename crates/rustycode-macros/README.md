# rustycode-macros

Procedural macros for tool definition and description generation.

## Purpose

Provides compile-time code generation for defining tools and extracting documentation. Automates tool struct boilerplate, generates name/description methods, and enables declarative tool definitions with doc comments or external documentation files.

## Key Macros

### `#[tool]` Attribute Macro

Converts a function into a `Tool` struct implementation:

```rust
use rustycode_macros::tool;

#[tool]
pub fn read_file(path: String) -> Result<String> {
    std::fs::read_to_string(path)
}
// Generates: struct read_file_Tool; impl Tool for read_file_Tool { ... }
```

### `#[derive(ToolDescription)]` Derive Macro

Extracts documentation and generates helper methods:

```rust
use rustycode_macros::ToolDescription;

#[derive(ToolDescription)]
/// Reads a file from the filesystem.
struct ReadFile;

// Generates:
// impl ReadFile {
//     fn description() -> &'static str { "Reads a file from the filesystem." }
//     fn tool_name() -> String { "read_file" }
// }
```

### Doc Comment Extraction

```rust
#[derive(ToolDescription)]
/// This is a multi-line
/// documentation string
/// that gets combined.
struct MyTool;

// description() returns the full combined doc string
```

### External Documentation Files

```rust
#[derive(ToolDescription)]
#[tool_description_file = "docs/my_tool.md"]
struct MyTool;
// description() returns contents of docs/my_tool.md at compile time
```

## Name Conversion

Automatically converts struct names to `snake_case`:

```rust
#[derive(ToolDescription)]
struct ReadFile;          // tool_name() → "read_file"

#[derive(ToolDescription)]
struct HTTPClient;        // tool_name() → "h_t_t_p_client"

#[derive(ToolDescription)]
struct FSReader;          // tool_name() → "fs_reader"
```

## Dependencies

- `proc-macro`, `proc-macro2` — Macro infrastructure
- `quote` — Code generation
- `syn` — AST parsing

## Output

Generated code is:
- Fully expanded at compile time
- Zero runtime overhead
- Type-safe (compiler catches errors)
- Easily inspected with `cargo expand`

## Use Cases

- **Tool registry** — Automatic Tool trait implementation
- **Documentation** — Extract docs at compile time, keep source of truth in comments
- **CLI** — Generate command metadata from struct definitions
- **Testing** — Verify tool descriptions match function signatures

## Best Practices

- Use doc comments on tool structs for inline documentation
- Use `#[tool_description_file = "..."]` for long documentation
- Keep descriptions short and actionable
- Use markdown in external docs for formatting

## Testing

- Macro expansion tests (use `cargo expand` to verify)
- Name conversion tests (CamelCase → snake_case)
- Doc comment extraction tests
- Derive macro attribute parsing tests

## See Also

- `rustycode-tools` — Tool execution framework
- `rustycode-tools-api` — Tool trait definitions
- `rustycode-tools-registry` — Tool registry system
