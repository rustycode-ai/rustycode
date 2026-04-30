# rustycode-cli

Command-line interface for RustyCode.

## Purpose

Non-interactive CLI for automated development workflows. Executes single commands, plans, and autonomous tasks without user intervention. Designed for CI/CD, scripts, and batch processing.

## Usage

```bash
# Start interactive session
rustycode

# Execute a single command
rustycode "Write a function that computes fibonacci"

# Execute from stdin
echo "Fix this bug in main.rs" | rustycode

# Execute a plan
rustycode plan --file my-plan.md

# Autonomous mode
rustycode auto "Complete the TODO list"

# Use specific model
rustycode --model claude-opus-4-7 "Generate tests"

# Load existing session
rustycode --session sess_123

# Set token limit
rustycode --max-tokens 4000 "task"

# Show usage
rustycode --help
rustycode <command> --help
```

## Commands

- **default** (no args) — Interactive session (TUI)
- **plan** — Create and execute multi-step plan
- **auto** — Autonomous development mode
- **eval** — Evaluate code/execute scripts
- **search** — Search codebase
- **refactor** — Refactor code with constraints
- **test** — Generate or run tests
- **debug** — Debug workflow

## Flags

- `-m, --model <MODEL>` — LLM model to use
- `-t, --max-tokens <N>` — Token limit
- `--session <ID>` — Load session
- `-s, --system <FILE>` — System prompt file
- `--no-tools` — Disable tool execution
- `--dry-run` — Show what would happen
- `-v, --verbose` — Detailed output
- `--json` — Output as JSON

## Examples

```bash
# Simple task
$ rustycode "implement binary search in rust"

# With tool constraints
$ rustycode --no-tools "review this function for bugs"

# From file
$ rustycode < task.txt

# Pipeline
$ cat requirements.txt | rustycode | tee output.md

# Batch processing
$ for task in task1.txt task2.txt; do
    rustycode < $task
  done
```

## Output

Default output:
- Streaming response as it arrives
- Tool execution visualization
- Final result

With `--json`:
```json
{
  "session_id": "sess_123",
  "status": "success",
  "output": "...",
  "tokens_used": 1234,
  "cost": 0.05
}
```

## Exit Codes

- 0 — Success
- 1 — Error during execution
- 2 — Invalid arguments
- 3 — Model/API error
- 4 — Tool execution failure

## Integration

Works well with:
- Shell scripts and Makefiles
- CI/CD pipelines (GitHub Actions, GitLab CI, Jenkins)
- Git hooks (pre-commit, commit-msg)
- IDE build systems

## Dependencies

- `clap` — CLI argument parsing
- `rustycode-core` — Session management
- `rustycode-session` — Session lifecycle
- `rustycode-config` — Configuration
- `tokio` — Async runtime
- `anyhow` — Error handling

## Architecture

Main entry point parses arguments, creates config, initializes session, executes command, and outputs result. Supports streaming output for long-running tasks.

## See Also

- `rustycode-tui` — Interactive UI library
- `rustycode-core` — Execution engine
- `rustycode-acp` — IDE protocol (different interface)
