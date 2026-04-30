#!/bin/bash
# Test script for LLM integration in TUI
# This script verifies that the TUI can successfully communicate with the LLM

set -e

echo "=== LLM Integration Test ==="
echo "Building TUI..."
cargo build -p rustycode-tui --release

echo ""
echo "✅ Build successful!"
echo ""
echo "To test the TUI with real LLM integration:"
echo "1. Run: ./target/release/rustycode-tui"
echo "2. Type a message (e.g., 'hello')"
echo "3. You should see a real LLM response (not simulated)"
echo "4. Press Ctrl+C to exit"
echo ""
echo "Expected behavior:"
echo "  - Message appears on screen"
echo "  - LLM streams response in chunks"
echo "  - No 'simulated response' message"
echo "  - No debug output on screen"
echo ""
echo "To run in background for testing:"
echo "  ./target/release/rustycode-tui &"
echo ""
