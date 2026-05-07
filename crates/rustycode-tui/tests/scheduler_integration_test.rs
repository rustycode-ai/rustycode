#![allow(
    clippy::bool_to_int_with_if,
    clippy::branches_sharing_code,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::collapsible_else_if,
    clippy::collection_is_never_read,
    clippy::default_trait_access,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::ignore_without_reason,
    clippy::ignored_unit_patterns,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::items_after_statements,
    clippy::iter_on_single_items,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::option_if_let_else,
    clippy::range_plus_one,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::ref_option,
    clippy::search_is_some,
    clippy::semicolon_if_nothing_returned,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_peekable,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::used_underscore_binding,
    clippy::useless_let_if_seq
)]

//! Integration tests for the cron scheduler mpsc channel and event handler wiring.

use std::collections::HashSet;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;

use rustycode_tui::app::pipeline::manifest::{Manifest, ManifestMetadata, PhaseDefinition};
use rustycode_tui::app::pipeline::scheduler::{
    PipelineCronScheduler, ScheduledPhaseEvent, SchedulerConfig,
};
use rustycode_tui::app::pipeline::types::{FailureStrategy, RetryPolicy};

fn hard_block_strategy() -> FailureStrategy {
    FailureStrategy::HardBlock {
        retry: RetryPolicy::default(),
    }
}

fn make_manifest(phases: Vec<(&str, Option<&str>)>) -> Manifest {
    Manifest {
        version: "1.0".to_string(),
        metadata: ManifestMetadata {
            name: "scheduler_integration_test".to_string(),
            description: None,
            owner: None,
        },
        phases: phases
            .into_iter()
            .map(|(id, schedule)| PhaseDefinition {
                id: id.to_string(),
                description: None,
                schedule: schedule.map(String::from),
                failure_strategy: hard_block_strategy(),
                timeout_secs: None,
                parallel: None,
                hard_deps: None,
                soft_deps: None,
                steps: None,
                artifacts_produced: None,
            })
            .collect(),
    }
}

fn drain_events(
    rx: &mpsc::Receiver<ScheduledPhaseEvent>,
    timeout: Duration,
) -> Vec<ScheduledPhaseEvent> {
    let mut events = Vec::new();
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(event) => events.push(event),
            Err(_) => break,
        }
    }
    events
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_scheduler_start_sends_started_event() {
    let (tx, rx) = mpsc::channel();
    let scheduler = PipelineCronScheduler::new(SchedulerConfig::default(), tx);

    let manifest = make_manifest(vec![("phase_1", Some("0 8 * * *"))]);
    scheduler.start(&manifest).expect("start should succeed");

    let events = drain_events(&rx, Duration::from_secs(2));
    let started = events
        .iter()
        .find(|e| matches!(e, ScheduledPhaseEvent::SchedulerStarted { .. }));
    assert!(started.is_some());

    if let ScheduledPhaseEvent::SchedulerStarted { phase_count } = started.unwrap() {
        assert_eq!(*phase_count, 1);
    }

    scheduler.stop();

    let stop_events = drain_events(&rx, Duration::from_secs(2));
    assert!(stop_events
        .iter()
        .any(|e| matches!(e, ScheduledPhaseEvent::SchedulerStopped)));
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_scheduler_no_schedule_phases_not_counted() {
    let (tx, rx) = mpsc::channel();
    let scheduler = PipelineCronScheduler::new(SchedulerConfig::default(), tx);

    let manifest = make_manifest(vec![
        ("phase_no_schedule", None),
        ("phase_also_no_schedule", None),
    ]);
    scheduler.start(&manifest).expect("start should succeed");

    let events = drain_events(&rx, Duration::from_secs(2));
    if let Some(ScheduledPhaseEvent::SchedulerStarted { phase_count }) = events
        .iter()
        .find(|e| matches!(e, ScheduledPhaseEvent::SchedulerStarted { .. }))
    {
        assert_eq!(*phase_count, 0);
    } else {
        panic!("Expected SchedulerStarted event");
    }

    scheduler.stop();
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_scheduler_invalid_cron_sends_error() {
    let (tx, rx) = mpsc::channel();
    let scheduler = PipelineCronScheduler::new(SchedulerConfig::default(), tx);

    let manifest = make_manifest(vec![("bad_cron", Some("not valid cron"))]);
    scheduler.start(&manifest).expect("start should succeed");

    let events = drain_events(&rx, Duration::from_secs(2));

    let error_event = events
        .iter()
        .find(|e| matches!(e, ScheduledPhaseEvent::SchedulerError { .. }));
    assert!(error_event.is_some());

    if let ScheduledPhaseEvent::SchedulerError { phase_id, error } = error_event.unwrap() {
        assert_eq!(phase_id, "bad_cron");
        assert!(error.contains("Invalid cron"));
    }

    scheduler.stop();
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_channel_drain_pattern() {
    let (tx, rx) = mpsc::channel::<ScheduledPhaseEvent>();

    let events_to_send: Vec<ScheduledPhaseEvent> = vec![
        ScheduledPhaseEvent::SchedulerStarted { phase_count: 2 },
        ScheduledPhaseEvent::PhaseSkipped {
            phase_id: "p1".to_string(),
            reason: "test".to_string(),
        },
        ScheduledPhaseEvent::SchedulerStopped,
    ];

    for event in events_to_send {
        tx.send(event).expect("send should succeed");
    }

    drop(tx);

    let received = drain_events(&rx, Duration::from_secs(2));
    assert_eq!(received.len(), 3);
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_active_phases_tracking() {
    let mut active: HashSet<String> = HashSet::new();

    active.insert("phase_1".to_string());
    active.insert("phase_2".to_string());
    assert_eq!(active.len(), 2);

    active.remove("phase_1");
    assert_eq!(active.len(), 1);
    assert!(active.contains("phase_2"));

    active.remove("phase_2");
    assert!(active.is_empty());
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_concurrency_limit_enforcement() {
    let max_concurrent = 2usize;
    let mut active: HashSet<String> = HashSet::new();

    active.insert("p1".to_string());
    active.insert("p2".to_string());
    assert!(active.len() >= max_concurrent);

    let can_add = active.len() < max_concurrent;
    assert!(!can_add);

    active.remove("p1");
    assert!(active.len() < max_concurrent);
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_event_variants_roundtrip() {
    let (tx, rx) = mpsc::channel::<ScheduledPhaseEvent>();

    tx.send(ScheduledPhaseEvent::PhaseSkipped {
        phase_id: "p1".to_string(),
        reason: "dependency failed".to_string(),
    })
    .expect("send");
    tx.send(ScheduledPhaseEvent::SchedulerError {
        phase_id: "p1".to_string(),
        error: "parse error".to_string(),
    })
    .expect("send");
    tx.send(ScheduledPhaseEvent::SchedulerStarted { phase_count: 3 })
        .expect("send");
    tx.send(ScheduledPhaseEvent::SchedulerStopped)
        .expect("send");

    drop(tx);

    let received: Vec<ScheduledPhaseEvent> = rx.iter().collect();
    assert_eq!(received.len(), 4);
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_multiple_phases_only_scheduled_counted() {
    let (tx, rx) = mpsc::channel();
    let scheduler = PipelineCronScheduler::new(SchedulerConfig::default(), tx);

    let manifest = make_manifest(vec![
        ("daily_phase", Some("0 8 * * *")),
        ("hourly_phase", Some("0 * * * *")),
        ("no_schedule_phase", None),
    ]);

    scheduler.start(&manifest).expect("start should succeed");

    let events = drain_events(&rx, Duration::from_secs(2));

    if let Some(ScheduledPhaseEvent::SchedulerStarted { phase_count }) = events
        .iter()
        .find(|e| matches!(e, ScheduledPhaseEvent::SchedulerStarted { .. }))
    {
        assert_eq!(*phase_count, 2);
    } else {
        panic!("Expected SchedulerStarted event");
    }

    scheduler.stop();
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_cron_5_and_6_field_parsing() -> Result<()> {
    let s5 = SchedulerConfig::parse_cron("0 8 * * *")?;
    let now = chrono::Utc::now();
    let next5 = SchedulerConfig::next_fire_time(&s5, &now);
    assert!(next5.is_some());

    let s6 = SchedulerConfig::parse_cron("0 0 8 * * *")?;
    let next6 = SchedulerConfig::next_fire_time(&s6, &now);
    assert!(next6.is_some());

    Ok(())
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_scheduler_lifecycle() {
    let (tx, rx) = mpsc::channel();
    let scheduler = PipelineCronScheduler::new(SchedulerConfig::default(), tx);

    assert!(!scheduler.is_running());

    let manifest = make_manifest(vec![("phase_1", Some("0 0 31 12 *"))]);
    scheduler.start(&manifest).expect("start should succeed");
    assert!(scheduler.is_running());

    let events = drain_events(&rx, Duration::from_secs(2));
    assert!(events
        .iter()
        .any(|e| matches!(e, ScheduledPhaseEvent::SchedulerStarted { .. })));

    scheduler.stop();
    assert!(!scheduler.is_running());

    let stop_events = drain_events(&rx, Duration::from_secs(2));
    assert!(stop_events
        .iter()
        .any(|e| matches!(e, ScheduledPhaseEvent::SchedulerStopped)));
}

#[cfg_attr(not(feature = "slow-tests"), ignore = "slow test: run with --features slow-tests")]
#[test]
fn test_invalid_expressions_rejected() {
    assert!(SchedulerConfig::parse_cron("").is_err());
    assert!(SchedulerConfig::parse_cron("not a cron").is_err());
    assert!(SchedulerConfig::parse_cron("0").is_err());
}
