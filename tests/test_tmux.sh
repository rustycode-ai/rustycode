#!/bin/bash
# Test RustyCode TUI in tmux
# This script helps test TUI functionality within tmux sessions

set -e

echo "╔═══════════════════════════════════════════════════════════════════════════╗"
echo "║              RustyCode TUI - Tmux Testing Script                           ║"
echo "╚═══════════════════════════════════════════════════════════════════════════╝"
echo ""

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Check if we're in tmux
if [ -n "$TMUX" ]; then
    echo -e "${GREEN}✓ Already inside tmux${NC}"
    echo "  Session: $TMUX"
    echo "  Pane: $TMUX_PANE"
else
    echo -e "${YELLOW}○ Not inside tmux${NC}"
    echo "  Starting new tmux session..."
    echo ""
    echo "To test in tmux, run:"
    echo "  $0"
    echo ""
    echo "Or manually:"
    echo "  tmux new-session -d -s rustycode './target/debug/rustycode-tui'"
    echo "  tmux attach-session -t rustycode"
    exit 0
fi

echo ""
echo -e "${BLUE}[1/5] Checking tmux configuration...${NC}"

# Check tmux version
TMUX_VERSION=$(tmux -V | awk '{print $2}')
echo "  Tmux version: $TMUX_VERSION"

# Check mouse support
if tmux show-options -g mouse on 2>/dev/null | grep -q "on"; then
    echo -e "${GREEN}  ✓ Mouse support enabled${NC}"
else
    echo -e "${YELLOW}  ○ Mouse support NOT enabled${NC}"
    echo "    Run: tmux set -g mouse on"
fi

# Check terminal type
TERM_VALUE=$(tmux show-options -gv default-terminal)
echo "  Default terminal: $TERM_VALUE"

echo ""
echo -e "${BLUE}[2/5] Checking RustyCode build...${NC}"

if [ -f "./target/debug/rustycode-tui" ]; then
    echo -e "${GREEN}  ✓ Debug binary found${NC}"
elif [ -f "./target/release/rustycode-tui" ]; then
    echo -e "${GREEN}  ✓ Release binary found${NC}"
else
    echo -e "${RED}  ✗ Binary not found${NC}"
    echo "  Building RustyCode..."
    cargo build -p rustycode-tui
fi

echo ""
echo -e "${BLUE}[3/5] Displaying test instructions...${NC}"

cat << 'EOF'
┌─────────────────────────────────────────────────────────────────────────┐
│                    TUI Test Instructions                             │
├─────────────────────────────────────────────────────────────────────────┤
│  1. Start TUI: ./target/debug/rustycode-tui                                    │
│                                                                             │
│  2. Test these features:                                                        │
│                                                                             │
│  File Finder:                                                                │
│  • Press Ctrl+O to open file finder (fuzzy search)                           │
│  • Type to filter, ↑/↓ to navigate, Enter to select                          │
│  • Selected file path is inserted into input                                  │
│                                                                             │
│  Message Search:                                                              │
│  • Press Ctrl+F to search within conversation messages                       │
│  • Type query, ↑/↓ to navigate matches, Esc to close                         │
│                                                                             │
│  Model Selector:                                                              │
│  • Press Alt+P to open model selector                                        │
│  • Use ↑/↓ to navigate, Enter to select                                       │
│                                                                             │
│  Agent Mode:                                                                 │
│  • Press Ctrl+M to cycle agent mode (Ask → Code → Agent)                     │
│  • Press Ctrl+Shift+M to cycle in reverse                                    │
│                                                                             │
│  Stop Button:                                                                 │
│  • Ask a long question: "Explain quantum computing in detail"                     │
│  • While streaming, press Esc to stop                                          │
│  • Verify partial response is preserved                                        │
│                                                                             │
│  Session Naming:                                                              │
│  • Type: /rename "My Test Session"                                           │
│  • Check header shows new name                                                 │
│                                                                             │
│  Regenerate Response:                                                         │
│  • Ask any question                                                           │
│  • Press Ctrl+Shift+R to regenerate                                           │
│  • Verify new response appears                                                  │
│                                                                             │
│  Tmux Specific Tests:                                                         │
│  • Resize tmux pane (Ctrl+B then resize keys)                                │
│  • Create new window: Ctrl+B c                                               │
│  • Switch panes: Ctrl+B o                                                   │
│  • Detach/attach: Ctrl+B d / tmux attach -t rustycode                         │
│                                                                             │
│  Other Features:                                                              │
│  • Ctrl+K - Command palette                                                   │
│  • Ctrl+P - Tool panel                                                        │
│  • Ctrl+B - Session sidebar                                                   │
│  • Ctrl+T - Toggle theme                                                      │
│  • ? (when input empty) - Help                                                │
│  • Ctrl+C - Exit                                                              │
│  • Ctrl+L - Clear input (or toggle sidebar when input empty)                 │
│  • Ctrl+S - Stash input                                                       │
│  • Ctrl+R - Reverse search through input history                             │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────┘
EOF

echo ""
echo -e "${BLUE}[4/5] Recommended tmux configuration...${NC}"

cat << 'EOF'
Add to ~/.tmux.conf for optimal RustyCode TUI experience:

# Enable mouse support
set -g mouse on

# Use 256-color or truecolor
set -g default-terminal 'tmux-256color'

# Faster escape sequences (better for Vi mode)
set -sg escape-time 10

# Allow focusing events
set -g focus-events on

# Set window titles
set -g set-titles on
set -g set-titles-string '#T - RustyCode'

# Better clipboard handling
set -g set-clipboard on

# Allow unlimited scrollback
set -g history-limit 100000

Then reload: tmux source-file ~/.tmux.conf
EOF

echo ""
echo -e "${BLUE}[5/5] Ready to test!${NC}"
echo ""

# Check if user wants to start TUI now
read -p "Start RustyCode TUI now? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${GREEN}Starting TUI...${NC}"
    echo "Press Ctrl+C to exit"
    echo ""

    if [ -f "./target/debug/rustycode-tui" ]; then
        ./target/debug/rustycode-tui
    else
        ./target/release/rustycode-tui
    fi
else
    echo -e "${YELLOW}Skipped. Run manually when ready:${NC}"
    echo "  ./target/debug/rustycode-tui"
fi

echo ""
echo -e "${GREEN}Test complete!${NC}"
