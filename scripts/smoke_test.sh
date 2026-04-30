#!/bin/bash
# Smoke tests for deployment verification
# Tests core functionality after deployment

set -e

echo "======================================"
echo "RustyCode 2.0.0 Smoke Tests"
echo "======================================"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Function to run a test
run_test() {
    local test_name=$1
    local test_command=$2

    echo -e "${YELLOW}Testing: $test_name${NC}"

    if eval "$test_command" > /tmp/smoke_test_$$.log 2>&1; then
        echo -e "${GREEN}✓ PASSED: $test_name${NC}"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        echo -e "${RED}✗ FAILED: $test_name${NC}"
        echo "Error output:"
        cat /tmp/smoke_test_$$.log
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi
}

# Check if rustycode is installed
if ! command -v rustycode &> /dev/null; then
    echo -e "${RED}Error: rustycode command not found${NC}"
    echo "Please install rustycode first:"
    echo "  cargo install --path ."
    exit 1
fi

echo -e "${GREEN}Found rustycode version:${NC}"
rustycode --version
echo ""

# Test 1: Version check
run_test "Version check" "rustycode --version"

# Test 2: System health check
run_test "System health check" "rustycode --check"

# Test 3: Configuration validation
echo ""
echo -e "${YELLOW}Testing: Configuration validation${NC}"
if rustycode config validate > /tmp/smoke_test_$$.log 2>&1; then
    echo -e "${GREEN}✓ PASSED: Configuration validation${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAILED: Configuration validation${NC}"
    cat /tmp/smoke_test_$$.log
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 4: Configuration display
run_test "Configuration display" "rustycode config show"

# Test 5: Provider discovery
echo ""
echo -e "${YELLOW}Testing: Provider discovery${NC}"
if rustycode providers list > /tmp/smoke_test_$$.log 2>&1; then
    PROVIDER_COUNT=$(grep -c "Provider:" /tmp/smoke_test_$$.log || echo "0")
    echo -e "${GREEN}✓ PASSED: Provider discovery ($PROVIDER_COUNT providers found)${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAILED: Provider discovery${NC}"
    cat /tmp/smoke_test_$$.log
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 6: Help command
run_test "Help command" "rustycode --help"

# Test 7: Session list (should work even if empty)
echo ""
echo -e "${YELLOW}Testing: Session list${NC}"
if rustycode sessions list > /tmp/smoke_test_$$.log 2>&1; then
    echo -e "${GREEN}✓ PASSED: Session list${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAILED: Session list${NC}"
    cat /tmp/smoke_test_$$.log
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 8: Agent list
echo ""
echo -e "${YELLOW}Testing: Agent list${NC}"
if rustycode agents list > /tmp/smoke_test_$$.log 2>&1; then
    AGENT_COUNT=$(grep -c "Agent:" /tmp/smoke_test_$$.log || echo "0")
    echo -e "${GREEN}✓ PASSED: Agent list ($AGENT_COUNT agents available)${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAILED: Agent list${NC}"
    cat /tmp/smoke_test_$$.log
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 9: Log directory exists
echo ""
echo -e "${YELLOW}Testing: Log directory setup${NC}"
if [ -d "$HOME/.rustycode/logs" ]; then
    echo -e "${GREEN}✓ PASSED: Log directory exists${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAILED: Log directory not found${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 10: Configuration file exists
echo ""
echo -e "${YELLOW}Testing: Configuration file exists${NC}"
if [ -f "$HOME/.rustycode/config.jsonc" ] || [ -f "$HOME/.rustycode/config.json" ]; then
    echo -e "${GREEN}✓ PASSED: Configuration file exists${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${YELLOW}⚠ WARNING: No configuration file found (may be using defaults)${NC}"
    # Don't count as failure, config is optional
fi

# Optional tests (run if API keys are available)

# Test 11: Provider test (if API key available)
echo ""
if [ -n "$ANTHROPIC_API_KEY" ]; then
    echo -e "${YELLOW}Testing: Anthropic provider connection (API key found)${NC}"
    if rustycode providers test anthropic > /tmp/smoke_test_$$.log 2>&1; then
        echo -e "${GREEN}✓ PASSED: Anthropic provider connection${NC}"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗ FAILED: Anthropic provider connection${NC}"
        cat /tmp/smoke_test_$$.log
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
else
    echo -e "${YELLOW}⚠ SKIPPED: Provider connection test (no ANTHROPIC_API_KEY set)${NC}"
fi

# Test 12: MCP test (if configured)
echo ""
if rustycode mcp list > /tmp/smoke_test_$$.log 2>&1 && [ -s /tmp/smoke_test_$$.log ]; then
    echo -e "${YELLOW}Testing: MCP server health${NC}"
    if rustycode mcp health > /tmp/smoke_test_$$.log 2>&1; then
        echo -e "${GREEN}✓ PASSED: MCP server health${NC}"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${YELLOW}⚠ WARNING: MCP health check failed (may not be configured)${NC}"
        cat /tmp/smoke_test_$$.log
    fi
else
    echo -e "${YELLOW}⚠ SKIPPED: MCP health check (no MCP servers configured)${NC}"
fi

# Summary
echo ""
echo "======================================"
echo "Smoke Test Summary"
echo "======================================"
echo -e "${GREEN}Tests Passed: $TESTS_PASSED${NC}"
echo -e "${RED}Tests Failed: $TESTS_FAILED${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}✓ All smoke tests passed!${NC}"
    echo "System is ready for use."
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    echo "Please review the errors above and:"
    echo "1. Check configuration: rustycode config show"
    echo "2. Verify environment variables are set"
    echo "3. Review logs: cat ~/.rustycode/logs/rustycode.log"
    echo "4. Consult troubleshooting guide: docs/architecture-upgrade/MIGRATION.md"
    exit 1
fi
