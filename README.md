# RustyCode

RustyCode is an autonomous development framework built in Rust.

## Start Here

- New to the project: read the [documentation hub](docs/README.md)
- Want to contribute: read [CONTRIBUTING.md](CONTRIBUTING.md)
- Working on the codebase: read [CLAUDE.md](CLAUDE.md)

## Install

### Unix (Linux/macOS)

```bash
curl -sSL https://raw.githubusercontent.com/luengnat/rustycode/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/luengnat/rustycode/main/scripts/install.ps1 | iex
```

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
