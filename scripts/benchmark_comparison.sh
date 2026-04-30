#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/benchmark_comparison.sh [--baseline NAME] [--candidate NAME] [--capture-only]

Run Criterion benchmarks and optionally compare them to an existing baseline.

Options:
  --baseline NAME    Existing baseline to compare against. Default: main.
  --candidate NAME   Baseline name to save. Default: migration-current.
  --capture-only     Save the candidate baseline without comparison.
  --help             Show this message.
EOF
}

BASELINE="main"
CANDIDATE="migration-current"
CAPTURE_ONLY=0
BENCHES=(
    "id_performance"
    "event_bus_performance"
    "tool_dispatch_performance"
    "programmatic_tool_calling_bench"
)

while [[ $# -gt 0 ]]; do
    case "$1" in
        --baseline)
            shift
            [[ $# -gt 0 ]] || die "--baseline requires a value"
            BASELINE="$1"
            ;;
        --candidate)
            shift
            [[ $# -gt 0 ]] || die "--candidate requires a value"
            CANDIDATE="$1"
            ;;
        --capture-only)
            CAPTURE_ONLY=1
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

RUN_ID="$(timestamp_utc)"
REPORT_FILE="${REPORT_ROOT}/benchmark_comparison_${RUN_ID}.md"
RUN_DIR="${LOG_ROOT}/benchmark_comparison_${RUN_ID}"
ensure_dir "${RUN_DIR}"
append_report_header "${REPORT_FILE}" "Benchmark Comparison"

mapfile -t BENCHES < <(
    python3 - <<'PY' "${REPO_ROOT}/Cargo.toml"
import re
import sys

text = open(sys.argv[1], "r", encoding="utf-8").read()
for name in re.findall(r'\[\[bench\]\]\s*name\s*=\s*"([^"]+)"', text, re.MULTILINE):
    print(name)
PY
)

baseline_exists() {
    find "${REPO_ROOT}/target/criterion" -type d -name "$1" -print -quit 2>/dev/null | grep -q .
}

append_report_section "${REPORT_FILE}" "Configuration"
printf -- "- Candidate baseline: %s\n- Requested comparison baseline: %s\n" "${CANDIDATE}" "${BASELINE}" >>"${REPORT_FILE}"

COMPARE=0
if [[ "${CAPTURE_ONLY}" -eq 0 ]] && baseline_exists "${BASELINE}"; then
    COMPARE=1
    printf -- "- Mode: compare and save\n" >>"${REPORT_FILE}"
else
    printf -- "- Mode: capture only\n" >>"${REPORT_FILE}"
fi

FAILURES=0
append_report_section "${REPORT_FILE}" "Benchmarks"
for bench in "${BENCHES[@]}"; do
    logfile="${RUN_DIR}/${bench}.log"
    args=(cargo bench --bench "${bench}" -- --save-baseline "${CANDIDATE}")
    if [[ "${COMPARE}" -eq 1 ]]; then
        args=(cargo bench --bench "${bench}" -- --baseline "${BASELINE}" --save-baseline "${CANDIDATE}")
    fi

    if run_and_capture "Benchmark ${bench}" "${logfile}" "${args[@]}"; then
        printf -- "- PASS: %s\n" "${bench}" >>"${REPORT_FILE}"
    else
        printf -- "- FAIL: %s\n" "${bench}" >>"${REPORT_FILE}"
        FAILURES=$((FAILURES + 1))
    fi
done

append_report_section "${REPORT_FILE}" "Artifacts"
printf -- "- Criterion output: %s\n" "${REPO_ROOT}/target/criterion" >>"${REPORT_FILE}"

copy_latest "${REPORT_FILE}" "benchmark_comparison_latest.md"

if [[ "${FAILURES}" -gt 0 ]]; then
    die "Benchmark comparison failed. Report: ${REPORT_FILE}"
fi

log_success "Benchmark run completed. Report: ${REPORT_FILE}"
