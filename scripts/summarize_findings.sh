#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATUS_DIR="${ROOT_DIR}/reports/migration/status"

mkdir -p "${STATUS_DIR}"
cd "${ROOT_DIR}"

today="$(date +"%Y%m%d")"
report_count="$(find "${STATUS_DIR}" -type f -name "status_${today}_*.md" 2>/dev/null | wc -l | tr -d ' ')"
latest_report="$(find "${STATUS_DIR}" -type f -name "status_*.md" 2>/dev/null | sort | tail -1)"
latest_build=""
latest_test=""

if [[ -n "${latest_report}" ]]; then
    latest_build="$(grep -m1 "cargo build --workspace" "${latest_report}" 2>/dev/null | sed 's/^[[:space:]]*//')"
    latest_test="$(grep -m1 "cargo test --workspace --no-run" "${latest_report}" 2>/dev/null | sed 's/^[[:space:]]*//')"
fi

active_blockers="$(
python3 - <<'PY'
import json, pathlib
path = pathlib.Path("harness-tasks.json")
if not path.exists():
    print("No harness task file found")
    raise SystemExit(0)
data = json.loads(path.read_text())
tasks = data.get("tasks", [])
pending = [t for t in tasks if t.get("status") != "completed"]
for task in pending[:8]:
    print(f"- {task['id']} [{task['priority']}] {task['status']}: {task['title']}")
PY
)"

cat <<EOF
# Daily Migration Findings Summary

- Date: $(date +"%Y-%m-%d")
- Status reports generated today: ${report_count}
- Latest status report: ${latest_report:-none}

## What Moved

- Monitoring is operational through the migration reporting scripts in scripts/.
- The active migration remains centered on provider-v2 adoption and protocol surface updates.
- The worktree still shows broad in-flight edits across LLM, TUI, runtime, tools, and supporting docs.

## What Is Blocking Completion

- Workspace build verification is currently not green.
- Protocol changes introduced new required fields that are not yet populated everywhere.
- Example compilation in rustycode-containers still fails, which prevents a clean workspace compile/test-compile pass.

## Latest Verification Snapshot

- ${latest_build:-No build status captured yet}
- ${latest_test:-No test-compilation status captured yet}

## Open Migration Tasks

${active_blockers}

## Next Morning Review Checklist

1. Read the latest generated status report and compare it to the prior one for movement on blockers.
2. Confirm whether rustycode-core protocol initializer fixes landed overnight.
3. Confirm whether rustycode-containers example type/export issues were resolved or deliberately excluded.
4. Decide whether to narrow workspace verification scope temporarily or keep full-workspace enforcement.
5. Refresh MIGRATION_REPORT.md if blockers or completed areas changed materially.
EOF
