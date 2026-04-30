#!/bin/bash
# Test script for TUI in tmux

cd /Users/nat/dev/rustycode

# Check if session exists and kill it
tmux has-session -t rustycode_test 2>/dev/null
if [ $? -eq 0 ]; then
    tmux kill-session -t rustycode_test
fi

# Start new session with TUI
echo "Starting TUI in tmux..."
tmux new-session -d -s rustycode_test -x 80 -y 24

# Wait for session to initialize
sleep 1

# Send commands to launch TUI
tmux send-keys -t rustycode_test "export ANTHROPIC_API_KEY='sk-ant-test-key'" Enter
tmux send-keys -t rustycode_test "cd /Users/nat/dev/rustycode" Enter
tmux send-keys -t rustycode_test "cargo run --manifest-path crates/rustycode-tui/Cargo.toml" Enter

# Wait for TUI to start
sleep 3

# Capture initial output
echo "=== Initial TUI Output ==="
tmux capture-pane -t rustycode_test -p | tail -30

echo ""
echo "To attach: tmux attach-session -t rustycode_test"
echo "To kill: tmux kill-session -t rustycode_test"
