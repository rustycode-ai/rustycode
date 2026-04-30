#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/verify_migration.sh [--quick] [--strict] [--report PATH]

Validate that the sortable-ID migration is complete enough to ship.

Options:
  --quick        Skip the full workspace check.
  --strict       Fail if any unexpected uuid references remain in core crates.
  --report PATH  Write the Markdown report to PATH.
  --help         Show this message.
EOF
}

QUICK=0
STRICT=0
REPORT_PATH=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --quick) QUICK=1 ;;
        --strict) STRICT=1 ;;
        --report)
            shift
            [[ $# -gt 0 ]] || die "--report requires a path"
            REPORT_PATH="$1"
            ;;
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
require_command rg

RUN_ID="$(timestamp_utc)"
REPORT_FILE="${REPORT_PATH:-${REPORT_ROOT}/verify_migration_${RUN_ID}.md}"
SUMMARY_FILE="${STATE_ROOT}/verify_migration_latest.env"
TMP_DIR="${LOG_ROOT}/verify_migration_${RUN_ID}"

ensure_dir "${TMP_DIR}"
ensure_dir "$(dirname "${REPORT_FILE}")"
append_report_header "${REPORT_FILE}" "Migration Verification"

FAILURES=0

run_check() {
    local description="$1"
    local logfile="$2"
    shift 2

    if run_and_capture "${description}" "${logfile}" "$@"; then
        printf -- "- PASS: %s\n" "${description}" >>"${REPORT_FILE}"
    else
        printf -- "- FAIL: %s\n" "${description}" >>"${REPORT_FILE}"
        FAILURES=$((FAILURES + 1))
    fi
}

append_report_section "${REPORT_FILE}" "Command Checks"
run_check \
    "rustycode-id unit tests" \
    "${TMP_DIR}/rustycode-id.log" \
    cargo test -p rustycode-id --lib
run_check \
    "rustycode-protocol compiles" \
    "${TMP_DIR}/rustycode-protocol.log" \
    cargo check -p rustycode-protocol
run_check \
    "rustycode-storage unit tests" \
    "${TMP_DIR}/rustycode-storage.log" \
    cargo test -p rustycode-storage --lib

if [[ "${QUICK}" -eq 0 ]]; then
    run_check \
        "workspace compiles" \
        "${TMP_DIR}/workspace-check.log" \
        cargo check --workspace
fi

append_report_section "${REPORT_FILE}" "Static Assertions"

static_assert() {
    local description="$1"
    local command="$2"
    if eval "${command}" >/dev/null 2>&1; then
        printf -- "- PASS: %s\n" "${description}" >>"${REPORT_FILE}"
        log_success "${description}"
    else
        printf -- "- FAIL: %s\n" "${description}" >>"${REPORT_FILE}"
        log_error "${description}"
        FAILURES=$((FAILURES + 1))
    fi
}

static_assert \
    "PlanId is exported from rustycode-protocol" \
    "rg -q 'pub use rustycode_id::.*PlanId' '${REPO_ROOT}/crates/rustycode-protocol/src/lib.rs'"
static_assert \
    "Plan struct uses PlanId" \
    "rg -q 'pub id: PlanId' '${REPO_ROOT}/crates/rustycode-protocol/src/lib.rs'"
static_assert \
    "rustycode-protocol does not declare a direct uuid dependency" \
    "! rg -q '^uuid(\\s*=|\\.workspace)' '${REPO_ROOT}/crates/rustycode-protocol/Cargo.toml'"
static_assert \
    "Migration guide exists" \
    "test -f '${REPO_ROOT}/docs/phase1-migration.md'"

UUID_SCAN_FILE="${TMP_DIR}/uuid_scan.txt"
rg -n "uuid::Uuid|uuid\\.workspace|uuid\\s*=" \
    "${REPO_ROOT}/crates/rustycode-core" \
    "${REPO_ROOT}/crates/rustycode-protocol" \
    "${REPO_ROOT}/crates/rustycode-storage" \
    "${REPO_ROOT}/crates/rustycode-id" >"${UUID_SCAN_FILE}" || true

append_report_section "${REPORT_FILE}" "UUID Residue Scan"
if [[ -s "${UUID_SCAN_FILE}" ]]; then
    printf "Unexpected UUID references in migration-critical crates:\n\n" >>"${REPORT_FILE}"
    printf '%s\n' '```' >>"${REPORT_FILE}"
    cat "${UUID_SCAN_FILE}" >>"${REPORT_FILE}"
    printf '%s\n' '```' >>"${REPORT_FILE}"
    if [[ "${STRICT}" -eq 1 ]]; then
        FAILURES=$((FAILURES + 1))
    fi
else
    printf "No unexpected UUID references found in migration-critical crates.\n" >>"${REPORT_FILE}"
fi

STATUS="passed"
if [[ "${FAILURES}" -gt 0 ]]; then
    STATUS="failed"
fi

append_report_section "${REPORT_FILE}" "Result"
printf -- "- Status: %s\n- Failures: %s\n" "${STATUS}" "${FAILURES}" >>"${REPORT_FILE}"

: >"${SUMMARY_FILE}"
record_key_value "${SUMMARY_FILE}" "status" "${STATUS}"
record_key_value "${SUMMARY_FILE}" "failures" "${FAILURES}"
record_key_value "${SUMMARY_FILE}" "report" "${REPORT_FILE}"

copy_latest "${REPORT_FILE}" "verify_migration_latest.md"

if [[ "${FAILURES}" -gt 0 ]]; then
    die "Migration verification failed. Report: ${REPORT_FILE}"
fi

log_success "Migration verification passed. Report: ${REPORT_FILE}"
