#!/bin/bash
# Test script to debug tool invocation

echo "Starting rustycode-cli..."
echo "After it starts, type: Read Cargo.toml"
echo "Then wait for the response and press Ctrl+C"
echo ""
echo "After running, check these files:"
echo "  - tool_debug.log (API calls)"
echo "  - parse_debug.log (conversation parsing)"
echo ""

rm -f tool_debug.log parse_debug.log

cargo run --package rustycode-cli

echo ""
echo "=== Debug Output ==="
if [ -f tool_debug.log ]; then
    echo "--- tool_debug.log ---"
    cat tool_debug.log
else
    echo "tool_debug.log not found!"
fi

echo ""
if [ -f parse_debug.log ]; then
    echo "--- parse_debug.log ---"
    cat parse_debug.log
else
    echo "parse_debug.log not found!"
fi
