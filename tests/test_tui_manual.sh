#!/bin/bash
# Manual test script for TUI enhancements
# This script will help verify the resize and tmux integration work correctly

echo "🧪 TUI Enhancement Manual Verification"
echo "======================================"
echo ""

# Build first
echo "📦 Building..."
cargo build -p rustycode-cli --quiet 2>&1 | grep -i "error" && exit 1
echo "✅ Build successful"
echo ""

# Test 1: Regular terminal
echo "📋 Test 1: Regular Terminal"
echo "   Starting TUI in regular terminal..."
echo "   Instructions:"
echo "   1. Resize the terminal window"
echo "   2. Verify the TUI redraws correctly"
echo "   3. Type 'test' and verify input works"
echo "   4. Press Ctrl+C to exit"
echo ""
read -p "Press Enter to start TUI (or Ctrl+C to skip)..."
./target/debug/rustycode-cli tui || true

echo ""
echo "📋 Test 2: Tmux Integration"
echo "   This test requires tmux to be installed"

if command -v tmux &> /dev/null; then
    echo "   ✅ tmux found"
    echo ""
    echo "   Instructions:"
    echo "   1. A tmux session will be created"
    echo "   2. Verify tmux setup instructions are displayed"
    echo "   3. Resize the tmux pane"
    echo "   4. Verify the TUI redraws correctly"
    echo "   5. Exit with Ctrl+C"
    echo ""
    read -p "Press Enter to start tmux test (or Ctrl+C to skip)..."

    # Create a new tmux session and run the TUI
    tmux new-session -d -s rustycode-test "cd /Users/nat/dev/rustycode && ./target/debug/rustycode-cli tui"
    sleep 1
    tmux attach-session -t rustycode-test

    # Clean up
    tmux kill-session -t rustycode-test 2>/dev/null || true
else
    echo "   ⚠️  tmux not found - skipping tmux test"
    echo "   Install tmux with: brew install tmux"
fi

echo ""
echo "✅ Manual verification complete!"
echo ""
echo "📝 Summary of Changes:"
echo "   1. ✅ Resize event handling implemented"
echo "   2. ✅ Tmux integration with bracketed paste mode"
echo "   3. ✅ Tmux setup instructions displayed on startup"
echo "   4. ✅ Terminal capability detection"
echo "   5. ✅ Proper cleanup on exit and panic"
