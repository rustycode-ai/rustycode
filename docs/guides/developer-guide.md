# RustyCode Developer Guide

Welcome to the RustyCode developer guide! This document will help you set up your development environment, understand the codebase, and contribute effectively to the project.

## Table of Contents

- [Development Environment](#development-environment)
- [Building and Testing](#building-and-testing)
- [Code Style and Conventions](#code-style-and-conventions)
- [Architecture Overview](#architecture-overview)
- [Contributing Workflow](#contributing-workflow)
- [Getting Help](#getting-help)

## Development Environment

### Prerequisites

RustyCode requires the following tools:

- **Rust** (latest stable, edition 2021)
- **Git** for version control
- **SQLite** (bundled with rusqlite)

### Installing Rust

If you don't have Rust installed, use rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Verify your installation:

```bash
rustc --version
cargo --version
```

### Cloning the Repository

```bash
git clone https://github.com/luengnat/rustycode.git
cd rustycode
```

### Development Tools

We recommend installing these useful development tools:

```bash
# Install cargo tools
cargo install cargo-watch      # Watch for changes and rebuild
cargo install cargo-edit       # Add dependencies from CLI
cargo install cargo-expand     # Expand macros
```

### IDE Setup

**VS Code** (recommended):

Install these extensions:
- `rust-analyzer` - Official Rust language server
- `CodeLLDB` - Debugger for Rust
- `Even Better TOML` - Better Cargo.toml syntax

**IntelliJ IDEA / RustRover**:
- Built-in Rust support is excellent
- No additional setup needed

**Vim/Neovim**:
- Install `rust-analyzer` via your plugin manager
- Configure LSP for Rust

## Building and Testing

### Building the Project

```bash
# Build all crates (development)
cargo build

# Build with optimizations (release)
cargo build --release

# Build specific crate
cargo build -p rustycode-id
cargo build -p rustycode-bus
cargo build -p rustycode-runtime
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p rustycode-id

# Run tests with output
cargo test -- --nocapture

# Run tests in parallel (faster)
cargo test --jobs 4

# Run specific test
cargo test test_session_id

# Run ignored tests (benchmarks)
cargo test -- --ignored
```

### Running Examples

```bash
# Sortable ID system
cargo run --package rustycode-id --example basic_usage

# Event bus
cargo run --package rustycode-bus --example basic_usage
cargo run --package rustycode-bus --example wildcard_matching

# Compile-time tools
cargo run --package rustycode-tools --example basic_tools

# Async runtime
cargo run --package rustycode-runtime --example async_runtime
```

### Checking Code

```bash
# Check without building (faster)
cargo check

# Format code
cargo fmt

# Check formatting without changes
cargo fmt --check

# Lint with Clippy
cargo clippy

# Fix Clippy warnings automatically
cargo clippy --fix
```

### Documentation

```bash
# Generate and open documentation
cargo doc --open

# Build documentation for all crates
cargo doc --all --no-deps --open

# Build documentation with private items
cargo doc --document-private-items --open
```

## Code Style and Conventions

### Rust Formatting

We use standard Rust formatting via `rustfmt`:

```bash
# Format all code
cargo fmt

# Check formatting
cargo fmt --check
```

**Never commit unformatted code.** The CI will reject it.

### Naming Conventions

Follow Rust naming conventions:

- **Types**: `PascalCase` (e.g., `SessionId`, `EventBus`)
- **Functions**: `snake_case` (e.g., `new_session`, `publish_event`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `MAX_SUBSCRIBERS`)
- **Traits**: `PascalCase` (e.g., `Tool`, `Event`)

### Documentation Comments

All public items must have documentation comments:

```rust
/// Create a new session ID.
///
/// # Examples
///
/// ```rust
/// use rustycode_id::SessionId;
///
/// let id = SessionId::new();
/// assert!(id.to_string().starts_with("sess_"));
/// ```
pub fn new() -> Self {
    // ...
}
```

### Error Handling

- Use `anyhow::Result` for application errors
- Use `thiserror` to define error types
- Always provide context for errors:

```rust
use anyhow::Context;

fn read_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .context(format!("failed to read config from {}", path.display()))?;
    // ...
}
```

### Testing Conventions

- Write tests in the same module as the code
- Use descriptive test names: `test_<what>_<when>_<expected>`
- Organize tests with nested modules:

```rust
#[cfg(test)]
mod tests {
    mod read_file {
        use super::*;

        #[test]
        fn test_read_file_basic() {
            // ...
        }

        #[test]
        fn test_read_file_with_line_range() {
            // ...
        }
    }
}
```

### Async/Await Guidelines

- Use `tokio` for async runtime
- Always spawn tasks with proper error handling:

```rust
tokio::spawn(async move {
    if let Err(e) = task().await {
        tracing::error!("Task failed: {}", e);
    }
});
```

### Performance Considerations

- Prefer compile-time to runtime checks
- Use zero-cost abstractions (compile-time tools)
- Avoid unnecessary allocations
- Profile with `cargo flamegraph` when optimizing

## Architecture Overview

### Crate Structure

RustyCode is organized as a workspace with focused crates:

```
rustycode-protocol   # Shared DTOs and types
rustycode-id         # Sortable ID system
rustycode-bus        # Type-safe event bus
rustycode-tools      # Compile-time tool system
rustycode-runtime    # Async runtime facade
rustycode-config     # Layered config loading
rustycode-storage    # SQLite persistence
rustycode-git        # Git/worktree inspection
rustycode-lsp        # LSP discovery/status
rustycode-memory     # User/project memory
rustycode-skill      # Skill discovery
rustycode-core       # Core runtime orchestration
rustycode-cli        # Terminal entrypoint
rustycode-tui        # Terminal UI (planned)
```

### Key Design Principles

1. **Compile-Time Guarantees Over Runtime Checks**
   - Use the type system to encode invariants
   - Zero runtime type errors in tool execution
   - Monomorphization for zero-cost abstractions

2. **Structured Concurrency with Async/Await**
   - Tokio runtime for async operations
   - Non-blocking I/O throughout
   - Event-driven decoupling

3. **Type-State Patterns**
   - Impossible states are unrepresentable
   - Compiler enforces correctness
   - Phantom types for lifecycle management

4. **Event-Driven Architecture**
   - Loose coupling between crates
   - Wildcard event subscriptions
   - Hooks for cross-cutting concerns

### Phase 1 Implementation

Phase 1 provides the foundation:

- **Sortable ID System**: Time-sortable, compact identifiers
- **Type-Safe Event Bus**: Decoupled communication with wildcards
- **Async Runtime**: Non-blocking facade over sync core
- **Compile-Time Tools**: Type-safe tool execution

See [Phase 1 Migration Guide](phase1-migration.md) for details.

## Contributing Workflow

### 1. Create a Branch

```bash
# Start from main
git checkout main
git pull origin main

# Create feature branch
git checkout -b feature/your-feature-name
```

Branch naming conventions:
- `feature/` - New features
- `fix/` - Bug fixes
- `refactor/` - Code refactoring
- `docs/` - Documentation updates
- `test/` - Test improvements

### 2. Make Changes

- Write code following the style guide
- Add tests for new functionality
- Update documentation as needed
- Run tests and ensure they pass

### 3. Commit Your Changes

```bash
# Stage changes
git add .

# Commit with descriptive message
git commit -m "feat: add compile-time tool validation

- Add Tool trait with associated types
- Implement ToolDispatcher for zero-cost dispatch
- Add tests for type safety"
```

Commit message format:
- `feat:` - New feature
- `fix:` - Bug fix
- `refactor:` - Code refactoring
- `docs:` - Documentation
- `test:` - Test changes
- `chore:` - Maintenance tasks

### 4. Run Tests

```bash
# Run all tests
cargo test

# Run with Clippy
cargo clippy -- -D warnings

# Check formatting
cargo fmt --check
```

### 5. Create a Pull Request

```bash
# Push to your fork
git push origin feature/your-feature-name

# Create PR on GitHub
```

PR requirements:
- Descriptive title and description
- All tests passing
- Code reviews addressed
- Documentation updated

### Code Review Process

1. **Automated Checks**: CI runs tests and lints
2. **Review**: Maintainer reviews your code
3. **Feedback**: Address review comments
4. **Approval**: Maintainer approves the PR
5. **Merge**: PR is merged into main

## Getting Help

### Documentation

- [Architecture Overview](architecture.md) - System design
- [Rust-First Architecture](architecture.md) - Detailed design
- [Phase 1 Migration Guide](phase1-migration.md) - Migration guide
- [Sortable IDs](sortable-ids.md) - ID system design

### ADRs (Architecture Decision Records)

- [ADR-0001: Core Principles](adr/0001-core-principles.md)
- [ADR-0002: Context Budgeting](adr/0002-context-budgeting.md)
- [ADR-0003: Event Bus System](adr/0003-event-bus-system.md)

### Community

- **GitHub Issues**: Bug reports and feature requests
- **Discussions**: Questions and ideas
- **Discord**: Real-time chat (if available)

### Debugging Tips

**Enable debug logging**:

```bash
RUST_LOG=debug cargo run
```

**Use rust-analyzer**:
- "Go to Definition" (F12)
- "Find References" (Shift+F12)
- "Type Hierarchy" (Ctrl+H)

**Debug with LLDB**:
```bash
cargo build
rust-lldb target/debug/rustycode
```

### Common Issues

**Build errors**:
- Ensure Rust version is 1.85+
- Run `cargo clean && cargo build`
- Check Rustup: `rustup update`

**Test failures**:
- Run tests sequentially: `cargo test -- --test-threads=1`
- Check for race conditions: `cargo test -- --ignored`

**Documentation build errors**:
- Check all public items have doc comments
- Run `cargo doc --document-private-items` to see all errors

## Next Steps

- Read the [Architecture Overview](architecture.md)
- Explore the [API Reference](api-reference.md)
- Check [CONTRIBUTING](../CONTRIBUTING.md) for contribution guidelines
- Review existing code to understand patterns

Happy hacking! 🦀
