#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/generate_report.sh [--refresh] [--output PATH]

Generate a consolidated migration status report from the latest automation artifacts.

Options:
  --refresh       Re-run quick validation before generating the report.
  --output PATH   Write the report to PATH.
  --help          Show this message.
EOF
}

REFRESH=0
OUTPUT=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --refresh) REFRESH=1 ;;
        --output)
            shift
            [[ $# -gt 0 ]] || die "--output requires a path"
            OUTPUT="$1"
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

if [[ "${REFRESH}" -eq 1 ]]; then
    "${SCRIPT_DIR}/validate_configs.sh"
    "${SCRIPT_DIR}/verify_migration.sh" --quick
fi

RUN_ID="$(timestamp_utc)"
REPORT_FILE="${OUTPUT:-${REPORT_ROOT}/migration_report_${RUN_ID}.md}"
append_report_header "${REPORT_FILE}" "Migration Completion Report"

append_report_section "${REPORT_FILE}" "Git Snapshot"
printf -- "- Branch: %s\n" "$(git -C "${REPO_ROOT}" rev-parse --abbrev-ref HEAD 2>/dev/null || printf 'unknown')" >>"${REPORT_FILE}"
printf -- "- Commit: %s\n" "$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || printf 'unknown')" >>"${REPORT_FILE}"
printf "\n\`\`\`\n" >>"${REPORT_FILE}"
git -C "${REPO_ROOT}" status --short >>"${REPORT_FILE}" 2>/dev/null || true
printf "\n\`\`\`\n" >>"${REPORT_FILE}"

append_report_section "${REPORT_FILE}" "Latest Artifacts"
for artifact in \
    "validate_configs_latest.md" \
    "verify_migration_latest.md" \
    "run_all_tests_latest.md" \
    "test_coverage_latest.md" \
    "benchmark_comparison_latest.md" \
    "cleanup_legacy_latest.md"
do
    path="${REPORT_ROOT}/${artifact}"
    if [[ -f "${path}" ]]; then
        printf -- "- %s\n" "${path}" >>"${REPORT_FILE}"
    else
        printf -- "- missing: %s\n" "${path}" >>"${REPORT_FILE}"
    fi
done

append_report_section "${REPORT_FILE}" "Recommendations"
printf -- "- Run ./scripts/run_all_tests.sh before final rollout.\n" >>"${REPORT_FILE}"
printf -- "- Use ./scripts/cleanup_legacy.sh first in dry-run mode, then with --apply.\n" >>"${REPORT_FILE}"
printf -- "- Restore via ./scripts/rollback.sh if cleanup removes something prematurely.\n" >>"${REPORT_FILE}"

copy_latest "${REPORT_FILE}" "migration_report_latest.md"
log_success "Migration report generated: ${REPORT_FILE}"
