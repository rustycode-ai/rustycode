# rustycode-tools-registry

Tool registry and discovery system for RustyCode.

## Purpose

Provides centralized tool discovery and registration. Maintains metadata about available tools, their capabilities, signatures, and execution requirements. Enables agents and the CLI to discover which tools are available and how to use them.

## Key Types

- `ToolRegistry` — Central registry of all available tools
- `ToolMetadata` — Tool identity, description, signature, and requirements
- `ToolDiscovery` — Discovers available tools from plugins and built-in sources
- `MetadataProvider` — Interface for tool metadata sources
- `RegistryConfig` — Configuration for registry behavior and discovery paths

## Public API

```rust
use rustycode_tools_registry::{ToolRegistry, ToolDiscovery, RegistryConfig};

// Create registry with discovery
let config = RegistryConfig::default()
    .with_plugin_directory("./plugins");

let discovery = ToolDiscovery::new(config);
let registry = ToolRegistry::new(discovery)?;

// List available tools
for tool in registry.list_tools() {
    println!("{}: {}", tool.name, tool.description);
}

// Get specific tool metadata
if let Some(tool) = registry.get_tool("Bash") {
    println!("Tool: {}", tool.name);
    println!("Signature: {}", tool.signature);
    println!("Required: {:?}", tool.required_permissions);
}
```

## Discovery Mechanisms

1. **Built-in Tools** — Hardcoded tools (Bash, git, file_operations)
2. **Plugin-Based** — Tools defined in plugin manifests
3. **Skill Frontmatter** — Tools defined in YAML skill files
4. **Custom Sources** — Custom MetadataProvider implementations

## Tool Metadata

Each tool entry includes:
- **Name** — Tool identifier (e.g., "Bash", "git")
- **Description** — Human-readable description
- **Signature** — Function signature or parameter schema
- **Categories** — Tool classification (file, git, code, etc.)
- **Permissions** — Required security permissions
- **Version** — Tool version
- **Documentation** — Usage guide and examples

## Dependencies

- `rustycode-tools-api` — Tool trait definitions
- `rustycode-plugins` — Plugin system
- `rustycode-skill` — Skill loading
- `serde` — Serialization
- `anyhow` — Error handling

## Architecture Notes

The registry is built on a discovery system that scans multiple sources. Discovery happens at startup and can be refreshed at runtime. Tools are immutable once registered — updates require re-discovery.

Registry queries are O(1) lookups by name. Category filtering is supported for finding tools by classification (e.g., all file tools).

## Testing

Tests verify tool discovery from different sources, metadata parsing, and registry queries. Mock metadata providers test custom sources.

## See Also

- `rustycode-tools` — Tool implementation and execution
- `rustycode-tools-api` — Tool trait definitions
- `rustycode-plugins` — Plugin system used for discovery
- `rustycode-skill` — Skill discovery (related pattern)
