#!/bin/bash
# Ultimate AI Coding Tools Comparison Launcher

set -e

cat << 'BANNER'
╔════════════════════════════════════════════════════════════════════════╗
║     AI Coding Tools - Mega Comparison (6 Tools Side-by-Side)          ║
╚════════════════════════════════════════════════════════════════════════╝
BANNER

# Check all tools
TOOLS=(
    "RustyCode:./target/release/rustycode-cli tui"
    "Claude:claude"
    "Kilocode:kilocode"
    "Gemini:gemini chat"
    "Codex:codex chat"
)

AVAILABLE=()
for tool in "${TOOLS[@]}"; do
    name="${tool%%:*}"
    cmd="${tool##*:}"
    if command -v ${cmd%% *} &> /dev/null || [ -f "./target/release/rustycode-cli" ]; then
        echo "✅ $name found"
        AVAILABLE+=("$name:$cmd")
    else
        echo "⚠️  $name not found"
    fi
done

echo ""
echo "📋 Test plan: FEATURE_COMPARISON_TEST.md"
echo ""

# Use tmux for side-by-side layout
if command -v tmux &> /dev/null; then
    read -p "Launch in tmux? (y/n): " -n 1 -r
    echo

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        SESSION="ai-comparison"

        # Kill existing session
        tmux kill-session -t $SESSION 2>/dev/null || true

        # Count available tools
        COUNT=${#AVAILABLE[@]}

        if [ $COUNT -eq 0 ]; then
            echo "❌ No tools available!"
            exit 1
        fi

        echo "🚀 Launching $COUNT tool(s)..."

        # Start first tool
        first="${AVAILABLE[0]}"
        first_name="${first%%:*}"
        first_cmd="${first##*:}"

        if [ "$first_name" = "RustyCode" ]; then
            tmux new-session -d -s $SESSION -n "$first_name" "cd $(pwd) && $first_cmd"
        else
            tmux new-session -d -s $SESSION -n "$first_name" "$first_cmd"
        fi

        # Add remaining tools
        for i in $(seq 1 $(($COUNT - 1))); do
            tool="${AVAILABLE[$i]}"
            name="${tool%%:*}"
            cmd="${tool##*:}"

            # Split window
            tmux split-window -h -t $SESSION

            # Get pane number and send command
            pane=$i
            tmux send-keys -t $SESSION:0.$pane "$cmd" C-m
        done

        # Even layout
        tmux select-layout -t $SESSION even-horizontal

        echo ""
        echo "✅ Launched $COUNT tools side-by-side!"
        echo ""
        echo "🎮 tmux Controls:"
        echo "   • Navigate: Ctrl+B then ← → ↑ ↓"
        echo "   • Resize:   Ctrl+B then Shift+← → ↑ ↓"
        echo "   • Detach:   Ctrl+B then D"
        echo "   • Reattach: tmux attach -t $SESSION"
        echo ""

        read -p "Press Enter to attach to session..."
        tmux attach-session -t $SESSION
        exit 0
    fi
fi

# Fallback: manual instructions
echo "═══════════════════════════════════════════════════════════════"
echo "Manual Launch Instructions"
echo "═══════════════════════════════════════════════════════════════"
echo ""

for tool in "${AVAILABLE[@]}"; do
    name="${tool%%:*}"
    cmd="${tool##*:}"
    echo "Terminal: $name"
    echo "  $cmd"
    echo ""
done

echo "═══════════════════════════════════════════════════════════════"
echo "Quick Test Prompts (try in all tools)"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "1. Markdown Table:"
echo "   'Create a table comparing Rust, Go, and Python'"
echo ""
echo "2. Long Response (scrolling):"
echo "   'Tell me about the complete architecture in detail'"
echo ""
echo "3. File Reading:"
echo "   'Read crates/rustycode-tui/src/minimal.rs'"
echo ""
echo "4. Tool Execution:"
echo "   'List all Rust files and count lines'"
echo ""
echo "5. Code Edit:"
echo "   'Add a doc comment to the top of minimal.rs'"
echo ""
echo "6. Commit:"
echo "   'Check git status and commit changes'"
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "📋 See FEATURE_COMPARISON_TEST.md for comprehensive test plan"
echo ""
