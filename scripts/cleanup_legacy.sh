#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/cleanup_legacy.sh [--apply] [--category NAME] [--path PATH]...

Dry-run by default. On --apply, files are backed up to target/migration/backups/
before removal and a manifest is written for rollback.sh.

Categories:
  backup-files
  generated-artifacts
  superseded-scripts
  all

Options:
  --apply            Perform deletion instead of previewing it.
  --category NAME    Limit cleanup to a built-in category. Repeatable.
  --path PATH        Add an explicit path to clean up. Repeatable.
  --help             Show this message.
EOF
}

APPLY=0
declare -a CATEGORIES=()
declare -a EXTRA_PATHS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply) APPLY=1 ;;
        --category)
            shift
            [[ $# -gt 0 ]] || die "--category requires a value"
            CATEGORIES+=("$1")
            ;;
        --path)
            shift
            [[ $# -gt 0 ]] || die "--path requires a value"
            EXTRA_PATHS+=("$1")
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
require_command python3

if [[ ${#CATEGORIES[@]} -eq 0 ]]; then
    CATEGORIES=("all")
fi

declare -a CANDIDATES=()

add_candidate() {
    local path="$1"
    local existing
    for existing in "${CANDIDATES[@]:-}"; do
        [[ "${existing}" == "${path}" ]] && return 0
    done
    CANDIDATES+=("$path")
}

expand_category() {
    local category="$1"
    case "${category}" in
        backup-files)
            add_candidate "${REPO_ROOT}/crates/rustycode-core/src/lib.rs.bak"
            add_candidate "${REPO_ROOT}/crates/rustycode-runtime/src/lib.rs.bak"
            add_candidate "${REPO_ROOT}/crates/rustycode-tui/src/render.rs.bak"
            add_candidate "${REPO_ROOT}/crates/rustycode-tui/src/ui_components.rs.bak"
            add_candidate "${REPO_ROOT}/crates/rustycode-web/src/main.rs.backup"
            add_candidate "${REPO_ROOT}/harness-tasks.json.bak"
            ;;
        generated-artifacts)
            add_candidate "${REPO_ROOT}/full_test.log"
            add_candidate "${REPO_ROOT}/test_output.log"
            add_candidate "${REPO_ROOT}/test_run_output.log"
            add_candidate "${REPO_ROOT}/test_conversation_format.txt"
            add_candidate "${REPO_ROOT}/libprompt.rlib"
            add_candidate "${REPO_ROOT}/libsearch.rlib"
            ;;
        superseded-scripts)
            add_candidate "${REPO_ROOT}/verify_migration.sh"
            ;;
        all)
            expand_category "backup-files"
            expand_category "generated-artifacts"
            expand_category "superseded-scripts"
            ;;
        *)
            die "Unknown category: ${category}"
            ;;
    esac
}

for category in "${CATEGORIES[@]}"; do
    expand_category "${category}"
done

if [[ ${#EXTRA_PATHS[@]} -gt 0 ]]; then
    for path in "${EXTRA_PATHS[@]}"; do
        add_candidate "$(escape_path "${path}")"
    done
fi

RUN_ID="$(timestamp_utc)"
REPORT_FILE="${REPORT_ROOT}/cleanup_legacy_${RUN_ID}.md"
BACKUP_DIR="${BACKUP_ROOT}/${RUN_ID}"
MANIFEST_FILE="${BACKUP_DIR}/manifest.tsv"

append_report_header "${REPORT_FILE}" "Legacy Cleanup"
append_report_section "${REPORT_FILE}" "Candidates"

FOUND=0
for path in "${CANDIDATES[@]}"; do
    if [[ -e "${path}" ]]; then
        printf -- "- %s\n" "${path}" >>"${REPORT_FILE}"
        FOUND=$((FOUND + 1))
    fi
done

if [[ "${FOUND}" -eq 0 ]]; then
    printf "No matching legacy files were found.\n" >>"${REPORT_FILE}"
    copy_latest "${REPORT_FILE}" "cleanup_legacy_latest.md"
    log_success "No legacy files to clean up."
    exit 0
fi

if [[ "${APPLY}" -eq 0 ]]; then
    append_report_section "${REPORT_FILE}" "Mode"
    printf "Dry-run only. Re-run with \`--apply\` to back up and remove the files listed above.\n" >>"${REPORT_FILE}"
    copy_latest "${REPORT_FILE}" "cleanup_legacy_latest.md"
    log_warn "Dry-run complete. Report: ${REPORT_FILE}"
    exit 0
fi

ensure_dir "${BACKUP_DIR}"
: >"${MANIFEST_FILE}"
append_report_section "${REPORT_FILE}" "Actions"

for path in "${CANDIDATES[@]}"; do
    [[ -e "${path}" ]] || continue
    backup_path="${BACKUP_DIR}${path#${REPO_ROOT}}"
    ensure_dir "$(dirname "${backup_path}")"
    cp -a "${path}" "${backup_path}"
    rm -rf "${path}"
    printf "%s\t%s\n" "${path}" "${backup_path}" >>"${MANIFEST_FILE}"
    printf -- "- Removed %s\n" "${path}" >>"${REPORT_FILE}"
done

append_report_section "${REPORT_FILE}" "Rollback"
printf "Use \`./scripts/rollback.sh --backup-dir %s --apply\` to restore removed files.\n" "${BACKUP_DIR}" >>"${REPORT_FILE}"

copy_latest "${REPORT_FILE}" "cleanup_legacy_latest.md"
log_success "Cleanup completed. Report: ${REPORT_FILE}"
