// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Event bus benchmarks
//!
//! Measures event bus throughput and performance for:
//! - Event publishing
//! - Subscription and unsubscription
//! - Wildcard matching
//! - Multi-subscriber delivery
//! - Hook execution overhead

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use rustycode_bus::{Event, EventBus, EventBusConfig, HookPhase, SessionStartedEvent};
use rustycode_protocol::SessionId;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Create a runtime for async benchmarks
fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Benchmark event publishing with no subscribers
fn bench_publish_no_subscribers(c: &mut Criterion) {
    let rt = runtime();
    let bus = rt.block_on(async { EventBus::new() });

    c.bench_function("publish_no_subscribers", |b| {
        b.iter(|| {
            let event = SessionStartedEvent::new(
                SessionId::new(),
                "test task".to_string(),
                "test detail".to_string(),
            );
            rt.block_on(bus.publish(event)).ok();
        });
    });
}

/// Benchmark event publishing with one subscriber
fn bench_publish_one_subscriber(c: &mut Criterion) {
    let rt = runtime();

    c.bench_function("publish_one_subscriber", |b| {
        b.iter(|| {
            let bus = rt.block_on(async { EventBus::new() });
            rt.block_on(async {
                let (_id, mut _rx) = bus.subscribe("session.started").await.unwrap();

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });
}

/// Benchmark event publishing with multiple subscribers
fn bench_publish_multiple_subscribers(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("publish_multiple_subscribers");

    for count in [1, 10, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter(|| {
                let bus = rt.block_on(async { EventBus::new() });
                rt.block_on(async {
                    // Create multiple subscribers
                    for _ in 0..count {
                        let (_id, mut _rx) = bus.subscribe("session.started").await.unwrap();
                        // Drop receiver to focus on publish performance
                    }

                    let event = SessionStartedEvent::new(
                        SessionId::new(),
                        "test task".to_string(),
                        "test detail".to_string(),
                    );
                    bus.publish(event).await.ok();
                });
            });
        });
    }

    group.finish();
}

/// Benchmark subscription creation
fn bench_subscribe(c: &mut Criterion) {
    let rt = runtime();
    c.bench_function("subscribe", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let (_id, _rx) = bus.subscribe("session.started").await.unwrap();
            });
        });
    });
}

/// Benchmark unsubscription
fn bench_unsubscribe(c: &mut Criterion) {
    let rt = runtime();

    c.bench_function("unsubscribe", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let (id, _rx) = bus.subscribe("session.started").await.unwrap();
                bus.unsubscribe(id).await.ok();
            });
        });
    });
}

/// Benchmark exact matching
fn bench_exact_matching(c: &mut Criterion) {
    let mut group = c.benchmark_group("pattern_matching");

    group.bench_function("exact_match", |b| {
        b.iter(|| {
            let filter = rustycode_bus::SubscriptionFilter::new("session.started").unwrap();
            let event_type = black_box("session.started");
            let _matches = filter.matches(event_type);
        });
    });

    group.finish();
}

/// Benchmark wildcard matching
fn bench_wildcard_matching(c: &mut Criterion) {
    let _rt = runtime();
    let mut group = c.benchmark_group("pattern_matching");

    // Test different wildcard patterns
    let patterns = vec![
        ("*", "simple_wildcard"),
        ("session.*", "prefix_wildcard"),
        ("*.started", "suffix_wildcard"),
        ("session.*.started", "middle_wildcard"),
    ];

    for (pattern, name) in patterns {
        group.bench_function(name, |b| {
            b.iter(|| {
                let filter = rustycode_bus::SubscriptionFilter::new(pattern).unwrap();
                let event_type = black_box("session.started");
                let _matches = filter.matches(event_type);
            });
        });
    }

    group.finish();
}

/// Benchmark wildcard matching with multiple event types
fn bench_wildcard_matching_multiple_events(c: &mut Criterion) {
    let _rt = runtime();
    let mut group = c.benchmark_group("wildcard_multiple_events");

    let event_types = vec![
        "session.started",
        "session.ended",
        "context.assembled",
        "tool.executed",
        "inspection.completed",
    ];

    group.bench_function("session_star_pattern", |b| {
        b.iter(|| {
            let filter = rustycode_bus::SubscriptionFilter::new("session.*").unwrap();
            for event_type in &event_types {
                let _matches = black_box(filter.matches(event_type));
            }
        });
    });

    group.bench_function("star_pattern", |b| {
        b.iter(|| {
            let filter = rustycode_bus::SubscriptionFilter::new("*").unwrap();
            for event_type in &event_types {
                let _matches = black_box(filter.matches(event_type));
            }
        });
    });

    group.finish();
}

/// Benchmark hook execution overhead
fn bench_hook_overhead(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("hook_overhead");

    group.bench_function("no_hooks", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.bench_function("one_hook", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                bus.register_hook(HookPhase::PrePublish, |_event| {
                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                })
                .await;

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.bench_function("three_hooks", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                for _ in 0..3 {
                    bus.register_hook(HookPhase::PrePublish, |_event| {
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await;
                }

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.finish();
}

/// Benchmark high-throughput event publishing
fn bench_throughput(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("event_throughput");

    let throughput = Throughput::Elements(1000);

    group.throughput(throughput.clone());
    group.bench_function("publish_1000_events", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let (_id, mut _rx) = bus.subscribe("session.started").await.unwrap();

                for _ in 0..1000 {
                    let event = SessionStartedEvent::new(
                        SessionId::new(),
                        "test task".to_string(),
                        "test detail".to_string(),
                    );
                    bus.publish(event).await.ok();
                }
            });
        });
    });

    group.finish();
}

/// Benchmark event serialization
fn bench_event_serialization(c: &mut Criterion) {
    let _rt = runtime();
    let mut group = c.benchmark_group("event_serialization");

    group.bench_function("serialize_event", |b| {
        b.iter(|| {
            let event = SessionStartedEvent::new(
                SessionId::new(),
                "test task".to_string(),
                "test detail".to_string(),
            );
            let serialized = event.serialize();
            black_box(serialized);
        });
    });

    group.finish();
}

/// Benchmark metrics collection
fn bench_metrics(c: &mut Criterion) {
    let _rt = runtime();
    c.bench_function("get_metrics", |b| {
        b.iter(|| {
            let bus = EventBus::new();
            let _metrics = bus.metrics();
        });
    });
}

/// Benchmark different channel capacities
fn bench_channel_capacity(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("channel_capacity");

    for capacity in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(capacity),
            capacity,
            |b, &capacity| {
                b.iter(|| {
                    rt.block_on(async {
                        let config = EventBusConfig {
                            channel_capacity: capacity,
                            ..Default::default()
                        };
                        let bus = EventBus::with_config(config);
                        let (_id, _rx) = bus.subscribe("session.started").await.unwrap();

                        let event = SessionStartedEvent::new(
                            SessionId::new(),
                            "test task".to_string(),
                            "test detail".to_string(),
                        );
                        bus.publish(event).await.ok();
                    });
                });
            },
        );
    }

    group.finish();
}

/// Benchmark concurrent publishing
fn bench_concurrent_publish(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("concurrent_operations");

    group.bench_function("concurrent_publish_10_tasks", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = Arc::new(EventBus::new());
                let (_id, _rx) = bus.subscribe("session.started").await.unwrap();

                let mut handles = vec![];
                for _ in 0..10 {
                    let bus = bus.clone();
                    let handle = tokio::spawn(async move {
                        for _ in 0..100 {
                            let event = SessionStartedEvent::new(
                                SessionId::new(),
                                "test task".to_string(),
                                "test detail".to_string(),
                            );
                            let _ = bus.publish(event).await;
                        }
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    handle.await.ok();
                }
            });
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_publish_no_subscribers,
    bench_publish_one_subscriber,
    bench_publish_multiple_subscribers,
    bench_subscribe,
    bench_unsubscribe,
    bench_exact_matching,
    bench_wildcard_matching,
    bench_wildcard_matching_multiple_events,
    bench_hook_overhead,
    bench_throughput,
    bench_event_serialization,
    bench_metrics,
    bench_channel_capacity,
    bench_concurrent_publish
);

// ========== Hybrid Event Publishing Benchmarks ==========

/// Benchmark callback subscription (zero-cost abstraction)
fn bench_callback_subscription(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("hybrid_publishing");

    group.bench_function("callback_single_subscriber", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let _id = bus
                    .subscribe_callback("session.started", |_event| {
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await
                    .unwrap();

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.bench_function("callback_with_processing", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let _id = bus
                    .subscribe_callback("session.started", |event| {
                        // Simulate some processing
                        let _ = event.event_type().len();
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await
                    .unwrap();

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.finish();
}

/// Benchmark hybrid subscription (callback + channel)
fn bench_hybrid_subscription(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("hybrid_publishing");

    group.bench_function("hybrid_single_subscriber", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let (_id, mut _rx) = bus
                    .subscribe_hybrid("session.started", |_event| {
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await
                    .unwrap();

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.finish();
}

/// Benchmark comparison: broadcast vs callback
fn bench_broadcast_vs_callback(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("broadcast_vs_callback");

    // Benchmark with single subscriber
    group.bench_function("single_broadcast", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let (_id, mut _rx) = bus.subscribe("session.started").await.unwrap();

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.bench_function("single_callback", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let _id = bus
                    .subscribe_callback("session.started", |_event| {
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await
                    .unwrap();

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.finish();
}

/// Benchmark callback subscription creation
fn bench_callback_subscribe(c: &mut Criterion) {
    let rt = runtime();
    c.bench_function("subscribe_callback", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let _id = bus
                    .subscribe_callback("session.started", |_event| {
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await
                    .unwrap();
            });
        });
    });
}

/// Benchmark high-throughput callback publishing
fn bench_callback_throughput(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("callback_throughput");

    let throughput = Throughput::Elements(1000);

    group.throughput(throughput.clone());
    group.bench_function("callback_publish_1000_events", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();
                let _id = bus
                    .subscribe_callback("session.started", |_event| {
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await
                    .unwrap();

                for _ in 0..1000 {
                    let event = SessionStartedEvent::new(
                        SessionId::new(),
                        "test task".to_string(),
                        "test detail".to_string(),
                    );
                    bus.publish(event).await.ok();
                }
            });
        });
    });

    group.finish();
}

/// Benchmark multiple callback subscribers
fn bench_multiple_callback_subscribers(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("multiple_callbacks");

    for count in [1, 10, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter(|| {
                rt.block_on(async {
                    let bus = EventBus::new();
                    // Create multiple callback subscribers
                    for _ in 0..count {
                        let _id = bus
                            .subscribe_callback("session.started", |_event| {
                                Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                            })
                            .await
                            .unwrap();
                    }

                    let event = SessionStartedEvent::new(
                        SessionId::new(),
                        "test task".to_string(),
                        "test detail".to_string(),
                    );
                    bus.publish(event).await.ok();
                });
            });
        });
    }

    group.finish();
}

/// Benchmark mixed subscription types
fn bench_mixed_subscriptions(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("mixed_subscriptions");

    group.bench_function("broadcast_and_callback", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();

                // Add broadcast subscriber
                let (_id, mut _rx) = bus.subscribe("session.started").await.unwrap();

                // Add callback subscriber
                let _id = bus
                    .subscribe_callback("session.started", |_event| {
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await
                    .unwrap();

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.bench_function("broadcast_callback_and_hybrid", |b| {
        b.iter(|| {
            rt.block_on(async {
                let bus = EventBus::new();

                // Add broadcast subscriber
                let (_id, mut _rx) = bus.subscribe("session.started").await.unwrap();

                // Add callback subscriber
                let _id = bus
                    .subscribe_callback("session.started", |_event| {
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await
                    .unwrap();

                // Add hybrid subscriber
                let (_id, mut _rx) = bus
                    .subscribe_hybrid("session.started", |_event| {
                        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                    })
                    .await
                    .unwrap();

                let event = SessionStartedEvent::new(
                    SessionId::new(),
                    "test task".to_string(),
                    "test detail".to_string(),
                );
                bus.publish(event).await.ok();
            });
        });
    });

    group.finish();
}

criterion_group!(
    hybrid_benches,
    bench_callback_subscription,
    bench_hybrid_subscription,
    bench_broadcast_vs_callback,
    bench_callback_subscribe,
    bench_callback_throughput,
    bench_multiple_callback_subscribers,
    bench_mixed_subscriptions
);

criterion_main!(benches, hybrid_benches);
