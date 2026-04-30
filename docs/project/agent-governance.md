# Agent Governance

This document is the canonical home for the project rules that previously lived in separate gate documents at the repository root.

## Who This Is For

Internal engineers and agent authors working on RustyCode.

## What You Should Be Able To Do After Reading

Choose the right execution profile for a task and apply the project's non-negotiable operating rules before making changes.

## Profile Gate

The profile gate is the deterministic entry point for autonomous work. It maps user intent to a task profile so the system can choose an appropriate team shape and execution path.

| Intent | Risk | Reach | Suggested Team |
| --- | --- | --- | --- |
| Fix a bug | Moderate | Local | Coordinator, Builder, Skeptic |
| Refactor code | High | Wide | Coordinator, Architect, Builder, Judge |
| Simple task or docs | Low | Single file | Coordinator, Builder |
| Security, auth, or infra | Critical | System-wide | Coordinator, Architect, Skeptic, Judge |

Execution flow:

1. Analyze the request and detect the task intent.
2. Map the request to a `TaskProfile`.
3. Assemble the execution team from that profile.
4. Select the smallest harness that fits the task.
5. Persist state when the work needs checkpointing.
6. Execute on the selected path.

Recommended harness choices:

- `ultrawork` for focused execution with retries and progress tracking
- `omo` for parallel analysis and cross-checking
- `sparv` for long-lived checkpointed work
- `direct` when no harness is needed

## Discipline Gate

These rules apply to every autonomous task, regardless of profile.

### Intent Gate

Every task should begin by classifying the request and naming the operating strategy.

Template:

```text
I detect [TYPE] intent - [REASON].
My approach: [STRATEGY].
```

Allowed intent labels:

- `research`
- `implementation`
- `investigation`
- `evaluation`
- `fix`
- `open-ended`

### Verification Mandate

- Do not claim completion without fresh verification evidence.
- Do not edit before reading the current source of truth.
- Do not assume a file, behavior, or dependency exists without checking.

### Tool Mandate

- Use tools instead of unsupported assumptions.
- Delegate when a subagent or specialized tool can do the work better.
- Parallelize independent reads and searches.
- Keep dependent operations ordered: read, edit, then verify.

## Related Guidance

- [CLAUDE.md](../../CLAUDE.md) contains the full development guide and codebase conventions.
- [CONTRIBUTING.md](../../CONTRIBUTING.md) contains the contributor workflow.
- [docs/README.md](../README.md) is the main documentation hub.
