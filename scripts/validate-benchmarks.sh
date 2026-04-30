#!/bin/bash
set -euo pipefail

BENCH_FILES=(
    "benches/id_performance.rs"
    "benches/event_bus_performance.rs"
    "benches/tool_dispatch_performance.rs"
    "benches/concurrent_runtime_benchmarks.rs"
)

BENCH_TARGETS=(
    "id_performance"
    "event_bus_performance"
    "tool_dispatch_performance"
    "concurrent_runtime_benchmarks"
)

echo "Validating benchmark configuration"

command -v cargo >/dev/null
echo "[pass] cargo is installed"

for file in "${BENCH_FILES[@]}"; do
    test -f "${file}"
    echo "[pass] ${file} exists"
done

for script in "scripts/bench.sh" "scripts/bench-compare.sh"; do
    test -x "${script}"
    echo "[pass] ${script} is executable"
done

for target in "${BENCH_TARGETS[@]}"; do
    if grep -rq "name = \"${target}\"" Cargo.toml crates/*/Cargo.toml 2>/dev/null; then
        echo "[pass] ${target} is registered in a Cargo.toml"
    else
        echo "[warn] ${target} not found in Cargo.toml (may use auto-discovery)"
    fi
done

echo
echo "Benchmark configuration is valid"
