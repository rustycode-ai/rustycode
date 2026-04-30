# Ensemble Agent Mode - Usage Guide

## Overview

Ensemble mode enables multi-agent collaboration for complex software tasks. Instead of a single LLM making all decisions, specialized agents work together:

| Agent | Role | Activates When |
|-------|------|----------------|
| **Architect** | High-risk planning, structural declarations | Risk = High/Critical |
| **Builder** | Implementation, code changes | All tasks |
| **Skeptic** | Independent review of claims | Risk >= Moderate |
| **Judge** | Local verification (cargo check/test) | All tasks |
| **Scalpel** | Surgical compile error fixes | Compile errors detected |

## Usage

### Basic Command

```bash
# Ensemble mode - full multi-agent collaboration
rustycode-cli agent new --mode ensemble "Add user authentication with JWT"

# Auto mode - intent-based agent selection (default)
rustycode-cli agent new "Fix the compilation error in lib.rs"

# Code mode - single agent implementation
rustycode-cli agent new --mode code "Add a helper function"
```

### Available Modes

| Mode | Description | Best For |
|------|-------------|----------|
| `ensemble` | Multi-agent collaboration | Complex features, security changes |
| `code` | Implementation focus | Feature development |
| `debug` | Troubleshooting | Bug investigation |
| `ask` | Q&A | Quick questions |
| `orchestrate` | Multi-agent coordination | Complex workflows |
| `plan` | Architecture design | Planning sessions |
| `test` | Test-driven development | Writing tests |

## Risk Levels

The ensemble system automatically assesses task risk:

| Risk | Agents | Example Tasks |
|------|--------|---------------|
| **Low** | Builder + Skeptic + Judge | Single file fixes, typo corrections |
| **Moderate** | Builder + Skeptic + Judge | Multi-file refactors |
| **High** | Architect + Builder + Skeptic + Judge | New features, API changes |
| **Critical** | Full ensemble + extra review | Security, authentication, core logic |

## Example Flows

### Low Risk: Quick Fix
```
Task: "Fix typo in error message"

Flow:
1. Builder → implements fix
2. Skeptic → reviews change
3. Judge → runs cargo check
4. Done (2-3 turns)
```

### High Risk: New Feature
```
Task: "Add user authentication"

Flow:
1. Architect → reads codebase, creates structural declaration
2. Builder → implements step 1
3. Skeptic → reviews step 1
4. Judge → verifies step 1
5. Builder → implements step 2
6. ... (repeats per step)
7. Done (10-20 turns)
```

### Compile Error: Scalpel Intervention
```
Task: "Make it compile"

Flow:
1. Builder → attempts fix
2. Skeptic → reviews
3. Judge → runs cargo check, finds errors
4. Scalpel → surgical fix (max 10 lines/file)
5. Judge → verifies compilation
6. Done (3-5 turns)
```

## Timeline Visualization

Ensemble mode outputs an ASCII timeline showing agent activations:

```
Task: add-auth-feature
Turns: 5
Status: Success

Agent Activation Timeline:
────────────────────────────────────────────────────────────
Coordinator  ░░░░░
Architect    █░░░░      ← High-risk planning
Builder      ███░░      ← 3 implementation steps
Skeptic      ███░░      ← Reviews each step
Judge        ███░░      ← Verifies each step
Scalpel      ░░░░░      ← Not needed (no compile errors)
────────────────────────────────────────────────────────────
Legend: █ Active  ░ Inactive
```

## Configuration

Edit `~/.rustycode/config.json` to customize ensemble behavior:

```json
{
  "ensemble": {
    "max_turns": 50,
    "max_retries_per_step": 3,
    "use_local_judge": true
  }
}
```

## Troubleshooting

### "Ensemble mode completed with issues"

The ensemble encountered a problem. Check logs for:
- Doom loop detection (repeated failed approaches)
- Budget exhaustion (too many turns)
- Skeptic veto (claims not verified)

### "Architect not invoked"

Architect only activates for High/Critical risk tasks. To force Architect involvement:
1. Use `--mode plan` first for architecture
2. Then use `--mode ensemble` for implementation

### "Scalpel not fixing logic errors"

Scalpel ONLY fixes compile errors. For logic errors:
1. Judge will report test failures
2. Builder will be invoked to fix
3. Scalpel cannot help with "wrong output" issues

## Best Practices

1. **Use ensemble mode for high-risk tasks**: Security changes, core logic, API modifications
2. **Let Architect go first**: For complex features, Architect creates the blueprint
3. **Trust the Skeptic**: Veto means real issues found
4. **Watch the timeline**: ASCII visualization shows agent activity patterns
5. **Review plan steps**: Each step has clear success criteria

## See Also

- `docs/TEAM_AGENT_USE_CASES.md` - Detailed use case specifications
- `docs/AGENT_LIFETIME_VISUALIZATION.md` - State machine diagrams
- `crates/rustycode-core/tests/ensemble_e2e_test.rs` - E2E test examples
