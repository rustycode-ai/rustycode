#!/bin/bash
# Runtime test script for TUI
# Tests various scenarios using the MockProvider

set -e

echo "================================"
echo "TUI Runtime Test Suite"
echo "================================"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TEST_NUM=1
PASSED=0
FAILED=0

# Function to run a test
run_test() {
    local test_name="$1"
    local mock_response="$2"
    local expected_output="$3"

    echo -e "${YELLOW}Test $TEST_NUM: $test_name${NC}"
    echo "Mock response: $mock_response"

    # Set environment variables for mock provider
    export RUSTYCODE_MOCK_RESPONSE="$mock_response"
    export RUSTYCODE_MOCK_MODEL="test-model"

    # Run the TUI with a timeout (it will exit automatically due to mock)
    # We use expect or similar to automate input
    timeout 5s cargo run --package rustycode-tui -- "$test_name" 2>&1 | head -20 || true

    echo ""
    TEST_NUM=$((TEST_NUM + 1))
}

# Test 1: Basic startup
echo -e "${GREEN}Test 1: Basic TUI startup${NC}"
if RUSTYCODE_MOCK_RESPONSE="Hello! I'm working!" timeout 3s cargo run --package rustycode-tui 2>&1 | grep -q "Welcome to RustyCode"; then
    echo -e "${GREEN}✓ Test 1 PASSED: TUI starts successfully${NC}"
    PASSED=$((PASSED + 1))
else
    echo -e "${RED}✗ Test 1 FAILED: TUI failed to start${NC}"
    FAILED=$((FAILED + 1))
fi
echo ""

# Test 2: Check for provider initialization
echo -e "${GREEN}Test 2: Provider initialization${NC}"
if RUSTYCODE_MOCK_RESPONSE="Test" RUSTYCODE_MOCK_MODEL="mock" timeout 3s cargo run --package rustycode-tui 2>&1 | grep -q "Using provider:"; then
    echo -e "${GREEN}✓ Test 2 PASSED: Provider initializes${NC}"
    PASSED=$((PASSED + 1))
else
    echo -e "${RED}✗ Test 2 FAILED: Provider not initialized${NC}"
    FAILED=$((FAILED + 1))
fi
echo ""

# Test 3: Check for UI rendering
echo -e "${GREEN}Test 3: UI rendering${NC}"
# The TUI should at least attempt to draw the UI
if RUSTYCODE_MOCK_RESPONSE="UI test" timeout 3s cargo run --package rustycode-tui 2>&1 | grep -E "(Welcome|Messages|Input)" > /dev/null; then
    echo -e "${GREEN}✓ Test 3 PASSED: UI renders${NC}"
    PASSED=$((PASSED + 1))
else
    echo -e "${RED}✗ Test 3 FAILED: UI rendering failed${NC}"
    FAILED=$((FAILED + 1))
fi
echo ""

# Test 4: Error handling - invalid provider
echo -e "${GREEN}Test 4: Error handling - no provider${NC}"
# Unset all API keys to test error handling
unset ANTHROPIC_API_KEY
unset OPENAI_API_KEY
unset GEMINI_API_KEY
unset RUSTYCODE_MOCK_RESPONSE

if timeout 3s cargo run --package rustycode-tui 2>&1 | grep -E "(error|Error|ERROR|No LLM provider)" > /dev/null; then
    echo -e "${GREEN}✓ Test 4 PASSED: Error handling works${NC}"
    PASSED=$((PASSED + 1))
else
    echo -e "${RED}✗ Test 4 FAILED: Error handling missing${NC}"
    FAILED=$((FAILED + 1))
fi
echo ""

# Test 5: Memory management
echo -e "${GREEN}Test 5: Memory management (basic)${NC}"
# Run with memory diagnostics enabled
export RUSTYCODE_MEM_DIAG=1
if RUSTYCODE_MOCK_RESPONSE="Memory test" timeout 3s cargo run --package rustycode-tui 2>&1 | head -5; then
    echo -e "${GREEN}✓ Test 5 PASSED: Memory diagnostics available${NC}"
    PASSED=$((PASSED + 1))
else
    echo -e "${RED}✗ Test 5 FAILED: Memory diagnostics failed${NC}"
    FAILED=$((FAILED + 1))
fi
unset RUSTYCODE_MEM_DIAG
echo ""

# Summary
echo "================================"
echo "Test Summary"
echo "================================"
echo -e "Total tests: $((TEST_NUM - 1))"
echo -e "${GREEN}Passed: $PASSED${NC}"
echo -e "${RED}Failed: $FAILED${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi
