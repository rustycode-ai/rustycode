# CI Native Benchmark Tasks

A curated set of 10 native benchmark tasks for the rustycode-bench CI pipeline. These tasks run on macOS and Linux WITHOUT Docker.

## Task Structure

Each task directory contains:
- `task.toml` - Task configuration (metadata, timeouts)
- `instruction.md` - Clear task description for the agent
- `tests/verify.sh` - Verification script (exit 0 = pass, exit 1 = fail)
- Optional starter files (CSV, JSON, text files, Python files)

## Tasks

### File I/O (2 tasks)
1. **sort-csv** - Sort a CSV file by a specific column (numeric sort)
2. **merge-json** - Merge two JSON files into a single array

### Code Generation (2 tasks)
3. **python-fibonacci** - Generate first 20 Fibonacci numbers
4. **python-fizzbuzz** - Generate FizzBuzz sequence for 1-100

### Text Processing (2 tasks)
5. **extract-urls** - Extract and sort HTTP/HTTPS URLs from text
6. **word-count** - Count word frequencies and output top 10

### Scripting (2 tasks)
7. **file-organizer** - Bash script to organize files by extension
8. **backup-script** - Bash script for timestamped backups

### Refactoring (2 tasks)
9. **extract-function** - Extract logic into a named function
10. **rename-variables** - Rename single-letter variables to descriptive names

## Usage

Run all CI native tasks:
```bash
rustycode-bench run --dataset crates/rustycode-bench/tasks/ci-native --env native
```

Run a specific task:
```bash
rustycode-bench run --dataset crates/rustycode-bench/tasks/ci-native/python-fibonacci --env native
```

Run with oracle agent (reference solutions):
```bash
rustycode-bench run --dataset crates/rustycode-bench/tasks/ci-native --agent oracle --env native
```

## Task Design Principles

- **No Docker required** - All tasks use standard tools (bash, python3, grep, sort)
- **Portable verification** - verify.sh scripts work on macOS and Linux
- **Simple validation** - Exit code 0 = pass, 1 = fail
- **Clear instructions** - Each task has unambiguous requirements
- **Quick execution** - 120 second timeout per task
- **CI-friendly** - Designed for automated testing pipelines

## Adding New Tasks

1. Create a new directory: `mkdir task-name/`
2. Add `task.toml` with metadata and timeouts
3. Add `instruction.md` with clear requirements
4. Create `tests/verify.sh` with validation logic
5. Add any starter files needed

Example task.toml:
```toml
version = "1.0"

[metadata]
difficulty = "easy"
category = "file-io"

[verifier]
timeout_sec = 120.0

[agent]
timeout_sec = 120.0

[environment]
# Empty for native tasks (no Docker)
```

## Verification

All verify.sh scripts must:
- Use `#!/bin/bash` and `set -e`
- Be portable (avoid bashisms that break on macOS)
- Use only basic tools (bash, python3, grep, sort, wc, etc.)
- Exit 0 on success, 1 on failure
- Provide clear error messages

## Testing Locally

Test a single task manually:
```bash
cd tasks/ci-native/python-fibonacci
# Create solution
echo 'a, b = 0, 1
for _ in range(20):
    print(a)
    a, b = b, a + b' > fib.py
# Verify
./tests/verify.sh
```

## Integration

These tasks are automatically discovered by the `ResolvedTask::discover()` method in `rustycode-bench/src/task/mod.rs`. The dataset path can be passed to the benchmark runner:

```rust
let dataset_path = PathBuf::from("crates/rustycode-bench/tasks/ci-native");
let tasks = ResolvedTask::discover(&dataset_path)?;
```
