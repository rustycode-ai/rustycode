# rtk-bench: Harbor Replacement Design

**Date:** 2026-04-26
**Status:** Approved
**Scope:** Enhance existing rtk-bench crate to replace Harbor Python framework for TB2 evaluation

## Problem

Harbor (Python) crashes on QEMU x86_64 on macOS arm64 when running TB2 verifier tasks. 7/10 verifiers crash with SIGSEGV importing numpy/pyarrow. Testing iteration is slow due to Python overhead + QEMU emulation layer.

## Solution

Enhance the existing `rustycode-bench` crate (~8700 LOC, 157 tests) with Harbor's missing abstractions, using bollard for Docker API calls. Two parallel workstreams.

## Architecture

### Module Layout

```
crates/rustycode-bench/src/
├── lib.rs                 # Public API (existing, extend exports)
├── main.rs                # CLI entry (existing)
├── config.rs              # BenchConfig (existing, extend)
├── task/
│   ├── mod.rs             # ResolvedTask parser (existing, extend)
│   └── steps.rs           # NEW: Multi-step task support
├── environment/
│   ├── mod.rs             # NEW: BenchEnvironment trait definition
│   ├── native.rs          # Refactored from runner/native.rs
│   └── docker.rs          # NEW: bollard-based DockerEnvironment
├── agent/
│   ├── mod.rs             # Agent trait (existing)
│   ├── code_agent.rs      # Existing
│   ├── oracle.rs          # Existing
│   └── nop.rs             # Existing
├── verifier/
│   ├── mod.rs             # NEW: Verifier trait + reward parsing
│   ├── reward.rs          # NEW: reward.txt + ctrf.json parsing
│   └── pass_at_k.rs       # NEW: pass@k computation
├── trial/
│   ├── mod.rs             # NEW: Trial lifecycle orchestrator
│   ├── hooks.rs           # NEW: Event hook system
│   ├── retry.rs           # NEW: Retry with backoff + filtering
│   └── artifacts.rs       # NEW: Artifact collection
├── runner/
│   ├── mod.rs             # Runner trait (existing, simplified)
│   ├── native.rs          # Existing (delegates to Environment)
│   └── docker.rs          # Replaced by environment/docker.rs
├── job/
│   ├── mod.rs             # Job orchestrator (existing, extend)
│   └── result.rs          # Results (existing, extend with pass@k)
├── dataset/mod.rs         # Dataset discovery (existing)
├── history/mod.rs         # History store (existing)
└── report/                # Report formatting (existing)
    ├── mod.rs
    ├── pretty.rs
    ├── json.rs
    └── markdown.rs
```

### Environment Trait

Unified interface for native and Docker execution:

```rust
#[async_trait]
pub trait BenchEnvironment: Send + Sync {
    async fn start(&mut self, task: &ResolvedTask) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn exec(&mut self, cmd: &str, opts: ExecOpts) -> Result<ExecResult>;
    async fn upload_file(&mut self, src: &Path, dest: &Path) -> Result<()>;
    async fn download_file(&mut self, src: &Path, dest: &Path) -> Result<()>;
    fn workdir(&self) -> &Path;
}

pub struct ExecOpts {
    pub timeout: Duration,
    pub env: HashMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub user: Option<String>,
}

pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
}
```

### DockerEnvironment (bollard)

- Build from Dockerfile or pull pre-built `docker_image` from task.toml
- Platform-aware: detect host arch via `uname -m`. On macOS arm64 with Docker, use `linux/amd64` platform via bollard's `ContainerConfig::platform` to match QEMU emulation. On native linux, use host arch.
- Healthcheck polling with configurable interval
- Container exec with timeout: wrap bollard exec stream in `tokio::time::timeout()`. On timeout, kill exec process via Docker API to prevent leaked containers
- Docker availability check: `DockerEnvironment::new()` attempts bollard connect; returns clear error if Docker daemon unavailable
- File transfer via bollard's tar archive API (upload/download)
- Resource limits: CPU, memory, storage from task.toml [environment]
- Bind-mount log directories: /logs/agent/, /logs/verifier/, /logs/artifacts/

### Verifier v2

```rust
pub trait Verifier: Send + Sync {
    async fn verify(&self, env: &mut dyn BenchEnvironment, task: &ResolvedTask) -> Result<VerifierResult>;
}

pub struct VerifierResult {
    pub rewards: HashMap<String, f64>,
    pub ctrf: Option<CtrfReport>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

pub struct CtrfReport {
    pub results: CtrfResults,  // matches CTRF spec top-level "results" key
    pub tests: Vec<CtrfTest>,
}

pub struct CtrfResults {
    pub summary: CtrfSummary,  // passed, failed, skipped, pending, other
    pub tool: Option<CtrfTool>,
}

pub struct CtrfSummary {
    pub tests: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub pending: usize,
}

pub struct CtrfTest {
    pub name: String,
    pub status: CtrfTestStatus,
    pub duration_ms: Option<u64>,
    pub message: Option<String>,
}

pub enum CtrfTestStatus { Passed, Failed, Skipped, Pending, Other }
}
```

Reward parsing:
- `reward.txt`: single float (0.0 or 1.0 for TB2)
- `reward.json`: dict of named rewards (for future multi-reward tasks)
- `ctrf.json`: structured pytest results with per-test pass/fail details

### Pass@k

```rust
pub fn pass_at_k(trial_results: &[TrialResult]) -> HashMap<usize, f64>;
```

Computes pass@k for k = 2, 5, 10, 20, 50 using the standard formula:
```rust
fn pass_at_k_for_task(n: usize, c: usize, k: usize) -> f64 {
    if n - c < k { return 1.0; }
    let product: f64 = (0..k)
        .map(|i| (n - c - i) as f64 / (n - i) as f64)
        .product();
    1.0 - product
}
```

Grouped by (agent_name, model_name) eval keys. Each group computes pass@k across all tasks in that group.

### Trial Lifecycle

```rust
pub struct Trial {
    pub task: ResolvedTask,
    pub agent: Box<dyn Agent>,
    pub environment: Box<dyn BenchEnvironment>,
    pub verifier: Box<dyn Verifier>,
    pub hooks: Vec<Box<dyn TrialHook>>,
    pub retry_config: RetryConfig,
}

pub enum TrialEvent {
    Start,
    EnvironmentStart,
    AgentSetup,
    AgentStart,
    AgentEnd,
    VerifyStart,
    VerifyEnd,
    End(TrialResult),
    Error(anyhow::Error),
}

pub trait TrialHook: Send + Sync {
    async fn on_event(&mut self, event: &TrialEvent) -> Result<()>;
}
```

Retry with exponential backoff:
- Base delay: 1s, max delay: 60s, multiplier: 2, jitter: +/- 20%
- Exception filtering: include/exclude regex patterns on error message
- Max retries configurable (default: 3)

### Multi-step Tasks

```rust
pub struct TaskStep {
    pub name: String,
    pub instruction: String,
    pub setup_script: Option<PathBuf>,
    pub test_script: PathBuf,
    pub timeout: Duration,
    pub min_reward: Option<f64>,
}

pub struct StepResult {
    pub step_name: String,
    pub reward: f64,
    pub timed_out: bool,
    pub error: Option<String>,
}
```

Steps execute sequentially. If a step's reward < min_reward, return that reward immediately and do not execute remaining steps. Mark trial as partial completion.

### Artifact Collection

- Convention: collect `/logs/artifacts/` from container
- Config-driven: additional paths from task.toml
- Exclude patterns for large/generated files (gitignore-style glob via `ignore` crate)
- Download via tar archive

## Dependencies

```toml
# Added to crates/rustycode-bench/Cargo.toml
bollard = "0.17"          # Docker API client (Rust 1.70+ compatible)
tokio-util = "0.7"        # Async codec utilities
flate2 = "0.4"            # gzip for tar archives
tar = "0.4"               # tar archive creation/extraction
ignore = "0.4"            # gitignore-style glob for artifact exclusion
```

## Parallel Workstreams

### Stream 1: Docker Engine

Unblocks full TB2 evaluation with proper container lifecycle.

1. Add bollard dependency
2. Implement `Environment` trait in `environment/mod.rs`
3. Implement `DockerEnvironment` in `environment/docker.rs`
   - Image build/pull with platform selection
   - Container create/start with resource limits
   - Exec with timeout + streaming output
   - File upload/download via tar
4. Refactor `runner/docker.rs` to use `DockerEnvironment`
5. Integration tests against local Docker daemon

### Stream 2: Scoring & Metrics

Works with native mode immediately, no Docker needed.

1. Implement `Verifier` trait + reward parsing in `verifier/`
2. Implement `pass@k` computation in `verifier/pass_at_k.rs`
3. Implement hook system in `trial/hooks.rs`
4. Implement retry logic in `trial/retry.rs`
5. Implement artifact collection in `trial/artifacts.rs`
6. Implement multi-step task support in `task/steps.rs`
7. Extend `Job` orchestrator to use new Trial lifecycle
8. Unit tests for each module

### Stream 3: Integration & CLI

1. Wire new modules into existing CLI
2. Add `--platform` flag for cross-arch Docker
3. Add `--pass-at-k` flag for pass@k computation
4. Add `--retry` and `--retry-filter` flags
5. Update report formatters with new fields
6. End-to-end test: run full TB2 dataset

## Compatibility

- All 89 TB2 tasks parseable (task.toml format unchanged)
- Existing native mode preserved (12/76 oracle pass)
- Existing CLI flags preserved
- Result history format backward-compatible (new fields optional)
- Harbor-compatible output: `reward.txt` binary scoring

## Success Criteria

1. Docker mode runs 89 TB2 tasks without QEMU crashes on arm64 Mac
2. Native mode continues working with 12+ oracle passes
3. pass@k computation matches Harbor's implementation
4. All existing 157 tests continue passing
5. 80%+ test coverage on new code
6. CLI iteration cycle <30s for single task (vs 2-5min with Harbor)
