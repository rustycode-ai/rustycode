#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/run_all_tests.sh [--fast] [--with-coverage]

Run the migration-oriented local quality gate.

Options:
  --fast            Skip bench build and full workspace migration verification.
  --with-coverage   Run scripts/test_coverage.sh after tests.
  --help            Show this message.
EOF
}

FAST=0
WITH_COVERAGE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --fast) FAST=1 ;;
        --with-coverage) WITH_COVERAGE=1 ;;
        --help)
            usage
            exit 0
            ;;
        *)
            die "Unknown option: $1"
            ;;
    esac
    shift
done

require_repo_root
require_command cargo

RUN_ID="$(timestamp_utc)"
REPORT_FILE="${REPORT_ROOT}/run_all_tests_${RUN_ID}.md"
RUN_DIR="${LOG_ROOT}/run_all_tests_${RUN_ID}"
ensure_dir "${RUN_DIR}"
append_report_header "${REPORT_FILE}" "Comprehensive Test Run"

FAILURES=0

run_step() {
    local description="$1"
    shift
    local logfile="${RUN_DIR}/$(printf "%s" "${description}" | tr ' /' '__').log"
    if run_and_capture "${description}" "${logfile}" "$@"; then
        printf -- "- PASS: %s\n" "${description}" >>"${REPORT_FILE}"
    else
        printf -- "- FAIL: %s\n" "${description}" >>"${REPORT_FILE}"
        FAILURES=$((FAILURES + 1))
    fi
}

append_report_section "${REPORT_FILE}" "Quality Gate"
run_step "validate configs" "${SCRIPT_DIR}/validate_configs.sh"
run_step "cargo fmt --check" cargo fmt -- --check
run_step "cargo clippy --workspace --all-targets -- -D warnings" cargo clippy --workspace --all-targets -- -D warnings
run_step "cargo test --workspace" cargo test --workspace
run_step "cargo test --doc --workspace" cargo test --doc --workspace

if [[ "${FAST}" -eq 0 ]]; then
    run_step "cargo bench --no-run" cargo bench --no-run
    run_step "verify migration" "${SCRIPT_DIR}/verify_migration.sh"
else
    run_step "verify migration (quick)" "${SCRIPT_DIR}/verify_migration.sh" --quick
fi

if [[ "${WITH_COVERAGE}" -eq 1 ]]; then
    run_step "test coverage" "${SCRIPT_DIR}/test_coverage.sh"
fi

copy_latest "${REPORT_FILE}" "run_all_tests_latest.md"

if [[ "${FAILURES}" -gt 0 ]]; then
    die "One or more test steps failed. Report: ${REPORT_FILE}"
fi

log_success "All test steps passed. Report: ${REPORT_FILE}"
