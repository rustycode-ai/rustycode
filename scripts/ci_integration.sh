#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/ci_integration.sh [--mode local|github] [--with-coverage]

Run the migration verification stack in a CI-friendly way.

Options:
  --mode MODE       local or github. Default: local.
  --with-coverage   Include coverage collection.
  --help            Show this message.
EOF
}

MODE="local"
WITH_COVERAGE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --mode)
            shift
            [[ $# -gt 0 ]] || die "--mode requires a value"
            MODE="$1"
            ;;
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

ARGS=("--fast")
if [[ "${WITH_COVERAGE}" -eq 1 ]]; then
    ARGS+=("--with-coverage")
fi

"${SCRIPT_DIR}/validate_configs.sh"
"${SCRIPT_DIR}/run_all_tests.sh" "${ARGS[@]}"
"${SCRIPT_DIR}/generate_report.sh"

LATEST_REPORT="${REPORT_ROOT}/migration_report_latest.md"
if [[ "${MODE}" == "github" && -n "${GITHUB_STEP_SUMMARY:-}" && -f "${LATEST_REPORT}" ]]; then
    cat "${LATEST_REPORT}" >>"${GITHUB_STEP_SUMMARY}"
fi

log_success "CI integration flow completed."
