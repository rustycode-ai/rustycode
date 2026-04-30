#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/test_coverage.sh [--engine auto|tarpaulin|llvm-cov] [--threshold PERCENT]

Generate workspace test coverage with the first available engine.

Options:
  --engine NAME      Coverage engine to use. Default: auto.
  --threshold NUM    Fail if measured coverage is below NUM.
  --output-dir PATH  Coverage output directory. Default: target/migration/coverage.
  --help             Show this message.
EOF
}

ENGINE="auto"
THRESHOLD=""
OUTPUT_DIR="${TARGET_ROOT}/coverage"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --engine)
            shift
            [[ $# -gt 0 ]] || die "--engine requires a value"
            ENGINE="$1"
            ;;
        --threshold)
            shift
            [[ $# -gt 0 ]] || die "--threshold requires a number"
            THRESHOLD="$1"
            ;;
        --output-dir)
            shift
            [[ $# -gt 0 ]] || die "--output-dir requires a path"
            OUTPUT_DIR="$1"
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
require_command python3

select_engine() {
    case "${ENGINE}" in
        auto)
            if command -v cargo-tarpaulin >/dev/null 2>&1; then
                printf "tarpaulin"
            elif command -v cargo-llvm-cov >/dev/null 2>&1; then
                printf "llvm-cov"
            else
                die "No coverage engine found. Install cargo-tarpaulin or cargo-llvm-cov."
            fi
            ;;
        tarpaulin|llvm-cov)
            printf "%s" "${ENGINE}"
            ;;
        *)
            die "Unsupported engine: ${ENGINE}"
            ;;
    esac
}

SELECTED_ENGINE="$(select_engine)"
RUN_ID="$(timestamp_utc)"
RUN_DIR="${OUTPUT_DIR}/${RUN_ID}"
LOG_FILE="${LOG_ROOT}/test_coverage_${RUN_ID}.log"
REPORT_FILE="${REPORT_ROOT}/test_coverage_${RUN_ID}.md"

ensure_dir "${RUN_DIR}"
append_report_header "${REPORT_FILE}" "Test Coverage"

append_report_section "${REPORT_FILE}" "Run"
printf -- "- Engine: %s\n- Output directory: %s\n" "${SELECTED_ENGINE}" "${RUN_DIR}" >>"${REPORT_FILE}"

if [[ "${SELECTED_ENGINE}" == "tarpaulin" ]]; then
    run_and_capture \
        "Generating coverage with cargo-tarpaulin" \
        "${LOG_FILE}" \
        cargo tarpaulin --workspace --out Xml --output-dir "${RUN_DIR}" -- --test-threads=1
else
    run_and_capture \
        "Generating coverage with cargo-llvm-cov" \
        "${LOG_FILE}" \
        cargo llvm-cov --workspace --lcov --output-path "${RUN_DIR}/lcov.info"
fi

COVERAGE_VALUE="$(
    python3 - <<'PY' "${LOG_FILE}"
import re
import sys

text = open(sys.argv[1], "r", encoding="utf-8", errors="replace").read()
patterns = [
    r"([\d.]+)% coverage",
    r"TOTAL\s+([\d.]+)%",
    r"lines:\s+([\d.]+)%",
]
for pattern in patterns:
    match = re.search(pattern, text, re.IGNORECASE)
    if match:
        print(match.group(1))
        raise SystemExit(0)
print("")
PY
)"

append_report_section "${REPORT_FILE}" "Summary"
if [[ -n "${COVERAGE_VALUE}" ]]; then
    printf -- "- Coverage: %s%%\n" "${COVERAGE_VALUE}" >>"${REPORT_FILE}"
else
    printf -- "- Coverage: unavailable in tool output; inspect %s\n" "${LOG_FILE}" >>"${REPORT_FILE}"
fi

if [[ -n "${THRESHOLD}" && -n "${COVERAGE_VALUE}" ]]; then
    if python3 - <<'PY' "${COVERAGE_VALUE}" "${THRESHOLD}"
import sys
actual = float(sys.argv[1])
threshold = float(sys.argv[2])
raise SystemExit(0 if actual >= threshold else 1)
PY
    then
        printf -- "- Threshold: met (%s%%)\n" "${THRESHOLD}" >>"${REPORT_FILE}"
    else
        printf -- "- Threshold: failed (%s%% required)\n" "${THRESHOLD}" >>"${REPORT_FILE}"
        copy_latest "${REPORT_FILE}" "test_coverage_latest.md"
        die "Coverage ${COVERAGE_VALUE}% is below threshold ${THRESHOLD}%"
    fi
fi

copy_latest "${REPORT_FILE}" "test_coverage_latest.md"
log_success "Coverage run completed. Report: ${REPORT_FILE}"
