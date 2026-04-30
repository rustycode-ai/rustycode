#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORT_ROOT="${ROOT_DIR}/reports/migration"
STATUS_DIR="${REPORT_ROOT}/status"
SUMMARY_DIR="${REPORT_ROOT}/summaries"
RESULTS_DIR="${REPORT_ROOT}/results"
MONITOR_DIR="${REPORT_ROOT}/monitoring"
LOG_DIR="${ROOT_DIR}/logs/migration-monitoring"
PID_FILE="${MONITOR_DIR}/monitor.pid"

mkdir -p "${STATUS_DIR}" "${SUMMARY_DIR}" "${RESULTS_DIR}" "${MONITOR_DIR}" "${LOG_DIR}"

INTERVAL_MINUTES="${INTERVAL_MINUTES:-30}"
DURATION_HOURS="${DURATION_HOURS:-10}"
RUN_CHECKS="${RUN_CHECKS:-1}"

usage() {
    cat <<'EOF'
Usage:
  ./scripts/monitor_progress.sh once
  ./scripts/monitor_progress.sh overnight
  ./scripts/monitor_progress.sh start
  ./scripts/monitor_progress.sh stop
  ./scripts/monitor_progress.sh status
EOF
}

is_running() {
    [[ -f "${PID_FILE}" ]] && kill -0 "$(cat "${PID_FILE}")" 2>/dev/null
}

run_snapshot() {
    local mode="${1:-manual}"
    local ts
    local status_path
    local results_path
    local summary_path
    ts="$(date +"%Y%m%d_%H%M%S")"
    status_path="${STATUS_DIR}/status_${ts}.md"
    results_path="${RESULTS_DIR}/collection_${ts}.md"
    summary_path="${SUMMARY_DIR}/daily_summary_${ts}.md"

    echo "[monitor] ${mode} snapshot at ${ts}"

    if [[ "${RUN_CHECKS}" == "1" ]]; then
        if ! "${ROOT_DIR}/scripts/generate_status_report.sh" --run-checks > "${status_path}"; then
            printf "# Migration Status Report\n\nStatus generation failed at %s\n" "${ts}" > "${status_path}"
            echo "[monitor] status generation failed"
        fi
    else
        if ! "${ROOT_DIR}/scripts/generate_status_report.sh" > "${status_path}"; then
            printf "# Migration Status Report\n\nStatus generation failed at %s\n" "${ts}" > "${status_path}"
            echo "[monitor] status generation failed"
        fi
    fi

    if ! "${ROOT_DIR}/scripts/collect_results.sh" > "${results_path}"; then
        printf "# Result Collection Report\n\nCollection failed at %s\n" "${ts}" > "${results_path}"
        echo "[monitor] result collection failed"
    fi

    if ! "${ROOT_DIR}/scripts/summarize_findings.sh" > "${summary_path}"; then
        printf "# Daily Migration Findings Summary\n\nSummary generation failed at %s\n" "${ts}" > "${summary_path}"
        echo "[monitor] summary generation failed"
    fi

    cat > "${MONITOR_DIR}/latest_snapshot.txt" <<EOF
timestamp=${ts}
mode=${mode}
status_report=${status_path}
results_report=${results_path}
summary_report=${summary_path}
EOF
}

run_loop() {
    local mode="${1:-overnight}"
    local total_minutes=$((DURATION_HOURS * 60))
    local elapsed=0

    echo "$$" > "${PID_FILE}"
    echo "[monitor] mode=${mode} interval=${INTERVAL_MINUTES}m duration=${DURATION_HOURS}h"

    while (( elapsed < total_minutes )); do
        run_snapshot "${mode}"
        elapsed=$((elapsed + INTERVAL_MINUTES))

        if (( elapsed < total_minutes )); then
            sleep $((INTERVAL_MINUTES * 60))
        fi
    done

    rm -f "${PID_FILE}"
    echo "[monitor] completed at $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
}

start_background() {
    if is_running; then
        echo "Monitoring already running with pid $(cat "${PID_FILE}")"
        exit 0
    fi

    local log_file="${LOG_DIR}/monitor_$(date +"%Y%m%d_%H%M%S").log"
    (
        cd "${ROOT_DIR}"
        export INTERVAL_MINUTES DURATION_HOURS RUN_CHECKS
        nohup bash ./scripts/monitor_progress.sh overnight > "${log_file}" 2>&1 &
        echo $! > "${PID_FILE}"
    )
    local pid
    pid="$(cat "${PID_FILE}")"
    echo "Started monitoring in background"
    echo "PID: ${pid}"
    echo "Log: ${log_file}"
}

stop_background() {
    if ! is_running; then
        echo "No monitoring process is running"
        rm -f "${PID_FILE}"
        exit 0
    fi

    local pid
    pid="$(cat "${PID_FILE}")"
    kill "${pid}"
    rm -f "${PID_FILE}"
    echo "Stopped monitoring process ${pid}"
}

show_status() {
    echo "=== Migration Monitor Status ==="
    if is_running; then
        echo "State: running"
        echo "PID: $(cat "${PID_FILE}")"
    else
        echo "State: stopped"
    fi

    if [[ -f "${MONITOR_DIR}/latest_snapshot.txt" ]]; then
        echo
        cat "${MONITOR_DIR}/latest_snapshot.txt"
    fi
}

command="${1:-overnight}"
case "${command}" in
    once)
        run_snapshot "once"
        ;;
    overnight)
        run_loop "overnight"
        ;;
    start)
        start_background
        ;;
    stop)
        stop_background
        ;;
    status)
        show_status
        ;;
    *)
        usage
        exit 1
        ;;
esac
