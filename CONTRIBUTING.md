# Contributing to RustyCode

Thank you for your interest in contributing to RustyCode!

## Quick Start

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Run tests (`cargo test --workspace`)
5. Submit a pull request

For the broader docs map, start at [docs/README.md](docs/README.md).

## Development Setup

```bash
git clone https://github.com/luengnat/rustycode.git
cd rustycode
cargo build
cargo test --workspace
```

## Code Style

- Follow standard Rust formatting (`cargo fmt`)
- Ensure `cargo clippy` passes with no warnings
- Add tests for new functionality

Project-specific development rules and agent operating guidance live in [CLAUDE.md](CLAUDE.md) and [docs/project/agent-governance.md](docs/project/agent-governance.md).

## Reporting Issues

Please use [GitHub Issues](https://github.com/luengnat/rustycode/issues) to report bugs or request features.
