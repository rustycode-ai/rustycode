# Benchmarking Guide

## Quick Start

```bash
# Run all benchmarks and save baseline
./scripts/bench.sh

# Compare with previous baseline
./scripts/bench-compare.sh new main

# Validate benchmark setup
./scripts/validate-benchmarks.sh
```

## Understanding Criterion Output

Criterion generates HTML reports in `target/criterion/`:

- **Index**: `target/criterion/report/index.html`
- Per-benchmark reports in individual subdirectories

### Key Metrics

- **Throughput**: Operations per second
- **Time**: Average execution time
- **Std Dev**: Variability in measurements
- **Median**: 50th percentile
- **Mean**: Average performance

## Benchmark Suites

### ID System Benchmarks (`id_benchmarks.rs`)

Tests the SortableID system vs UUID:

- `uuid_generation` vs `sortable_id_generation`
- `id_sorting` - O(n) for SortableID vs O(n log n) for UUID
- `id_parsing` - String parsing performance
- `id_serialization` - String conversion
- `base62` - Encoding/decoding performance

**Expected Results:**
- ID generation: < 3ns per ID
- Sorting (10K items): < 1ms for SortableID
- Base62 encode/decode: < 100ns

### Event Bus Benchmarks (`event_bus_benchmarks.rs`)

Tests event bus throughput and latency:

- `publish` - Single event publishing
- `subscribe` - Subscription creation
- `wildcard_matching` - Pattern matching performance
- `event_throughput` - End-to-end throughput

**Expected Results:**
- Publish: < 1000ns
- Subscribe: < 500ns
- Throughput: > 1000 events/sec

### Tool Dispatch Benchmarks (`tool_benchmarks.rs`)

Tests tool invocation performance:

- `compile_time_dispatch` - Zero-cost abstractions
- `runtime_dispatch` - Trait object overhead
- `tool_resolution` - Tool lookup performance

**Expected Results:**
- Compile-time: < 10ns (inlined)
- Runtime: < 100ns
- Speedup: 5-10x for compile-time

## Regression Detection

### Automated Comparison

```bash
# Compare new changes against main baseline
./scripts/bench-compare.sh feature-branch main
```

### Thresholds

- **5%**: Warning threshold
- **10%**: Critical threshold - investigate before merging

### CI Integration

Benchmarks run automatically:
- On every push to `main`
- Daily at 00:00 UTC
- Manual trigger via GitHub Actions

## Performance Optimization Workflow

1. **Baseline**: `./scripts/bench.sh`
2. **Optimize**: Make changes
3. **Compare**: `./scripts/bench-compare.sh optimized main`
4. **Validate**: Review Criterion HTML reports
5. **Commit**: Include performance notes in commit message

## Interpreting Results

### Good Signs

✅ Consistent times across runs
✅ Low standard deviation (< 10% of mean)
✅ Monotonic improvements with optimizations

### Warning Signs

⚠️ High variability (> 20% std dev)
⚠️ Regression > 5%
⚠️ Inconsistent results across runs

### Debugging Tips

1. **Warm up**: First run may be slower (CPU caching)
2. **Consistency**: Run multiple times to establish baseline
3. **Environment**: Close other apps for consistent results
4. **Power**: Ensure laptop is charging for consistent CPU

## Custom Benchmarks

To add a new benchmark:

1. Create file in `benches/`:
   ```rust
   use criterion::{black_box, criterion_group, criterion_main, Criterion};

   fn bench_my_feature(c: &mut Criterion) {
       c.bench_function("my_feature", |b| {
           b.iter(|| {
               // Your code here
               black_box(result)
           })
       });
   }

   criterion_group!(benches, bench_my_feature);
   criterion_main!(benches);
   ```

2. Add to `Cargo.toml`:
   ```toml
   [[bench]]
   name = "my_benchmarks"
   harness = false
   ```

3. Add to benchmark scripts

## Resources

- [Criterion.rs Documentation](https://bheisler.github.io/criterion.rs/book/)
- Rust Performance Book: https://nnethercote.github.io/perf-book/
- Internal: `/Users/nat/dev/rustycode/docs/performance-baselines.md`
