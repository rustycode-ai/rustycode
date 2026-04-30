# Performance Baselines

## Current Benchmarks

Run with: `./scripts/bench.sh`

### ID System

- **Generation**: 2.5ms per 1000 IDs
- **Parsing**: 1.8ms per 1000 IDs
- **Sorting**: O(n) due to time-ordered design

### Event Bus

- **Publish**: ~500ns per event
- **Subscribe**: ~200ns per subscription
- **Throughput**: 1000+ events/second
- **Wildcard matching**: ~800ns

### Tool Dispatch

- **Compile-time**: ~5ns per call (inlined)
- **Runtime**: ~50ns per call (trait object)
- **Speedup**: 5-10x

## Targets

| Metric | Current | Target | Status |
|--------|---------|--------|--------|
| ID size | 26 chars | <30 chars | ✅ |
| ID gen rate | 400K/sec | >100K/sec | ✅ |
| Event throughput | 1000+/sec | >500/sec | ✅ |
| Tool dispatch | 5-10ns | <50ns | ✅ |

## Regression Detection

Benchmarks should not regress by more than 5%.

To check:
```bash
./scripts/bench-compare.sh new main
```

If regression >5%, investigate before merging.
