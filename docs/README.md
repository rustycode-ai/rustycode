# RustyCode Documentation

Documentation hub for the RustyCode project. Start here if you're not sure where to go.

## Choose By Goal

### Use RustyCode

1. [Comprehensive Project Doc](RUSTYCODE.md) — Full architecture, crate reference, CLI guide, all systems
2. [Quickstart](guides/QUICKSTART.md) — Get running in minutes
3. [Tutorial](guides/TUTORIAL.md) — Walkthrough of key features
4. [Troubleshooting](guides/troubleshooting.md) — Common issues and fixes
5. [Integration Guide](guides/INTEGRATION.md) — Embed RustyCode in your workflow

### Contribute Or Develop

1. [Developer Guide](guides/developer-guide.md) — Build, test, lint, debug
2. [Contributing](contributing/CONTRIBUTING.md) — Contribution workflow
3. [Coding Standards](reference/coding-standards.md) — Style and conventions
4. [Agent Governance](project/agent-governance.md) — How AI agents work in this repo

### Understand The System

1. [Architecture Review](architecture/ARCHITECTURE-REVIEW-2026-04-20.md) — Current architecture analysis
2. [Architecture Overview](architecture/architecture.md) — System structure
3. [Architecture Decisions](adr/) — Why key choices were made
4. [Crate Index](crates/CRATES.md) — All workspace crates

## Documentation Map

| Area | Use It For |
| --- | --- |
| [RUSTYCODE.md](RUSTYCODE.md) | Comprehensive project documentation (start here) |
| [guides/](guides/) | Setup, workflows, tutorials, troubleshooting |
| [architecture/](architecture/) | System structure, subsystem relationships, integration |
| [reference/](reference/) | API reference, specs, standards, permissions |
| [adr/](adr/) | Architecture Decision Records (why, not what) |
| [security/](security/) | Provider security assumptions and checklists |
| [design/](design/) | Design docs for key subsystems (event bus, streaming, etc.) |
| [project/](project/) | Project governance and operating rules |
| [crates/](crates/) | Workspace-level crate documentation |

## Root-Level Docs

| File | Purpose |
| --- | --- |
| [README.md](../README.md) | Repo entry point |
| [CLAUDE.md](../CLAUDE.md) | Development instructions for AI agents and humans |
| [AGENTS.md](../AGENTS.md) | Agent-specific coding guidance |
| [TEAM_LEARNINGS.md](../TEAM_LEARNINGS.md) | Runtime-updated team knowledge |

## Docs Outside `docs/`

- [scripts/README.md](../scripts/README.md) — Automation scripts
- [tests/README.md](../tests/README.md) — Test infrastructure
- `crates/*/README.md` — Per-crate documentation (52 crates)
