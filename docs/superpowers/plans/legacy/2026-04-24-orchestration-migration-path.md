# Orchestration Migration Path & Cutover Strategy

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define and implement a safe, data-driven migration strategy from legacy execution to orchestration-default, with continuous monitoring, gradual rollout, and rollback capability.

**Architecture:** 
- **Phase 0 (Shadow Mode)**: Both paths run in parallel, collect metrics, NO behavior change
- **Phase 1 (Monitoring)**: Establish baseline metrics for success criteria (solve rate, cost, latency)
- **Phase 2 (Gradual Rollout)**: Route simple task categories to orchestration first (data_etl, simple_scripting)
- **Phase 3 (Complex Categories)**: Gradually roll out to complex categories as confidence grows
- **Phase 4 (Default)**: Orchestration becomes default; legacy path available only via flag
- **Phase 5 (Deprecation)**: Mark legacy path as deprecated, plan removal
- **Rollback**: Any metrics regression > 5% triggers automatic fallback to legacy path

**Tech Stack:** Rust, tokio, metrics collection (SQLite), tracing for observability

---

## File Structure

### New Files
- `crates/rustycode-classification/src/metrics_db.rs` — Metrics persistence & queries
- `crates/rustycode-classification/src/migration_state.rs` — Track migration phase & rollback flags
- `docs/orchestration-migration-runbook.md` — Operations runbook for monitoring & rollback

### Modified Files
- `crates/rustycode-cli/src/main.rs` — Add migration phase awareness
- `crates/rustycode-tui/src/config/` — Add migration settings

---

## Phase 0: Shadow Mode & Baseline (Tasks 1-3)

### Task 1: Setup Metrics Database & Dashboards

**Files:**
- Create: `crates/rustycode-classification/src/metrics_db.rs`
- Create: SQL migration file for metrics schema
- Modify: `crates/rustycode-classification/Cargo.toml`

**Goal:** Persistent metrics storage to track orchestration vs legacy performance over time.

- [ ] **Step 1: Define metrics schema**

```sql
CREATE TABLE execution_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL,
    task_description TEXT NOT NULL,
    classification TEXT,  -- "Mundane" or "Complex"
    execution_path TEXT,  -- "Orchestration" or "Legacy"
    outcome TEXT,         -- "Success", "Abandoned", "BudgetExceeded", etc.
    duration_ms INTEGER,
    cost_usd REAL,
    escalations INTEGER,  -- How many tier escalations
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    
    INDEX idx_path_outcome (execution_path, outcome),
    INDEX idx_created (created_at),
    INDEX idx_classification (classification)
);

CREATE TABLE migration_phase_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_phase TEXT,
    to_phase TEXT,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    metrics_summary TEXT,  -- JSON snapshot of metrics at phase transition
    triggered_by TEXT,     -- "manual", "metrics_threshold", "rollback", etc.
    
    INDEX idx_timestamp (timestamp)
);

CREATE TABLE rollback_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    phase TEXT,
    reason TEXT,
    metric_degradation TEXT,  -- e.g., "solve_rate: 85% -> 78%"
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    manual BOOLEAN DEFAULT FALSE,
    
    INDEX idx_timestamp (timestamp)
);
```

- [ ] **Step 2: Implement MetricsDb struct**

```rust
pub struct MetricsDb {
    conn: Arc<Mutex<Connection>>,
}

impl MetricsDb {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }
    
    pub fn record_execution(&self, metrics: &ExecutionMetrics) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO execution_metrics (task_id, task_description, classification, 
                execution_path, outcome, duration_ms, cost_usd, escalations)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![/* ... */],
        )?;
        Ok(())
    }
    
    pub fn get_phase_metrics(&self, phase: &str, hours: i32) -> Result<PhaseMetrics> {
        // Query all metrics from the past N hours
        // Calculate: solve_rate, avg_cost, avg_latency, escalation_frequency
    }
}

pub struct PhaseMetrics {
    pub solve_rate: f64,
    pub avg_cost_usd: f64,
    pub avg_duration_ms: f64,
    pub escalation_frequency: f64,
    pub sample_count: usize,
}
```

- [ ] **Step 3: Write test for metrics recording**

```rust
#[test]
fn test_record_and_query_metrics() {
    let db = MetricsDb::new(Path::new(":memory:")).unwrap();
    
    let metrics = ExecutionMetrics {
        task_id: "test_1".into(),
        execution_path: ExecutionPath::Orchestration,
        outcome: TaskOutcome::SuccessAtTier(2),
        cost_usd: 0.01,
        duration_ms: 1000,
        escalations: 0,
        // ...
    };
    
    db.record_execution(&metrics).unwrap();
    let phase = db.get_phase_metrics("Phase0", 24).unwrap();
    assert_eq!(phase.sample_count, 1);
    assert_eq!(phase.solve_rate, 1.0);
}
```

- [ ] **Step 4: Build and commit**

```bash
git add crates/rustycode-classification/src/metrics_db.rs
git commit -m "feat(metrics): add metrics database with phase tracking"
```

---

### Task 2: Define Success Criteria & Alert Thresholds

**Files:**
- Create: `docs/orchestration-migration-runbook.md`
- Create: `crates/rustycode-classification/src/success_criteria.rs`

**Goal:** Formalize what "success" means to determine when to progress to next phase.

- [ ] **Step 1: Define success metrics struct**

```rust
pub struct SuccessCriteria {
    pub solve_rate_min: f64,         // 95%+
    pub cost_efficiency_min: f64,    // 80%+ at Tier 2
    pub latency_max_ms: u64,         // reasonable time
    pub budget_waste_max: f64,       // 5% tolerance
    pub escalation_rate_max: f64,    // avoid excessive escalation
}

impl Default for SuccessCriteria {
    fn default() -> Self {
        Self {
            solve_rate_min: 0.95,
            cost_efficiency_min: 0.80,
            latency_max_ms: 5000,
            budget_waste_max: 0.05,
            escalation_rate_max: 0.20,  // 20% escalation acceptable
        }
    }
}

pub fn evaluate_metrics(metrics: &PhaseMetrics, criteria: &SuccessCriteria) -> MigrationDecision {
    if metrics.solve_rate < criteria.solve_rate_min {
        return MigrationDecision::Rollback { reason: "solve_rate_low" };
    }
    if metrics.avg_cost_usd > (1.0 - criteria.cost_efficiency_min) {
        return MigrationDecision::Rollback { reason: "cost_too_high" };
    }
    MigrationDecision::Continue
}

pub enum MigrationDecision {
    Continue,                           // Metrics look good
    Investigate,                        // Some metrics warn, need deeper look
    Rollback { reason: String },        // Abort, go back to Phase N-1
}
```

- [ ] **Step 2: Write runbook document**

```markdown
# Orchestration Migration Runbook

## Phases

### Phase 0: Shadow Mode (1-2 weeks)
- Both paths run in parallel
- No behavior change to users
- Collect 1000+ samples per category
- Establish baseline metrics

Success criteria:
- Orchestration solve rate ≥ 95%
- Cost within 10% of legacy
- No systematic errors

### Phase 1: Monitoring (ongoing)
- Establish prometheus/grafana dashboards
- Set up alerts for > 5% regression

### Phase 2: Gradual Rollout (category by category)
- Start with "simple" categories (file operations, simple scripting)
- Wait 3-5 days between category rollouts
- Monitor for regressions

### Phase 3-4: Full Rollout
- Route all complex categories
- Make orchestration default

## Alert Conditions

Trigger rollback if:
- Solve rate drops > 5% (e.g., 95% → 90%)
- Cost increases > 10%
- P95 latency exceeds baseline + 2x
- Tier 2 only success rate drops < 70%

## Rollback Procedure

1. Switch default back to legacy
2. Keep shadow mode running
3. Investigate metrics divergence
4. Fix issues
5. Re-enter Phase N-1
```

- [ ] **Step 3: Create success criteria config**

Add to config.yaml:

```yaml
migration:
  phase: ShadowMode  # Phases: ShadowMode, Monitoring, GradualRollout, Default, Deprecated
  success_criteria:
    solve_rate_min: 0.95
    cost_efficiency_min: 0.80
    latency_max_ms: 5000
    escalation_rate_max: 0.20
  rollback_enabled: true
  rollback_threshold_pct: 5
```

- [ ] **Step 4: Commit**

```bash
git add docs/orchestration-migration-runbook.md crates/rustycode-classification/src/success_criteria.rs
git commit -m "docs: add migration runbook and success criteria"
```

---

### Task 3: Implement Automatic Rollback Logic

**Files:**
- Modify: `crates/rustycode-classification/src/migration_state.rs` (new file)
- Modify: CLI/TUI execution paths to check rollback flags

**Goal:** Automatically detect metric degradation and trigger rollback.

- [ ] **Step 1: Create migration state tracker**

```rust
pub struct MigrationState {
    pub current_phase: MigrationPhase,
    pub rollback_triggered: bool,
    pub rollback_reason: Option<String>,
    pub last_phase_transition: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationPhase {
    ShadowMode,
    Monitoring,
    GradualRollout,
    Default,
    Deprecated,
}

impl MigrationState {
    pub fn evaluate_and_maybe_rollback(
        db: &MetricsDb,
        criteria: &SuccessCriteria,
    ) -> Result<bool> {
        // Query metrics from past 4 hours
        let metrics = db.get_phase_metrics("current", 4)?;
        
        match evaluate_metrics(&metrics, criteria) {
            MigrationDecision::Rollback { reason } => {
                // Log rollback event
                db.record_rollback(&reason)?;
                // Update migration state
                Ok(true)  // Signal to switch back to legacy
            }
            _ => Ok(false),  // No rollback needed
        }
    }
}
```

- [ ] **Step 2: Wire into CLI execution**

```rust
// In execute_task_with_orchestration:
let migration_state = MigrationState::load()?;

if migration_state.rollback_triggered {
    tracing::warn!("Orchestration rollback active: using legacy path");
    return execute_legacy(task).await;
}

if migration_state.current_phase == MigrationPhase::ShadowMode {
    // Run both paths
    run_shadow_mode(task).await
} else if migration_state.current_phase == MigrationPhase::Default {
    // Route through orchestration
    run_orchestration(task).await
} else {
    // Other phases: use classification
    run_with_classification(task).await
}
```

- [ ] **Step 3: Write test for automatic rollback**

```rust
#[test]
fn test_automatic_rollback_on_solve_rate_drop() {
    let db = MetricsDb::new(Path::new(":memory:")).unwrap();
    
    // Simulate 10 failed tasks
    for i in 0..10 {
        db.record_execution(&ExecutionMetrics {
            outcome: TaskOutcome::Abandoned { reason: "failed".into() },
            // ... other fields
        }).unwrap();
    }
    
    let criteria = SuccessCriteria { solve_rate_min: 0.95, ..Default::default() };
    let should_rollback = MigrationState::evaluate_and_maybe_rollback(&db, &criteria).unwrap();
    assert!(should_rollback);  // 0% solve rate < 95%
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-classification/src/migration_state.rs
git commit -m "feat(migration): implement automatic rollback on metric degradation"
```

---

## Phase 1: Establish Monitoring (Task 4)

### Task 4: Setup Observability & Dashboards

**Files:**
- Create: `docs/grafana-dashboard.json` or similar
- Create: Prometheus/OpenTelemetry config (if applicable)
- Modify: Execution paths to emit structured logs

**Goal:** Real-time visibility into orchestration vs legacy performance during shadow mode.

- [ ] **Step 1: Add structured logging to execution paths**

```rust
tracing::info!(
    task_id = %metrics.task_id,
    classification = ?metrics.classification,
    path = ?metrics.execution_path,
    outcome = ?metrics.outcome,
    duration_ms = metrics.duration_ms,
    cost_usd = metrics.cost_usd,
    escalations = metrics.escalations,
    "execution_completed"
);
```

- [ ] **Step 2: Emit metrics to collector**

If using Prometheus:

```rust
EXECUTION_DURATION.observe(metrics.duration_ms as f64);
EXECUTION_COST.observe(metrics.cost_usd);
EXECUTION_OUTCOME.with_label_values(&[&format!("{:?}", metrics.outcome)]).inc();
```

- [ ] **Step 3: Create dashboard template**

Document key panels:
- Solve rate by path (Orchestration vs Legacy)
- Cost per task by path
- Tier escalation frequency
- Latency percentiles (P50, P95, P99)

- [ ] **Step 4: Commit**

```bash
git add docs/ crates/
git commit -m "docs: add observability setup and dashboard templates"
```

---

## Phase 2: Gradual Rollout (Tasks 5-7)

### Task 5: Implement Category-Based Routing

**Files:**
- Modify: `crates/rustycode-classification/src/migration_state.rs`
- Modify: CLI/TUI execution to route by task category + phase

**Goal:** Route specific task categories to orchestration first, keep others on legacy.

- [ ] **Step 1: Add category routing logic**

```rust
pub struct CategoryRoutingPolicy {
    orchestration_categories: Vec<String>,  // ["data_etl", "simple_scripting"]
    legacy_categories: Vec<String>,
}

impl CategoryRoutingPolicy {
    pub fn should_use_orchestration(&self, category: &str) -> bool {
        self.orchestration_categories.contains(&category.to_string())
    }
    
    pub fn update_from_phase(phase: MigrationPhase) -> Self {
        match phase {
            MigrationPhase::ShadowMode => Self {
                orchestration_categories: vec![],  // None yet
                legacy_categories: vec!["*".into()],  // All use legacy
            },
            MigrationPhase::GradualRollout => Self {
                orchestration_categories: vec![
                    "data_etl".into(),
                    "simple_scripting".into(),
                ],
                legacy_categories: vec!["*".into()],
            },
            MigrationPhase::Default => Self {
                orchestration_categories: vec!["*".into()],  // All use orchestration
                legacy_categories: vec![],
            },
            _ => Self::default(),
        }
    }
}
```

- [ ] **Step 2: Detect task category from description**

```rust
fn infer_category(task: &str) -> String {
    if task.contains("json") || task.contains("csv") || task.contains("data") {
        "data_etl".to_string()
    } else if task.contains("bash") || task.contains("shell") || task.contains("command") {
        "simple_scripting".to_string()
    } else if task.contains("rust") || task.contains("refactor") {
        "rust_refactoring".to_string()
    } else {
        "unknown".to_string()
    }
}
```

- [ ] **Step 3: Route based on category + phase**

```rust
let category = infer_category(task);
let policy = CategoryRoutingPolicy::update_from_phase(migration_state.current_phase);

if policy.should_use_orchestration(&category) {
    run_orchestration(task).await
} else {
    run_legacy(task).await
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-classification/src/
git commit -m "feat(migration): add category-based routing for gradual rollout"
```

---

### Task 6: Implement Phase Progression Logic

**Files:**
- Modify: `crates/rustycode-classification/src/migration_state.rs`

**Goal:** Automatically progress through phases when metrics are healthy, or hold back if degradation detected.

- [ ] **Step 1: Define phase progression rules**

```rust
pub async fn evaluate_phase_progression(
    db: &MetricsDb,
    current_phase: MigrationPhase,
    criteria: &SuccessCriteria,
) -> Result<MigrationPhase> {
    // Get metrics for past 3-5 days
    let metrics = db.get_phase_metrics(&format!("{:?}", current_phase), 3 * 24)?;
    
    // Require 500+ samples before progressing
    if metrics.sample_count < 500 {
        return Ok(current_phase);  // Not enough data yet
    }
    
    match evaluate_metrics(&metrics, criteria) {
        MigrationDecision::Rollback { .. } => {
            // Don't progress, consider rollback
            Ok(current_phase)
        }
        MigrationDecision::Continue => {
            // Progress to next phase
            Ok(next_phase(current_phase))
        }
        MigrationDecision::Investigate => {
            // Hold at current phase, need investigation
            Ok(current_phase)
        }
    }
}

fn next_phase(current: MigrationPhase) -> MigrationPhase {
    match current {
        MigrationPhase::ShadowMode => MigrationPhase::Monitoring,
        MigrationPhase::Monitoring => MigrationPhase::GradualRollout,
        MigrationPhase::GradualRollout => MigrationPhase::Default,
        MigrationPhase::Default => MigrationPhase::Deprecated,
        MigrationPhase::Deprecated => MigrationPhase::Deprecated,
    }
}
```

- [ ] **Step 2: Add periodic phase evaluation**

Schedule a task to check phase progression every 6 hours during gradual rollout:

```rust
pub async fn start_phase_progression_monitor() {
    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
            
            let migration = MigrationState::load().unwrap();
            if migration.current_phase == MigrationPhase::GradualRollout {
                let next = evaluate_phase_progression(/* ... */).await.unwrap();
                if next != migration.current_phase {
                    tracing::info!("Progressing phase: {:?} -> {:?}", migration.current_phase, next);
                    migration.save_phase(next).unwrap();
                }
            }
        }
    });
}
```

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(migration): add automatic phase progression logic"
```

---

### Task 7: Category Rollout Schedule

**Files:**
- Create: `docs/rollout-schedule.md`
- Modify: `crates/rustycode-classification/src/migration_state.rs`

**Goal:** Define which categories roll out first and when.

- [ ] **Step 1: Write rollout schedule doc**

```markdown
# Orchestration Rollout Schedule

## Phase 2a: Week 1-2
Categories: `data_etl`, `simple_scripting`, `file_operations`
- Low risk, high success rate historically
- Mostly deterministic operations
- Few dependencies on external services

Rollback trigger: < 90% solve rate

## Phase 2b: Week 3-4
Categories: `rust_refactoring`, `bash_scripting`
- Medium complexity
- More state-dependent
- May require iteration

Rollback trigger: < 85% solve rate

## Phase 2c: Week 5-6
Categories: `debugging`, `architecture_design`
- High complexity
- Most escalation-heavy
- Most value from orchestration

Rollback trigger: < 80% solve rate

## Phase 3: Week 7+
All categories → Orchestration (default)
Legacy available via `--use-legacy` flag
```

- [ ] **Step 2: Update category routing per schedule**

Implement configuration file:

```yaml
migration:
  phase: GradualRollout
  rollout_categories:
    phase_2a:
      categories: ["data_etl", "simple_scripting"]
      started_at: "2026-04-25T00:00:00Z"
      duration_days: 14
      rollback_threshold_pct: 10
    phase_2b:
      categories: ["rust_refactoring", "bash_scripting"]
      starts_after_phase_2a: true
      duration_days: 14
      rollback_threshold_pct: 15
```

- [ ] **Step 3: Commit**

```bash
git add docs/rollout-schedule.md
git commit -m "docs: add category rollout schedule and risk assessment"
```

---

## Phase 3: Full Rollout & Deprecation (Tasks 8-9)

### Task 8: Migrate to Default (Orchestration) Execution

**Files:**
- Modify: CLI/TUI execution defaults
- Modify: Config defaults

**Goal:** Switch default path from legacy to orchestration, keep legacy as opt-out.

- [ ] **Step 1: Flip execution default**

```rust
// In execute_task_with_orchestration:
let use_legacy = std::env::var("RUSTYCODE_USE_LEGACY").is_ok();

if use_legacy {
    run_legacy(task).await
} else {
    run_orchestration(task).await  // New default
}
```

- [ ] **Step 2: Update config default**

```yaml
execution:
  default_path: "orchestration"  # was "legacy"
  allow_legacy: true             # Still available via flag
```

- [ ] **Step 3: Emit deprecation warning for legacy path**

```rust
if use_legacy {
    tracing::warn!("Using legacy execution path (RUSTYCODE_USE_LEGACY). This will be removed in v2.0");
}
```

- [ ] **Step 4: Update documentation**

Update CLI help text, README, guides to indicate orchestration is now default.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat: make orchestration the default execution path"
```

---

### Task 9: Legacy Path Deprecation & Cleanup

**Files:**
- Create: `DEPRECATION_NOTICE.md`
- Modify: Legacy execution code paths

**Goal:** Mark legacy path as deprecated, plan for removal in future major version.

- [ ] **Step 1: Create deprecation notice**

```markdown
# Deprecation Notice: Legacy Execution Path

**Status**: Deprecated as of v1.5 (2026-04-25)

**Timeline**:
- v1.5-1.9: Legacy path available via `RUSTYCODE_USE_LEGACY=1`, deprecation warnings emitted
- v2.0+: Legacy path removed entirely

**Migration Path**:
The orchestration system is now the default. It provides:
- Better error recovery (tier escalation)
- Cost optimization (80%+ on cheapest models)
- Learning loop (FailurePatternStore)
- Multi-model support (Claude, OpenAI, local)

**Feedback**: Report issues with orchestration to [GitHub Issues]
```

- [ ] **Step 2: Add version check to legacy path**

```rust
#[deprecated(
    note = "Legacy execution path is deprecated. Orchestration is now default."
)]
pub async fn execute_legacy(task: &str) -> Result<()> {
    // ...
}
```

- [ ] **Step 3: Update CHANGELOG**

Add entry indicating legacy path deprecation.

- [ ] **Step 4: Commit**

```bash
git add DEPRECATION_NOTICE.md
git commit -m "docs: mark legacy execution path as deprecated"
```

---

## Summary

**Total Tasks: 9**

- **Phase 0 (Shadow Mode)**: Tasks 1-3 — Metrics database, success criteria, automatic rollback
- **Phase 1 (Monitoring)**: Task 4 — Observability & dashboards
- **Phase 2 (Gradual Rollout)**: Tasks 5-7 — Category routing, phase progression, schedule
- **Phase 3 (Full Rollout)**: Tasks 8-9 — Make orchestration default, deprecate legacy

**Estimated Time:** 2-3 weeks (mostly monitoring and validation, not heavy coding)

**Deliverables:**
- ✅ Metrics database tracking all task executions
- ✅ Automatic rollback on > 5% metric degradation
- ✅ Real-time dashboards for phase monitoring
- ✅ Safe category-based rollout schedule
- ✅ Automatic phase progression logic
- ✅ Default switch to orchestration
- ✅ Legacy deprecation plan

**Success Metrics for Migration:**
- Orchestration solve rate ≥ 95% (target: 98%)
- Cost per task ≤ legacy (target: 15% savings)
- P95 latency within 1.5x of legacy
- Tier 2-only success rate ≥ 80%
- Zero manual rollbacks needed

**Post-Migration (v2.0):**
- Remove legacy execution path entirely
- Consolidate to orchestration-only codebase
- Optimize for orchestration (remove legacy branches)
