#!/bin/bash
set -e

# Install uv if not present
if ! command -v uv &>/dev/null; then
    curl -LsSf https://astral.sh/uv/0.9.5/install.sh | sh
    source "$HOME/.local/bin/env"
fi

# Set APP_DIR for tests
export APP_DIR="${APP_DIR:-/app}"

# Run pytest
uvx -p 3.13 -w pytest==8.4.1 pytest --tb=short -v /tests/test_mips.py

# Write reward
if [ $? -eq 0 ]; then
    echo 1 > /logs/verifier/reward.txt
else
    echo 0 > /logs/verifier/reward.txt
fi
