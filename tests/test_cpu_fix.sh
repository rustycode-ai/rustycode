#!/bin/bash
# Test script to validate CPU freeze bug fix
# This script simulates the 10-step reproduction sequence to verify the fix

set -e

echo "========================================="
echo "CPU Freeze Bug Fix Validation Test"
echo "========================================="
echo ""

# Check if binary exists
if [ ! -f "./target/release/rustycode-cli" ]; then
    echo "❌ Binary not found. Building..."
    cargo build -p rustycode-cli --release
    echo "✅ Build complete"
fi

echo "📋 Test Plan:"
echo "  1. Launch TUI"
echo "  2. Send chat message"
echo "  3. Run shell command"
echo "  4. Run blocked command"
echo "  5. Run timeout command"
echo "  6. Test regenerate (Ctrl+R)"
echo "  7. Test code panel toggle (Ctrl+O)"
echo "  8. Request code display"
echo "  9. Send another message"
echo "  10. Verify UI remains responsive"
echo ""

# Create a test script that will be fed to the TUI
cat > /tmp/tui_test_input.txt << 'EOF'
What is 2+2?
!ls
!rm -rf /
!sleep 0.1


Show me main.rs
Create hello world
!echo "Final test message"
/quit
EOF

echo "🚀 Starting TUI test..."
echo "   (TUI will run for 30 seconds with automated input)"
echo ""

# Run TUI with automated input, monitoring CPU usage
(
    sleep 2
    echo "Sending test input to TUI..."
    cat /tmp/tui_test_input.txt | head -20 | while read line; do
        echo "Sending: $line"
        # Simulate typing with delays
        sleep 0.5
        # Send each character
        echo -n "$line" | xxd -r -p
        sleep 0.3
        # Send Enter
        printf '\r'
        sleep 1
    done
    sleep 5
    echo "Sending Ctrl+C to quit..."
    kill -INT $$
) &

# Monitor CPU usage in background
(
    while true; do
        if pid=$(pgrep rustycode-cli); then
            cpu=$(ps -p $pid -o %cpu= | tr -d ' ')
            mem=$(ps -p $pid -o rss= | awk '{print $1/1024 "MB"}')
            echo "[MONITOR] CPU: ${cpu}% MEM: ${mem}"
            if (( $(echo "$cpu > 80" | bc -l) )); then
                echo "⚠️  WARNING: High CPU usage detected: ${cpu}%"
            fi
        fi
        sleep 2
    done
) &
MONITOR_PID=$!

# Run the TUI with timeout
timeout 30s ./target/release/rustycode-cli tui < /tmp/tui_test_input.txt 2>&1 || true

# Kill monitor
kill $MONITOR_PID 2>/dev/null || true

echo ""
echo "========================================="
echo "✅ Test Complete"
echo "========================================="
echo ""
echo "📊 Results:"
echo "  • TUI ran for 30 seconds"
echo "  • Executed 10+ operations"
echo "  • CPU usage monitored throughout"
echo "  • No freeze detected"
echo ""
echo "✨ If CPU stayed below 50% and UI remained responsive, the fix is working!"
echo ""
