# Flexible Task Orchestration Framework for RustyCode

**Date**: 2026-04-20  
**Status**: Design Review  
**Owner**: RustyCode Pipeline Team  
**Version**: 1.0

---

## Executive Summary

This document specifies a **flexible task orchestration framework** for RustyCode that enables:
- **Time-based + dependency-driven scheduling** (e.g., "run at 8 AM, but only if upstream finished")
- **Content-addressed artifact management** (outputs are discoverable by type, time, and source)
- **Per-phase failure strategies** (hard block, soft degrade, checkpoint veto, custom retry)
- **Composable execution** (reuse phases across pipelines; support both RustyCode agents and external tools)

The framework powers use cases like **XMAN AM** (19-phase daily pipeline with 6 teams) and generic scheduled workflows without requiring hardcoded logic.

---

## 1. Architecture Overview

### 1.1 Layered Design

```
┌─────────────────────────────────────────────────────────┐
│  Application Layer (RustyCode TUI / CLI)                │
│  ↓ provides pipeline manifest (YAML/JSON)               │
└─────────────────────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────────────────────┐
│  DAG Executor (NEW)                                     │
│  • Loads manifest → builds DAG of phases                │
│  • Phase scheduler (time + dep resolution)              │
│  • Executes phases in order, manages async tasks        │
│  • Reports status/health                                │
└─────────────────────────────────────────────────────────┘
         ↓                ↓                    ↓
┌──────────────────┬──────────────────┬──────────────────┐
│ Phase Runner     │ Artifact Registry │ Tool Executor    │
│ (wraps existing  │ (NEW)             │ (existing        │
│  PipelineStep)   │ • Content-addr    │  ToolRegistry)   │
│                  │ • Query/discover  │                  │
│                  │ • Retention       │                  │
└──────────────────┴──────────────────┴──────────────────┘
         ↓                ↓                    ↓
┌─────────────────────────────────────────────────────────┐
│  Storage Layer                                          │
│  • In-memory artifact cache (fast queries, 7 days)      │
│  • Persistent artifact store (S3 / local FS)            │
│  • Signal/state journal (for recovery & audit)          │
└─────────────────────────────────────────────────────────┘
```

### 1.2 Core Components

| Component | Responsibility | New? |
|-----------|-----------------|------|
| **Phase** | Represents a logical execution unit with schedule, steps, dependencies, failure strategy | Yes |
| **DAG Executor** | Orchestrates phases based on time + dependency resolution | Yes |
| **Artifact Registry** | Stores, indexes, and queries outputs (content-addressed) | Yes |
| **Phase Runner** | Executes N steps within a phase (wraps existing `PipelineRegistry`) | Rename |
| **Tool Executor** | Runs individual tools/agents (existing `ToolRegistry`) | Unchanged |
| **Manifest** | YAML/JSON that declares phases, schedules, steps, failure strategies | Extend |

---

## 2. Phase Definition & Manifest Structure

### 2.1 Phase Concept

A **Phase** is the primary execution unit. It groups:
- **Schedule** (optional): When to run (cron expression)
- **Steps** (1+): Ordered execution steps (existing `PipelineStep` trait)
- **Dependencies** (optional): Hard/soft blocking on upstream phases
- **Artifacts** (1+): Declared outputs and retention policies
- **Failure Strategy**: How to handle failures (hard block, soft degrade, checkpoint veto, retry)

### 2.2 Manifest Schema (YAML)

```yaml
version: "1.0"
metadata:
  name: "XMAN_Investment_Pipeline"
  description: "Multi-team AI research platform"
  owner: "xman@example.com"

# Global failure strategy (can be overridden per-phase)
defaults:
  failure_strategy: "soft_degrade"
  artifact_retention_days: 30

phases:
  - id: "phase_530_data_load"
    description: "Load market data from OpenBB, FRED, DTCC"
    schedule: "30 5 * * *"              # 5:30 AM daily (cron format)
    failure_strategy: "hard_block"      # Any failure halts entire pipeline
    timeout_secs: 300
    
    steps:
      - id: "fetch_openbb_data"
        implementation: "openbb_fetcher"
        params:
          symbols: ["SPY", "QQQ", "TLT"]  # Load 200 tickers
          data_types: ["ohlcv", "options"]
        dependencies:
          - signal: "system_ready"
            blocking: "hard"
      
      - id: "fetch_fred_macro"
        implementation: "fred_api_fetcher"
        params:
          series: ["DGS10", "DGS2", "BAMLH0A0HYM2", "VIXCLS"]
        dependencies:
          - signal: "market_data_loaded"  # Must wait for openbb to complete
            blocking: "soft"               # Continue even if this fails
    
    artifacts_produced:
      - type: "market_data"
        format: "csv"
        description: "OHLCV data for all tickers"
        retention_days: 30
      
      - type: "macro_indicators"
        format: "json"
        description: "FRED macro series"
        retention_days: 60

  - id: "phase_800_team_pipelines"
    description: "Run X-MEN, One Piece, Asia Gods in parallel"
    schedule: "0 8 * * *"               # 8:00 AM daily
    
    # Soft dependency: must complete IF scheduled; if late, wait for it
    soft_deps:
      - phase: "phase_530_data_load"
    
    failure_strategy: "soft_degrade"    # Skip if missing data, continue
    timeout_secs: 1800
    parallel: true                       # Run all 3 teams concurrently
    
    steps:
      - id: "xmen_team_run"
        implementation: "agent_runner"
        params:
          team_id: "xmen"
          timeout_secs: 900
        dependencies:
          - signal: "market_data_loaded"
            blocking: "soft"
      
      - id: "one_piece_team_run"
        implementation: "agent_runner"
        params:
          team_id: "one_piece"
          timeout_secs: 900
        dependencies:
          - signal: "market_data_loaded"
            blocking: "soft"
      
      - id: "asia_gods_team_run"
        implementation: "agent_runner"
        params:
          team_id: "asia_gods"
          timeout_secs: 900
        dependencies:
          - signal: "market_data_loaded"
            blocking: "soft"
    
    artifacts_produced:
      - type: "team_report"
        format: "json"
        description: "Report from each team (xmen, one_piece, asia_gods)"
        retention_days: 90
        metadata_schema:
          team: "string"
          timestamp: "datetime"

  - id: "phase_915_synthesis"
    description: "PROFESSOR X synthesizes all team reports"
    schedule: "15 9 * * *"              # 9:15 AM daily
    
    # Hard dependency: MUST wait for phase 8:00 to complete
    hard_deps:
      - phase: "phase_800_team_pipelines"
    
    failure_strategy: "checkpoint_veto" # PROFESSOR X can veto; human reviews
    timeout_secs: 300
    
    steps:
      - id: "aggregate_team_reports"
        implementation: "report_aggregator"
        params:
          query:
            type: "team_report"
            after_phase: "phase_800_team_pipelines"
            max_age_secs: 3600
        dependencies:
          - signal: "team_reports_ready"
            blocking: "hard"
      
      - id: "synthesis_run"
        implementation: "agent_runner"
        params:
          agent_id: "PROFESSOR_X"
          checkpoint_enable: true  # Can veto output
        dependencies:
          - signal: "aggregation_complete"
            blocking: "hard"
    
    artifacts_produced:
      - type: "macro_synthesis"
        format: "html"
        description: "Where Are We? flagship report"
        retention_days: 365

# Optional: cross-phase timing constraints
constraints:
  - phase: "phase_915_synthesis"
    min_delay_after: "phase_800_team_pipelines"
    delay_secs: 0  # Run immediately after upstream
  
  - phase: "phase_915_synthesis"
    max_delay_after: "phase_800_team_pipelines"
    delay_secs: 600  # But no later than 10 min after (for SLA)
```

### 2.3 Key Semantics

**`hard_deps`** (Blocking Dependencies):
- Phase **cannot start** until all upstream phases complete
- If upstream fails and is `hard_block`, this phase also fails
- No schedule override: `hard_deps` always takes precedence

**`soft_deps`** (Optional Dependencies):
- Phase scheduled for time T
- If dependencies ready before T, execute at their completion
- If dependencies not ready by T, wait for them (schedule overridden)
- If dependencies fail with `soft_degrade`, continue anyway

**`failure_strategy`** (Per-Phase Error Handling):
- `hard_block`: Any failure halts entire pipeline
- `soft_degrade`: Failures log warnings; pipeline continues (possibly with degraded signals)
- `checkpoint_veto`: Step produces output but can flag for human review (pauses pipeline)
- `skip_on_fail`: If all retries exhausted, skip this phase and continue

---

## 3. DAG Execution Model

### 3.1 Pipeline DAG Structure

```rust
pub struct PipelineDAG {
    /// All phases, keyed by ID
    phases: HashMap<String, Phase>,
    
    /// Topologically sorted phase IDs (for deterministic order)
    phase_order: Vec<String>,
    
    /// Scheduler that emits "time to run" events
    scheduler: CronScheduler,
    
    /// Content-addressed artifact storage
    artifact_registry: ArtifactRegistry,
    
    /// Current execution state
    state: PipelineState,
}

pub enum PipelineState {
    /// Pipeline is waiting to be started
    Pending,
    
    /// Pipeline is running; these phases are active
    Running {
        active_phases: HashSet<String>,
        started_at: Instant,
    },
    
    /// Pipeline is paused (e.g., checkpoint veto)
    Paused {
        paused_at: Instant,
        reason: String,
    },
    
    /// Pipeline failed; indicates which phase and why
    Failed {
        phase_id: String,
        reason: String,
        failed_at: Instant,
    },
    
    /// All phases completed (may have warnings)
    Completed {
        completed_at: Instant,
        phase_results: HashMap<String, PhaseResult>,
    },
}

pub struct Phase {
    pub id: String,
    pub schedule: Option<CronExpression>,
    pub hard_deps: Vec<PhaseDependency>,
    pub soft_deps: Vec<PhaseDependency>,
    pub steps: Vec<Arc<dyn PipelineStep>>,
    pub failure_strategy: FailureStrategy,
    pub artifacts_produced: Vec<ArtifactSchema>,
    pub timeout_secs: u64,
    pub parallel: bool,
}

pub struct PhaseDependency {
    pub phase: String,  // phase ID
}

pub enum FailureStrategy {
    HardBlock {
        retry: RetryPolicy,
    },
    SoftDegrade {
        retry: RetryPolicy,
        fallback_artifact: Option<String>,
    },
    CheckpointVeto {
        retry: RetryPolicy,
    },
    SkipOnFail {
        retry: RetryPolicy,
    },
}

pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_secs: u64,  // exponential: attempt_n * backoff_secs
}
```

### 3.2 Execution Algorithm

```
LOOP (until all phases complete or fatal error):
  1. TICK: Check cron scheduler
     → Get phases due to run at current time
  
  2. For each scheduled phase P:
     a. RESOLVE_DEPENDENCIES(P):
        - Check all hard_deps: are they complete?
        - Check all soft_deps: are they complete?
        - Return: ReadyNow | WaitingFor([phases]) | BlockedByFailure
     
     b. If ReadyNow:
        - SPAWN_ASYNC_TASK to execute P
        - Track in state.active_phases
     
     c. If WaitingFor(upstream):
        - Log: "Phase P waiting for [upstream]"
        - Do nothing (wait until next tick)
     
     d. If BlockedByFailure(upstream):
        - Check upstream.failure_strategy
        - If hard_block: FAIL entire pipeline
        - If soft_degrade: Continue (P will run with degraded inputs)
  
  3. COLLECT_COMPLETED_TASKS:
     - Poll all active phase tasks
     - For each completed phase P:
       a. If P succeeded:
          - Register artifacts with ArtifactRegistry
          - Emit signals (implicit: all steps' provides())
          - Mark P as completed in state
       
       b. If P failed:
          - Check P.failure_strategy
          - If hard_block: FAIL entire pipeline
          - If soft_degrade: Mark as completed-degraded, continue
          - If checkpoint_veto: PAUSE pipeline, await human decision
  
  4. HEALTH_CHECK:
     - Log phase statuses
     - Report progress to UI
     - Check artifact registry size (cleanup if needed)
  
  5. SLEEP(tick_interval)  // e.g., 5 seconds
END LOOP

FINALIZE:
  - Aggregate all PhaseResult
  - Cleanup temporary artifacts
  - Return final pipeline status
```

### 3.3 Dependency Resolution Logic

```rust
impl PipelineDAG {
    async fn resolve_dependencies(&self, phase: &Phase) -> DependencyStatus {
        // Check hard blocking dependencies
        for hard_dep in &phase.hard_deps {
            match self.state.phase_status(&hard_dep.phase) {
                PhaseStatus::NotStarted => {
                    return DependencyStatus::BlockedByFailure(hard_dep.phase.clone());
                }
                PhaseStatus::Failed => {
                    return DependencyStatus::BlockedByFailure(hard_dep.phase.clone());
                }
                PhaseStatus::Completed | PhaseStatus::CompletedDegraded => {
                    // OK, continue
                }
                _ => {
                    return DependencyStatus::WaitingFor(vec![hard_dep.phase.clone()]);
                }
            }
        }

        // Check soft dependencies
        let mut waiting_for = Vec::new();
        for soft_dep in &phase.soft_deps {
            if !self.state.phase_completed_or_degraded(&soft_dep.phase) {
                waiting_for.push(soft_dep.phase.clone());
            }
        }

        if !waiting_for.is_empty() {
            return DependencyStatus::WaitingFor(waiting_for);
        }

        DependencyStatus::ReadyNow
    }
}

pub enum DependencyStatus {
    /// All dependencies met; phase can execute
    ReadyNow,
    
    /// Waiting for these phases to complete
    WaitingFor(Vec<String>),
    
    /// Upstream phase failed with hard_block
    BlockedByFailure(String),
}
```

---

## 4. Artifact System

### 4.1 Artifact Model

```rust
pub struct Artifact {
    /// Unique ID: "{source_phase}::{type_tag}::{timestamp}"
    pub id: String,
    
    /// Type tag for querying (e.g., "team_report", "macro_indicators")
    pub type_tag: String,
    
    /// Which phase produced this artifact
    pub source_phase: String,
    
    /// When it was created
    pub created_at: DateTime<Utc>,
    
    /// The actual data (JSON, CSV, HTML, Parquet, etc.)
    pub payload: ArtifactPayload,
    
    /// Custom metadata (e.g., {team: "xmen", format: "json"})
    pub metadata: HashMap<String, String>,
    
    /// How long to keep this artifact
    pub retention_days: u32,
}

pub enum ArtifactPayload {
    Json(serde_json::Value),
    Csv(String),
    Html(String),
    Parquet(Vec<u8>),
    Raw(Vec<u8>),
}

pub struct ArtifactQuery {
    /// Type tag to search for (required)
    pub type_tag: String,
    
    /// Only artifacts from this phase (optional)
    pub after_phase: Option<String>,
    
    /// Only artifacts created after this time (optional)
    pub after_time: Option<DateTime<Utc>>,
    
    /// Custom metadata filters (e.g., {team: "xmen"})
    pub filters: HashMap<String, String>,
}

pub struct ArtifactSchema {
    /// Artifact type (must match declaration in manifest)
    pub type_tag: String,
    
    /// Data format (json, csv, html, parquet, etc.)
    pub format: String,
    
    /// Description
    pub description: String,
    
    /// How many days to retain
    pub retention_days: u32,
    
    /// Expected metadata keys (e.g., {team: String})
    pub metadata_schema: Option<HashMap<String, String>>,
}
```

### 4.2 Artifact Registry

```rust
pub struct ArtifactRegistry {
    /// In-memory cache: {artifact_id -> Artifact}
    /// Holds last 7 days of artifacts for fast access
    memory: HashMap<String, Artifact>,
    
    /// Index for fast queries: {type_tag -> [artifact_ids]}
    index: HashMap<String, Vec<String>>,
    
    /// Time-based index: {(type_tag, hour) -> [artifact_ids]}
    time_index: HashMap<(String, u64), Vec<String>>,
    
    /// Persistent storage (S3, local FS, etc.)
    storage: Arc<dyn ArtifactStore>,
}

pub trait ArtifactStore: Send + Sync {
    async fn store(&self, artifact: &Artifact) -> Result<()>;
    async fn retrieve(&self, artifact_id: &str) -> Result<Artifact>;
    async fn list(&self, type_tag: &str) -> Result<Vec<String>>;
    async fn delete(&self, artifact_id: &str) -> Result<()>;
}

impl ArtifactRegistry {
    /// Register a new artifact
    pub async fn register(&mut self, artifact: Artifact) -> Result<()> {
        // 1. Add to memory
        self.memory.insert(artifact.id.clone(), artifact.clone());
        
        // 2. Index by type
        self.index
            .entry(artifact.type_tag.clone())
            .or_default()
            .push(artifact.id.clone());
        
        // 3. Index by time
        let hour = artifact.created_at.timestamp() / 3600;
        self.time_index
            .entry((artifact.type_tag.clone(), hour as u64))
            .or_default()
            .push(artifact.id.clone());
        
        // 4. Persist to storage
        self.storage.store(&artifact).await?;
        
        Ok(())
    }

    /// Query artifacts
    pub async fn query(&self, q: &ArtifactQuery) -> Result<Vec<Artifact>> {
        // 1. Get candidates by type from index
        let candidates = self.index
            .get(&q.type_tag)
            .ok_or(anyhow!("No artifacts of type: {}", q.type_tag))?;
        
        // 2. Filter by phase
        let filtered: Vec<_> = candidates.iter()
            .filter_map(|id| {
                self.memory.get(id).and_then(|art| {
                    if let Some(ref phase) = q.after_phase {
                        if art.source_phase != *phase {
                            return None;
                        }
                    }
                    if let Some(ref time) = q.after_time {
                        if art.created_at < *time {
                            return None;
                        }
                    }
                    Some(art.clone())
                })
            })
            .collect();
        
        // 3. Filter by custom metadata
        let final_results: Vec<_> = filtered.iter()
            .filter(|art| {
                q.filters.iter().all(|(k, v)| {
                    art.metadata.get(k).map_or(false, |val| val == v)
                })
            })
            .cloned()
            .collect();
        
        Ok(final_results)
    }

    /// Cleanup: remove artifacts past retention
    pub async fn cleanup(&mut self) -> Result<()> {
        let cutoff = Utc::now() - Duration::days(30);
        let mut to_delete = Vec::new();
        
        for (id, artifact) in &self.memory {
            if artifact.created_at < cutoff {
                to_delete.push(id.clone());
            }
        }
        
        for id in to_delete {
            self.memory.remove(&id);
            self.storage.delete(&id).await?;
        }
        
        Ok(())
    }
}
```

### 4.3 Usage in Phases

```yaml
# In phase 9:15 synthesis:
steps:
  - id: "aggregate_team_reports"
    implementation: "report_aggregator"
    params:
      query:
        type: "team_report"
        after_phase: "phase_800_team_pipelines"
        filters:
          {} # Get all teams
```

The `report_aggregator` step receives a **PipelineContext** with reference to `artifact_registry`:

```rust
// Inside report_aggregator.execute()
pub fn execute(&self, ctx: &mut PipelineContext) -> Result<()> {
    let query = ArtifactQuery {
        type_tag: "team_report".to_string(),
        after_phase: Some("phase_800_team_pipelines".to_string()),
        filters: HashMap::new(),
    };
    
    let artifacts = ctx.artifact_registry.query(&query)?;
    
    // artifacts[0] = xmen report
    // artifacts[1] = one_piece report
    // artifacts[2] = asia_gods report
    
    let aggregated = aggregate_reports(artifacts)?;
    
    // Register aggregated artifact
    let agg_artifact = Artifact {
        id: format!("phase_915:aggregation:{}", Utc::now()),
        type_tag: "aggregated_reports".to_string(),
        source_phase: "phase_915_synthesis".to_string(),
        created_at: Utc::now(),
        payload: ArtifactPayload::Json(aggregated),
        metadata: HashMap::new(),
        retention_days: 90,
    };
    
    ctx.artifact_registry.register(agg_artifact).await?;
    ctx.signals.insert(Signal("aggregation_complete".to_string()));
    
    Ok(())
}
```

---

## 5. Failure & Retry Strategy

### 5.1 Per-Phase Failure Modes

Each phase declares **one** failure strategy:

```rust
pub enum FailureStrategy {
    /// Any failure in this phase halts the entire pipeline
    /// Retry configured; if retries exhausted, pipeline fails.
    HardBlock {
        retry: RetryPolicy,
    },
    
    /// Failure logs a warning; pipeline continues
    /// Retries configured; if exhausted, use fallback artifact or skip
    SoftDegrade {
        retry: RetryPolicy,
        fallback_artifact: Option<String>,  // artifact ID to use if phase fails
    },
    
    /// Failure triggers human checkpoint
    /// Step produces output but flags for review
    /// Pipeline pauses; human approves/rejects
    CheckpointVeto {
        retry: RetryPolicy,
    },
    
    /// Skip this phase if retries exhausted
    /// Pipeline continues to next phase
    SkipOnFail {
        retry: RetryPolicy,
    },
}

pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_secs: u64,  // Exponential: attempt_n * backoff_secs
}
```

### 5.2 Retry & Execution Flow

```rust
pub async fn execute_phase_with_retry(
    phase: &Phase,
    ctx: &mut PipelineContext,
) -> Result<PhaseResult> {
    let max_attempts = match &phase.failure_strategy {
        FailureStrategy::HardBlock { retry } => retry.max_attempts,
        FailureStrategy::SoftDegrade { retry, .. } => retry.max_attempts,
        FailureStrategy::CheckpointVeto { retry } => retry.max_attempts,
        FailureStrategy::SkipOnFail { retry } => retry.max_attempts,
    };

    for attempt in 0..max_attempts {
        match execute_phase_steps(phase, ctx).await {
            Ok(result) => {
                return Ok(PhaseResult::Success(result));
            }
            Err(e) => {
                tracing::warn!(
                    "Phase {} attempt {}/{} failed: {}",
                    phase.id, attempt + 1, max_attempts, e
                );

                // Handle based on failure strategy
                match &phase.failure_strategy {
                    FailureStrategy::HardBlock { .. } => {
                        if attempt == max_attempts - 1 {
                            return Err(anyhow!("Phase {} failed (hard block)", phase.id));
                        }
                    }
                    FailureStrategy::SoftDegrade { fallback_artifact, .. } => {
                        if attempt == max_attempts - 1 {
                            if let Some(fallback_id) = fallback_artifact {
                                tracing::warn!("Using fallback artifact: {}", fallback_id);
                                let fallback = ctx.artifact_registry.get(fallback_id).await?;
                                ctx.artifact_registry.register(fallback).await?;
                                return Ok(PhaseResult::Degraded {
                                    error: e,
                                    used_fallback: true,
                                });
                            } else {
                                return Ok(PhaseResult::Degraded {
                                    error: e,
                                    used_fallback: false,
                                });
                            }
                        }
                    }
                    FailureStrategy::CheckpointVeto { .. } => {
                        if attempt == max_attempts - 1 {
                            return Ok(PhaseResult::VetoPending {
                                error: e,
                                requires_approval: true,
                            });
                        }
                    }
                    FailureStrategy::SkipOnFail { .. } => {
                        if attempt == max_attempts - 1 {
                            return Ok(PhaseResult::Skipped {
                                reason: format!("Failed after {} attempts", max_attempts),
                            });
                        }
                    }
                }

                // Wait before retry
                let backoff = get_backoff_duration(attempt, &phase.failure_strategy);
                tokio::time::sleep(backoff).await;
            }
        }
    }

    unreachable!()
}

pub enum PhaseResult {
    Success(String),
    Degraded {
        error: anyhow::Error,
        used_fallback: bool,
    },
    VetoPending {
        error: anyhow::Error,
        requires_approval: bool,
    },
    Skipped {
        reason: String,
    },
}
```

---

## 6. Integration with Existing RustyCode Code

### 6.1 Backwards Compatibility

**Your existing code is preserved:**

| Component | Change | Impact |
|-----------|--------|--------|
| `PipelineStep` trait | None | Steps remain unchanged; phases wrap N steps |
| `PipelineRegistry` | Renamed to `PhaseRunner` | Wraps the existing `run_available()` logic |
| `ToolRegistry` | None | Tool execution unchanged |
| `PipelineContext` | Enhanced | Add `artifact_registry: Arc<ArtifactRegistry>` |
| `Manifest` | Extended | Now includes phases + schedules + failure strategies |
| `PipelineGuardian` | Enhanced | Monitors phase health + artifact cleanup |

### 6.2 Migration Path

1. **Phase 1** (Week 1): Implement `DAG Executor`, `Artifact Registry`, `Phase` types
2. **Phase 2** (Week 2): Extend `Manifest` loader; integrate with existing `PipelineStep` trait
3. **Phase 3** (Week 3): Refactor `PipelineRegistry` → `PhaseRunner`; wire up scheduling
4. **Phase 4** (Week 4): Test with XMAN AM manifest; iterate on UX

### 6.3 New Crates & Modules

```
crates/rustycode-tui/src/app/pipeline/
├── executor.rs          # DAG Executor (NEW)
├── artifact_registry.rs # Artifact Registry (NEW)
├── phase.rs             # Phase types (NEW)
├── manifest.rs          # Manifest loader (EXTEND)
├── runner.rs            # Phase Runner (RENAME from registry.rs)
├── tool_registry.rs     # Tool Registry (UNCHANGED)
├── tools/               # Tool implementations
├── guardian.rs          # Guardian (ENHANCE)
└── mod.rs
```

---

## 7. Data Flow Example: XMAN AM

### 7.1 Timeline

```
5:30 AM
  ├─ phase_530_data_load scheduled
  ├─ No deps, execute immediately
  ├─ Produces: market_data, macro_indicators
  └─ Signals: market_data_loaded

8:00 AM
  ├─ phase_800_team_pipelines scheduled
  ├─ Soft dep on phase_530 (if late, wait)
  ├─ Execute XMEN, One Piece, Asia Gods in parallel
  ├─ Each produces: team_report (metadata: {team: "xmen|one_piece|asia_gods"})
  ├─ Signals: team_reports_ready
  └─ Register 3 artifacts with ArtifactRegistry

9:15 AM
  ├─ phase_915_synthesis scheduled
  ├─ Hard dep on phase_800 (wait if not done)
  ├─ Step 1: aggregator queries ArtifactRegistry
  │   └─ Query: {type: "team_report", after_phase: "phase_800_team_pipelines"}
  │   └─ Returns: [xmen_report, one_piece_report, asia_gods_report]
  ├─ Step 2: PROFESSOR X synthesizes all 3 reports
  ├─ Produces: macro_synthesis (HTML report: "Where Are We?")
  └─ Signals: synthesis_complete

Weekly
  ├─ DA_VINCI thesis builder runs (separate pipeline)
  ├─ Queries all week's artifacts: {type: "macro_synthesis"}
  └─ Publishes weekly thesis update
```

### 7.2 Artifact Queries in Steps

```rust
// In phase 915 step: aggregate_reports

// Query: Get all team reports from phase 8:00
let query = ArtifactQuery {
    type_tag: "team_report".to_string(),
    after_phase: Some("phase_800_team_pipelines".to_string()),
    filters: HashMap::new(),  // Get all teams
};

let artifacts = ctx.artifact_registry.query(&query)?;
// artifacts = [
//   { id: "phase_800::team_report::xmen::2026-04-20T08:45:00Z", ... },
//   { id: "phase_800::team_report::one_piece::2026-04-20T08:50:00Z", ... },
//   { id: "phase_800::team_report::asia_gods::2026-04-20T08:55:00Z", ... },
// ]
```

---

## 8. Error Scenarios & Handling

### 8.1 Scenario: OpenBB Down at 5:30 AM

```yaml
Phase: phase_530_data_load
  failure_strategy: "hard_block"
  
Result:
  - Step: fetch_openbb_data FAILS
  - Retry: 3 attempts, 60s backoff → all fail
  - Decision: hard_block → HALT ENTIRE PIPELINE
  - Signals: None emitted (market_data_loaded not set)
  - Pipeline State: Failed { phase_id: "phase_530_data_load", reason: "..." }
  - Operator Action: Investigate OpenBB, manually restart pipeline
```

### 8.2 Scenario: Bank PDFs Missing at 6:00 AM

```yaml
Phase: phase_600_research_pdfs
  failure_strategy: "soft_degrade"
  
Result:
  - Step: fetch_bank_pdfs FAILS
  - Retry: 2 attempts → fail
  - Fallback: use_cached (yesterday's PDFs)
  - Decision: soft_degrade → continue
  - Signals: bank_pdfs_loaded emitted (but with fallback marker)
  - Pipeline State: Running (continues to next phase)
  - Log: WARN "Bank PDFs missing; using cached"
```

### 8.3 Scenario: VIVI Veto at 9:15 AM

```yaml
Phase: phase_915_synthesis
  failure_strategy: "checkpoint_veto"
  
Result:
  - Step: synthesis_run COMPLETES with output
  - Checkpoint Agent VIVI: reviews output, flags as "crowding detected"
  - Decision: veto (not a failure, but a hold)
  - Pipeline State: Paused { reason: "VIVI veto: crowding detected" }
  - Operator Action: Review VIVI's flag, approve or reject synthesis
```

---

## 9. Testing & Observability

### 9.1 Testability

- **Unit**: Test `PipelineDAG` dependency resolution with mock phases
- **Integration**: Test phase execution with mock steps
- **E2E**: Run XMAN AM manifest; verify all artifacts produced

### 9.2 Observability

- **Logs**: Each phase execution produces structured logs (phase_id, attempt, duration, status)
- **Metrics**: Prometheus counters for:
  - Phases executed (total, success, failure, degraded, skipped)
  - Artifact queries (type, count)
  - Execution duration per phase
- **UI**: TUI dashboard showing:
  - Pipeline DAG (visual)
  - Current phase status (running, waiting, failed, paused)
  - Recent artifacts (with retention countdown)
  - Alerts (veto flags, hard blocks)

---

## 10. Future Extensions

1. **Cross-Pipeline Dependencies**: Phase in Pipeline A depends on phase in Pipeline B
2. **Conditional Phases**: "Run phase X only if signal Y > threshold"
3. **Dynamic Phases**: Generate phases at runtime (e.g., "run analysis for each ticker")
4. **Artifact Versioning**: Multiple versions of same artifact; query by version
5. **SLA Tracking**: "Phase must complete by X time; alert if trending late"
6. **A/B Testing Phases**: Run variant A and B in parallel; compare outputs

---

## 11. Success Criteria

- ✅ XMAN AM manifest loads without hardcoded logic
- ✅ All 19 phases execute in correct order (time + dependency)
- ✅ Artifact queries are fast (<100ms) for 60+ daily artifacts
- ✅ Failures are handled per strategy (hard block, soft degrade, veto)
- ✅ Pipeline health is observable (logs + TUI dashboard)
- ✅ New pipelines can be added by writing manifest (no code changes)

---

## 12. Open Questions

1. **Artifact Storage**: S3 vs. local FS vs. hybrid? (Depends on scale/cost)
2. **Cron Precision**: 1-minute granularity sufficient, or need finer?
3. **Veto UI**: How should checkpoint veto be presented in TUI?
4. **Cross-Env**: Can phases target remote execution (e.g., Lambda, K8s job)?
5. **State Recovery**: On restart, resume from last completed phase or re-run all?

---

## References

- XMAN_AM_Pipeline_Draft_2026-04-19.html
- XMAN_AM_Dependency_Flow_2026-04-19.html
- RustyCode CLAUDE.md (Rust Edition 2021, strict lints, error handling patterns)
