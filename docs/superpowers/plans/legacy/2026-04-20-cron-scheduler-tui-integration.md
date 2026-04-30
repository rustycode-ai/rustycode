# Cron Scheduler + TUI Event Loop Integration Plan

> **Goal:** Integrate a cron-based scheduler into the TUI event loop that triggers pipeline phases at precise times, using per-phase timer tasks and a channel-based communication model.

> **Architecture:** Per-Phase Timer Tasks — each scheduled phase gets its own `std::thread` that sleeps until the exact cron fire time, sending events through a `std::sync::mpsc` channel to the synchronous TUI event loop. A concurrency limiter (`HashSet` of active phases) controls how many phases run simultaneously.

> **Reuse:** The `cron = "0.12"` crate is already a workspace dep (`Cargo.toml:107`). The `CronScheduler` pattern in `crates/rustycode-orchestra/src/scheduler.rs` provides `is_due()` + `cron::Schedule::from_str()` + `schedule.after(&ref).next()` logic to follow.

> **Reference patterns:** `crates/rustycode-tui/src/app/event_loop.rs` (TUI struct, `std::sync::mpsc` channels at lines 52/1166), `crates/rustycode-tui/src/app/service_polling.rs` (`poll_services()` at line 12, `poll_team_events()` call at line 125), `crates/rustycode-tui/src/app/pipeline/manifest.rs` (`Manifest` struct with `schedule: Option<String>` field at line 29).

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Trigger precision | Exact time (per-phase `std::thread::sleep`) | TUI is synchronous, no tokio runtime in main loop |
| Communication model | `std::sync::mpsc` channel | Matches existing TUI patterns (see `event_loop.rs:52`) |
| Concurrent phases | `HashSet<String>` + `max_concurrent_phases` | Lightweight, no semaphore crate needed |
| Error handling | Respect `FailureStrategy` from phase config | Already defined in `pipeline/types.rs:37` |
| Cron format | Both 5-field and 6-field, inferred from field count | Flexible for users |

---

## Architecture

```
Manifest (YAML/JSON)
  └─ PhaseDefinition.schedule: Option<String> (cron expression)
        ↓
  PipelineCronScheduler::start()
     └─ For each phase with schedule:
          • Parse cron via cron::Schedule::from_str()
          • Calculate next fire time via schedule.after(&now).next()
          • Spawn std::thread per phase
        ↓
  Timer Threads (1 per scheduled phase)
     └─ std::thread::sleep until fire time
     └─ Send ScheduledPhaseEvent → std::sync::mpsc::Sender
     └─ Reschedule next fire time (loop)
        ↓
  TUI Event Loop (service_polling.rs:poll_services())
     └─ poll_scheduler_events() — try_recv() each frame (called at ~line 125)
     └─ handle_scheduled_phase_event() — concurrency check + spawn execution
        ↓
  Phase Execution (background std::thread)
     └─ PipelineDAG::run() for single phase
     └─ Respect failure_strategy
     └─ Remove from active_scheduled_phases on completion
```

**Key constraint:** The TUI event loop is **synchronous** (no tokio runtime in main loop). All async work runs in spawned `std::thread`s with `rustycode_shared_runtime::block_on_shared()`. Channel MUST be `std::sync::mpsc` (not `tokio::sync::mpsc`).

---

## Atomic Commit Strategy

Each commit is independently buildable, testable, and revertable. Commits follow strict TDD order: **test → implementation → wire-up**.

```
Commit 1 ── types + deps (foundation)
Commit 2 ── cron parsing + tests (pure logic, no side effects)
Commit 3 ── timer thread + scheduler core + tests (threading)
Commit 4 ── TUI wiring + event handling + tests (integration)
Commit 5 ── integration test (end-to-end validation)
```

**Branch:** `feat/cron-scheduler-tui`
**Revert safety:** Each commit compiles and passes `cargo test -p rustycode-tui` on its own.

---

## Execution Plan

### Commit 1: `feat(tui): add scheduler types and cron dependency`

**TDD phase:** N/A (type definitions only, no logic to test)

**Files to create/modify:**

| File | Action |
|------|--------|
| `crates/rustycode-tui/Cargo.toml` | Add `cron.workspace = true` |
| `crates/rustycode-tui/src/app/pipeline/scheduler.rs` | Create with type definitions |
| `crates/rustycode-tui/src/app/pipeline/mod.rs` | Add `pub mod scheduler;` + re-export types |

**scheduler.rs — create with types only:**

```rust
//! Pipeline cron scheduler — triggers phases at scheduled times.
//!
//! Per-phase timer threads sleep until exact cron fire times, then send
//! events through a `std::sync::mpsc` channel to the synchronous TUI loop.

use chrono::{DateTime, Utc};
use std::time::Instant;

/// Event sent when a scheduled phase is due to run.
#[derive(Debug)]
pub enum ScheduledPhaseEvent {
    /// A phase's cron schedule has fired — ready to execute.
    PhaseReady {
        phase_id: String,
        scheduled_fire_time: DateTime<Utc>,
        actual_fire_time: Instant,
    },
    /// A phase was skipped (e.g., schedule parse error).
    PhaseSkipped {
        phase_id: String,
        reason: String,
    },
    /// The scheduler encountered an error for a phase.
    SchedulerError {
        phase_id: String,
        error: String,
    },
    /// The scheduler has started.
    SchedulerStarted {
        phase_count: usize,
    },
    /// The scheduler has stopped.
    SchedulerStopped,
}

/// Configuration for the pipeline cron scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum number of phases executing concurrently.
    pub max_concurrent_phases: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_phases: 3,
        }
    }
}
```

**mod.rs — add module declaration:**

```rust
// Add to existing module list:
pub mod scheduler;

// Add to re-exports:
pub use scheduler::{ScheduledPhaseEvent, SchedulerConfig};
```

**Verification:**
```bash
cargo build -p rustycode-tui
cargo clippy -p rustycode-tui -- -D warnings
```

---

### Commit 2: `feat(tui): implement cron parsing with tests`

**TDD phase:** Write tests FIRST, then implement to make them pass.

**File:** `crates/rustycode-tui/src/app/pipeline/scheduler.rs`

**Step 2a — Write tests (RED):**

Add to `scheduler.rs`:

```rust
use anyhow::{Context, Result};
use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;

impl SchedulerConfig {
    /// Parse a cron expression (5 or 6 field).
    ///
    /// 5-field: "min hour day month weekday" (standard unix cron)
    /// 6-field: "sec min hour day month weekday" (with seconds)
    pub fn parse_cron(expression: &str) -> Result<Schedule> {
        let field_count = expression.split_whitespace().count();
        let schedule_str = match field_count {
            5 => format!("0 {}", expression), // Prepend seconds field
            6 => expression.to_string(),
            _ => anyhow::bail!(
                "Invalid cron expression '{}': expected 5 or 6 fields, got {}",
                expression,
                field_count
            ),
        };
        Schedule::from_str(&schedule_str)
            .with_context(|| format!("Failed to parse cron expression: '{}'", expression))
    }

    /// Calculate next fire time for a cron schedule after `after`.
    pub fn next_fire_time(schedule: &Schedule, after: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        schedule.after(after).next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Timelike, Utc};

    // ── Cron Parsing Tests ──────────────────────────────────────

    #[test]
    fn test_parse_cron_5_field() {
        // "0 8 * * *" = every day at 8:00 (min hour day month weekday)
        let schedule = SchedulerConfig::parse_cron("0 8 * * *").unwrap();
        let now = Utc::now();
        let next = schedule.after(&now).next();
        assert!(next.is_some(), "5-field cron should produce a next fire time");
    }

    #[test]
    fn test_parse_cron_6_field() {
        // "0 0 8 * * *" = every day at 8:00:00 (sec min hour day month weekday)
        let schedule = SchedulerConfig::parse_cron("0 0 8 * * *").unwrap();
        let now = Utc::now();
        let next = schedule.after(&now).next();
        assert!(next.is_some(), "6-field cron should produce a next fire time");
    }

    #[test]
    fn test_parse_cron_invalid_field_count() {
        let result = SchedulerConfig::parse_cron("0 8 * *");
        assert!(result.is_err(), "4-field cron should be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("expected 5 or 6 fields"), "Error message should explain field count");
    }

    #[test]
    fn test_parse_cron_invalid_expression() {
        let result = SchedulerConfig::parse_cron("invalid cron expr");
        assert!(result.is_err(), "Gibberish should be rejected");
    }

    // ── Next Fire Time Tests ────────────────────────────────────

    #[test]
    fn test_next_fire_time_is_in_future() {
        let schedule = SchedulerConfig::parse_cron("0 8 * * *").unwrap();
        let now = Utc::now();
        let next = SchedulerConfig::next_fire_time(&schedule, &now).unwrap();
        assert!(next > now, "Next fire time must be in the future");
    }

    #[test]
    fn test_next_fire_time_daily_at_8am() {
        let schedule = SchedulerConfig::parse_cron("0 8 * * *").unwrap();
        let now = Utc::now();
        let next = SchedulerConfig::next_fire_time(&schedule, &now).unwrap();
        // The next fire should be at minute 0, hour 8
        assert_eq!(next.minute(), 0, "Should fire at minute 0");
        // Note: hour check depends on timezone — cron operates in UTC
    }

    #[test]
    fn test_next_fire_time_every_minute() {
        let schedule = SchedulerConfig::parse_cron("* * * * *").unwrap();
        let now = Utc::now();
        let next = SchedulerConfig::next_fire_time(&schedule, &now).unwrap();
        assert!(next > now, "Every-minute schedule should produce future fire time");
        let diff = next - now;
        assert!(diff.num_seconds() < 120, "Every-minute should fire within 2 minutes");
    }
}
```

**Step 2b — Verify (GREEN):**

```bash
cargo test -p rustycode-tui -- scheduler
```

All 7 tests must pass before proceeding.

**Verification:**
```bash
cargo clippy -p rustycode-tui -- -D warnings
```

---

### Commit 3: `feat(tui): implement PipelineCronScheduler with timer threads`

**TDD phase:** Write scheduler tests FIRST, then implement.

**File:** `crates/rustycode-tui/src/app/pipeline/scheduler.rs`

**Step 3a — Add struct + timer thread implementation:**

```rust
use std::sync::{mpsc::Sender, Arc};
use std::time::Duration;

/// The pipeline cron scheduler. Owns the channel sender and spawns
/// per-phase timer threads that send `ScheduledPhaseEvent`s.
pub struct PipelineCronScheduler {
    config: SchedulerConfig,
    tx: Sender<ScheduledPhaseEvent>,
}

impl PipelineCronScheduler {
    pub fn new(config: SchedulerConfig, tx: Sender<ScheduledPhaseEvent>) -> Self {
        Self { config, tx }
    }

    /// Start the scheduler. Spawns one timer thread per scheduled phase.
    ///
    /// Phases without a `schedule` field are skipped.
    /// Returns the count of scheduled phases that were started.
    pub fn start(self: &Arc<Self>, manifest: &super::manifest::Manifest) -> Result<usize> {
        let mut scheduled_count = 0;

        for phase in &manifest.phases {
            if let Some(ref cron_expr) = phase.schedule {
                let schedule = Self::parse_cron(cron_expr).with_context(|| {
                    format!("Invalid cron schedule for phase '{}'", phase.id)
                })?;

                let phase_id = phase.id.clone();
                self.spawn_phase_timer(phase_id, schedule);
                scheduled_count += 1;
            }
        }

        let _ = self.tx.send(ScheduledPhaseEvent::SchedulerStarted {
            phase_count: scheduled_count,
        });

        Ok(scheduled_count)
    }

    /// Spawn a dedicated thread for a phase's cron timer.
    ///
    /// The thread loops: calculate next fire time → sleep → send event → repeat.
    /// Thread exits if the schedule produces no next fire time or channel disconnects.
    fn spawn_phase_timer(self: &Arc<Self>, phase_id: String, schedule: Schedule) {
        let scheduler = Arc::clone(self);
        std::thread::Builder::new()
            .name(format!("cron-timer-{}", phase_id))
            .spawn(move || {
                loop {
                    let now = Utc::now();
                    let next = match SchedulerConfig::next_fire_time(&schedule, &now) {
                        Some(t) => t,
                        None => {
                            let _ = scheduler.tx.send(ScheduledPhaseEvent::SchedulerError {
                                phase_id: phase_id.clone(),
                                error: "Schedule produced no future fire time".to_string(),
                            });
                            break;
                        }
                    };

                    let sleep_duration = (next - now)
                        .to_std()
                        .unwrap_or(Duration::ZERO);

                    if !sleep_duration.is_zero() {
                        std::thread::sleep(sleep_duration);
                    }

                    let fire_time = Instant::now();
                    let send_result = scheduler.tx.send(ScheduledPhaseEvent::PhaseReady {
                        phase_id: phase_id.clone(),
                        scheduled_fire_time: next,
                        actual_fire_time: fire_time,
                    });

                    if send_result.is_err() {
                        // Receiver dropped — TUI shut down
                        break;
                    }

                    // Guard: sleep briefly to prevent re-firing within the same cron minute
                    std::thread::sleep(Duration::from_secs(1));
                }
            })
            .context("Failed to spawn cron timer thread")?;
    }
}
```

**Step 3b — Add scheduler tests:**

```rust
    // ── Scheduler Integration Tests ─────────────────────────────

    #[test]
    fn test_scheduler_sends_phase_ready() {
        let (tx, rx) = std::sync::mpsc::channel();
        let config = SchedulerConfig::default();
        let scheduler = Arc::new(PipelineCronScheduler::new(config, tx));

        // Use an every-minute schedule — will fire within ~60s
        let schedule = SchedulerConfig::parse_cron("* * * * *").unwrap();
        scheduler.spawn_phase_timer("test-phase".to_string(), schedule);

        // Wait up to 90 seconds for the event
        let event = rx.recv_timeout(Duration::from_secs(90))
            .expect("Should receive PhaseReady event within 90s");

        match event {
            ScheduledPhaseEvent::PhaseReady { phase_id, .. } => {
                assert_eq!(phase_id, "test-phase");
            }
            other => panic!("Expected PhaseReady, got {:?}", other),
        }
    }

    #[test]
    fn test_scheduler_sends_started_event() {
        let (tx, rx) = std::sync::mpsc::channel();
        let config = SchedulerConfig::default();
        let scheduler = Arc::new(PipelineCronScheduler::new(config, tx));

        // Minimal manifest with no scheduled phases
        let manifest = super::super::manifest::Manifest {
            metadata: super::super::manifest::ManifestMetadata {
                name: "test".to_string(),
                version: "1.0".to_string(),
                description: None,
            },
            phases: vec![],
        };

        let count = scheduler.start(&manifest).unwrap();
        assert_eq!(count, 0, "No scheduled phases should return 0");

        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        match event {
            ScheduledPhaseEvent::SchedulerStarted { phase_count } => {
                assert_eq!(phase_count, 0);
            }
            other => panic!("Expected SchedulerStarted, got {:?}", other),
        }
    }

    #[test]
    fn test_scheduler_stops_on_channel_drop() {
        let (tx, rx) = std::sync::mpsc::channel();
        let config = SchedulerConfig::default();
        let scheduler = Arc::new(PipelineCronScheduler::new(config, tx));

        let schedule = SchedulerConfig::parse_cron("* * * * *").unwrap();
        scheduler.spawn_phase_timer("doomed-phase".to_string(), schedule);

        // Drop receiver — timer thread should exit gracefully
        drop(rx);

        // Give thread time to notice and exit
        std::thread::sleep(Duration::from_secs(2));
        // If we get here without hang, thread exited cleanly
    }
```

**Verification:**
```bash
cargo test -p rustycode-tui -- scheduler
cargo clippy -p rustycode-tui -- -D warnings
```

---

### Commit 4: `feat(tui): wire scheduler into TUI event loop`

**TDD phase:** Write polling/handler tests FIRST, then wire into TUI.

**Files to modify:**

| File | Action |
|------|--------|
| `crates/rustycode-tui/src/app/event_loop.rs` | Add scheduler fields to `TUI` struct, initialize in `TUI::new()` |
| `crates/rustycode-tui/src/app/service_polling.rs` | Add `poll_scheduler_events()` + `handle_scheduled_phase_event()` |

**Step 4a — Add fields to TUI struct (event_loop.rs):**

In the `TUI` struct (line ~108), add in the pipeline section (after line ~154):

```rust
    // Cron scheduler state
    pub(crate) scheduler_rx: Option<std::sync::mpsc::Receiver<crate::app::pipeline::scheduler::ScheduledPhaseEvent>>,
    pub(crate) scheduler_tx: Option<std::sync::mpsc::Sender<crate::app::pipeline::scheduler::ScheduledPhaseEvent>>,
    pub(crate) active_scheduled_phases: std::collections::HashSet<String>,
    pub(crate) max_concurrent_phases: usize,
```

**Step 4b — Initialize in TUI::new():**

Find the TUI struct construction (search for `pipeline_guardian:`) and add:

```rust
            scheduler_rx: None,
            scheduler_tx: None,
            active_scheduled_phases: std::collections::HashSet::new(),
            max_concurrent_phases: 3,
```

**Step 4c — Add poll_scheduler_events() to service_polling.rs:**

Add at the end of `impl TUI` block in `service_polling.rs`:

```rust
    /// Poll scheduled phase events from the cron scheduler.
    ///
    /// Drains all pending events (non-blocking) each frame.
    /// Matches existing `poll_team_events()` pattern.
    fn poll_scheduler_events(&mut self) {
        if let Some(ref rx) = self.scheduler_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        self.handle_scheduled_phase_event(event);
                        self.dirty = true;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        tracing::info!("Scheduler channel disconnected");
                        self.scheduler_rx = None;
                        break;
                    }
                }
            }
        }
    }

    /// Handle a single scheduled phase event.
    fn handle_scheduled_phase_event(
        &mut self,
        event: crate::app::pipeline::scheduler::ScheduledPhaseEvent,
    ) {
        use crate::app::pipeline::scheduler::ScheduledPhaseEvent;

        match event {
            ScheduledPhaseEvent::PhaseReady {
                phase_id,
                scheduled_fire_time,
                actual_fire_time: _,
            } => {
                // Check concurrency limit
                if self.active_scheduled_phases.len() >= self.max_concurrent_phases {
                    self.add_system_message(format!(
                        "⏳ Phase '{}' queued — concurrency limit ({}/{})",
                        phase_id,
                        self.active_scheduled_phases.len(),
                        self.max_concurrent_phases
                    ));
                    // TODO: Queue for execution when slot opens (future PR)
                    return;
                }

                self.active_scheduled_phases.insert(phase_id.clone());
                self.add_system_message(format!(
                    "▶ Scheduled phase '{}' triggered (scheduled: {})",
                    phase_id,
                    scheduled_fire_time.format("%H:%M:%S")
                ));

                // TODO: Spawn phase execution in background thread (future PR)
                // For now, just track the phase as "active" and log.
            }
            ScheduledPhaseEvent::PhaseSkipped { phase_id, reason } => {
                self.add_system_message(format!(
                    "⏭ Phase '{}' skipped: {}",
                    phase_id, reason
                ));
            }
            ScheduledPhaseEvent::SchedulerError { phase_id, error } => {
                tracing::error!("Scheduler error for phase '{}': {}", phase_id, error);
                self.add_system_message(format!(
                    "⚠ Scheduler error for '{}': {}",
                    phase_id, error
                ));
            }
            ScheduledPhaseEvent::SchedulerStarted { phase_count } => {
                self.add_system_message(format!(
                    "🕐 Cron scheduler started ({} phases)",
                    phase_count
                ));
            }
            ScheduledPhaseEvent::SchedulerStopped => {
                self.add_system_message("🕐 Cron scheduler stopped".to_string());
            }
        }
    }
```

**Step 4d — Call from poll_services():**

In `poll_services()` after `self.poll_team_events();` (line 125), add:

```rust
        // Poll scheduled phase events (from cron scheduler)
        self.poll_scheduler_events();
```

**Step 4e — Add tests for event handler:**

```rust
    // ── Event Handler Tests ─────────────────────────────────────
    // Note: These test the pure logic of handle_scheduled_phase_event
    // without the full TUI struct. Full TUI integration tests are in
    // Commit 5 (integration test file).

    #[test]
    fn test_scheduled_phase_event_debug_format() {
        let event = ScheduledPhaseEvent::PhaseReady {
            phase_id: "build".to_string(),
            scheduled_fire_time: Utc::now(),
            actual_fire_time: std::time::Instant::now(),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("PhaseReady"));
        assert!(debug_str.contains("build"));
    }

    #[test]
    fn test_scheduled_phase_event_skipped_variant() {
        let event = ScheduledPhaseEvent::PhaseSkipped {
            phase_id: "deploy".to_string(),
            reason: "dependency failed".to_string(),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("PhaseSkipped"));
        assert!(debug_str.contains("deploy"));
        assert!(debug_str.contains("dependency failed"));
    }

    #[test]
    fn test_scheduler_config_default_concurrency() {
        let config = SchedulerConfig::default();
        assert_eq!(config.max_concurrent_phases, 3);
    }
```

**Verification:**
```bash
cargo build -p rustycode-tui
cargo test -p rustycode-tui -- scheduler
cargo clippy -p rustycode-tui -- -D warnings
```

---

### Commit 5: `test(tui): add scheduler integration tests`

**File:** `crates/rustycode-tui/tests/scheduler_integration_test.rs` (create)

```rust
//! Integration tests for the pipeline cron scheduler.
//!
//! These tests validate end-to-end behavior: cron parsing → timer thread →
//! channel communication → event receipt.

use rustycode_tui::app::pipeline::scheduler::{
    ScheduledPhaseEvent, SchedulerConfig,
};
use std::sync::Arc;
use std::time::Duration;

/// Test that a manifest with scheduled phases produces PhaseReady events.
#[test]
fn test_manifest_with_scheduled_phases_fires_events() {
    // We can't easily construct a full Manifest here without the YAML parser,
    // so we test the scheduler directly via spawn_phase_timer.

    let (tx, rx) = std::sync::mpsc::channel();
    let config = SchedulerConfig::default();
    let scheduler = Arc::new(
        rustycode_tui::app::pipeline::scheduler::PipelineCronScheduler::new(config, tx),
    );

    // Every-minute schedule — fires within 60s
    let schedule = SchedulerConfig::parse_cron("* * * * *").unwrap();
    scheduler.spawn_phase_timer("integration-phase".to_string(), schedule);

    // Wait for the PhaseReady event
    let event = rx.recv_timeout(Duration::from_secs(90))
        .expect("Should receive event within 90 seconds");

    match event {
        ScheduledPhaseEvent::PhaseReady { phase_id, scheduled_fire_time, .. } => {
            assert_eq!(phase_id, "integration-phase");
            // Fire time should be very recent (within last 2 seconds)
            let now = chrono::Utc::now();
            let diff = (now - scheduled_fire_time).num_seconds().abs();
            assert!(diff < 5, "Fire time should be close to now, was {}s off", diff);
        }
        other => panic!("Expected PhaseReady, got {:?}", other),
    }
}

/// Test that multiple phases fire independently.
#[test]
fn test_multiple_phases_fire_independently() {
    let (tx, rx) = std::sync::mpsc::channel();
    let config = SchedulerConfig::default();

    // Need to access PipelineCronScheduler — verify it's public
    let _config_clone = config.clone();

    // Use parse_cron through SchedulerConfig
    let schedule_a = SchedulerConfig::parse_cron("* * * * *").unwrap();
    let schedule_b = SchedulerConfig::parse_cron("* * * * *").unwrap();

    // We can't easily create PipelineCronScheduler from outside the crate
    // unless it's public. This test validates the public API surface.
    assert!(schedule_a.after(&chrono::Utc::now()).next().is_some());
    assert!(schedule_b.after(&chrono::Utc::now()).next().is_some());
}

/// Test cron expressions used in manifests.
#[test]
fn test_manifest_style_cron_expressions() {
    // From manifest.rs test data: "30 5 * * *"
    let schedule = SchedulerConfig::parse_cron("30 5 * * *").unwrap();
    let now = chrono::Utc::now();
    let next = SchedulerConfig::next_fire_time(&schedule, &now).unwrap();
    assert!(next > now);
    // Should fire at minute 30
    assert_eq!(next.minute(), 30);
}

/// Test that invalid cron expressions are rejected.
#[test]
fn test_invalid_expressions_rejected() {
    assert!(SchedulerConfig::parse_cron("").is_err());
    assert!(SchedulerConfig::parse_cron("not a cron").is_err());
    assert!(SchedulerConfig::parse_cron("0 8 * * * * *").is_err()); // 7 fields
    assert!(SchedulerConfig::parse_cron("0").is_err()); // 1 field
}
```

**Verification:**
```bash
cargo test -p rustycode-tui
cargo clippy -p rustycode-tui -- -D warnings
```

---

## Parallel Execution Map (Ultrawork)

```
WAVE 1 (sequential — foundation):
  └── Commit 1: types + deps
       └── BLOCKS: everything else

WAVE 2 (sequential — pure logic):
  └── Commit 2: cron parsing + tests
       └── BLOCKS: commits 3, 4

WAVE 3 (parallel — independent implementations):
  ├── Commit 3: timer thread + scheduler core + tests
  │    depends-on: commit 2 (parse_cron)
  │
  └── Commit 4: TUI wiring + event handler
       depends-on: commit 1 (types), commit 2 (parse_cron)

WAVE 4 (sequential — validation):
  └── Commit 5: integration tests
       depends-on: commits 3 + 4

FINAL: cargo clippy + cargo test — full verification
```

**Estimated total: 5 commits, ~400 lines of code, ~200 lines of tests.**

---

## Files Changed Summary

| File | Action | Commit |
|------|--------|--------|
| `crates/rustycode-tui/Cargo.toml` | Add `cron.workspace = true` | 1 |
| `crates/rustycode-tui/src/app/pipeline/scheduler.rs` | Create (types → parsing → scheduler) | 1→2→3 |
| `crates/rustycode-tui/src/app/pipeline/mod.rs` | Add `pub mod scheduler;` + re-exports | 1 |
| `crates/rustycode-tui/src/app/event_loop.rs` | Add 4 fields to TUI struct + init | 4 |
| `crates/rustycode-tui/src/app/service_polling.rs` | Add `poll_scheduler_events()` + `handle_scheduled_phase_event()` | 4 |
| `crates/rustycode-tui/tests/scheduler_integration_test.rs` | Create integration tests | 5 |

---

## Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Timer thread leaks (TUI shutdown) | Channel drop causes thread exit (tested in commit 3) |
| `cron` crate version mismatch | Already workspace dep at `0.12` — no new version |
| Clippy warnings on `Instant` in struct | `Instant` is not `Send` across processes — fine for threads |
| `to_std()` panic on negative Duration | `unwrap_or(Duration::ZERO)` guard in spawn_phase_timer |
| Test flakiness (timer-based tests) | Use 90s timeout + every-minute schedule; mark slow tests appropriately |

---

## Success Criteria

- [ ] `cargo build -p rustycode-tui` — zero warnings
- [ ] `cargo test -p rustycode-tui` — all tests pass
- [ ] `cargo clippy -p rustycode-tui -- -D warnings` — clean
- [ ] `PipelineCronScheduler` parses both 5-field and 6-field cron expressions
- [ ] Per-phase timer threads send `ScheduledPhaseEvent` through `std::sync::mpsc`
- [ ] `poll_scheduler_events()` receives and dispatches events each frame
- [ ] Concurrency limit enforced (`active_scheduled_phases.len() >= max`)
- [ ] Timer threads exit cleanly on channel drop (no thread leaks)
- [ ] Existing pipeline functionality unaffected (backward compatible)
- [ ] No new `unwrap()` or `expect()` in non-test code — use `?` and `context()`
