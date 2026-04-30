#!/bin/bash
set -e

echo "=== Harness Initialization ==="

# Check cargo is available
if ! command -v cargo &> /dev/null; then
    echo "ERROR: cargo not found"
    exit 1
fi

# Quick build smoke test
echo "Running build smoke test..."
cargo build --workspace 2>&1 | head -5

# Quick test smoke test  
echo "Running test smoke test..."
cargo test --workspace --no-run 2>&1 | head -5

echo "✓ Harness initialization complete"
