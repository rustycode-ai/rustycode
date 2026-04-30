#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/rollback.sh [--backup-dir PATH] [--apply] [--force]

Restore files removed by cleanup_legacy.sh using the generated manifest.

Options:
  --backup-dir PATH  Backup directory to restore from. Default: latest backup.
  --apply            Perform the restore. Dry-run by default.
  --force            Overwrite files that currently exist.
  --help             Show this message.
EOF
}

BACKUP_DIR=""
APPLY=0
FORCE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --backup-dir)
            shift
            [[ $# -gt 0 ]] || die "--backup-dir requires a path"
            BACKUP_DIR="$1"
            ;;
        --apply) APPLY=1 ;;
        --force) FORCE=1 ;;
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

if [[ -z "${BACKUP_DIR}" ]]; then
    BACKUP_DIR="$(latest_backup_dir)"
fi

[[ -n "${BACKUP_DIR}" ]] || die "No backup directory found."
MANIFEST_FILE="${BACKUP_DIR}/manifest.tsv"
[[ -f "${MANIFEST_FILE}" ]] || die "Backup manifest not found: ${MANIFEST_FILE}"

RUN_ID="$(timestamp_utc)"
REPORT_FILE="${REPORT_ROOT}/rollback_${RUN_ID}.md"
append_report_header "${REPORT_FILE}" "Rollback"
append_report_section "${REPORT_FILE}" "Manifest"
printf -- "- Backup directory: %s\n" "${BACKUP_DIR}" >>"${REPORT_FILE}"

if [[ "${APPLY}" -eq 0 ]]; then
    printf "Dry-run only. The following files are restorable:\n\n" >>"${REPORT_FILE}"
    while IFS=$'\t' read -r original backup; do
        printf -- "- %s <= %s\n" "${original}" "${backup}" >>"${REPORT_FILE}"
    done <"${MANIFEST_FILE}"
    copy_latest "${REPORT_FILE}" "rollback_latest.md"
    log_warn "Dry-run complete. Re-run with --apply to restore files."
    exit 0
fi

append_report_section "${REPORT_FILE}" "Actions"
while IFS=$'\t' read -r original backup; do
    if [[ ! -e "${backup}" ]]; then
        printf -- "- Skipped missing backup %s\n" "${backup}" >>"${REPORT_FILE}"
        continue
    fi
    if [[ -e "${original}" && "${FORCE}" -ne 1 ]]; then
        printf -- "- Skipped existing file %s\n" "${original}" >>"${REPORT_FILE}"
        continue
    fi
    ensure_dir "$(dirname "${original}")"
    cp -a "${backup}" "${original}"
    printf -- "- Restored %s\n" "${original}" >>"${REPORT_FILE}"
done <"${MANIFEST_FILE}"

copy_latest "${REPORT_FILE}" "rollback_latest.md"
log_success "Rollback completed. Report: ${REPORT_FILE}"
