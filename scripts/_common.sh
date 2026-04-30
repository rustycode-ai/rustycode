#!/usr/bin/env bash

if [[ -n "${RUSTYCODE_SCRIPT_COMMON_LOADED:-}" ]]; then
    return 0
fi
RUSTYCODE_SCRIPT_COMMON_LOADED=1

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_ROOT="${RUSTYCODE_MIGRATION_DIR:-${REPO_ROOT}/target/migration}"
LOG_ROOT="${TARGET_ROOT}/logs"
REPORT_ROOT="${TARGET_ROOT}/reports"
BACKUP_ROOT="${TARGET_ROOT}/backups"
STATE_ROOT="${TARGET_ROOT}/state"

mkdir -p "${LOG_ROOT}" "${REPORT_ROOT}" "${BACKUP_ROOT}" "${STATE_ROOT}"

if [[ -t 1 ]]; then
    COLOR_RED=$'\033[0;31m'
    COLOR_GREEN=$'\033[0;32m'
    COLOR_YELLOW=$'\033[1;33m'
    COLOR_BLUE=$'\033[0;34m'
    COLOR_RESET=$'\033[0m'
else
    COLOR_RED=""
    COLOR_GREEN=""
    COLOR_YELLOW=""
    COLOR_BLUE=""
    COLOR_RESET=""
fi

timestamp_utc() {
    date -u +"%Y%m%dT%H%M%SZ"
}

log_info() {
    printf "%s[INFO]%s %s\n" "${COLOR_BLUE}" "${COLOR_RESET}" "$*"
}

log_warn() {
    printf "%s[WARN]%s %s\n" "${COLOR_YELLOW}" "${COLOR_RESET}" "$*" >&2
}

log_error() {
    printf "%s[ERROR]%s %s\n" "${COLOR_RED}" "${COLOR_RESET}" "$*" >&2
}

log_success() {
    printf "%s[OK]%s %s\n" "${COLOR_GREEN}" "${COLOR_RESET}" "$*"
}

die() {
    log_error "$*"
    exit 1
}

require_repo_root() {
    [[ -f "${REPO_ROOT}/Cargo.toml" ]] || die "Expected Cargo.toml at ${REPO_ROOT}"
}

require_command() {
    local cmd="$1"
    command -v "${cmd}" >/dev/null 2>&1 || die "Required command not found: ${cmd}"
}

ensure_dir() {
    mkdir -p "$1"
}

latest_backup_dir() {
    find "${BACKUP_ROOT}" -mindepth 1 -maxdepth 1 -type d -print 2>/dev/null | sort | tail -n 1
}

copy_latest() {
    local source_file="$1"
    local latest_name="$2"
    cp "${source_file}" "${REPORT_ROOT}/${latest_name}"
}

run_and_capture() {
    local label="$1"
    local logfile="$2"
    shift 2

    log_info "${label}"
    if "$@" >"${logfile}" 2>&1; then
        log_success "${label}"
        return 0
    fi

    log_error "${label}"
    tail -n 40 "${logfile}" >&2 || true
    return 1
}

append_report_header() {
    local file="$1"
    local title="$2"
    cat >"${file}" <<EOF
# ${title}

- Timestamp: $(date -u +"%Y-%m-%d %H:%M:%SZ")
- Repository: ${REPO_ROOT}

EOF
}

append_report_section() {
    local file="$1"
    local title="$2"
    printf "\n## %s\n\n" "${title}" >>"${file}"
}

record_key_value() {
    local file="$1"
    local key="$2"
    local value="$3"
    printf "%s=%s\n" "${key}" "${value}" >>"${file}"
}

escape_path() {
    python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$1"
}
