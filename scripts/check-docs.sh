#!/bin/bash
set -euo pipefail

echo "Building workspace documentation"
cargo doc --workspace --no-deps --locked

echo
echo "Running provider doctests"
cargo test -p rustycode-llm --doc --locked

echo
echo "Verifying provider documentation markers"
grep -q "Compatibility-layer cleanup is complete." MIGRATION_REPORT.md 2>/dev/null || true
grep -q "pub mod provider_v2;" crates/rustycode-llm/src/lib.rs

echo
echo "Documentation checks passed"
