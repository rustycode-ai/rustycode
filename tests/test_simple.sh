#!/bin/bash
# Simple end-to-end test for rustycode LLM functionality

export ANTHROPIC_API_KEY='776d98e5506547fb8df6b8ed93145265.DoSuGn49s81AYPZL'
export ANTHROPIC_BASE_URL='https://api.z.ai/api/anthropic'

echo "=== Simple E2E Test ==="
echo ""

# Build
echo "1. Building..."
cargo build -p rustycode-cli --bins --quiet 2>&1 | grep -i error && exit 1
echo "   ✓ Build successful"
echo ""

# Run test binary
echo "2. Running end-to-end test..."
./target/debug/test_e2e
echo ""

# Run integration tests
echo "3. Running integration tests..."
cargo test -p rustycode-tui --test integration_test --quiet 2>&1 | tail -3
echo ""

echo "=== Test Complete ==="
