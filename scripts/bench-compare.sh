#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
exec "${SCRIPT_DIR}/benchmark_comparison.sh" --candidate "${1:-migration-current}" --baseline "${2:-main}"
