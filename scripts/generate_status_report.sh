#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

RUN_CHECKS=0
if [[ "${1:-}" == "--run-checks" ]]; then
    RUN_CHECKS=1
fi

timestamp_utc="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
timestamp_local="$(date +"%Y-%m-%d %H:%M:%S %Z")"
branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")"
active_agents="$(ps aux | grep -E "codex|codeagent" | grep -v grep | wc -l | tr -d ' ' || true)"
modified_files="$(git status --short | wc -l | tr -d ' ')"
diff_stat="$(git diff --stat | tail -1)"
top_areas="$(git diff --name-only | awk -F/ 'NF==1{print "repo-root"} NF>1{print $1"/"$2}' | sort | uniq -c | sort -nr | head -10)"
recent_activity="$(tail -20 harness-progress.txt 2>/dev/null || true)"

task_summary="$(
python3 - <<'PY'
import json, pathlib
path = pathlib.Path("harness-tasks.json")
if not path.exists():
    print("No harness task file found")
    raise SystemExit(0)
data = json.loads(path.read_text())
tasks = data.get("tasks", [])
counts = {}
for task in tasks:
    counts[task["status"]] = counts.get(task["status"], 0) + 1
print("Total tasks: %d" % len(tasks))
for key in sorted(counts):
    print(f"{key}: {counts[key]}")
for task in tasks[:10]:
    print(f"- {task['id']} [{task['priority']}] {task['status']}: {task['title']}")
PY
)"

build_status="Not run in this snapshot"
build_details=""
test_status="Not run in this snapshot"
test_details=""

if [[ "${RUN_CHECKS}" == "1" ]]; then
    build_log="$(mktemp)"
    test_log="$(mktemp)"

    if cargo build --workspace >"${build_log}" 2>&1; then
        build_status="PASS"
        build_details="$(tail -20 "${build_log}")"
    else
        build_status="FAIL"
        build_details="$(tail -40 "${build_log}")"
    fi

    if cargo test --workspace --no-run >"${test_log}" 2>&1; then
        test_status="PASS"
        test_details="$(tail -20 "${test_log}")"
    else
        test_status="FAIL"
        test_details="$(tail -40 "${test_log}")"
    fi
fi

cat <<EOF
# Migration Status Report

- Generated UTC: ${timestamp_utc}
- Generated Local: ${timestamp_local}
- Branch: ${branch}
- Active agent processes: ${active_agents}
- Modified files in worktree: ${modified_files}

## Executive Snapshot

- Current migration focus: anthropic_compat.rs to provider-v2 migration with surrounding runtime, TUI, tool server, and protocol updates.
- Current diff footprint: ${diff_stat}
- Most heavily touched areas are listed below to guide review and triage.
- Verification in this snapshot:
  - cargo build --workspace: ${build_status}
  - cargo test --workspace --no-run: ${test_status}

## Top Change Areas

\`\`\`text
${top_areas}
\`\`\`

## Harness Task Summary

\`\`\`text
${task_summary}
\`\`\`

## Recent Harness Activity

\`\`\`text
${recent_activity}
\`\`\`

## Build Details

\`\`\`text
${build_details}
\`\`\`

## Test Compilation Details

\`\`\`text
${test_details}
\`\`\`

## Recommended Immediate Actions

1. Fix rustycode-core initializers to populate the new ToolCall and ToolResult fields introduced in rustycode-protocol.
2. Resolve rustycode-containers example build failures caused by private module access and mismatched config types.
3. Re-run workspace build/test compilation after the protocol alignment changes land.
4. Preserve harness artifacts and generated status reports so overnight progress can be compared against this baseline.
EOF
