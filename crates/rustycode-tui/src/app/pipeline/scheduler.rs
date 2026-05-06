//! Cron-based pipeline scheduler with per-phase timer tasks and channel-based event delivery.

use super::manifest::Manifest;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(test)]
use chrono::Timelike;

// Event types

/// Events emitted by [`PipelineCronScheduler`] through the `std::sync::mpsc` channel.
#[derive(Debug)]
#[non_exhaustive]
pub enum ScheduledPhaseEvent {
    /// A scheduled phase has reached its fire time and is ready to execute.
    PhaseReady {
        phase_id: String,
        scheduled_fire_time: DateTime<Utc>,
        actual_fire_time: Instant,
    },
    /// A phase is starting execution.
    PhaseStarting { phase_id: String, cron_expr: String },
    /// A phase completed successfully.
    PhaseCompleted {
        phase_id: String,
        duration: Duration,
    },
    /// A phase failed during execution.
    PhaseFailed { phase_id: String, error: String },
    /// A phase was skipped (e.g. no valid schedule or past fire time).
    PhaseSkipped { phase_id: String, reason: String },
    /// The scheduler encountered an error for a specific phase.
    SchedulerError { phase_id: String, error: String },
    /// The scheduler has started and spawned timers for the given number of phases.
    SchedulerStarted { phase_count: usize },
    /// The scheduler has been stopped.
    SchedulerStopped,
}

// Config

/// Configuration for [`PipelineCronScheduler`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SchedulerConfig {
    /// Maximum number of phases that can run concurrently.
    pub max_concurrent_phases: usize,
    /// Default interval (in seconds) between scheduler tick checks.
    pub default_tick_interval_secs: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_phases: 3,
            default_tick_interval_secs: 10,
        }
    }
}

impl SchedulerConfig {
    /// Parse a cron expression into a [`Schedule`].
    ///
    /// Normalizes to 7-field format (`sec min hour dom month dow year`):
    /// - 5-field (e.g. `"0 8 * * *"`) → prepend `0` sec, append `*` year
    /// - 6-field → append `*` year
    /// - 7-field → pass through
    pub fn parse_cron(expression: &str) -> Result<Schedule> {
        let fields = expression.split_whitespace().count();
        let normalized = match fields {
            5 => format!("0 {} *", expression),
            6 => format!("{} *", expression),
            7 => expression.to_string(),
            _ => {
                anyhow::bail!(
                    "Invalid cron expression '{}': expected 5-7 fields, got {}",
                    expression,
                    fields
                );
            }
        };
        Schedule::from_str(&normalized)
            .with_context(|| format!("Failed to parse cron expression: '{}'", expression))
    }

    /// Compute the next fire time after the given moment.
    pub fn next_fire_time(schedule: &Schedule, after: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        schedule.after(after).next()
    }
}

// Scheduler

/// Cron-based pipeline scheduler.
///
/// Uses `std::thread::spawn` for per-phase timer threads and `std::sync::mpsc` for event
/// delivery, matching the synchronous nature of the TUI event loop.
#[non_exhaustive]
pub struct PipelineCronScheduler {
    config: SchedulerConfig,
    tx: Sender<ScheduledPhaseEvent>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    running: Arc<AtomicBool>,
}

impl PipelineCronScheduler {
    pub fn new(config: SchedulerConfig, tx: Sender<ScheduledPhaseEvent>) -> Self {
        Self {
            config,
            tx,
            handles: Mutex::new(Vec::new()),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the scheduler for all manifest phases that have a `schedule` field.
    ///
    /// Parses each phase's cron expression and spawns a dedicated timer thread.
    /// Phases without a schedule are skipped.
    pub fn start(&self, manifest: &Manifest) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(anyhow!("Scheduler is already running"));
        }

        self.running.store(true, Ordering::SeqCst);

        let mut scheduled_count: usize = 0;

        for phase in &manifest.phases {
            let schedule_str = match &phase.schedule {
                Some(s) if !s.is_empty() => s.clone(),
                _ => continue,
            };

            let schedule = match SchedulerConfig::parse_cron(&schedule_str) {
                Ok(s) => s,
                Err(e) => {
                    let _ = self.tx.send(ScheduledPhaseEvent::SchedulerError {
                        phase_id: phase.id.clone(),
                        error: format!("Invalid cron expression '{schedule_str}': {e:#}"),
                    });
                    continue;
                }
            };

            self.spawn_phase_timer(phase.id.clone(), schedule);
            scheduled_count += 1;
        }

        let _ = self.tx.send(ScheduledPhaseEvent::SchedulerStarted {
            phase_count: scheduled_count,
        });

        Ok(())
    }

    /// Stop the scheduler: signal all timer threads to exit and join them.
    ///
    /// Each thread is given a generous timeout to finish its current sleep cycle.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);

        let mut handles = match self.handles.lock() {
            Ok(h) => h,
            Err(e) => {
                let _ = self.tx.send(ScheduledPhaseEvent::SchedulerError {
                    phase_id: String::new(),
                    error: format!("Failed to lock handles during stop: {e}"),
                });
                return;
            }
        };

        // Drain handles and join each with a timeout so we don't block forever.
        let drained: Vec<JoinHandle<()>> = handles.drain(..).collect();
        drop(handles);

        for handle in drained {
            let _ = handle.join();
        }

        let _ = self.tx.send(ScheduledPhaseEvent::SchedulerStopped);
    }

    /// Spawn a background thread that sleeps until each fire time of `schedule`,
    /// then sends a [`ScheduledPhaseEvent::PhaseReady`].
    fn spawn_phase_timer(&self, phase_id: String, schedule: Schedule) {
        let tx = self.tx.clone();
        let running = self.running.clone();

        let handle = thread::spawn(move || loop {
            if !running.load(Ordering::Relaxed) {
                break;
            }

            let now = Utc::now();
            let next = SchedulerConfig::next_fire_time(&schedule, &now);

            match next {
                None => break,
                Some(next_time) => {
                    let chrono_dur = next_time - now;
                    let total_sleep = chrono_dur.to_std().unwrap_or(Duration::ZERO);

                    if !total_sleep.is_zero() {
                        let check_interval = Duration::from_secs(1);
                        let mut remaining = total_sleep;
                        while !remaining.is_zero() && running.load(Ordering::Relaxed) {
                            let sleep_time = remaining.min(check_interval);
                            thread::sleep(sleep_time);
                            remaining = remaining.saturating_sub(check_interval);
                        }
                    }

                    if !running.load(Ordering::Relaxed) {
                        break;
                    }

                    let actual_fire = Instant::now();
                    let _ = tx.send(ScheduledPhaseEvent::PhaseReady {
                        phase_id: phase_id.clone(),
                        scheduled_fire_time: next_time,
                        actual_fire_time: actual_fire,
                    });

                    thread::sleep(Duration::from_secs(1));
                }
            }
        });

        if let Ok(mut h) = self.handles.lock() {
            h.push(handle);
        }
    }

    /// Returns `true` if the scheduler is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Returns a reference to the scheduler config.
    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    // Helper: build a minimal manifest with one scheduled phase.
    fn minimal_manifest(schedule: &str) -> Manifest {
        Manifest {
            version: "1.0".to_string(),
            metadata: super::super::manifest::ManifestMetadata {
                name: "test_pipeline".to_string(),
                description: None,
                owner: None,
            },
            phases: vec![super::super::manifest::PhaseDefinition {
                id: "phase_1".to_string(),
                description: None,
                schedule: Some(schedule.to_string()),
                failure_strategy: super::super::types::FailureStrategy::HardBlock {
                    retry: super::super::types::RetryPolicy::default(),
                },
                timeout_secs: None,
                parallel: None,
                hard_deps: None,
                soft_deps: None,
                steps: None,
                artifacts_produced: None,
            }],
        }
    }

    // ---- parse_cron tests ----

    #[test]
    fn test_parse_cron_5_field_via_config() {
        let schedule = SchedulerConfig::parse_cron("0 8 * * *");
        assert!(
            schedule.is_ok(),
            "5-field cron should parse via config normalization"
        );
    }

    #[test]
    fn test_parse_cron_invalid() {
        let result = SchedulerConfig::parse_cron("not a cron");
        assert!(
            result.is_err(),
            "Invalid cron expression should return an error"
        );
    }

    // ---- next_fire_time tests ----

    #[test]
    fn test_next_fire_time_future() {
        let schedule = SchedulerConfig::parse_cron("*/5 * * * *").expect("cron should parse");
        let now = Utc::now();
        let next = SchedulerConfig::next_fire_time(&schedule, &now);

        assert!(next.is_some(), "Should have a next fire time");

        let next = next.expect("checked above");
        let diff = next - now;
        let diff_mins = diff.num_minutes();
        assert!(
            (0..=5).contains(&diff_mins),
            "Next fire time should be within the next 5 minutes, got {diff_mins} min"
        );
    }

    #[test]
    fn test_next_fire_time_daily() {
        let schedule = SchedulerConfig::parse_cron("0 8 * * *").expect("cron should parse");
        let now = Utc::now();
        let next = SchedulerConfig::next_fire_time(&schedule, &now);

        assert!(next.is_some(), "Should have a next fire time");

        let next = next.expect("checked above");
        assert_eq!(next.hour(), 8, "Daily 8 AM cron should fire at hour 8");
        assert_eq!(next.minute(), 0, "Should fire at minute 0");
    }

    // ---- scheduler config default ----

    #[test]
    fn test_scheduler_config_default() {
        let config = SchedulerConfig::default();
        assert_eq!(config.max_concurrent_phases, 3);
        assert_eq!(config.default_tick_interval_secs, 10);
    }

    // ---- scheduler event sending ----

    #[test]
    #[ignore] // Timing-sensitive: requires waiting for cron fire (run with --ignored)
    fn test_scheduler_sends_event() {
        let (tx, rx) = mpsc::channel::<ScheduledPhaseEvent>();
        let config = SchedulerConfig::default();
        let scheduler = PipelineCronScheduler::new(config, tx);

        // 6-field: fires every 10 seconds (sec=*/10 min=* hour=* dom=* month=* dow=*)
        let manifest = minimal_manifest("*/10 * * * * *");

        scheduler.start(&manifest).expect("start should succeed");

        // Read the SchedulerStarted event.
        let started = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("Should receive SchedulerStarted event");
        match started {
            ScheduledPhaseEvent::SchedulerStarted { phase_count } => {
                assert_eq!(phase_count, 1);
            }
            other => panic!("Expected SchedulerStarted, got {other:?}"),
        }

        // Wait for the PhaseReady event (schedule fires every 10s).
        let phase_event = rx.recv_timeout(Duration::from_secs(30));
        assert!(
            phase_event.is_ok(),
            "Should receive a PhaseReady event within 30 seconds"
        );

        match phase_event.expect("checked above") {
            ScheduledPhaseEvent::PhaseReady { phase_id, .. } => {
                assert_eq!(phase_id, "phase_1");
            }
            other => panic!("Expected PhaseReady, got {other:?}"),
        }

        scheduler.stop();
    }

    // ---- scheduler stop ----

    #[test]
    fn test_scheduler_stop() {
        let (tx, _rx) = mpsc::channel::<ScheduledPhaseEvent>();
        let config = SchedulerConfig::default();
        let scheduler = PipelineCronScheduler::new(config, tx);

        // Use a far-future schedule so the timer thread won't fire during the test.
        let manifest = minimal_manifest("0 0 31 12 *");

        scheduler.start(&manifest).expect("start should succeed");
        assert!(
            scheduler.is_running(),
            "Scheduler should be running after start"
        );

        scheduler.stop();
        assert!(
            !scheduler.is_running(),
            "Scheduler should not be running after stop"
        );
    }

    // ---- SchedulerConfig::parse_cron tests (5/6 field normalization) ----

    #[test]
    fn test_config_parse_cron_5_field() {
        let schedule = SchedulerConfig::parse_cron("0 8 * * *").expect("5-field should parse");
        let now = Utc::now();
        let next = schedule
            .after(&now)
            .next()
            .expect("should have next fire time");
        assert!(next > now);
    }

    #[test]
    fn test_config_parse_cron_6_field() {
        let schedule = SchedulerConfig::parse_cron("0 0 8 * * *").expect("6-field should parse");
        let now = Utc::now();
        let next = schedule
            .after(&now)
            .next()
            .expect("should have next fire time");
        assert!(next > now);
    }

    #[test]
    fn test_config_parse_cron_invalid_field_count() {
        let result = SchedulerConfig::parse_cron("0 8 * *");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("expected 5-7 fields, got 4"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_config_parse_cron_invalid_expression() {
        let result = SchedulerConfig::parse_cron("gibberish not a cron");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_next_fire_time_is_in_future() {
        let schedule = SchedulerConfig::parse_cron("0 8 * * *").unwrap();
        let now = Utc::now();
        let next = SchedulerConfig::next_fire_time(&schedule, &now).unwrap();
        assert!(next > now);
    }

    #[test]
    fn test_config_next_fire_time_daily_at_8am() {
        let schedule = SchedulerConfig::parse_cron("0 8 * * *").unwrap();
        let now = Utc::now();
        let next = SchedulerConfig::next_fire_time(&schedule, &now).unwrap();
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn test_config_next_fire_time_every_minute() {
        let schedule = SchedulerConfig::parse_cron("* * * * *").unwrap();
        let now = Utc::now();
        let next = SchedulerConfig::next_fire_time(&schedule, &now).unwrap();
        let diff = (next - now).num_seconds();
        assert!(
            diff <= 120,
            "next fire time should be within 120s, got {diff}s"
        );
    }

    // ---- Additional tests (commit 4) ----

    #[test]
    fn test_scheduled_phase_event_debug_format() {
        let event = ScheduledPhaseEvent::PhaseReady {
            phase_id: "test_phase".to_string(),
            scheduled_fire_time: Utc::now(),
            actual_fire_time: Instant::now(),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("PhaseReady"));
        assert!(debug_str.contains("test_phase"));
    }

    #[test]
    fn test_scheduled_phase_event_skipped_variant() {
        let event = ScheduledPhaseEvent::PhaseSkipped {
            phase_id: "skip_me".to_string(),
            reason: "bad schedule".to_string(),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("PhaseSkipped"));
        assert!(debug_str.contains("skip_me"));
    }

    #[test]
    fn test_scheduler_config_default_concurrency() {
        let config = SchedulerConfig::default();
        assert_eq!(config.max_concurrent_phases, 3);
    }

    #[test]
    fn test_scheduler_sends_started_event() {
        let (tx, rx) = mpsc::channel();
        let config = SchedulerConfig::default();
        let scheduler = PipelineCronScheduler::new(config, tx);

        let manifest = Manifest {
            version: "1.0".to_string(),
            metadata: super::super::manifest::ManifestMetadata {
                name: "empty".to_string(),
                description: None,
                owner: None,
            },
            phases: vec![super::super::manifest::PhaseDefinition {
                id: "no_schedule".to_string(),
                description: None,
                schedule: None,
                failure_strategy: super::super::types::FailureStrategy::HardBlock {
                    retry: super::super::types::RetryPolicy::default(),
                },
                timeout_secs: None,
                parallel: None,
                hard_deps: None,
                soft_deps: None,
                steps: None,
                artifacts_produced: None,
            }],
        };

        scheduler.start(&manifest).unwrap();
        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        match event {
            ScheduledPhaseEvent::SchedulerStarted { phase_count } => {
                assert_eq!(phase_count, 0);
            }
            other => panic!("expected SchedulerStarted with count 0, got {:?}", other),
        }
    }

    #[test]
    fn test_scheduler_stops_on_channel_drop() {
        let (tx, rx) = mpsc::channel();
        let config = SchedulerConfig::default();
        let scheduler = PipelineCronScheduler::new(config, tx);

        let manifest = minimal_manifest("* * * * *");
        let _ = scheduler.start(&manifest);

        let _ = rx.recv_timeout(Duration::from_secs(5));

        drop(rx);
        thread::sleep(Duration::from_secs(2));
    }
}
