#!/usr/bin/env bash
# Run LiveBench evaluation through RustyCode CLI.
#
# Usage:
#   ./scripts/livebench_run.sh                          # All non-coding categories
#   ./scripts/livebench_run.sh reasoning math            # Specific categories
#   ./scripts/livebench_run.sh --dry-run reasoning       # Dry-run test
#
# Prerequisites:
#   1. export ANTHROPIC_API_KEY="sk-ant-..."  (or OPENAI_API_KEY)
#   2. cargo build --release -p rustycode-cli
#   3. cd /path/to/livebench && python3 -m venv .venv && .venv/bin/pip install -e . 'setuptools<82'

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LIVEBENCH_ROOT="${LIVEBENCH_ROOT:-$(cd "$PROJECT_ROOT/../livebench" 2>/dev/null && pwd)}"

# Default binary
RUSTYCODE_BIN="${RUSTYCODE_BIN:-$PROJECT_ROOT/target/release/rustycode-cli}"
if [ ! -x "$RUSTYCODE_BIN" ]; then
    RUSTYCODE_BIN="$PROJECT_ROOT/target/debug/rustycode-cli"
fi

# Check binary exists
if [ ! -x "$RUSTYCODE_BIN" ]; then
    echo "ERROR: rustycode-cli not found. Build it first:"
    echo "  cargo build --release -p rustycode-cli"
    exit 1
fi

echo "Using: $RUSTYCODE_BIN"

# Check LiveBench
if [ ! -d "$LIVEBENCH_ROOT" ]; then
    echo "ERROR: LiveBench not found at $LIVEBENCH_ROOT"
    echo "Clone it: cd /path/to/dev && git clone https://github.com/livebench/livebench"
    exit 1
fi

VENV_PYTHON="$LIVEBENCH_ROOT/.venv/bin/python3"
if [ ! -x "$VENV_PYTHON" ]; then
    echo "ERROR: LiveBench venv not found. Set it up:"
    echo "  cd $LIVEBENCH_ROOT && python3 -m venv .venv"
    echo "  .venv/bin/pip install -e . 'setuptools<82'"
    exit 1
fi

# Check API key
if [ -z "${ANTHROPIC_API_KEY:-}" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
    echo "WARNING: No ANTHROPIC_API_KEY or OPENAI_API_KEY set."
    echo "         Inference will fail. Use --dry-run to test the pipeline."
fi

# Forward all args to the adapter
export RUSTYCODE_BIN
exec "$VENV_PYTHON" "$SCRIPT_DIR/livebench_adapter.py" \
    --rustycode-bin "$RUSTYCODE_BIN" \
    "$@"
