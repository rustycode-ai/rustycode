#!/bin/bash
# Test script for responsive TUI improvements
# This script tests the TUI at different screen sizes

set -e

echo "=== Responsive TUI Test Suite ==="
echo ""

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test 1: Small screen (80x24)
echo -e "${YELLOW}Test 1: Small screen (80x24)${NC}"
echo "Expected: Compact mode, 70/30 split, shortened text"
echo "Resize your terminal to 80x24 and run: cargo run --release"
echo ""
read -p "Press Enter after testing small screen..."

# Test 2: Medium screen (120x40)
echo -e "${YELLOW}Test 2: Medium screen (120x40)${NC}"
echo "Expected: Balanced 60/40 split, normal text lengths"
echo "Resize your terminal to 120x40 and run: cargo run --release"
echo ""
read -p "Press Enter after testing medium screen..."

# Test 3: Large screen (200x60)
echo -e "${YELLOW}Test 3: Large screen (200x60)${NC}"
echo "Expected: Equal 50/50 split, extended previews"
echo "Resize your terminal to 200x60 and run: cargo run --release"
echo ""
read -p "Press Enter after testing large screen..."

# Test 4: Minimum size warning
echo -e "${YELLOW}Test 4: Minimum size (50x12)${NC}"
echo "Expected: User-friendly warning message in a box"
echo "Resize your terminal to 50x12 and run: cargo run --release"
echo ""
read -p "Press Enter after testing minimum size..."

# Test 5: Too small screen
echo -e "${YELLOW}Test 5: Too small (< 50x12)${NC}"
echo "Expected: Clear error message showing current vs required size"
echo "Resize your terminal to 40x10 and run: cargo run --release"
echo ""
read -p "Press Enter after testing too small screen..."

# Test 6: Resize during operation
echo -e "${YELLOW}Test 6: Resize during operation${NC}"
echo "Expected: Layout adapts smoothly without crashes"
echo "1. Start at 120x40: cargo run --release"
echo "2. While running, resize to 80x24, then 200x60"
echo "3. Verify layout updates smoothly"
echo ""
read -p "Press Enter after testing resize..."

# Test 7: Code panel on different screen sizes
echo -e "${YELLOW}Test 7: Code panel responsiveness${NC}"
echo "Expected: Panel split ratio changes based on screen size"
echo "1. Small screen (80x24): 70% chat / 30% code"
echo "2. Medium screen (120x40): 60% chat / 40% code"
echo "3. Large screen (200x60): 50% chat / 50% code"
echo ""
read -p "Press Enter after testing code panel..."

# Test 8: Long message preview on different sizes
echo -e "${YELLOW}Test 8: Long message preview${NC}"
echo "Expected: Preview length adapts to screen size"
echo "Small: 40 chars, Medium: 72 chars, Large: 120 chars"
echo ""
read -p "Press Enter after testing message preview..."

# Test 9: Header/footer truncation
echo -e "${YELLOW}Test 9: Header/footer truncation${NC}"
echo "Expected: Long text truncated appropriately on small screens"
echo "1. Session title truncated on small screens"
echo "2. Model name truncated on small screens"
echo "3. CWD path truncated on small screens"
echo ""
read -p "Press Enter after testing truncation..."

# Test 10: Input area scrolling
echo -e "${YELLOW}Test 10: Input area horizontal scrolling${NC}"
echo "Expected: Long input lines scroll horizontally"
echo "1. Type a very long line (> screen width)"
echo "2. Verify text scrolls to show cursor"
echo ""
read -p "Press Enter after testing input scrolling..."

echo ""
echo -e "${GREEN}=== All tests completed ===${NC}"
echo ""
echo "Summary of responsive improvements:"
echo "1. ✓ Screen size detection (Small/Medium/Large)"
echo "2. ✓ Adaptive panel split ratio (70/30, 60/40, 50/50)"
echo "3. ✓ User-friendly minimum size warning"
echo "4. ✓ Responsive text truncation"
echo "5. ✓ Adaptive preview lengths"
echo "6. ✓ Compact mode for small screens"
echo "7. ✓ Horizontal scrolling in input area"
echo "8. ✓ Smooth resize handling"
echo ""
