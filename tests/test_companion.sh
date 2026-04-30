#!/bin/bash
# TUI Testing Companion - Automated testing assistant

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Test results file
RESULTS_FILE="test_results_$(date +%Y%m%d_%H%M%S).md"

cat << 'BANNER'
╔════════════════════════════════════════════════════════════════════════╗
║           🧪 TUI Testing Companion - Your Automated Assistant         ║
╚════════════════════════════════════════════════════════════════════════╝

I'll guide you through systematic testing of all 5 tools.
Results will be saved to: test_results_TIMESTAMP.md
BANNER

echo ""
echo -e "${CYAN}📋 Available Test Sessions:${NC}"
echo "   1. Quick Scan (5 min) - First impressions only"
echo "   2. Core Features (15 min) - Essential functionality"
echo "   3. Deep Dive (30 min) - Comprehensive testing"
echo "   4. Full Review (60 min) - Everything"
echo ""
read -p "Choose session (1-4): " session_choice

case $session_choice in
    1) DURATION="quick" ;;
    2) DURATION="core" ;;
    3) DURATION="deep" ;;
    4) DURATION="full" ;;
    *) DURATION="core" ;;
esac

# Initialize results file
cat > "$RESULTS_FILE" << INIT
# TUI Comparison Test Results

**Date:** $(date +%Y-%m-%d)
**Session:** $DURATION
**Tester:** Alex Chen (QA Persona)

---

## Tools Being Tested

- ✅ RustyCode
- ✅ Claude Code
- ✅ Kilocode
- ✅ Gemini CLI
- ✅ Codex CLI

---

INIT

echo -e "${GREEN}✅ Results file created: $RESULTS_FILE${NC}"
echo ""
echo -e "${BOLD}📝 Testing Instructions:${NC}"
echo ""
echo "1. Attach to tmux session:"
echo "   ${CYAN}tmux attach -t ai-comparison${NC}"
echo ""
echo "2. Navigate between tools:"
echo "   ${CYAN}Ctrl+B then ← →${NC}"
echo ""
echo "3. I'll guide you through each test"
echo "4. Document your findings"
echo ""
echo "   When you see this prompt:"
echo "   ${YELLOW}► Run in each tool and observe${NC}"
echo ""
echo "   Switch to each pane and run the test"
echo "   Come back here and record your findings"
echo ""
read -p "Ready to start? (y/n): " ready

if [[ ! $ready =~ ^[Yy]$ ]]; then
    echo "Exiting..."
    exit 0
fi

# Test functions
run_test() {
    local test_name=$1
    local prompt=$2
    local what_to_look_for=$3
    local duration=$4

    clear
    cat << TEST_HEADER
╔════════════════════════════════════════════════════════════════════════╗
║  $test_name
╚════════════════════════════════════════════════════════════════════════╝

📝 PROMPT:
$prompt

🔍 WHAT TO LOOK FOR:
$what_to_look_for

⏱️  TIME LIMIT: $duration minutes

TEST_HEADER

    echo ""
    echo -e "${YELLOW}► INSTRUCTIONS:${NC}"
    echo "1. Switch to tmux session (in another terminal):"
    echo "   ${CYAN}tmux attach -t ai-comparison${NC}"
    echo ""
    echo "2. Navigate to each tool with: Ctrl+B then ← →"
    echo ""
    echo "3. Run the prompt in EACH tool"
    echo ""
    echo "4. Observe and take notes"
    echo ""
    echo "5. Come back here and record findings"
    echo ""
    read -p "Press Enter when you've completed this test..."

    echo ""
    echo -e "${CYAN}📊 Record your findings:${NC}"
    echo ""

    # Prompt for findings for each tool
    for tool in RustyCode Claude Kilocode Gemini Codex; do
        echo -e "${BOLD}$tool:${NC}"
        echo "  Score (1-10): \c"
        read score
        echo "  Notes: \c"
        read notes
        echo "  $tool: $score/10 - $notes" >> "$RESULTS_FILE"
        echo "" >> "$RESULTS_FILE"
    done

    echo "✅ Test completed and recorded"
    sleep 1
}

# Test sequence based on duration
case $DURATION in
    quick)
        # Quick scan - first impressions only
        run_test "First Impressions" \
            "Just look at each tool" \
            "Visual appeal, clarity, welcome message, colors, spacing" \
            "5"

        run_test "Basic Greeting" \
            "hello" \
            "Response speed, typing indicator, formatting, personality" \
            "2"
        ;;

    core)
        # Core features
        run_test "First Impressions" \
            "Just look at each tool" \
            "Visual appeal, clarity, welcome message" \
            "3"

        run_test "Basic Greeting" \
            "hello" \
            "Response speed, typing indicator, formatting" \
            "2"

        run_test "Markdown Table" \
            "Create a table comparing Rust, Go, and Python" \
            "Table grid, alignment, headers, colors, readability" \
            "3"

        run_test "Long Response" \
            "Tell me about the complete architecture" \
            "Scrolling smoothness, scroll indicator, auto-scroll" \
            "3"

        run_test "File Reading" \
            "Read crates/rustycode-tui/src/minimal.rs" \
            "Code panel, syntax highlighting, line numbers" \
            "4"
        ;;

    deep)
        # Deep dive
        run_test "First Impressions" \
            "Just look at each tool" \
            "Visual appeal, clarity, welcome message" \
            "2"

        run_test "Basic Greeting" \
            "hello" \
            "Response speed, typing indicator, formatting" \
            "2"

        run_test "Markdown Table" \
            "Create a table comparing Rust, Go, and Python" \
            "Table grid, alignment, headers, colors" \
            "2"

        run_test "Long Response" \
            "Tell me about the complete architecture" \
            "Scrolling, scroll indicator, smoothness" \
            "2"

        run_test "File Reading" \
            "Read crates/rustycode-tui/src/minimal.rs" \
            "Code panel, syntax highlighting, line numbers" \
            "3"

        run_test "Tool Execution" \
            "List all Rust files and count total lines" \
            "Tool names, progress indicators, timing, tokens" \
            "3"

        run_test "Error Handling" \
            "Read /nonexistent/file.txt" \
            "Error color, message, recovery, suggestions" \
            "2"

        run_test "Visual Polish" \
            "Observe overall UI" \
            "Colors, separators, animations, icons, consistency" \
            "2"
        ;;

    full)
        # Full review - all tests
        run_test "First Impressions" \
            "Just look at each tool" \
            "Everything: visuals, layout, colors, spacing" \
            "2"

        run_test "Basic Greeting" \
            "hello" \
            "Speed, indicator, formatting, personality" \
            "1"

        run_test "Markdown Table" \
            "Create a table comparing Rust, Go, and Python" \
            "Grid, alignment, headers, colors, readability" \
            "2"

        run_test "Long Response" \
            "Tell me about the complete architecture" \
            "Scrolling, indicator, smoothness, auto-scroll" \
            "2"

        run_test "File Reading" \
            "Read crates/rustycode-tui/src/minimal.rs" \
            "Code panel, syntax highlighting, line numbers, scroll" \
            "3"

        run_test "Tool Execution" \
            "List all Rust files and count total lines" \
            "Tool names, progress, timing, tokens, parallel" \
            "3"

        run_test "Edit & Commit" \
            "Add a doc comment to minimal.rs" \
            "Edit preview, diff, confirmation, commit" \
            "5"

        run_test "Error Handling" \
            "Read /nonexistent/file.txt" \
            "Error color, message, recovery, suggestions" \
            "1"

        run_test "Keyboard Shortcuts" \
            "Try Ctrl+C, /help, Opt+Enter, Esc" \
            "Works? Helpful? Documented?" \
            "2"

        run_test "Visual Polish" \
            "Observe overall UI" \
            "Colors, separators, animations, consistency" \
            "2"

        run_test "Message Expand/Collapse" \
            "Try selecting and collapsing messages" \
            "Can collapse? Indicator? Preview?" \
            "2"
        ;;
esac

# Summary
clear
cat << 'SUMMARY'
╔════════════════════════════════════════════════════════════════════════╗
║                       Testing Complete! 🎉                             ║
╚════════════════════════════════════════════════════════════════════════╝

SUMMARY

echo -e "${GREEN}✅ All tests completed!${NC}"
echo ""
echo -e "${CYAN}📊 Your results are saved in:${NC}"
echo "   $RESULTS_FILE"
echo ""
echo -e "${CYAN}📝 Next steps:${NC}"
echo "1. Review your results: cat $RESULTS_FILE"
echo "2. Identify patterns"
echo "3. List top 10 missing features"
echo "4. Prioritize by impact/effort"
echo ""
echo -e "${BOLD}🎯 Quick analysis:${NC}"

# Calculate average scores
echo ""
echo "Creating summary..."
cat >> "$RESULTS_FILE" << 'ANALYSIS'

---

## Summary Analysis

### Overall Rankings (by average score)

### Top 5 Missing Features in RustyCode

1.
2.
3.
4.
5.

### Quick Wins (High Impact, Low Effort)

1.
2.
3.

### Implementation Priority

Phase 1 (This Week):
-

Phase 2 (Next Sprint):
-

Phase 3 (Future):
-

ANALYSIS

echo -e "${GREEN}✅ Analysis template added to results${NC}"
echo ""
echo -e "${YELLOW}💡 Pro tip:${NC} Open the results file and fill in the analysis section"
echo ""
