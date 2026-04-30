#!/bin/bash
# Diagnostic script for TUI freeze issues

echo "=== RustyCode TUI Freeze Diagnostics ==="
echo ""

# Check if rustycode is running
if pgrep -x "rustycode-cli" > /dev/null; then
    echo "✓ RustyCode process is running"

    # Get PID
    PID=$(pgrep -x "rustycode-cli" | head -1)
    echo "  PID: $PID"

    # Check CPU usage
    CPU=$(ps -p $PID -o %cpu | tail -1)
    echo "  CPU: $CPU%"

    # Check if it's stuck in a loop (>90% CPU)
    if (( $(echo "$CPU > 90" | bc -l) )); then
        echo "  ⚠️  WARNING: Process at 100% CPU - likely stuck in infinite loop!"
    fi

    # Get thread count
    THREADS=$(ps -p $PID -o nlwp | tail -1)
    echo "  Threads: $THREADS"

    # Sample the process (what is it doing?)
    echo ""
    echo "  Sampling process (10 snapshots)..."
    for i in {1..10}; do
        # Get a stack trace using sample
        if command -v sample &> /dev/null; then
            echo "  Snapshot $i:"
            sample $PID 1 -file /dev/stdout 2>/dev/null | head -20 | grep -A 10 "Call graph" || echo "    (no stack trace available)"
        fi
        sleep 0.1
    done
else
    echo "✗ RustyCode process is NOT running"
fi

echo ""
echo "=== Recent Logs ==="
if [ -f "/tmp/rustycode.log" ]; then
    tail -20 /tmp/rustycode.log
else
    echo "No log file found at /tmp/rustycode.log"
fi

echo ""
echo "=== System Info ==="
echo "Terminal: $TERM"
echo "Shell: $SHELL"
echo "RustyCode version:"
cargo metadata --format-version 1 --no-deps 2>/dev/null | grep -A 3 '"name":"rustycode-cli"' || echo "  (unable to get version)"

echo ""
echo "=== Recommendations ==="
echo "1. If process is at 100% CPU, kill it: kill -9 $PID"
echo "2. Restart rustycode with debug logging: RUST_LOG=debug rustycode tui"
echo "3. Run diagnostics again when it freezes"
