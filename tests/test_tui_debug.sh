#!/bin/bash
# Test script to debug TUI event loop

echo "=== Starting TUI with debug logging ==="
echo "Debug logs will appear on stderr"
echo "Type something and press Enter to test"
echo "Press Ctrl+C to exit"
echo ""

# Run the TUI with stderr visible
RUSTYCODE_LOG=debug cargo run --bin rustycode-tui 2>&1 | tee /tmp/tui_debug.log

echo ""
echo "=== TUI exited ==="
echo "Debug log saved to /tmp/tui_debug.log"
