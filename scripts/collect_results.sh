#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_ROOT="${ROOT_DIR}/reports/migration"
BUNDLE_ROOT="${REPORT_ROOT}/bundles"
mkdir -p "${BUNDLE_ROOT}"

cd "${ROOT_DIR}"

ts="$(date +"%Y%m%d_%H%M%S")"
bundle_dir="${BUNDLE_ROOT}/bundle_${ts}"
mkdir -p "${bundle_dir}"

copy_if_exists() {
    local path="$1"
    if [[ -e "${path}" ]]; then
        cp -R "${path}" "${bundle_dir}/"
    fi
}

copy_if_exists "MIGRATION_REPORT.md"
copy_if_exists "harness-progress.txt"
copy_if_exists "harness-tasks.json"
copy_if_exists "MONITORING_GUIDE.md"
copy_if_exists "TOOL_MONITORING_IMPLEMENTATION.md"
copy_if_exists "docs/phase1-migration.md"
copy_if_exists "reports/migration/status"
copy_if_exists "reports/migration/summaries"

git status --short > "${bundle_dir}/git_status.txt"
git diff --stat > "${bundle_dir}/git_diff_stat.txt"
git diff --name-status > "${bundle_dir}/git_name_status.txt"
git log --oneline --decorate -10 > "${bundle_dir}/recent_commits.txt"

tarball="${bundle_dir}.tar.gz"
tar -czf "${tarball}" -C "${BUNDLE_ROOT}" "$(basename "${bundle_dir}")"

cat <<EOF
# Result Collection Report

- Bundle directory: ${bundle_dir}
- Archive: ${tarball}

## Included Artifacts

- MIGRATION_REPORT.md
- harness-progress.txt
- harness-tasks.json
- MONITORING_GUIDE.md
- TOOL_MONITORING_IMPLEMENTATION.md
- docs/phase1-migration.md
- Generated status and summary directories under reports/migration/
- Git status, diff stat, name-status listing, and recent commits

## Suggested Use

1. Use the bundle directory for local inspection during active migration work.
2. Use the tarball for handoff, archival, or morning review after overnight monitoring.
3. Compare successive bundles to see whether blockers moved, shrank, or spread.
EOF
