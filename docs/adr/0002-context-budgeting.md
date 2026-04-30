# ADR 0002: Context Budgeting

- Status: Accepted
- Date: 2026-03-12

## Decision

Each model turn will use a reserved context budget split across system
instructions, active task, recent turns, tool schemas, memory, Git/LSP state,
and focused code excerpts.

## Consequences

- Context selection is observable and testable.
- Broad file reads are discouraged when Git/LSP/search can narrow scope.
