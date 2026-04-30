# ADR 0001: Core Principles

- Status: Accepted
- Date: 2026-03-12

## Decision

RustyCode will optimize for:

1. Clean crate boundaries
2. Fast local-first execution
3. Token-efficient context assembly
4. First-class Git/worktree and LSP support
5. Explicit user/project memory and Markdown skills

## Consequences

- Session events must capture notable runtime decisions.
- Git/LSP/memory/skill subsystems ship in the foundation, not as add-ons.
- CLI-first implementation precedes TUI/web work.
