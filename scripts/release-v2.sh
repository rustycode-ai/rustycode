#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

echo "=========================================="
echo "RustyCode v2 Release Verification"
echo "=========================================="
echo

echo "1. Format check"
cargo fmt --all --check
echo

echo "2. Lint check"
cargo clippy --workspace --exclude phase1-integration-tests --exclude rustycode-runtime --all-targets -- -D warnings
cargo clippy -p rustycode-runtime --lib -- -D warnings
echo

echo "3. Test suite"
cargo test --workspace --exclude phase1-integration-tests --exclude rustycode-runtime --lib --bins --locked
cargo check -p rustycode-runtime --locked
echo

echo "4. Provider migration verification"
./verify_migration.sh
echo

echo "5. Documentation verification"
./scripts/check-docs.sh
echo

echo "6. Benchmark manifest verification"
./scripts/validate-benchmarks.sh
echo

echo "7. Benchmark build verification"
for bench in \
    id_performance \
    event_bus_performance \
    tool_dispatch_performance \
    concurrent_runtime_benchmarks
do
    cargo bench --bench "${bench}" --no-run --locked
done
echo

if command -v cargo-audit >/dev/null 2>&1; then
    echo "8. Security audit"
    cargo audit
    echo
fi

echo "Release verification passed for v2 providers"
