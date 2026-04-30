#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE_REF="${1:-origin/main}"
THRESHOLD="${2:-10}"
CRITERION_ARGS=(${CRITERION_ARGS:-"--sample-size" "10" "--warm-up-time" "0.1" "--measurement-time" "0.2"})
BENCHES=(
    "id_performance"
    "event_bus_performance"
    "tool_dispatch_performance"
)

TMP_DIR="$(mktemp -d)"
BASE_WORKTREE="${TMP_DIR}/base"
export CARGO_TARGET_DIR="${TMP_DIR}/target"

cleanup() {
    git -C "${ROOT_DIR}" worktree remove --force "${BASE_WORKTREE}" >/dev/null 2>&1 || true
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

echo "Preparing base worktree from ${BASE_REF}"
git -C "${ROOT_DIR}" worktree add --quiet --detach "${BASE_WORKTREE}" "${BASE_REF}"

run_bench_set() {
    local workdir="$1"
    local mode="$2"
    local extra_arg="$3"
    local log_file="${TMP_DIR}/${mode}.log"

    : > "${log_file}"

    for bench in "${BENCHES[@]}"; do
        echo "Running ${mode} benchmark: ${bench}" | tee -a "${log_file}"
        (
            cd "${workdir}"
            cargo bench --bench "${bench}" -- "${CRITERION_ARGS[@]}" "${extra_arg}"
        ) 2>&1 | tee -a "${log_file}"
    done
}

run_bench_set "${BASE_WORKTREE}" "base" "--save-baseline=base"
run_bench_set "${ROOT_DIR}" "head" "--baseline=base"

FAILURES=0
while IFS= read -r line; do
    if [[ "${line}" =~ change:\ \[[^]]*[[:space:]]([-+0-9.]+)%\] ]]; then
        UPPER_BOUND="${BASH_REMATCH[1]}"
        if awk "BEGIN { exit !(${UPPER_BOUND} > ${THRESHOLD}) }"; then
            echo "Regression exceeds ${THRESHOLD}%: ${line}"
            FAILURES=$((FAILURES + 1))
        fi
    fi
done < "${TMP_DIR}/head.log"

if [[ "${FAILURES}" -gt 0 ]]; then
    echo
    echo "Performance regression check failed"
    exit 1
fi

echo
echo "Performance regression check passed"
