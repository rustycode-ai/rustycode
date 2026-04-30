#!/bin/bash
# Manual TUI Input Testing Script
# This script automates sending test input to the TUI via tmux

set -e

echo "=== TUI Character Input Testing ==="
echo ""

# Check if tmux is available
if ! command -v tmux &> /dev/null; then
    echo "❌ tmux not found. Please install tmux first."
    echo "   macOS: brew install tmux"
    echo "   Linux: sudo apt-get install tmux"
    exit 1
fi

# Check if session exists
SESSION_NAME="tui_test"
if tmux has-session -t $SESSION_NAME 2>/dev/null; then
    echo "⚠️  Session '$SESSION_NAME' already exists. Killing it..."
    tmux kill-session -t $SESSION_NAME
    sleep 1
fi

echo "🚀 Launching TUI in tmux session..."
tmux new-session -d -s $SESSION_NAME \
    "cd /Users/nat/dev/rustycode && cargo run --manifest-path crates/rustycode-tui/Cargo.toml 2>&1 | tee /tmp/tui_test.log"

echo "⏳ Waiting for TUI to initialize (5 seconds)..."
sleep 5

# Check if TUI is still running
if ! tmux has-session -t $SESSION_NAME 2>/dev/null; then
    echo "❌ TUI failed to start. Check log:"
    cat /tmp/tui_test.log
    exit 1
fi

echo "✅ TUI is running"
echo ""

# Test 1: Basic character input
echo "📝 Test 1: Basic character input"
tmux send-keys -t $SESSION_NAME "hello world" Enter
sleep 2
echo "   ✓ Sent: 'hello world'"
sleep 2

# Test 2: Uppercase
echo "📝 Test 2: Uppercase input"
tmux send-keys -t $SESSION_NAME "HELLO WORLD" Enter
sleep 2
echo "   ✓ Sent: 'HELLO WORLD'"
sleep 2

# Test 3: Numbers
echo "📝 Test 3: Numbers"
tmux send-keys -t $SESSION_NAME "12345" Enter
sleep 2
echo "   ✓ Sent: '12345'"
sleep 2

# Test 4: Special characters
echo "📝 Test 4: Special characters"
tmux send-keys -t $SESSION_NAME '!@#$%^&*()' Enter
sleep 2
echo "   ✓ Sent: '!@#$%^&*()'"
sleep 2

# Test 5: Unicode - Latin extended
echo "📝 Test 5: Unicode (Latin extended)"
tmux send-keys -t $SESSION_NAME 'café naïve' Enter
sleep 2
echo "   ✓ Sent: 'café naïve'"
sleep 2

# Test 6: Unicode - Emoji
echo "📝 Test 6: Unicode (Emoji)"
tmux send-keys -t $SESSION_NAME '🎉 🔥 🚀' Enter
sleep 2
echo "   ✓ Sent: '🎉 🔥 🚀'"
sleep 2

# Test 7: Backspace
echo "📝 Test 7: Backspace deletion"
tmux send-keys -t $SESSION_NAME "hello"
sleep 1
tmux send-keys -t $SESSION_NAME C-u?  # This might not work, backspace is tricky
sleep 1
echo "   ✓ Attempted backspace test (manual verification needed)"
sleep 2

# Test 8: Multi-line input
echo "📝 Test 8: Multi-line input (Option+Enter)"
tmux send-keys -t $SESSION_NAME "line 1"
sleep 1
# Note: Option+Enter is difficult to send via tmux
# This test requires manual interaction
echo "   ⚠️  Multi-line test requires manual interaction"
echo "      In tmux: Press Option+Enter, type multiple lines, then Option+Enter again"
sleep 2

# Test 9: Arrow keys
echo "📝 Test 9: Arrow navigation"
tmux send-keys -t $SESSION_NAME "test"
sleep 1
tmux send-keys -t $SESSION_NAME Left
sleep 1
tmux send-keys -t $SESSION_NAME Right
sleep 1
echo "   ✓ Sent arrow keys (manual verification needed)"
sleep 2

# Test 10: Quit
echo "📝 Test 10: Quit (Ctrl+C)"
tmux send-keys -t $SESSION_NAME C-c
sleep 2

# Check if session ended
if tmux has-session -t $SESSION_NAME 2>/dev/null; then
    echo "⚠️  Session still running. Force killing..."
    tmux kill-session -t $SESSION_NAME
fi

echo ""
echo "=== Testing Complete ==="
echo ""
echo "📋 Test Log: /tmp/tui_test.log"
echo ""
echo "⚠️  IMPORTANT: Some tests require visual verification"
echo "   To manually test, run:"
echo "   tmux attach-session -t $SESSION_NAME"
echo ""
echo "📝 Manual Testing Checklist:"
echo "   [ ] Characters appear as typed"
echo "   [ ] No input lag"
echo "   [ ] Unicode renders correctly (é, ñ, 中文)"
echo "   [ ] Backspace works"
echo "   [ ] Arrow keys navigate"
echo "   [ ] Enter sends messages"
echo "   [ ] Option+Enter enables multi-line"
echo "   [ ] Ctrl+C quits"
echo ""
