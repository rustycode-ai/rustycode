#!/bin/bash
# Side-by-Side Comparison Test Script
# Launches RustyCode, Claude Code (if available), and Kilocode (if available)

set -e

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Side-by-Side Comparison: RustyCode vs Claude vs Kilocode  ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Check which tools are available
HAS_CLAUDE=0
HAS_KILOCODE=0

if command -v claude &> /dev/null; then
    HAS_CLAUDE=1
    echo "✅ Claude Code found"
else
    echo "⚠️  Claude Code not found (will skip)"
fi

if command -v kilocode &> /dev/null; then
    HAS_KILOCODE=1
    echo "✅ Kilocode found"
else
    echo "⚠️  Kilocode not found (will skip)"
fi

echo ""

# Function to launch in new terminal window
launch_in_terminal() {
    local title="$1"
    local command="$2"

    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        osascript <<EOF
tell application "Terminal"
    activate
    do script "cd $(pwd) && $command"
    set custom title of front window to "$title"
end tell
EOF
    else
        # Linux
        if command -v gnome-terminal &> /dev/null; then
            gnome-terminal --title="$title" -- bash -c "cd $(pwd) && $command; exec bash"
        elif command -v xterm &> /dev/null; then
            xterm -title "$title" -e "bash -c \"cd $(pwd) && $command; exec bash\"" &
        else
            echo "⚠️  Cannot open new terminal window. Please run manually:"
            echo "   $command"
        fi
    fi
}

# Option 1: Launch in separate terminal windows
echo "═══════════════════════════════════════════════════════════════"
echo "Option 1: Launch in separate terminal windows"
echo "═══════════════════════════════════════════════════════════════"
echo ""

read -p "Launch in separate windows? (y/n): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "Launching RustyCode..."
    launch_in_terminal "RustyCode" "./target/release/rustycode-cli tui"

    if [ $HAS_CLAUDE -eq 1 ]; then
        sleep 1
        echo "Launching Claude Code..."
        launch_in_terminal "Claude Code" "claude"
    fi

    if [ $HAS_KILOCODE -eq 1 ]; then
        sleep 1
        echo "Launching Kilocode..."
        launch_in_terminal "Kilocode" "kilocode"
    fi

    echo ""
    echo "✅ Launched in separate windows!"
    echo "   Arrange them side-by-side for comparison"
    echo ""
    exit 0
fi

# Option 2: Use tmux for side-by-side layout
if command -v tmux &> /dev/null; then
    echo "═══════════════════════════════════════════════════════════════"
    echo "Option 2: Launch in tmux panes"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    read -p "Use tmux for side-by-side layout? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        SESSION="rustycode-comparison"

        # Kill existing session if it exists
        tmux kill-session -t $SESSION 2>/dev/null || true

        # Create new session
        tmux new-session -d -s $SESSION -n "RustyCode" "./target/release/rustycode-cli tui"

        if [ $HAS_CLAUDE -eq 1 ]; then
            tmux split-window -h -t $SESSION
            tmux select-pane -t $SESSION:0.1
            tmux send-keys -t $SESSION:0.1 "claude" C-m
            tmux select-pane -t $SESSION:0.0
        fi

        if [ $HAS_KILOCODE -eq 1 ] && [ $HAS_CLAUDE -eq 0 ]; then
            tmux split-window -h -t $SESSION
            tmux select-pane -t $SESSION:0.1
            tmux send-keys -t $SESSION:0.1 "kilocode" C-m
            tmux select-pane -t $SESSION:0.0
        fi

        # Attach to session
        tmux attach-session -t $SESSION

        echo ""
        echo "✅ Launched in tmux!"
        echo "   Use Ctrl+B then arrows to navigate panes"
        echo "   Use Ctrl+B then Q to close panes"
        echo ""
        exit 0
    fi
fi

# Option 3: Manual instructions
echo "═══════════════════════════════════════════════════════════════"
echo "Manual Launch Instructions"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "Open 3 terminal windows and run:"
echo ""
echo "Terminal 1 (RustyCode):"
echo "  cd $(pwd)"
echo "  ./target/release/rustycode-cli tui"
echo ""

if [ $HAS_CLAUDE -eq 1 ]; then
    echo "Terminal 2 (Claude Code):"
    echo "  claude"
    echo ""
fi

if [ $HAS_KILOCODE -eq 1 ]; then
    echo "Terminal 3 (Kilocode):"
    echo "  kilocode"
    echo ""
fi

echo "═══════════════════════════════════════════════════════════════"
echo "Test Prompts to Try:"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "1. File reading:"
echo "   'Read crates/rustycode-tui/src/minimal.rs and explain the architecture'"
echo ""
echo "2. Code editing:"
echo "   'Add a comment to the top of minimal.rs explaining this is a TUI'"
echo ""
echo "3. Long response:"
echo "   'Tell me about the complete architecture of this project in detail'"
echo ""
echo "4. Tool use:"
echo "   'List all Rust files in the project and count total lines of code'"
echo ""
echo "5. Multi-step:"
echo "   'Find the largest file, read it, and suggest improvements'"
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "📋 See COMPARISON_TEST.md for detailed feature checklist"
echo ""
