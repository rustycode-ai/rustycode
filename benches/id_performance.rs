// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! ID generation and sorting performance benchmarks
//!
//! Comprehensive benchmark suite comparing SortableID vs UUID performance:
//! - ID generation speed
//! - Sorting performance with varying dataset sizes
//! - Parsing/serialization overhead
//! - Memory footprint
//! - Hash-based operations (HashSet/HashMap)

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use rustycode_protocol::{EventId, FileId, MemoryId, PlanId, SessionId, SkillId, ToolId};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

// ============================================================================
// Generation Benchmarks
// ============================================================================

/// Benchmark UUID v4 generation as baseline
fn bench_uuid_generation(c: &mut Criterion) {
    c.bench_function("generation/uuid_v4", |b| b.iter(Uuid::new_v4));
}

/// Benchmark SortableID generation
fn bench_sortable_id_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("generation/sortable_id");

    group.bench_function("session_id", |b| b.iter(SessionId::new));

    group.bench_function("event_id", |b| b.iter(EventId::new));

    group.bench_function("memory_id", |b| b.iter(MemoryId::new));

    group.bench_function("skill_id", |b| b.iter(SkillId::new));

    group.bench_function("plan_id", |b| b.iter(PlanId::new));

    group.bench_function("tool_id", |b| b.iter(ToolId::new));

    group.bench_function("file_id", |b| b.iter(FileId::new));

    group.finish();
}

/// High-throughput generation benchmark
fn bench_generation_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("generation/throughput");

    let throughput = Throughput::Elements(10_000);

    group.throughput(throughput.clone());
    group.bench_function("uuid_10k", |b| {
        b.iter(|| {
            for _ in 0..10_000 {
                black_box(Uuid::new_v4());
            }
        })
    });

    group.throughput(throughput);
    group.bench_function("sortable_id_10k", |b| {
        b.iter(|| {
            for _ in 0..10_000 {
                black_box(SessionId::new());
            }
        })
    });

    group.finish();
}

// ============================================================================
// Size and Memory Benchmarks
// ============================================================================

/// Compare memory footprint
fn bench_memory_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");

    group.bench_function("uuid_size", |b| {
        b.iter(|| {
            let id = Uuid::new_v4();
            black_box(std::mem::size_of_val(&id));
        })
    });

    group.bench_function("sortable_id_size", |b| {
        b.iter(|| {
            let id = SessionId::new();
            black_box(std::mem::size_of_val(&id));
        })
    });

    group.bench_function("uuid_string_len", |b| {
        b.iter(|| {
            let id = Uuid::new_v4();
            black_box(id.to_string().len());
        })
    });

    group.bench_function("sortable_id_string_len", |b| {
        b.iter(|| {
            let id = SessionId::new();
            black_box(id.to_string().len());
        })
    });

    group.finish();
}

// ============================================================================
// Sorting Benchmarks
// ============================================================================

/// Benchmark sorting performance with varying dataset sizes
fn bench_sorting_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("sorting");

    for size in [100, 1_000, 10_000, 100_000].iter() {
        // UUID sorting
        group.bench_with_input(BenchmarkId::new("uuid", size), size, |b, &size| {
            let mut ids: Vec<Uuid> = (0..size).map(|_| Uuid::new_v4()).collect();
            b.iter(|| {
                ids.sort();
                black_box(&ids);
            });
        });

        // SortableID sorting
        group.bench_with_input(BenchmarkId::new("sortable_id", size), size, |b, &size| {
            let mut ids: Vec<SessionId> = (0..size).map(|_| SessionId::new()).collect();
            b.iter(|| {
                ids.sort();
                black_box(&ids);
            });
        });

        // String-based sorting (sortable ID as string)
        group.bench_with_input(
            BenchmarkId::new("sortable_id_as_string", size),
            size,
            |b, &size| {
                let mut ids: Vec<String> =
                    (0..size).map(|_| SessionId::new().to_string()).collect();
                b.iter(|| {
                    ids.sort();
                    black_box(&ids);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark sorting with partially sorted data (realistic scenario)
fn bench_partial_sorting(c: &mut Criterion) {
    let mut group = c.benchmark_group("sorting/partial");

    let size = 10_000;

    group.bench_function("uuid_90_percent_sorted", |b| {
        let mut ids: Vec<Uuid> = (0..size).map(|_| Uuid::new_v4()).collect();
        ids.sort();
        // Shuffle last 10%
        let last_10_percent = (size * 90 / 100)..size;
        ids[last_10_percent].rotate_right(1);

        b.iter(|| {
            ids.sort();
            black_box(&ids);
        });
    });

    group.bench_function("sortable_id_90_percent_sorted", |b| {
        let mut ids: Vec<SessionId> = (0..size).map(|_| SessionId::new()).collect();
        ids.sort();
        // Shuffle last 10%
        let last_10_percent = (size * 90 / 100)..size;
        ids[last_10_percent].rotate_right(1);

        b.iter(|| {
            ids.sort();
            black_box(&ids);
        });
    });

    group.finish();
}

// ============================================================================
// Parsing and Serialization Benchmarks
// ============================================================================

/// Benchmark parsing from string
fn bench_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("parsing");

    let uuid_str = Uuid::new_v4().to_string();
    group.bench_function("uuid_from_str", |b| {
        b.iter(|| {
            let parsed = Uuid::parse_str(black_box(&uuid_str));
            black_box(parsed)
        })
    });

    let sortable_id = SessionId::new();
    let sortable_id_str = sortable_id.to_string();
    group.bench_function("sortable_id_from_str", |b| {
        b.iter(|| {
            let parsed = SessionId::parse(black_box(&sortable_id_str));
            black_box(parsed)
        })
    });

    group.finish();
}

/// Benchmark serialization to string
fn bench_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");

    let uuid = Uuid::new_v4();
    group.bench_function("uuid_to_string", |b| {
        b.iter(|| {
            let s = uuid.to_string();
            black_box(s)
        })
    });

    let sortable_id = SessionId::new();
    group.bench_function("sortable_id_to_string", |b| {
        b.iter(|| {
            let s = sortable_id.to_string();
            black_box(s)
        })
    });

    group.finish();
}

/// Benchmark round-trip (serialize + parse)
fn bench_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("roundtrip");

    group.bench_function("uuid", |b| {
        let original = Uuid::new_v4();
        b.iter(|| {
            let serialized = original.to_string();
            let parsed = Uuid::parse_str(&serialized).unwrap();
            black_box(parsed)
        })
    });

    group.bench_function("sortable_id", |b| {
        let original = SessionId::new();
        b.iter(|| {
            let serialized = original.to_string();
            let parsed = SessionId::parse(&serialized).unwrap();
            black_box(parsed)
        })
    });

    group.finish();
}

// ============================================================================
// Hash-Based Operations
// ============================================================================

/// Benchmark HashSet insertion
fn bench_hashset_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashset/insertion");

    let throughput = Throughput::Elements(1_000);

    group.throughput(throughput.clone());
    group.bench_function("uuid_1k", |b| {
        b.iter(|| {
            let mut set = HashSet::new();
            for _ in 0..1_000 {
                set.insert(Uuid::new_v4());
            }
            black_box(set)
        })
    });

    group.throughput(throughput);
    group.bench_function("sortable_id_1k", |b| {
        b.iter(|| {
            let mut set = HashSet::new();
            for _ in 0..1_000 {
                set.insert(SessionId::new());
            }
            black_box(set)
        })
    });

    group.finish();
}

/// Benchmark HashSet lookup
fn bench_hashset_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashset/lookup");

    let uuid_set: HashSet<Uuid> = (0..10_000).map(|_| Uuid::new_v4()).collect();
    let uuid_to_find = uuid_set.iter().next().unwrap();

    group.bench_function("uuid_10k", |b| {
        b.iter(|| {
            let found = black_box(uuid_set.contains(uuid_to_find));
            black_box(found)
        })
    });

    let id_set: HashSet<SessionId> = (0..10_000).map(|_| SessionId::new()).collect();
    let id_to_find = id_set.iter().next().unwrap();

    group.bench_function("sortable_id_10k", |b| {
        b.iter(|| {
            let found = black_box(id_set.contains(id_to_find));
            black_box(found)
        })
    });

    group.finish();
}

/// Benchmark HashMap operations
fn bench_hashmap_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashmap");

    group.bench_function("uuid_insertion_1k", |b| {
        b.iter(|| {
            let mut map: HashMap<Uuid, &'static str> = HashMap::new();
            for _ in 0..1_000 {
                map.insert(Uuid::new_v4(), "value");
            }
            black_box(map)
        })
    });

    group.bench_function("sortable_id_insertion_1k", |b| {
        b.iter(|| {
            let mut map: HashMap<SessionId, &'static str> = HashMap::new();
            for _ in 0..1_000 {
                map.insert(SessionId::new(), "value");
            }
            black_box(map)
        })
    });

    group.finish();
}

// ============================================================================
// Feature-Specific Benchmarks
// ============================================================================

/// Benchmark timestamp extraction (sortable ID feature)
fn bench_timestamp_extraction(c: &mut Criterion) {
    c.bench_function("features/timestamp_extraction", |b| {
        let id = SessionId::new();
        b.iter(|| {
            let ts = id.timestamp();
            black_box(ts)
        })
    });
}

/// Benchmark prefix validation (sortable ID feature)
fn bench_prefix_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("features/prefix");

    group.bench_function("valid_session_id", |b| {
        let id_str = SessionId::new().to_string();
        b.iter(|| {
            let parsed = SessionId::parse(black_box(&id_str));
            black_box(parsed)
        })
    });

    group.bench_function("invalid_prefix", |b| {
        let invalid_id = "evt_1234567890123"; // Wrong prefix for SessionId
        b.iter(|| {
            let parsed = SessionId::parse(black_box(invalid_id));
            black_box(parsed)
        })
    });

    group.finish();
}

/// Benchmark time-based ordering (sortable ID key feature)
fn bench_time_ordering(c: &mut Criterion) {
    use std::thread;
    use std::time::Duration;

    c.bench_function("features/chronological_ordering", |b| {
        b.iter(|| {
            let mut ids = vec![];
            for _ in 0..100 {
                ids.push(SessionId::new());
                thread::sleep(Duration::from_micros(100));
            }
            // Should already be sorted by time
            let is_sorted = ids.windows(2).all(|w| w[0] < w[1]);
            black_box(is_sorted)
        })
    });
}

// ============================================================================
// Comparison Benchmarks
// ============================================================================

/// Direct comparison: generation speed
fn bench_comparison_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison/generation");

    group.bench_function("uuid", |b| b.iter(Uuid::new_v4));
    group.bench_function("sortable_id", |b| b.iter(SessionId::new));

    group.finish();
}

/// Direct comparison: sorting speed
fn bench_comparison_sorting(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison/sorting");

    let size = 10_000;

    group.bench_function("uuid", |b| {
        let mut ids: Vec<Uuid> = (0..size).map(|_| Uuid::new_v4()).collect();
        b.iter(|| {
            ids.sort();
            black_box(&ids);
        });
    });

    group.bench_function("sortable_id", |b| {
        let mut ids: Vec<SessionId> = (0..size).map(|_| SessionId::new()).collect();
        b.iter(|| {
            ids.sort();
            black_box(&ids);
        });
    });

    group.finish();
}

/// Direct comparison: parsing speed
fn bench_comparison_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison/parsing");

    let uuid_str = Uuid::new_v4().to_string();
    let sortable_id_str = SessionId::new().to_string();

    group.bench_function("uuid", |b| {
        b.iter(|| {
            let parsed = Uuid::parse_str(black_box(&uuid_str));
            black_box(parsed)
        })
    });

    group.bench_function("sortable_id", |b| {
        b.iter(|| {
            let parsed = SessionId::parse(black_box(&sortable_id_str));
            black_box(parsed)
        })
    });

    group.finish();
}

// ============================================================================
// Main Benchmark Groups
// ============================================================================

criterion_group!(
    generation_benches,
    bench_uuid_generation,
    bench_sortable_id_generation,
    bench_generation_throughput,
);

criterion_group!(memory_benches, bench_memory_size,);

criterion_group!(
    sorting_benches,
    bench_sorting_performance,
    bench_partial_sorting,
);

criterion_group!(
    parsing_benches,
    bench_parsing,
    bench_serialization,
    bench_roundtrip,
);

criterion_group!(
    hash_benches,
    bench_hashset_insertion,
    bench_hashset_lookup,
    bench_hashmap_operations,
);

criterion_group!(
    feature_benches,
    bench_timestamp_extraction,
    bench_prefix_validation,
    bench_time_ordering,
);

criterion_group!(
    comparison_benches,
    bench_comparison_generation,
    bench_comparison_sorting,
    bench_comparison_parsing,
);

criterion_main!(
    generation_benches,
    memory_benches,
    sorting_benches,
    parsing_benches,
    hash_benches,
    feature_benches,
    comparison_benches,
);
