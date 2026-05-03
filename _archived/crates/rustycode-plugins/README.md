# rustycode-plugins

Plugin system for dynamic loading and management of tools, agents, and LLM providers in RustyCode.

## Purpose

Provides a extensible plugin architecture that allows RustyCode to dynamically load and manage three types of plugins: tools, agents, and LLM providers. Handles plugin lifecycle (loading, initialization, shutdown), dependency resolution, and metadata management.

## Key Types

- `PluginRegistry` — Central registry for loaded plugins with lifecycle management
- `PluginMetadata` — Plugin identity, version, requirements, and capabilities
- `ToolPlugin` — Plugin trait for tool providers
- `AgentPlugin` — Plugin trait for agent implementations
- `LLMProviderPlugin` — Plugin trait for LLM provider implementations
- `PluginManifest` — Plugin specification with dependencies and exports
- `DependencyResolver` — Resolves plugin dependencies and load order

## Public API

```rust
use rustycode_plugins::{PluginRegistry, PluginMetadata};

// Create registry
let mut registry = PluginRegistry::new();

// Load plugins from directory
registry.load_from_directory("./plugins")?;

// Get loaded plugin by name
if let Some(plugin) = registry.get_plugin("my-tool-plugin") {
    println!("Loaded: {} v{}", plugin.metadata.name, plugin.metadata.version);
}

// List all plugins
for plugin in registry.list_plugins() {
    println!("{}: {}", plugin.name, plugin.description);
}
```

## Plugin Types

- **ToolPlugin** — Extends available tools with custom implementations
- **AgentPlugin** — Registers new agent types for task automation
- **LLMProviderPlugin** — Adds support for new LLM providers (local, commercial, custom)

## Lifecycle Phases

1. **Discovery** — Scan plugin directories for manifests
2. **Loading** — Load plugin binaries/modules
3. **Dependency Resolution** — Build load order respecting dependencies
4. **Initialization** — Initialize plugins in dependency order
5. **Registration** — Register plugins in central registry
6. **Runtime** — Plugins available for use
7. **Shutdown** — Clean shutdown in reverse load order

## Dependencies

- `rustycode-protocol` — Shared types
- `rustycode-config` — Configuration loading
- `anyhow` — Error handling
- `serde` — Serialization for manifests
- `tokio` — Async runtime

## Architecture Notes

Plugins are isolated units that implement well-defined traits. The registry manages discovery, loading, and lifecycle. Dependency resolution ensures plugins load in correct order. Each plugin declares its dependencies in a manifest file (TOML or YAML).

Plugin implementation details are abstract — a plugin can be a Rust binary, a WASM module, or a script, as long as it provides the required trait interface.

## Testing

Tests verify plugin discovery, manifest parsing, dependency resolution, and lifecycle transitions. Mock plugins test the registry without external dependencies.

## See Also

- `rustycode-skill` — Skill discovery (similar but for YAML frontmatter skills)
- `rustycode-agents` — Agent trait implementations
- `rustycode-tools-api` — Tool trait definitions
- `rustycode-llm` — LLM provider trait
