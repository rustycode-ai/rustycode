# Benchmark Suites

RustyCode evaluates agent quality through multiple benchmark suites, each targeting different capabilities.

## SWE-bench

**What it measures**: Ability to fix real bugs in open-source Python projects by reading code, identifying the issue, and producing a correct patch.

**Scale**: 500 verified instances from popular GitHub repos (Django, scikit-learn, sympy, matplotlib, flask, requests, etc.).

**How it works**:
1. Each instance is a real GitHub issue with a known fix
2. The agent receives the problem statement, repo structure, and FAIL_TO_PASS test names
3. The agent edits the code to fix the bug
4. The evaluator applies the patch and runs FAIL_TO_PASS (must now pass) and PASS_TO_PASS (must still pass) tests

**Key features of our runner**:
- Honest evaluation — minimal system prompt (identity only, no rules or workflow coaching)
- Task prompt is standard SWE-bench format (problem statement + file tree + test names)
- Pretest: optionally pre-runs FAIL_TO_PASS tests and includes error output as context
- Post-agent verification with retry loop (`--verify-retries`)
- Per-instance test runner detection (pytest, Django `runtests.py`, unittest)

**Running**:
```bash
# Basic SWE-bench run
rtk-bench swebench --instances instances.json --output predictions.json

# With custom model and retries
rtk-bench swebench \
  --instances instances.json \
  --output predictions.json \
  --model claude-sonnet-4-6 \
  --max-turns 40 \
  --timeout 900 \
  --verify-retries 1

# A/B testing: disable symbol tools
rtk-bench swebench \
  --instances instances.json \
  --output predictions.json \
  --no-symbol-tools

# Evaluate predictions after running
rtk-bench evaluate \
  --predictions predictions.json \
  --instances instances.json
```

**Key flags**:
| Flag | Default | Purpose |
|------|---------|---------|
| `--max-turns` | 40 | Max tool-use turns per attempt |
| `--timeout` | 900 | Wall-clock timeout per attempt (seconds) |
| `--verify-retries` | 1 | Extra attempts after test failure |
| `--pretest` / `--no-pretest` | on | Pre-run FAIL_TO_PASS tests, include error output |
| `--no-symbol-tools` | off | Disable tree-sitter symbol tools |
| `--no-thinking-guide` | off | Disable thinking workflow tool |
| `--agent` | code | Agent type: code, oracle, nop |

**Top scores** (open-source, as of 2026):
- Refact.ai: 70.4% (352/500)
- OpenHands: 53%+
- SWE-agent: ~40%

---

## TerminalBench 2.0 (TB2)

**What it measures**: Ability to complete diverse programming tasks in containerized Linux environments — from regex parsing to async debugging to security analysis.

**Scale**: ~89 tasks across multiple categories (proteins, async, security, parsing, algorithms, video, etc.).

**How it works**:
1. Each task has a Dockerfile that sets up the environment
2. The agent receives an instruction and must produce files or modify code
3. A verifier script scores the output (0 or 1 reward)

**Execution modes**:
- **Docker mode**: Full containerized isolation (requires Docker/Podman)
- **Native mode**: Direct execution on host (no Docker, faster)

**Running**:
```bash
# TB2 via Docker
rtk-bench run --dataset terminal-bench@2.0 --agent code --model claude-sonnet-4-6

# Native mode (no Docker)
rtk-bench run --dataset terminal-bench@2.0 --agent oracle --env native

# Specific tasks
rtk-bench run --dataset terminal-bench@2.0 --task "regex,async" --timeout 300
```

**Known results**:
- Oracle baseline (native): 12/76 tasks pass (many need Dockerfile-built deps)
- Docker oracle: 79/89 (88.8%) pass
- Agent runs: 10/10 agent success, QEMU verifier crashes on arm64 macOS

**Limitations on arm64 macOS**:
- TB2 images are x86_64 (run via QEMU in Podman)
- 7/10 verifiers crash with SIGSEGV when importing numpy/pyarrow under QEMU
- Static musl binary required for fresh containers

---

## LiveBench

**What it measures**: Coding agent performance on continuously updated tasks derived from recent GitHub issues.

**How it works**: Tasks are refreshed periodically to prevent contamination with training data.

**Running**:
```bash
rtk-bench run --dataset livebench --agent code --model claude-sonnet-4-6
```

---

## rtk-bench

**What it measures**: General-purpose benchmark infrastructure for evaluating coding agents on custom task sets.

**Supported features**:
- Multiple agent types: oracle (pre-written solution), code (LLM-powered), nop (smoke test)
- Multiple environments: native, docker, bollard
- Composite datasets: merge and deduplicate multiple sources
- Result history: track scores across runs, diff against baselines
- Exception-based filtering: include/exclude tasks by regex

**Running**:
```bash
# Run with default settings
rtk-bench run --dataset ./my-tasks --agent code --output json

# List available tasks
rtk-bench list --dataset terminal-bench@2.0 --verbose

# View run history
rtk-bench history show
rtk-bench history diff --run 1 --baseline 0

# Generate report
rtk-bench run --dataset ./my-tasks --report results.md --output markdown
```

**Report formats**: `pretty` (terminal), `json`, `csv`, `markdown`, `summary`

**Task structure**:
```
my-task/
├── task.toml          # Metadata, verifier config, environment
├── instruction.md     # Task description for the agent
└── environment/
    └── Dockerfile     # Optional container setup
```

**Task TOML**:
```toml
[metadata]
category = "parsing"
difficulty = "medium"

[verifier]
timeout_sec = 30
command = "python3 verify.py"

[environment]
dockerfile = "Dockerfile"
```

---

## Agent Types

| Agent | Description | Use Case |
|-------|-------------|----------|
| `oracle` | Runs pre-written `solution.sh` | Infrastructure validation, upper bound |
| `code` | LLM-powered with tool access | Real evaluation |
| `nop` | Does nothing | Smoke test for infrastructure |

## Metrics

| Metric | Description |
|--------|-------------|
| Accuracy | Tasks passed / total tasks |
| Mean reward | Average score across all tasks (0-1 per task) |
| Pass@k | Probability of solving a task in k attempts |
| Token usage | Input/output tokens consumed |
| Wall-clock time | Total and per-task duration |
| Attempts | Number of agent invocations per task (SWE-bench) |
