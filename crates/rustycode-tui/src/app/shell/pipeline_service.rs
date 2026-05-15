//! Pipeline service owned by AppShell.
//!
//! This is infrastructure, NOT a UI feature. It wraps the pipeline registry,
//! scheduler event receiver, and active phase tracking so the AppShell can
//! drive the pipeline on every tick without coupling to the TUI god struct.

use std::collections::HashSet;

use crate::app::pipeline::registry::PipelineRegistry;
use crate::app::pipeline::scheduler::ScheduledPhaseEvent;

/// Result of a single [`PipelineService::tick()`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickResult {
    /// No scheduler event was available this tick.
    Idle,
    /// A phase is ready to execute.
    PhaseReady {
        phase_id: String,
    },
    /// A phase completed successfully.
    PhaseCompleted {
        phase_id: String,
    },
    /// A phase failed.
    PhaseFailed {
        phase_id: String,
    },
    /// A phase was skipped.
    PhaseSkipped {
        phase_id: String,
    },
    /// The scheduler started and spawned timers.
    SchedulerStarted {
        phase_count: usize,
    },
    /// The scheduler was stopped.
    SchedulerStopped,
    /// An unrecognised / scheduler-internal event was consumed.
    Other,
}

/// Infrastructure service that drives the pipeline subsystem.
///
/// Owned by [`super::AppShell`] and ticked once per frame. Does NOT implement
/// [`crate::app::features::TuiFeature`] — pipelines are background
/// infrastructure, not interactive UI.
pub struct PipelineService {
    registry: PipelineRegistry,
    scheduler_rx: Option<std::sync::mpsc::Receiver<ScheduledPhaseEvent>>,
    active_scheduled_phases: HashSet<String>,
    max_concurrent_phases: usize,
}

impl Default for PipelineService {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineService {
    /// Create an empty service with no scheduler receiver.
    pub fn new() -> Self {
        Self {
            registry: PipelineRegistry::new(),
            scheduler_rx: None,
            active_scheduled_phases: HashSet::new(),
            max_concurrent_phases: 3,
        }
    }

    /// Create a service wired to a scheduler event channel.
    pub fn with_scheduler_rx(
        rx: std::sync::mpsc::Receiver<ScheduledPhaseEvent>,
        max_concurrent_phases: usize,
    ) -> Self {
        Self {
            registry: PipelineRegistry::new(),
            scheduler_rx: Some(rx),
            active_scheduled_phases: HashSet::new(),
            max_concurrent_phases,
        }
    }

    /// Process at most one scheduler event (non-blocking).
    ///
    /// Returns [`TickResult::Idle`] when no event is available.
    pub fn tick(&mut self) -> TickResult {
        let rx = match self.scheduler_rx.as_ref() {
            Some(rx) => rx,
            None => return TickResult::Idle,
        };

        match rx.try_recv() {
            Ok(event) => self.handle_event(event),
            Err(std::sync::mpsc::TryRecvError::Empty) => TickResult::Idle,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Channel closed — clear receiver so we stop polling it.
                self.scheduler_rx = None;
                TickResult::Idle
            }
        }
    }

    /// Handle a single scheduler event, updating active phase tracking.
    fn handle_event(&mut self, event: ScheduledPhaseEvent) -> TickResult {
        match event {
            ScheduledPhaseEvent::PhaseReady { phase_id, .. } => {
                if self.active_scheduled_phases.len() < self.max_concurrent_phases {
                    self.active_scheduled_phases.insert(phase_id.clone());
                    TickResult::PhaseReady { phase_id }
                } else {
                    // At concurrency limit — skip this phase.
                    TickResult::PhaseSkipped { phase_id }
                }
            }
            ScheduledPhaseEvent::PhaseCompleted { phase_id, .. } => {
                self.active_scheduled_phases.remove(&phase_id);
                TickResult::PhaseCompleted { phase_id }
            }
            ScheduledPhaseEvent::PhaseFailed { phase_id, .. } => {
                self.active_scheduled_phases.remove(&phase_id);
                TickResult::PhaseFailed { phase_id }
            }
            ScheduledPhaseEvent::PhaseSkipped { phase_id, .. } => {
                TickResult::PhaseSkipped { phase_id }
            }
            ScheduledPhaseEvent::SchedulerStarted { phase_count } => {
                TickResult::SchedulerStarted { phase_count }
            }
            ScheduledPhaseEvent::SchedulerStopped => TickResult::SchedulerStopped,
            ScheduledPhaseEvent::PhaseStarting { .. } | ScheduledPhaseEvent::SchedulerError { .. } => {
                TickResult::Other
            }
        }
    }

    /// Register a pipeline in the registry. Returns the total step count after
    /// registration.
    pub fn register_pipeline(
        &mut self,
        manifest: &crate::app::pipeline::manifest::Manifest,
    ) -> anyhow::Result<usize> {
        self.registry.load_from_manifest(manifest)?;
        Ok(self.registry.step_count())
    }

    /// Return the IDs of currently active (in-flight) scheduled phases.
    pub fn get_active_phases(&self) -> &HashSet<String> {
        &self.active_scheduled_phases
    }

    /// Return a reference to the underlying pipeline registry.
    pub fn registry(&self) -> &PipelineRegistry {
        &self.registry
    }

    /// Return a mutable reference to the underlying pipeline registry.
    pub fn registry_mut(&mut self) -> &mut PipelineRegistry {
        &mut self.registry
    }

    /// Check whether a given phase is currently active.
    pub fn is_phase_active(&self, phase_id: &str) -> bool {
        self.active_scheduled_phases.contains(phase_id)
    }

    /// Return the maximum number of concurrent phases.
    pub fn max_concurrent_phases(&self) -> usize {
        self.max_concurrent_phases
    }

    /// Check whether a scheduler receiver is wired up.
    pub fn has_scheduler(&self) -> bool {
        self.scheduler_rx.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use chrono::Utc;

    // ── Construction ──────────────────────────────────────────────────

    #[test]
    fn new_creates_empty_service() {
        let svc = PipelineService::new();
        assert!(svc.get_active_phases().is_empty());
        assert!(!svc.has_scheduler());
        assert_eq!(svc.max_concurrent_phases(), 3);
    }

    #[test]
    fn default_same_as_new() {
        let svc = PipelineService::default();
        assert!(svc.get_active_phases().is_empty());
    }

    #[test]
    fn with_scheduler_rx_sets_up_receiver() {
        let (_tx, rx) = mpsc::channel();
        let svc = PipelineService::with_scheduler_rx(rx, 5);
        assert!(svc.has_scheduler());
        assert_eq!(svc.max_concurrent_phases(), 5);
    }

    // ── tick() behaviour ──────────────────────────────────────────────

    #[test]
    fn tick_returns_idle_when_no_receiver() {
        let mut svc = PipelineService::new();
        assert_eq!(svc.tick(), TickResult::Idle);
    }

    #[test]
    fn tick_returns_idle_when_channel_empty() {
        let (_tx, rx) = mpsc::channel();
        let mut svc = PipelineService::with_scheduler_rx(rx, 3);
        assert_eq!(svc.tick(), TickResult::Idle);
    }

    #[test]
    fn tick_processes_phase_ready() {
        let (tx, rx) = mpsc::channel();
        let mut svc = PipelineService::with_scheduler_rx(rx, 3);

        tx.send(ScheduledPhaseEvent::PhaseReady {
            phase_id: "p1".into(),
            scheduled_fire_time: Utc::now(),
            actual_fire_time: Instant::now(),
        })
        .unwrap();

        let result = svc.tick();
        assert_eq!(result, TickResult::PhaseReady {
            phase_id: "p1".into(),
        });
        assert!(svc.is_phase_active("p1"));
    }

    #[test]
    fn tick_processes_phase_completed_and_removes_from_active() {
        let (tx, rx) = mpsc::channel();
        let mut svc = PipelineService::with_scheduler_rx(rx, 3);

        // First: add a phase.
        tx.send(ScheduledPhaseEvent::PhaseReady {
            phase_id: "p1".into(),
            scheduled_fire_time: Utc::now(),
            actual_fire_time: Instant::now(),
        })
        .unwrap();
        let _ = svc.tick();
        assert!(svc.is_phase_active("p1"));

        // Second: complete it.
        tx.send(ScheduledPhaseEvent::PhaseCompleted {
            phase_id: "p1".into(),
            duration: Duration::from_secs(1),
        })
        .unwrap();
        let result = svc.tick();
        assert_eq!(result, TickResult::PhaseCompleted {
            phase_id: "p1".into(),
        });
        assert!(!svc.is_phase_active("p1"));
    }

    #[test]
    fn tick_processes_phase_failed_and_removes_from_active() {
        let (tx, rx) = mpsc::channel();
        let mut svc = PipelineService::with_scheduler_rx(rx, 3);

        tx.send(ScheduledPhaseEvent::PhaseReady {
            phase_id: "pX".into(),
            scheduled_fire_time: Utc::now(),
            actual_fire_time: Instant::now(),
        })
        .unwrap();
        let _ = svc.tick();

        tx.send(ScheduledPhaseEvent::PhaseFailed {
            phase_id: "pX".into(),
            error: "boom".into(),
        })
        .unwrap();
        let result = svc.tick();
        assert_eq!(result, TickResult::Failed {
            phase_id: "pX".into(),
        });
        assert!(!svc.is_phase_active("pX"));
    }

    #[test]
    fn tick_processes_scheduler_started() {
        let (tx, rx) = mpsc::channel();
        let mut svc = PipelineService::with_scheduler_rx(rx, 3);

        tx.send(ScheduledPhaseEvent::SchedulerStarted { phase_count: 7 })
            .unwrap();

        let result = svc.tick();
        assert_eq!(result, TickResult::SchedulerStarted { phase_count: 7 });
    }

    #[test]
    fn tick_processes_scheduler_stopped() {
        let (tx, rx) = mpsc::channel();
        let mut svc = PipelineService::with_scheduler_rx(rx, 3);

        tx.send(ScheduledPhaseEvent::SchedulerStopped).unwrap();

        let result = svc.tick();
        assert_eq!(result, TickResult::SchedulerStopped);
    }

    #[test]
    fn tick_skips_phase_at_concurrency_limit() {
        let (tx, rx) = mpsc::channel();
        let mut svc = PipelineService::with_scheduler_rx(rx, 1);

        // Fill the one slot.
        tx.send(ScheduledPhaseEvent::PhaseReady {
            phase_id: "p1".into(),
            scheduled_fire_time: Utc::now(),
            actual_fire_time: Instant::now(),
        })
        .unwrap();
        let _ = svc.tick();
        assert_eq!(svc.get_active_phases().len(), 1);

        // Second phase should be skipped.
        tx.send(ScheduledPhaseEvent::PhaseReady {
            phase_id: "p2".into(),
            scheduled_fire_time: Utc::now(),
            actual_fire_time: Instant::now(),
        })
        .unwrap();
        let result = svc.tick();
        assert_eq!(result, TickResult::PhaseSkipped {
            phase_id: "p2".into(),
        });
    }

    #[test]
    fn tick_clears_receiver_on_disconnect() {
        let (tx, rx) = mpsc::channel();
        let mut svc = PipelineService::with_scheduler_rx(rx, 3);
        assert!(svc.has_scheduler());

        drop(tx);

        let result = svc.tick();
        assert_eq!(result, TickResult::Idle);
        assert!(!svc.has_scheduler(), "receiver should be cleared after disconnect");
    }
}
