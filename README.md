# RustyCode

RustyCode is an autonomous development framework built in Rust.

## Start Here

- New to the project: read the [documentation hub](docs/README.md)
- Want to contribute: read [CONTRIBUTING.md](docs/contributing/CONTRIBUTING.md)
- Working on the codebase: read [CLAUDE.md](CLAUDE.md)

## Install

### Unix (Linux/macOS)

Download from [GitHub Releases](https://github.com/rustycode-ai/rustycode/releases):

```bash
# macOS arm64
curl -sSL https://github.com/rustycode-ai/rustycode/releases/latest/download/rustycode-macos-arm64.tar.gz | tar xz
chmod +x rustycode-macos-arm64 && mv rustycode-macos-arm64 /usr/local/bin/rustycode
```

### Windows

Download `rustycode-windows-x86_64.zip` from [GitHub Releases](https://github.com/rustycode-ai/rustycode/releases).

## Build Requirements

- Linux: `protobuf-compiler`, `libssl-dev`, `pkg-config`
- macOS: `protobuf` via Homebrew

## Common Paths

- User guides: [docs/guides](docs/guides/)
- Architecture and design: [docs/architecture](docs/architecture/) and [docs/design](docs/design/)
- Reference material: [docs/reference](docs/reference/)
- Autonomous mode docs: [docs/orchestra](docs/orchestra/)

Two files intentionally stay at the repository root because the code reads them directly:

- [CLAUDE.md](CLAUDE.md) for project-wide development instructions
- [TEAM_LEARNINGS.md](TEAM_LEARNINGS.md) for persisted team learnings
