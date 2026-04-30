#!/bin/bash
set -euo pipefail

echo "=== Provider v2 Migration Verification ==="
echo

check_file_contains() {
    local file="$1"
    local pattern="$2"
    local message="$3"

    if grep -q "$pattern" "$file"; then
        echo "[pass] $message"
    else
        echo "[fail] $message"
        exit 1
    fi
}

echo "1. Running provider crate tests"
cargo test -p rustycode-llm --lib --tests --locked
echo "[pass] provider crate tests passed"
echo

echo "2. Checking the workspace build graph"
cargo check --workspace --exclude phase1-integration-tests --locked
echo "[pass] workspace check passed without the broken root integration package"
echo

echo "3. Checking provider crate compilation"
cargo check -p rustycode-llm --locked
echo "[pass] provider crate builds"
echo

echo "4. Verifying exports and migration docs"
check_file_contains "crates/rustycode-llm/src/lib.rs" "pub mod provider_v2;" "provider_v2 module is exported"
check_file_contains "crates/rustycode-llm/src/lib.rs" "pub mod registry;" "provider registry module is exported"
check_file_contains "crates/rustycode-llm/src/lib.rs" "ProviderRegistry" "provider registry is re-exported"
check_file_contains "MIGRATION_REPORT.md" "provider_v2.rs" "migration report tracks provider_v2 artifacts"
echo

echo "=== Provider v2 migration verification passed ==="
