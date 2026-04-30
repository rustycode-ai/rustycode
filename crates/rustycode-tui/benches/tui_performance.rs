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

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use crossterm::event::{KeyCode, KeyModifiers};
use rustycode_tui::ui::input::InputHandler;
use rustycode_tui::ui::message::{Message, MessageRole};
use std::time::{Duration, Instant};

fn bench_input_latency_single_char(c: &mut Criterion) {
    let mut handler = InputHandler::new();
    c.bench_function("input_latency/single_char", |b| {
        b.iter(|| {
            handler.handle_key_event(KeyCode::Char('a'), KeyModifiers::NONE);
            black_box(&handler);
        })
    });
}

fn bench_input_latency_multiline(c: &mut Criterion) {
    let mut handler = InputHandler::new();
    c.bench_function("input_latency/multiline", |b| {
        b.iter(|| {
            handler.handle_key_event(KeyCode::Char('a'), KeyModifiers::NONE);
            black_box(&handler);
        })
    });
}

fn bench_message_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_construction");
    for count in [10, 50, 100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                let messages: Vec<Message> = (0..*count)
                    .map(|i| {
                        Message::new(
                            if i % 2 == 0 {
                                MessageRole::User
                            } else {
                                MessageRole::Assistant
                            },
                            format!("Message {} with some content that needs rendering", i),
                        )
                    })
                    .collect();
                black_box(messages);
            })
        });
    }
    group.finish();
}

fn bench_message_memory_growth(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");
    for count in [100, 500, 1000, 5000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter_custom(|iters| {
                let start = Instant::now();
                for _ in 0..iters {
                    let messages: Vec<Message> = (0..*count)
                        .map(|i| {
                            Message::new(
                                MessageRole::Assistant,
                                format!(
                                    "Message {} with increasingly long content to simulate real usage",
                                    i
                                ),
                            )
                        })
                        .collect();
                    black_box(messages);
                }
                start.elapsed()
            })
        });
    }
    group.finish();
}

fn bench_channel_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("channels");
    for capacity in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("streaming", capacity),
            capacity,
            |b, cap| {
                let (tx, rx) = std::sync::mpsc::sync_channel(*cap);
                b.iter(|| {
                    for i in 0..100 {
                        tx.send(format!("Chunk {}", i)).unwrap();
                    }
                    black_box(&rx);
                })
            },
        );
    }
    group.finish();
}

fn bench_p99_input_latency(c: &mut Criterion) {
    let mut handler = InputHandler::new();
    let mut latencies = Vec::with_capacity(1000);
    c.bench_function("p99/input_latency", |b| {
        b.iter(|| {
            let start = Instant::now();
            handler.handle_key_event(KeyCode::Char('x'), KeyModifiers::NONE);
            latencies.push(start.elapsed());
        })
    });
    latencies.sort();
    if latencies.is_empty() {
        println!("No latency samples collected (skipped)");
        return;
    }
    let p99 = latencies[latencies.len() * 99 / 100];
    println!("P99 input latency: {:?}", p99);
    assert!(
        p99 < Duration::from_millis(50),
        "P99 input latency exceeds 50ms threshold"
    );
}

criterion_group!(
    input_benches,
    bench_input_latency_single_char,
    bench_input_latency_multiline
);
criterion_group!(
    message_benches,
    bench_message_construction,
    bench_message_memory_growth
);
criterion_group!(channel_benches, bench_channel_throughput);
criterion_group!(latency_benches, bench_p99_input_latency);
criterion_main!(
    input_benches,
    message_benches,
    channel_benches,
    latency_benches
);
