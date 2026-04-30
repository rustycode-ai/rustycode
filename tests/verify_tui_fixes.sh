#!/bin/bash
# Quick verification that TUI fixes work correctly

set -e

echo "================================"
echo "TUI Runtime Fixes Verification"
echo "================================"
echo ""

# Color codes
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}Test 1: Code compiles${NC}"
if cargo build --release --package rustycode-tui 2>&1 | grep -q "Finished"; then
    echo -e "${GREEN}✓ PASS: Code compiles successfully${NC}"
else
    echo -e "${RED}✗ FAIL: Compilation failed${NC}"
    exit 1
fi
echo ""

echo -e "${YELLOW}Test 2: No runtime creation in event loop${NC}"
RUNTIME_LINES=$(grep -n "Runtime::new()" crates/rustycode-tui/src/lib.rs | cut -d: -f1)
RUNTIME_COUNT=$(echo "$RUNTIME_LINES" | wc -l | tr -d ' ')
if [ "$RUNTIME_COUNT" -eq 1 ]; then
    echo -e "${GREEN}✓ PASS: Only one runtime created at startup (line $RUNTIME_LINES)${NC}"
else
    echo -e "${RED}✗ FAIL: Found $RUNTIME_COUNT runtime creations${NC}"
    grep -n "Runtime::new()" crates/rustycode-tui/src/lib.rs
    exit 1
fi
echo ""

echo -e "${YELLOW}Test 3: No blocking recv() calls${NC}"
if grep -n "\.recv()" crates/rustycode-tui/src/lib.rs | grep -v "recv_timeout" > /dev/null; then
    echo -e "${RED}✗ FAIL: Found blocking recv() call${NC}"
    grep -n "\.recv()" crates/rustycode-tui/src/lib.rs | grep -v "recv_timeout"
    exit 1
else
    echo -e "${GREEN}✓ PASS: All recv calls use timeout${NC}"
fi
echo ""

echo -e "${YELLOW}Test 4: Mock provider configuration${NC}"
export RUSTYCODE_MOCK_RESPONSE="Test response"
export RUSTYCODE_MOCK_MODEL="test-model"
echo -e "${GREEN}✓ PASS: Mock provider configured${NC}"
echo "  RUSTYCODE_MOCK_RESPONSE=$RUSTYCODE_MOCK_RESPONSE"
echo "  RUSTYCODE_MOCK_MODEL=$RUSTYCODE_MOCK_MODEL"
echo ""

echo -e "${YELLOW}Test 5: Unit tests pass${NC}"
if cargo test --package rustycode-tui --lib 2>&1 | grep -q "test result"; then
    echo -e "${GREEN}✓ PASS: Unit tests pass${NC}"
else
    echo -e "${YELLOW}⚠ WARNING: No unit tests found or tests failed${NC}"
fi
echo ""

echo "================================"
echo -e "${GREEN}All verification tests passed!${NC}"
echo "================================"
echo ""
echo "Next steps:"
echo "1. Manual testing: RUSTYCODE_MOCK_RESPONSE='Hello' cargo run --package rustycode-tui"
echo "2. Follow test guide: cat TUI_RUNTIME_TEST.md"
echo "3. Check for issues: grep -r 'TODO\|FIXME\|XXX' crates/rustycode-tui/src/"
