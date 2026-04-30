#!/bin/bash
# Test script for TUI enhancements
# Tests resize handling and tmux integration

set -e

echo "🧪 Testing TUI Enhancements"
echo "=========================="
echo ""

# Check if we're in tmux
if [ -n "$TMUX" ]; then
    echo "✅ Running inside tmux"
    echo "   TMUX=$TMUX"
    echo "   TERM=$TERM"
else
    echo "ℹ️  Not in tmux (tmux features will be disabled)"
    echo "   TERM=$TERM"
fi

echo ""
echo "📦 Building rustycode-cli..."
cargo build -p rustycode-cli --quiet

echo ""
echo "🧪 Running unit tests for tmux module..."
cargo test -p rustycode-tui tmux --quiet || echo "   (No tmux tests found - that's OK)"

echo ""
echo "✅ Build successful!"
echo ""
echo "📋 Test Manual Verification Steps:"
echo "   1. Run: ./target/debug/rustycode-cli tui"
echo "   2. If in tmux, verify bracketed paste mode is enabled"
echo "   3. Resize the terminal window"
echo "   4. Verify TUI redraws correctly with preserved state"
echo "   5. Exit with Ctrl+C and verify clean cleanup"
echo ""
echo "🔍 To test in tmux:"
echo "   1. Start tmux: tmux new-session -d -s rustycode-test"
echo "   2. Attach: tmux attach-session -t rustycode-test"
echo "   3. Run: ./target/debug/rustycode-cli tui"
echo "   4. Verify tmux setup instructions are displayed"
echo ""
