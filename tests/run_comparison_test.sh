#!/bin/bash
# Quick launcher for side-by-side TUI comparison testing

set -e

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     TUI Feature Comparison - RustyCode vs Claude vs Kilocode ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Check availability
HAS_CLAUDE=0
HAS_KILOCODE=0

if command -v claude &> /dev/null; then
    HAS_CLAUDE=1
    echo "✅ Claude Code found"
else
    echo "⚠️  Claude Code not found"
fi

if command -v kilocode &> /dev/null; then
    HAS_KILOCODE=1
    echo "✅ Kilocode found"
else
    echo "⚠️  Kilocode not found"
fi

echo ""
echo "📋 Test plan: FEATURE_COMPARISON_TEST.md"
echo ""

# Use tmux for side-by-side layout
if command -v tmux &> /dev/null; then
    read -p "Launch in tmux side-by-side? (y/n): " -n 1 -r
    echo

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        SESSION="tui-comparison"

        # Kill existing session
        tmux kill-session -t $SESSION 2>/dev/null || true

        # Calculate layout
        if [ $HAS_CLAUDE -eq 1 ] && [ $HAS_KILOCODE -eq 1 ]; then
            # 3 panes
            tmux new-session -d -s $SESSION -n "RustyCode" "cd $(pwd) && ./target/release/rustycode-cli tui"
            tmux split-window -h -t $SESSION
            tmux split-window -h -t $SESSION
            tmux select-layout -t $SESSION even-horizontal

            # Start tools in panes 1 and 2
            tmux send-keys -t $SESSION:0.1 "claude" C-m
            tmux send-keys -t $SESSION:0.2 "kilocode" C-m

            echo "✅ Launched all 3 tools side-by-side!"
            echo "   Use Ctrl+B then arrow keys to navigate panes"
        elif [ $HAS_CLAUDE -eq 1 ]; then
            # 2 panes
            tmux new-session -d -s $SESSION -n "RustyCode" "cd $(pwd) && ./target/release/rustycode-cli tui"
            tmux split-window -h -t $SESSION
            tmux send-keys -t $SESSION:0.1 "claude" C-m

            echo "✅ Launched RustyCode and Claude Code side-by-side!"
            echo "   Use Ctrl+B then arrow keys to navigate panes"
        else
            # Just RustyCode
            tmux new-session -d -s $SESSION -n "RustyCode" "cd $(pwd) && ./target/release/rustycode-cli tui"
            echo "✅ Launched RustyCode!"
        fi

        # Attach to session
        tmux attach-session -t $SESSION
        exit 0
    fi
fi

# Fallback: manual instructions
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
echo "Quick Test Prompts"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "1. Markdown:"
echo "   'Create a table comparing Rust, Go, and Python'"
echo ""
echo "2. Long response (scrolling):"
echo "   'Tell me about the complete architecture in detail'"
echo ""
echo "3. File reading:"
echo "   'Read crates/rustycode-tui/src/minimal.rs'"
echo ""
echo "4. Tool execution:"
echo "   'List all Rust files and count lines'"
echo ""
echo "5. Commit:"
echo "   'Check git status and commit changes'"
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "📋 See FEATURE_COMPARISON_TEST.md for comprehensive test plan"
echo ""
