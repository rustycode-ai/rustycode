#![allow(
    clippy::explicit_iter_loop,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Benchmarks for session compaction

use std::hint::black_box;

use chrono::Duration;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rustycode_session::{CompactionEngine, CompactionStrategy, Message, Session};

fn create_large_session(message_count: usize) -> Session {
    let mut session = Session::new("Benchmark Session");
    for i in 0..message_count {
        if i % 2 == 0 {
            session.add_message(Message::user(format!(
                "User message {} with some content that makes it a bit longer",
                i
            )));
        } else {
            session.add_message(Message::assistant(format!(
                "Assistant response {} with more content to increase token count",
                i
            )));
        }
    }
    session
}

fn bench_compaction_by_tokens(c: &mut Criterion) {
    let mut group = c.benchmark_group("token_compaction");

    for message_count in [100, 500, 1000, 5000].iter() {
        group.bench_with_input(
            BenchmarkId::new("compact", message_count),
            message_count,
            |b, &count| {
                let session = create_large_session(count);
                let engine = CompactionEngine::new(CompactionStrategy::token_threshold(0.5, 10));

                b.iter(|| {
                    let (compacted, _) = engine.compact(black_box(&session)).unwrap();
                    black_box(compacted)
                });
            },
        );
    }

    group.finish();
}

fn bench_compaction_by_age(c: &mut Criterion) {
    let mut group = c.benchmark_group("age_compaction");

    for message_count in [100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("compact", message_count),
            message_count,
            |b, &count| {
                let session = create_large_session(count);
                let engine = CompactionEngine::new(CompactionStrategy::message_age(
                    Duration::seconds(3600),
                    10,
                ));

                b.iter(|| {
                    let (compacted, _) = engine.compact(black_box(&session)).unwrap();
                    black_box(compacted)
                });
            },
        );
    }

    group.finish();
}

fn bench_token_estimation(c: &mut Criterion) {
    let mut group = c.benchmark_group("token_estimation");

    for message_count in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("estimate", message_count),
            message_count,
            |b, &count| {
                let session = create_large_session(count);

                b.iter(|| black_box(session.estimate_tokens()));
            },
        );
    }

    group.finish();
}

fn bench_message_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_creation");

    group.bench_function("user_message", |b| {
        b.iter(|| {
            black_box(Message::user("Test message with some content"));
        });
    });

    group.bench_function("assistant_message", |b| {
        b.iter(|| {
            black_box(Message::assistant("Test response with some content"));
        });
    });

    group.finish();
}

fn bench_session_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_operations");

    group.bench_function("add_message", |b| {
        let mut session = Session::new("Test");
        b.iter(|| {
            session.add_message(Message::user("Test message"));
            black_box(&session);
        });
    });

    group.bench_function("fork_session", |b| {
        let session = create_large_session(100);
        b.iter(|| {
            black_box(session.fork());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_compaction_by_tokens,
    bench_compaction_by_age,
    bench_token_estimation,
    bench_message_creation,
    bench_session_operations
);

criterion_main!(benches);
