#!/usr/bin/env bash

set -euo pipefail

source "$(cd "$(dirname "$0")" && pwd)/_common.sh"

usage() {
    cat <<'EOF'
Usage: ./scripts/validate_configs.sh [--strict]

Validate core repository configuration files used by migration automation.

Options:
  --strict   Fail when GitHub benchmark workflow references unknown benchmark names.
  --help     Show this message.
EOF
}

STRICT=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --strict) STRICT=1 ;;
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

RUN_ID="$(timestamp_utc)"
REPORT_FILE="${REPORT_ROOT}/validate_configs_${RUN_ID}.md"
LOG_FILE="${LOG_ROOT}/validate_configs_${RUN_ID}.log"
append_report_header "${REPORT_FILE}" "Configuration Validation"

python3 - <<'PY' "${REPO_ROOT}" >"${LOG_FILE}"
import pathlib
import re
import sys
import tomllib

root = pathlib.Path(sys.argv[1])
results = []

def record(ok, message):
    results.append((ok, message))

def parse_toml(path):
    with open(path, "rb") as handle:
        return tomllib.load(handle)

workspace = parse_toml(root / "Cargo.toml")
members = workspace.get("workspace", {}).get("members", [])
for member in members:
    cargo_toml = root / member / "Cargo.toml"
    if cargo_toml.exists():
        parse_toml(cargo_toml)
        record(True, f"Parsed {cargo_toml}")
    else:
        record(False, f"Missing {cargo_toml}")

security_cfg = root / "docs/security/security_monitoring_config.toml"
if security_cfg.exists():
    parse_toml(security_cfg)
    record(True, f"Parsed {security_cfg}")
else:
    record(True, f"Optional file not found: {security_cfg}")

expected_scripts = [
    "verify_migration.sh",
    "cleanup_legacy.sh",
    "test_coverage.sh",
    "benchmark_comparison.sh",
    "run_all_tests.sh",
    "generate_report.sh",
    "validate_configs.sh",
    "ci_integration.sh",
    "rollback.sh",
]
for script in expected_scripts:
    path = root / "scripts" / script
    record(path.exists(), f"Found {path}")

bench_names = re.findall(r'\[\[bench\]\]\s*name\s*=\s*"([^"]+)"', (root / "Cargo.toml").read_text(encoding="utf-8"), re.MULTILINE)

workflow_path = root / ".github/workflows/bench.yml"
if workflow_path.exists():
    workflow_text = workflow_path.read_text(encoding="utf-8")
    workflow_benches = re.findall(r'cargo bench --bench ([A-Za-z0-9_-]+)', workflow_text)
    for name in workflow_benches:
        record(name in bench_names, f"Workflow benchmark '{name}' exists in Cargo.toml")
else:
    record(False, f"Missing {workflow_path}")

for ok, message in results:
    prefix = "PASS" if ok else "FAIL"
    print(f"{prefix}: {message}")

failed = sum(1 for ok, _ in results if not ok)
print(f"FAILED_COUNT={failed}")
PY

FAILURES=0
WORKFLOW_MISMATCHES=0
append_report_section "${REPORT_FILE}" "Checks"
while IFS= read -r line; do
    if [[ "${line}" == FAILED_COUNT=* ]]; then
        FAILURES="${line#FAILED_COUNT=}"
        continue
    fi
    if [[ "${line}" == FAIL:\ Workflow\ benchmark* ]]; then
        WORKFLOW_MISMATCHES=$((WORKFLOW_MISMATCHES + 1))
    fi
    printf -- "- %s\n" "${line}" >>"${REPORT_FILE}"
done <"${LOG_FILE}"

if [[ "${STRICT}" -eq 0 && "${WORKFLOW_MISMATCHES}" -gt 0 ]]; then
    FAILURES=$((FAILURES - WORKFLOW_MISMATCHES))
    append_report_section "${REPORT_FILE}" "Warnings"
    printf "Workflow benchmark mismatches were downgraded to warnings because --strict was not set.\n" >>"${REPORT_FILE}"
fi

copy_latest "${REPORT_FILE}" "validate_configs_latest.md"

if [[ "${FAILURES}" -gt 0 ]]; then
    die "Configuration validation failed. Report: ${REPORT_FILE}"
fi

log_success "Configuration validation passed. Report: ${REPORT_FILE}"
